use chrono::{NaiveDate, Utc};
use domain::rules::urgency::calculate_urgency;
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::*;
use crate::services::*;

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

    let jira_tasks = jira_client
        .fetch_tasks(
            &config.project_keys,
            config.assignees.as_deref(),
        )
        .await
        .map_err(|e| {
            AppError::Connector {
                connector_source: Source::Jira,
                message: e.to_string(),
            }
        })?;

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
                task.jira_status = Some(jira_task.status.clone());
                task.status = map_jira_status(&jira_task.status);
                task.assignee = jira_task.assignee.clone();
                task.deadline = jira_task.deadline;
                task.project_id = project_id;
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
                    source: Source::Jira,
                    source_id: Some(jira_task.key.clone()),
                    jira_status: Some(jira_task.status.clone()),
                    status: map_jira_status(&jira_task.status),
                    project_id,
                    assignee: jira_task.assignee.clone(),
                    deadline: jira_task.deadline,
                    planned_start: None,
                    planned_end: None,
                    estimated_hours: None,
                    urgency: calculate_urgency(jira_task.deadline, today),
                    urgency_manual: false,
                    impact: ImpactLevel::Medium,
                    tags: vec![],
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
        tasks_removed: 0,
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
) -> Result<SyncResult, AppError> {
    let now = Utc::now();

    // Mark sync as in progress.
    sync_repo
        .upsert(&SyncStatus {
            source: Source::Obsidian, // We reuse Obsidian enum value for Outlook
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
            connector_source: Source::Obsidian,
            message: e.to_string(),
        })?;

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
            source: Source::Obsidian,
            user_id,
            last_sync_at: Some(Utc::now()),
            status: SyncSourceStatus::Success,
            error_message: None,
        })
        .await?;

    Ok(SyncResult {
        source: Source::Obsidian,
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
                    source: Source::Excel,
                    source_id: Some(source_id),
                    jira_status: None,
                    status,
                    project_id,
                    assignee,
                    deadline,
                    planned_start: None,
                    planned_end: None,
                    estimated_hours: None,
                    urgency: calculate_urgency(deadline, today),
                    urgency_manual: false,
                    impact: ImpactLevel::Medium,
                    tags: vec![],
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
pub async fn sync_all(
    jira_client: Option<&dyn JiraClient>,
    outlook_client: Option<&dyn OutlookClient>,
    excel_client: Option<&dyn ExcelClient>,
    task_repo: &dyn TaskRepository,
    meeting_repo: &dyn MeetingRepository,
    project_repo: &dyn ProjectRepository,
    sync_repo: &dyn SyncStatusRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
) -> Result<Vec<SyncResult>, AppError> {
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

            let config = JiraConfig {
                project_keys,
                assignees,
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
        // Sync the next 30 days by default.
        let end = today + chrono::Duration::days(30);
        match sync_outlook(client, meeting_repo, sync_repo, user_id, (today, end)).await {
            Ok(result) => results.push(result),
            Err(e) => {
                update_sync_error(sync_repo, user_id, Source::Obsidian, &e.to_string()).await?;
                results.push(SyncResult {
                    source: Source::Obsidian,
                    tasks_created: 0,
                    tasks_updated: 0,
                    tasks_removed: 0,
                    meetings_synced: 0,
                    errors: vec![e.to_string()],
                });
            }
        }
    } else {
        update_sync_error(sync_repo, user_id, Source::Obsidian, "Not configured").await?;
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
pub async fn sync_source(
    source: Source,
    task_repo: &dyn TaskRepository,
    meeting_repo: &dyn MeetingRepository,
    project_repo: &dyn ProjectRepository,
    sync_repo: &dyn SyncStatusRepository,
    jira_client: Option<&dyn JiraClient>,
    outlook_client: Option<&dyn OutlookClient>,
    excel_client: Option<&dyn ExcelClient>,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
) -> Result<SyncStatus, AppError> {
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
                    let config = JiraConfig {
                        project_keys,
                        assignees,
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
        Source::Obsidian => {
            if let Some(client) = outlook_client {
                let today = Utc::now().date_naive();
                let end = today + chrono::Duration::days(30);
                sync_outlook(client, meeting_repo, sync_repo, user_id, (today, end)).await?;
            } else {
                update_sync_error(sync_repo, user_id, Source::Obsidian, "Not configured").await?;
            }
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
fn map_jira_status(jira_status: &str) -> TaskStatus {
    match jira_status.to_lowercase().as_str() {
        "done" | "closed" | "resolved" | "complete" | "completed" => TaskStatus::Done,
        "in progress" | "in review" | "review" | "active" => TaskStatus::InProgress,
        "blocked" | "impediment" => TaskStatus::Blocked,
        _ => TaskStatus::Todo,
    }
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

    #[test]
    fn jira_status_mapping() {
        assert_eq!(map_jira_status("Done"), TaskStatus::Done);
        assert_eq!(map_jira_status("Closed"), TaskStatus::Done);
        assert_eq!(map_jira_status("Resolved"), TaskStatus::Done);
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
}
