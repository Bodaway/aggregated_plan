use chrono::{NaiveDate, Utc};
use domain::rules::urgency::calculate_urgency;
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::*;
use crate::services::*;

/// Aggregates all dependencies needed to run a full or partial sync.
pub struct SyncContext<'a> {
    pub task_repo: &'a dyn TaskRepository,
    pub meeting_repo: &'a dyn MeetingRepository,
    pub project_repo: &'a dyn ProjectRepository,
    pub sync_repo: &'a dyn SyncStatusRepository,
    pub config_repo: &'a dyn ConfigRepository,
    pub jira_client: Option<&'a dyn JiraClient>,
    pub outlook_client: Option<&'a dyn OutlookClient>,
    pub excel_client: Option<&'a dyn ExcelClient>,
}

/// Result of a synchronization operation with a single source.
pub struct SyncResult {
    pub source: Source,
    pub tasks_created: usize,
    pub tasks_updated: usize,
    pub tasks_removed: usize,
    pub meetings_synced: usize,
    pub errors: Vec<String>,
}

/// Configuration for Jira synchronization.
pub struct JiraConfig {
    pub project_keys: Vec<String>,
    pub assignees: Option<Vec<String>>,
    /// When true, restrict the JQL to issues assigned to or watched by the
    /// authenticated API user (`currentUser()`).
    pub my_tasks_only: bool,
}

/// Synchronize tasks from Jira.
pub async fn sync_jira(
    jira_client: &dyn JiraClient,
    task_repo: &dyn TaskRepository,
    project_repo: &dyn ProjectRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    config: &JiraConfig,
) -> Result<SyncResult, AppError> {
    let now = Utc::now();
    let today = now.date_naive();

    // Mark sync as in progress.
    sync_repo
        .upsert(&SyncStatus {
            source: Source::Jira,
            user_id,
            last_sync_at: Some(now),
            status: SyncSourceStatus::Syncing,
            error_message: None,
        })
        .await?;

    let jira_tasks = match jira_client
        .fetch_tasks(
            &config.project_keys,
            config.assignees.as_deref(),
            config.my_tasks_only,
        )
        .await
    {
        Ok(tasks) => tasks,
        Err(e) => {
            update_sync_error(sync_repo, user_id, Source::Jira, &e.to_string()).await?;
            return Err(AppError::Connector {
                connector_source: Source::Jira,
                message: e.to_string(),
            });
        }
    };

    let mut created = 0usize;
    let mut updated = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for jira_task in &jira_tasks {
        // Ensure we have a local project for this Jira project.
        let project_id = match ensure_project(
            project_repo,
            user_id,
            Source::Jira,
            &jira_task.project_key,
            &jira_task.project_name,
        )
        .await
        {
            Ok(id) => Some(id),
            Err(e) => {
                errors.push(format!(
                    "Failed to upsert project {} : {}",
                    jira_task.project_key, e
                ));
                None
            }
        };

        // Check if we already have this task.
        let existing = task_repo
            .find_by_source(user_id, Source::Jira, &jira_task.key)
            .await?;

        match existing {
            Some(mut task) => {
                // Update existing task fields from Jira.
                task.title = jira_task.title.clone();
                task.description = jira_task.description.clone();
                // NOTE: `task.notes` is intentionally NOT touched here — it is the user's
                // local markdown journal and must survive every Jira resync. Same for
                // `urgency_manual`, `remaining_hours_override`, `estimated_hours_override`.
                task.jira_status = Some(jira_task.status.clone());
                task.status = map_jira_status(&jira_task.status);
                task.assignee = jira_task.assignee.clone();
                task.deadline = jira_task.deadline;
                task.project_id = project_id;
                task.jira_remaining_seconds = jira_task.time_estimate_seconds;
                task.jira_original_estimate_seconds = jira_task.time_original_estimate_seconds;
                task.jira_time_spent_seconds = jira_task.time_spent_seconds;
                // Override fields are NOT touched by sync — user's local data preserved
                if !task.urgency_manual {
                    task.urgency = calculate_urgency(task.deadline, today);
                }
                task.updated_at = now;
                task_repo.save(&task).await?;
                updated += 1;
            }
            None => {
                // Create a new task from Jira data.
                let task = Task {
                    id: Uuid::new_v4(),
                    user_id,
                    title: jira_task.title.clone(),
                    description: jira_task.description.clone(),
                    notes: None,
                    source: Source::Jira,
                    source_id: Some(jira_task.key.clone()),
                    jira_status: Some(jira_task.status.clone()),
                    status: map_jira_status(&jira_task.status),
                    project_id,
                    assignee: jira_task.assignee.clone(),
                    delegated_to: None,
                    deadline: jira_task.deadline,
                    planned_start: None,
                    planned_end: None,
                    estimated_hours: None,
                    urgency: calculate_urgency(jira_task.deadline, today),
                    urgency_manual: false,
                    impact: ImpactLevel::Medium,
                    tags: vec![],
                    tracking_state: TrackingState::Inbox,
                    jira_remaining_seconds: jira_task.time_estimate_seconds,
                    jira_original_estimate_seconds: jira_task.time_original_estimate_seconds,
                    jira_time_spent_seconds: jira_task.time_spent_seconds,
                    remaining_hours_override: None,
                    estimated_hours_override: None,
                    recurrence_id: None,
                    occurrence_date: None,
                    created_at: now,
                    updated_at: now,
                };
                task_repo.save(&task).await?;
                created += 1;
            }
        }
    }

    // Remove tasks from a previous (broader) sync that are no longer in the
    // current result set, keeping the local task list in sync with the filter.
    let fetched_ids: Vec<String> = jira_tasks.iter().map(|t| t.key.clone()).collect();
    let removed = task_repo
        .delete_stale_by_source(user_id, Source::Jira, &fetched_ids)
        .await
        .unwrap_or(0);

    // Update sync status to success.
    sync_repo
        .upsert(&SyncStatus {
            source: Source::Jira,
            user_id,
            last_sync_at: Some(Utc::now()),
            status: SyncSourceStatus::Success,
            error_message: if errors.is_empty() {
                None
            } else {
                Some(errors.join("; "))
            },
        })
        .await?;

    Ok(SyncResult {
        source: Source::Jira,
        tasks_created: created,
        tasks_updated: updated,
        tasks_removed: removed as usize,
        meetings_synced: 0,
        errors,
    })
}

/// Synchronize calendar events from Outlook.
pub async fn sync_outlook(
    outlook_client: &dyn OutlookClient,
    meeting_repo: &dyn MeetingRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    date_range: (NaiveDate, NaiveDate),
    exclude_patterns: &[String],
) -> Result<SyncResult, AppError> {
    let now = Utc::now();

    // Mark sync as in progress.
    sync_repo
        .upsert(&SyncStatus {
            source: Source::Outlook,
            user_id,
            last_sync_at: Some(now),
            status: SyncSourceStatus::Syncing,
            error_message: None,
        })
        .await?;

    let events = outlook_client
        .fetch_calendar(date_range.0, date_range.1)
        .await
        .map_err(|e| AppError::Connector {
            connector_source: Source::Outlook,
            message: e.to_string(),
        })?;

    // Skip events whose title matches the user's exclusion list (case-insensitive contains).
    let events: Vec<_> = events
        .into_iter()
        .filter(|e| !domain::rules::meeting::is_excluded(&e.title, exclude_patterns))
        .collect();

    // Convert events to meetings.
    let meetings: Vec<Meeting> = events
        .into_iter()
        .map(|event| Meeting {
            id: Uuid::new_v4(),
            user_id,
            title: event.title,
            start_time: event.start_time,
            end_time: event.end_time,
            location: event.location,
            participants: event.participants,
            project_id: None,
            outlook_id: event.outlook_id,
            show_as: event.show_as,
            created_at: now,
        })
        .collect();

    let meeting_count = meetings.len();

    // Upsert all meetings.
    meeting_repo.upsert_batch(&meetings).await?;

    // Collect current outlook_ids and remove stale entries.
    let current_ids: Vec<String> = meetings.iter().map(|m| m.outlook_id.clone()).collect();
    let deleted = meeting_repo.delete_stale(user_id, &current_ids).await?;

    // Update sync status to success.
    sync_repo
        .upsert(&SyncStatus {
            source: Source::Outlook,
            user_id,
            last_sync_at: Some(Utc::now()),
            status: SyncSourceStatus::Success,
            error_message: None,
        })
        .await?;

    Ok(SyncResult {
        source: Source::Outlook,
        tasks_created: 0,
        tasks_updated: 0,
        tasks_removed: deleted as usize,
        meetings_synced: meeting_count,
        errors: Vec::new(),
    })
}

/// Synchronize tasks from an Excel/SharePoint spreadsheet.
pub async fn sync_excel(
    excel_client: &dyn ExcelClient,
    task_repo: &dyn TaskRepository,
    project_repo: &dyn ProjectRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    config: &ExcelMappingConfig,
) -> Result<SyncResult, AppError> {
    let now = Utc::now();
    let today = now.date_naive();

    // Mark sync as in progress.
    sync_repo
        .upsert(&SyncStatus {
            source: Source::Excel,
            user_id,
            last_sync_at: Some(now),
            status: SyncSourceStatus::Syncing,
            error_message: None,
        })
        .await?;

    let rows = excel_client.fetch_rows(config).await.map_err(|e| {
        AppError::Connector {
            connector_source: Source::Excel,
            message: e.to_string(),
        }
    })?;

    let mut created = 0usize;
    let mut updated = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for row in &rows {
        let title = match row.columns.get(&config.title_column) {
            Some(t) if !t.is_empty() => t.clone(),
            _ => continue, // Skip rows without a title.
        };

        // Use the row index as the source identifier for Excel tasks.
        let source_id = format!(
            "{}:{}:row{}",
            config.sharepoint_path,
            config.sheet_name.as_deref().unwrap_or("Sheet1"),
            row.row_index
        );

        // Optionally resolve project.
        let project_id = if let Some(ref proj_col) = config.project_column {
            if let Some(proj_name) = row.columns.get(proj_col) {
                if !proj_name.is_empty() {
                    match ensure_project(
                        project_repo,
                        user_id,
                        Source::Excel,
                        proj_name,
                        proj_name,
                    )
                    .await
                    {
                        Ok(id) => Some(id),
                        Err(e) => {
                            errors.push(format!(
                                "Row {}: failed to upsert project '{}': {}",
                                row.row_index, proj_name, e
                            ));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let assignee = config
            .assignee_column
            .as_ref()
            .and_then(|col| row.columns.get(col))
            .filter(|s| !s.is_empty())
            .cloned();

        let deadline: Option<NaiveDate> = config
            .date_column
            .as_ref()
            .and_then(|col| row.columns.get(col))
            .and_then(|d| d.parse().ok());

        let status = config
            .status_column
            .as_ref()
            .and_then(|col| row.columns.get(col))
            .map(|s| map_excel_status(s))
            .unwrap_or(TaskStatus::Todo);

        let existing = task_repo
            .find_by_source(user_id, Source::Excel, &source_id)
            .await?;

        match existing {
            Some(mut task) => {
                task.title = title;
                task.assignee = assignee;
                task.deadline = deadline;
                task.project_id = project_id;
                task.status = status;
                if !task.urgency_manual {
                    task.urgency = calculate_urgency(task.deadline, today);
                }
                task.updated_at = now;
                task_repo.save(&task).await?;
                updated += 1;
            }
            None => {
                let task = Task {
                    id: Uuid::new_v4(),
                    user_id,
                    title,
                    description: None,
                    notes: None,
                    source: Source::Excel,
                    source_id: Some(source_id),
                    jira_status: None,
                    status,
                    project_id,
                    assignee,
                    delegated_to: None,
                    deadline,
                    planned_start: None,
                    planned_end: None,
                    estimated_hours: None,
                    urgency: calculate_urgency(deadline, today),
                    urgency_manual: false,
                    impact: ImpactLevel::Medium,
                    tags: vec![],
                    tracking_state: TrackingState::Inbox,
                    jira_remaining_seconds: None,
                    jira_original_estimate_seconds: None,
                    jira_time_spent_seconds: None,
                    remaining_hours_override: None,
                    estimated_hours_override: None,
                    recurrence_id: None,
                    occurrence_date: None,
                    created_at: now,
                    updated_at: now,
                };
                task_repo.save(&task).await?;
                created += 1;
            }
        }
    }

    // Update sync status to success.
    sync_repo
        .upsert(&SyncStatus {
            source: Source::Excel,
            user_id,
            last_sync_at: Some(Utc::now()),
            status: SyncSourceStatus::Success,
            error_message: if errors.is_empty() {
                None
            } else {
                Some(errors.join("; "))
            },
        })
        .await?;

    Ok(SyncResult {
        source: Source::Excel,
        tasks_created: created,
        tasks_updated: updated,
        tasks_removed: 0,
        meetings_synced: 0,
        errors,
    })
}

/// Run all configured synchronizations for a user.
pub async fn sync_all(ctx: &SyncContext<'_>, user_id: UserId) -> Result<Vec<SyncResult>, AppError> {
    let task_repo = ctx.task_repo;
    let meeting_repo = ctx.meeting_repo;
    let project_repo = ctx.project_repo;
    let sync_repo = ctx.sync_repo;
    let config_repo = ctx.config_repo;
    let jira_client = ctx.jira_client;
    let outlook_client = ctx.outlook_client;
    let excel_client = ctx.excel_client;
    let mut results: Vec<SyncResult> = Vec::new();

    // Jira sync.
    if let Some(client) = jira_client {
        let keys_str = config_repo
            .get(user_id, "jira.project_keys")
            .await?;

        if let Some(keys_raw) = keys_str {
            let project_keys: Vec<String> =
                keys_raw.split(',').map(|s| s.trim().to_string()).collect();
            let assignees_str = config_repo
                .get(user_id, "jira.assignees")
                .await?;
            let assignees = assignees_str.map(|s| {
                s.split(',').map(|a| a.trim().to_string()).collect::<Vec<_>>()
            });
            let my_tasks_only = config_repo
                .get(user_id, "jira.my_tasks_only")
                .await?
                .as_deref() == Some("true");

            let config = JiraConfig {
                project_keys,
                assignees,
                my_tasks_only,
            };
            match sync_jira(client, task_repo, project_repo, sync_repo, user_id, &config).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    update_sync_error(sync_repo, user_id, Source::Jira, &e.to_string()).await?;
                    results.push(SyncResult {
                        source: Source::Jira,
                        tasks_created: 0,
                        tasks_updated: 0,
                        tasks_removed: 0,
                        meetings_synced: 0,
                        errors: vec![e.to_string()],
                    });
                }
            }
        } else {
            update_sync_error(sync_repo, user_id, Source::Jira, "Not configured").await?;
        }
    } else {
        update_sync_error(sync_repo, user_id, Source::Jira, "Not configured").await?;
    }

    // Outlook sync.
    if let Some(client) = outlook_client {
        let today = Utc::now().date_naive();
        let days: i64 = config_repo
            .get(user_id, "outlook.calendar_days")
            .await?
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|d| *d > 0)
            .unwrap_or(14);
        let end = today + chrono::Duration::days(days);
        let exclude_patterns: Vec<String> = config_repo
            .get(user_id, "outlook.exclude_patterns")
            .await?
            .map(|raw| {
                raw.lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        match sync_outlook(client, meeting_repo, sync_repo, user_id, (today, end), &exclude_patterns).await {
            Ok(result) => results.push(result),
            Err(e) => {
                update_sync_error(sync_repo, user_id, Source::Outlook, &e.to_string()).await?;
                results.push(SyncResult {
                    source: Source::Outlook,
                    tasks_created: 0,
                    tasks_updated: 0,
                    tasks_removed: 0,
                    meetings_synced: 0,
                    errors: vec![e.to_string()],
                });
            }
        }
    } else {
        update_sync_error(sync_repo, user_id, Source::Outlook, "Not configured").await?;
    }

    // Excel sync.
    if let Some(client) = excel_client {
        let path = config_repo
            .get(user_id, "excel.sharepoint_path")
            .await?;

        if let Some(sharepoint_path) = path {
            let sheet_name = config_repo
                .get(user_id, "excel.sheet_name")
                .await?;
            let title_column = config_repo
                .get(user_id, "excel.title_column")
                .await?
                .unwrap_or_else(|| "Title".to_string());

            let excel_config = ExcelMappingConfig {
                sharepoint_path,
                sheet_name,
                title_column,
                assignee_column: config_repo
                    .get(user_id, "excel.assignee_column")
                    .await?,
                project_column: config_repo
                    .get(user_id, "excel.project_column")
                    .await?,
                date_column: config_repo
                    .get(user_id, "excel.date_column")
                    .await?,
                jira_key_column: config_repo
                    .get(user_id, "excel.jira_key_column")
                    .await?,
                status_column: config_repo
                    .get(user_id, "excel.status_column")
                    .await?,
            };

            match sync_excel(
                client,
                task_repo,
                project_repo,
                sync_repo,
                user_id,
                &excel_config,
            )
            .await
            {
                Ok(result) => results.push(result),
                Err(e) => {
                    update_sync_error(sync_repo, user_id, Source::Excel, &e.to_string()).await?;
                    results.push(SyncResult {
                        source: Source::Excel,
                        tasks_created: 0,
                        tasks_updated: 0,
                        tasks_removed: 0,
                        meetings_synced: 0,
                        errors: vec![e.to_string()],
                    });
                }
            }
        } else {
            update_sync_error(sync_repo, user_id, Source::Excel, "Not configured").await?;
        }
    } else {
        update_sync_error(sync_repo, user_id, Source::Excel, "Not configured").await?;
    }

    Ok(results)
}

/// Synchronize a specific source. Convenience function for the force_sync mutation.
pub async fn sync_source(ctx: &SyncContext<'_>, source: Source, user_id: UserId) -> Result<SyncStatus, AppError> {
    let task_repo = ctx.task_repo;
    let meeting_repo = ctx.meeting_repo;
    let project_repo = ctx.project_repo;
    let sync_repo = ctx.sync_repo;
    let config_repo = ctx.config_repo;
    let jira_client = ctx.jira_client;
    let outlook_client = ctx.outlook_client;
    let excel_client = ctx.excel_client;
    match source {
        Source::Jira => {
            if let Some(client) = jira_client {
                let keys_str = config_repo.get(user_id, "jira.project_keys").await?;
                if let Some(keys_raw) = keys_str {
                    let project_keys: Vec<String> =
                        keys_raw.split(',').map(|s| s.trim().to_string()).collect();
                    let assignees_str = config_repo.get(user_id, "jira.assignees").await?;
                    let assignees = assignees_str.map(|s| {
                        s.split(',').map(|a| a.trim().to_string()).collect::<Vec<_>>()
                    });
                    let my_tasks_only = config_repo
                        .get(user_id, "jira.my_tasks_only")
                        .await?
                        .as_deref() == Some("true");
                    let config = JiraConfig {
                        project_keys,
                        assignees,
                        my_tasks_only,
                    };
                    sync_jira(client, task_repo, project_repo, sync_repo, user_id, &config)
                        .await?;
                } else {
                    update_sync_error(sync_repo, user_id, Source::Jira, "Not configured").await?;
                }
            } else {
                update_sync_error(sync_repo, user_id, Source::Jira, "Not configured").await?;
            }
        }
        Source::Outlook => {
            if let Some(client) = outlook_client {
                let today = Utc::now().date_naive();
                let days: i64 = config_repo
                    .get(user_id, "outlook.calendar_days")
                    .await?
                    .and_then(|v| v.trim().parse::<i64>().ok())
                    .filter(|d| *d > 0)
                    .unwrap_or(14);
                let end = today + chrono::Duration::days(days);
                let exclude_patterns: Vec<String> = config_repo
                    .get(user_id, "outlook.exclude_patterns")
                    .await?
                    .map(|raw| {
                        raw.lines()
                            .map(|l| l.trim().to_string())
                            .filter(|l| !l.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                sync_outlook(client, meeting_repo, sync_repo, user_id, (today, end), &exclude_patterns).await?;
            } else {
                update_sync_error(sync_repo, user_id, Source::Outlook, "Not configured").await?;
            }
        }
        Source::Obsidian => {
            // Obsidian is not a real sync source; nothing to do.
        }
        Source::Excel => {
            if let Some(client) = excel_client {
                let path = config_repo.get(user_id, "excel.sharepoint_path").await?;
                if let Some(sharepoint_path) = path {
                    let sheet_name = config_repo.get(user_id, "excel.sheet_name").await?;
                    let title_column = config_repo
                        .get(user_id, "excel.title_column")
                        .await?
                        .unwrap_or_else(|| "Title".to_string());

                    let excel_config = ExcelMappingConfig {
                        sharepoint_path,
                        sheet_name,
                        title_column,
                        assignee_column: config_repo
                            .get(user_id, "excel.assignee_column")
                            .await?,
                        project_column: config_repo
                            .get(user_id, "excel.project_column")
                            .await?,
                        date_column: config_repo
                            .get(user_id, "excel.date_column")
                            .await?,
                        jira_key_column: config_repo
                            .get(user_id, "excel.jira_key_column")
                            .await?,
                        status_column: config_repo
                            .get(user_id, "excel.status_column")
                            .await?,
                    };
                    sync_excel(
                        client,
                        task_repo,
                        project_repo,
                        sync_repo,
                        user_id,
                        &excel_config,
                    )
                    .await?;
                } else {
                    update_sync_error(sync_repo, user_id, Source::Excel, "Not configured").await?;
                }
            } else {
                update_sync_error(sync_repo, user_id, Source::Excel, "Not configured").await?;
            }
        }
        Source::Personal => {
            // Personal tasks are not synced from an external source.
        }
        Source::Gryzzly => {
            // Gryzzly sync not yet implemented — placeholder for Task 5.
        }
    }

    // Return the current sync status for the requested source.
    let statuses = sync_repo.find_by_user(user_id).await?;
    statuses
        .into_iter()
        .find(|s| s.source == source)
        .ok_or_else(|| AppError::NotFound(format!("SyncStatus for {:?}", source)))
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Ensure that a local project exists for the given source/key, creating it if needed.
async fn ensure_project(
    project_repo: &dyn ProjectRepository,
    user_id: UserId,
    source: Source,
    source_id: &str,
    name: &str,
) -> Result<ProjectId, AppError> {
    if let Some(project) = project_repo
        .find_by_source(user_id, source, source_id)
        .await?
    {
        return Ok(project.id);
    }

    let now = Utc::now();
    let project = Project {
        id: Uuid::new_v4(),
        user_id,
        name: name.to_string(),
        source,
        source_id: Some(source_id.to_string()),
        status: ProjectStatus::Active,
        created_at: now,
        updated_at: now,
    };
    project_repo.save(&project).await?;
    Ok(project.id)
}

/// Map a raw Jira status name to our internal TaskStatus.
///
/// Matching is substring-based and case-insensitive to handle statuses that
/// include numeric prefixes or parenthetical suffixes (e.g. "14. Clos",
/// "4. En cours (W)").
fn map_jira_status(jira_status: &str) -> TaskStatus {
    let lower = jira_status.to_lowercase();
    // Done — terminal states
    if lower.contains("done")
        || lower.contains("closed")
        || lower.contains("resolved")
        || lower.contains("complete")
        || lower.contains("clos")
        || lower.contains("en production")
        || lower.contains("abandonné")
        || lower.contains("abandonne")
    {
        return TaskStatus::Done;
    }
    // In-progress states
    if lower.contains("in progress")
        || lower.contains("in review")
        || lower.contains("review")
        || lower.contains("active")
        || lower.contains("en cours")
    {
        return TaskStatus::InProgress;
    }
    // Blocked states
    if lower.contains("blocked") || lower.contains("impediment") {
        return TaskStatus::Blocked;
    }
    TaskStatus::Todo
}

/// Map a raw Excel status string to our internal TaskStatus.
fn map_excel_status(status: &str) -> TaskStatus {
    match status.to_lowercase().as_str() {
        "done" | "closed" | "resolved" | "complete" | "completed" | "terminé" => TaskStatus::Done,
        "in progress" | "en cours" | "active" => TaskStatus::InProgress,
        "blocked" | "bloqué" => TaskStatus::Blocked,
        _ => TaskStatus::Todo,
    }
}

/// Update the sync status for a source to Error.
async fn update_sync_error(
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    source: Source,
    message: &str,
) -> Result<(), AppError> {
    sync_repo
        .upsert(&SyncStatus {
            source,
            user_id,
            last_sync_at: Some(Utc::now()),
            status: SyncSourceStatus::Error,
            error_message: Some(message.to_string()),
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::{ConnectorError, RepositoryError};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Returns one fixed Jira task ("AP-1") on every fetch.
    struct StubJiraClient;

    #[async_trait]
    impl JiraClient for StubJiraClient {
        async fn fetch_tasks(
            &self,
            _project_keys: &[String],
            _assignees: Option<&[String]>,
            _my_tasks_only: bool,
        ) -> Result<Vec<JiraTask>, ConnectorError> {
            Ok(vec![JiraTask {
                key: "AP-1".to_string(),
                title: "Synced title".to_string(),
                description: Some("Synced description".to_string()),
                status: "In Progress".to_string(),
                assignee: Some("jira.user@example.com".to_string()),
                deadline: None,
                priority: None,
                project_key: "AP".to_string(),
                project_name: "Aggregated Plan".to_string(),
                time_estimate_seconds: None,
                time_spent_seconds: None,
                time_original_estimate_seconds: None,
            }])
        }
    }

    /// Minimal in-memory TaskRepository covering only what sync_jira touches.
    #[derive(Default)]
    struct MiniTaskRepo {
        tasks: Mutex<HashMap<TaskId, Task>>,
    }

    #[async_trait]
    impl TaskRepository for MiniTaskRepo {
        async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
            Ok(self.tasks.lock().unwrap().get(&id).cloned())
        }
        async fn find_by_user(
            &self,
            user_id: UserId,
            _filter: &TaskFilter,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .values()
                .filter(|t| t.user_id == user_id)
                .cloned()
                .collect())
        }
        async fn find_by_source(
            &self,
            user_id: UserId,
            source: Source,
            source_id: &str,
        ) -> Result<Option<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .values()
                .find(|t| {
                    t.user_id == user_id
                        && t.source == source
                        && t.source_id.as_deref() == Some(source_id)
                })
                .cloned())
        }
        async fn find_by_date_range(
            &self,
            _user_id: UserId,
            _start: NaiveDate,
            _end: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(vec![])
        }
        async fn find_planned_before(
            &self,
            _user_id: UserId,
            _before_date: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(vec![])
        }
        async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
            self.tasks.lock().unwrap().insert(task.id, task.clone());
            Ok(())
        }
        async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError> {
            for t in tasks {
                self.save(t).await?;
            }
            Ok(())
        }
        async fn delete(&self, id: TaskId) -> Result<(), RepositoryError> {
            self.tasks.lock().unwrap().remove(&id);
            Ok(())
        }
        async fn delete_stale_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _keep_ids: &[String],
        ) -> Result<u64, RepositoryError> {
            Ok(0)
        }
    }

    struct StubProjectRepo;

    #[async_trait]
    impl ProjectRepository for StubProjectRepo {
        async fn find_by_id(&self, _id: ProjectId) -> Result<Option<Project>, RepositoryError> {
            Ok(None)
        }
        async fn find_by_user(&self, _user_id: UserId) -> Result<Vec<Project>, RepositoryError> {
            Ok(vec![])
        }
        async fn find_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _source_key: &str,
        ) -> Result<Option<Project>, RepositoryError> {
            Ok(None)
        }
        async fn save(&self, _project: &Project) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn delete(&self, _id: ProjectId) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    struct StubSyncRepo;

    #[async_trait]
    impl SyncStatusRepository for StubSyncRepo {
        async fn find_by_user(&self, _user_id: UserId) -> Result<Vec<SyncStatus>, RepositoryError> {
            Ok(vec![])
        }
        async fn upsert(&self, _status: &SyncStatus) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn jira_sync_preserves_delegated_to() {
        let user_id: UserId =
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let task_repo = MiniTaskRepo::default();
        let now = Utc::now();

        // Pre-existing synced task that the user has delegated locally.
        let existing = Task {
            id: Uuid::new_v4(),
            user_id,
            title: "Old title".to_string(),
            description: None,
            notes: Some("local notes".to_string()),
            source: Source::Jira,
            source_id: Some("AP-1".to_string()),
            jira_status: Some("To Do".to_string()),
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            delegated_to: Some("Marie".to_string()),
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            tags: vec![],
            tracking_state: TrackingState::Followed,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            recurrence_id: None,
            occurrence_date: None,
            created_at: now,
            updated_at: now,
        };
        task_repo.save(&existing).await.unwrap();

        let config = JiraConfig {
            project_keys: vec!["AP".to_string()],
            assignees: None,
            my_tasks_only: false,
        };
        let result = sync_jira(
            &StubJiraClient,
            &task_repo,
            &StubProjectRepo,
            &StubSyncRepo,
            user_id,
            &config,
        )
        .await
        .unwrap();
        assert_eq!(result.tasks_updated, 1);

        let after = task_repo
            .find_by_source(user_id, Source::Jira, "AP-1")
            .await
            .unwrap()
            .unwrap();
        // Sync did run and updated Jira-owned fields…
        assert_eq!(after.title, "Synced title");
        assert_eq!(after.assignee.as_deref(), Some("jira.user@example.com"));
        // …but user-owned fields survived.
        assert_eq!(
            after.delegated_to.as_deref(),
            Some("Marie"),
            "delegated_to must survive a Jira resync"
        );
        assert_eq!(after.notes.as_deref(), Some("local notes"));
    }

    #[test]
    fn jira_status_mapping() {
        assert_eq!(map_jira_status("Done"), TaskStatus::Done);
        assert_eq!(map_jira_status("Closed"), TaskStatus::Done);
        assert_eq!(map_jira_status("Resolved"), TaskStatus::Done);
        assert_eq!(map_jira_status("Clos"), TaskStatus::Done);
        assert_eq!(map_jira_status("En Production"), TaskStatus::Done);
        assert_eq!(map_jira_status("Abandonné"), TaskStatus::Done);
        assert_eq!(map_jira_status("In Progress"), TaskStatus::InProgress);
        assert_eq!(map_jira_status("In Review"), TaskStatus::InProgress);
        assert_eq!(map_jira_status("Blocked"), TaskStatus::Blocked);
        assert_eq!(map_jira_status("To Do"), TaskStatus::Todo);
        assert_eq!(map_jira_status("Backlog"), TaskStatus::Todo);
        assert_eq!(map_jira_status("unknown status"), TaskStatus::Todo);
    }

    #[test]
    fn excel_status_mapping() {
        assert_eq!(map_excel_status("Done"), TaskStatus::Done);
        assert_eq!(map_excel_status("Terminé"), TaskStatus::Done);
        assert_eq!(map_excel_status("En cours"), TaskStatus::InProgress);
        assert_eq!(map_excel_status("Bloqué"), TaskStatus::Blocked);
        assert_eq!(map_excel_status(""), TaskStatus::Todo);
        assert_eq!(map_excel_status("anything"), TaskStatus::Todo);
    }

    // -----------------------------------------------------------------------
    // Outlook exclusion test stubs
    // -----------------------------------------------------------------------

    /// Minimal in-memory MeetingRepository; records which outlook_ids were upserted.
    #[derive(Default)]
    struct MiniMeetingRepo {
        upserted: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl MeetingRepository for MiniMeetingRepo {
        async fn find_by_id(&self, _id: MeetingId) -> Result<Option<Meeting>, RepositoryError> {
            Ok(None)
        }
        async fn update(&self, _meeting: &Meeting) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn find_by_user_and_date(
            &self,
            _user_id: UserId,
            _date: NaiveDate,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(vec![])
        }
        async fn find_by_user_and_range(
            &self,
            _user_id: UserId,
            _start: NaiveDate,
            _end: NaiveDate,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(vec![])
        }
        async fn upsert_batch(&self, meetings: &[Meeting]) -> Result<(), RepositoryError> {
            let mut ids = self.upserted.lock().unwrap();
            for m in meetings {
                ids.push(m.outlook_id.clone());
            }
            Ok(())
        }
        async fn delete_stale(
            &self,
            _user_id: UserId,
            _current_outlook_ids: &[String],
        ) -> Result<u64, RepositoryError> {
            Ok(0)
        }
        async fn find_by_project(
            &self,
            _user_id: UserId,
            _project_id: ProjectId,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(vec![])
        }
    }

    /// Returns two calendar events: one matching "pause midi", one not.
    struct StubOutlookClientTwoEvents;

    #[async_trait]
    impl OutlookClient for StubOutlookClientTwoEvents {
        async fn fetch_calendar(
            &self,
            _start: NaiveDate,
            _end: NaiveDate,
        ) -> Result<Vec<OutlookEvent>, ConnectorError> {
            use chrono::TimeZone;
            let base = chrono::Utc.with_ymd_and_hms(2026, 6, 9, 9, 0, 0).unwrap();
            Ok(vec![
                OutlookEvent {
                    outlook_id: "evt-excluded".to_string(),
                    title: "Pause Midi — équipe".to_string(),
                    start_time: base,
                    end_time: base + chrono::Duration::hours(1),
                    location: None,
                    participants: vec![],
                    is_cancelled: false,
                    show_as: None,
                },
                OutlookEvent {
                    outlook_id: "evt-kept".to_string(),
                    title: "Sprint review".to_string(),
                    start_time: base,
                    end_time: base + chrono::Duration::hours(1),
                    location: None,
                    participants: vec![],
                    is_cancelled: false,
                    show_as: None,
                },
            ])
        }
    }

    #[tokio::test]
    async fn sync_outlook_excludes_matching_events() {
        let user_id: UserId =
            Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        let meeting_repo = MiniMeetingRepo::default();
        let today = chrono::Utc::now().date_naive();
        let end = today + chrono::Duration::days(14);
        let patterns = vec!["pause midi".to_string()];

        let result = sync_outlook(
            &StubOutlookClientTwoEvents,
            &meeting_repo,
            &StubSyncRepo,
            user_id,
            (today, end),
            &patterns,
        )
        .await
        .unwrap();

        let upserted = meeting_repo.upserted.lock().unwrap().clone();
        // Only the non-matching event should have been upserted.
        assert!(
            !upserted.contains(&"evt-excluded".to_string()),
            "excluded event must not be upserted"
        );
        assert!(
            upserted.contains(&"evt-kept".to_string()),
            "non-excluded event must be upserted"
        );
        // meetings_synced reflects the filtered count.
        assert_eq!(result.meetings_synced, 1);
    }
}
