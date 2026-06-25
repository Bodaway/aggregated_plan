use std::sync::Arc;

use async_graphql::{Context, Object, Result, ID};
use chrono::NaiveDate;
use domain::types::UserId;
use uuid::Uuid;

use application::repositories::*;
use application::use_cases::{activity_reporting, activity_tracking, alerts, configuration, dashboard, deduplication, priority, task_management, worklog as worklog_uc};
use application::use_cases::recurrence as recurrence_uc;

use super::types::*;

/// Microsoft session status returned by the `session` query.
#[derive(async_graphql::SimpleObject)]
pub struct SessionGql {
    pub authenticated: bool,
    pub account: Option<String>,
}

/// Root query type for the GraphQL schema.
#[derive(Default)]
pub struct QueryRoot;

/// Trigger lazy materialization of recurring task instances for the current user.
///
/// Runs silently: errors are logged at WARN level but not propagated to the caller, so
/// a transient recurrence error never breaks an unrelated query.
async fn trigger_lazy_materialization(ctx: &async_graphql::Context<'_>) {
    let user_id = match ctx.data::<UserId>() {
        Ok(id) => *id,
        Err(_) => return,
    };
    let rec_repo = match ctx.data::<Arc<dyn RecurrenceRepository>>() {
        Ok(r) => r.clone(),
        Err(_) => return,
    };
    let task_repo = match ctx.data::<Arc<dyn TaskRepository>>() {
        Ok(r) => r.clone(),
        Err(_) => return,
    };
    let today = chrono::Utc::now().date_naive();
    if let Err(e) = recurrence_uc::materialize_due_occurrences(
        rec_repo.as_ref(),
        task_repo.as_ref(),
        user_id,
        today,
        14,
    )
    .await
    {
        tracing::warn!("lazy materialization error: {e}");
    }
}

#[Object]
impl QueryRoot {
    /// Health check query. Returns true if the server is running.
    async fn health(&self) -> bool {
        true
    }

    /// Fetch a single task by its ID.
    async fn task(&self, ctx: &Context<'_>, id: ID) -> Result<Option<TaskGql>> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let task = task_management::get_task(task_repo.as_ref(), task_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(task.map(TaskGql))
    }

    /// Fetch tasks with optional filtering and cursor-based pagination.
    async fn tasks(
        &self,
        ctx: &Context<'_>,
        filter: Option<TaskFilterInput>,
        #[graphql(default = 50)] first: i32,
        after: Option<String>,
    ) -> Result<TaskConnection> {
        trigger_lazy_materialization(ctx).await;

        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;

        let domain_filter = convert_task_filter(filter);

        let all_tasks = task_management::get_tasks(task_repo.as_ref(), *user_id, &domain_filter)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let total_count = all_tasks.len() as i32;

        // Determine start index from cursor
        let start_index = match after {
            Some(ref cursor) => cursor
                .parse::<usize>()
                .map(|i| i + 1)
                .unwrap_or(0),
            None => 0,
        };

        let limit = first.max(0) as usize;
        let page: Vec<_> = all_tasks
            .into_iter()
            .skip(start_index)
            .take(limit)
            .collect();

        let edges: Vec<TaskEdge> = page
            .into_iter()
            .enumerate()
            .map(|(i, task)| {
                let cursor = (start_index + i).to_string();
                TaskEdge {
                    node: TaskGql(task),
                    cursor,
                }
            })
            .collect();

        let has_next_page = if let Some(last_edge) = edges.last() {
            last_edge
                .cursor
                .parse::<usize>()
                .map(|i| (i + 1) < total_count as usize)
                .unwrap_or(false)
        } else {
            false
        };

        let page_info = PageInfo {
            has_next_page,
            has_previous_page: start_index > 0,
            start_cursor: edges.first().map(|e| e.cursor.clone()),
            end_cursor: edges.last().map(|e| e.cursor.clone()),
        };

        Ok(TaskConnection {
            edges,
            page_info,
            total_count,
        })
    }

    /// Fetch all projects for the current user.
    async fn projects(&self, ctx: &Context<'_>) -> Result<Vec<ProjectGql>> {
        let user_id = ctx.data::<UserId>()?;
        let project_repo = ctx.data::<Arc<dyn ProjectRepository>>()?;

        let projects = project_repo
            .find_by_user(*user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(projects.into_iter().map(ProjectGql).collect())
    }

    /// Distinct delegate names previously used on the current user's tasks.
    /// Backs the auto-learned suggestion list for the delegation field.
    async fn delegates(&self, ctx: &Context<'_>) -> Result<Vec<String>> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let user_id = ctx.data::<UserId>()?;
        task_repo
            .list_delegates(*user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))
    }

    /// Fetch all tags for the current user.
    async fn tags(&self, ctx: &Context<'_>) -> Result<Vec<TagGql>> {
        let user_id = ctx.data::<UserId>()?;
        let tag_repo = ctx.data::<Arc<dyn TagRepository>>()?;

        let tags = tag_repo
            .find_by_user(*user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(tags.into_iter().map(TagGql).collect())
    }

    /// Fetch the daily dashboard for a given date, including tasks, meetings, alerts,
    /// sync statuses, and the weekly workload for the containing week.
    async fn daily_dashboard(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
    ) -> Result<DailyDashboardGql> {
        trigger_lazy_materialization(ctx).await;
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;
        let alert_repo = ctx.data::<Arc<dyn AlertRepository>>()?;
        let sync_repo = ctx.data::<Arc<dyn SyncStatusRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;

        let data = dashboard::get_daily_dashboard(
            task_repo.as_ref(),
            meeting_repo.as_ref(),
            alert_repo.as_ref(),
            sync_repo.as_ref(),
            config_repo.as_ref(),
            *user_id,
            date,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(DailyDashboardGql::from(data))
    }

    /// Fetch the weekly workload for a given week (identified by the Monday date).
    async fn weekly_workload(
        &self,
        ctx: &Context<'_>,
        week_start: NaiveDate,
    ) -> Result<WeeklyWorkloadGql> {
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;

        let data = dashboard::get_weekly_workload(
            task_repo.as_ref(),
            meeting_repo.as_ref(),
            config_repo.as_ref(),
            *user_id,
            week_start,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(WeeklyWorkloadGql::from(data))
    }

    /// Get the Eisenhower priority matrix for the current user.
    async fn priority_matrix(&self, ctx: &Context<'_>) -> Result<PriorityMatrixGql> {
        trigger_lazy_materialization(ctx).await;

        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;

        let today = chrono::Utc::now().date_naive();
        let data = priority::get_priority_matrix(task_repo.as_ref(), *user_id, today)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(PriorityMatrixGql {
            urgent_important: data.urgent_important.into_iter().map(TaskGql).collect(),
            important: data.important.into_iter().map(TaskGql).collect(),
            urgent: data.urgent.into_iter().map(TaskGql).collect(),
            neither: data.neither.into_iter().map(TaskGql).collect(),
        })
    }

    /// Fetch all sync statuses for the current user.
    async fn sync_statuses(&self, ctx: &Context<'_>) -> Result<Vec<SyncStatusGql>> {
        let user_id = ctx.data::<UserId>()?;
        let sync_repo = ctx.data::<Arc<dyn SyncStatusRepository>>()?;

        let statuses = sync_repo
            .find_by_user(*user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(statuses.into_iter().map(SyncStatusGql).collect())
    }

    /// Get deduplication suggestions for the current user.
    async fn deduplication_suggestions(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Vec<DeduplicationSuggestionGql>> {
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_link_repo = ctx.data::<Arc<dyn TaskLinkRepository>>()?;

        let suggestions = deduplication::find_suggestions(
            task_repo.as_ref(),
            task_link_repo.as_ref(),
            *user_id,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(suggestions
            .into_iter()
            .map(DeduplicationSuggestionGql::from)
            .collect())
    }

    /// Get alerts for the current user with optional filtering and cursor-based pagination.
    async fn alerts(
        &self,
        ctx: &Context<'_>,
        resolved: Option<bool>,
        #[graphql(default = 50)] first: i32,
        after: Option<String>,
    ) -> Result<AlertConnection> {
        let user_id = ctx.data::<UserId>()?;
        let alert_repo = ctx.data::<Arc<dyn AlertRepository>>()?;

        let all_alerts = alerts::get_alerts(alert_repo.as_ref(), *user_id, resolved)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let total_count = all_alerts.len() as i32;

        let start_index = match after {
            Some(ref cursor) => cursor
                .parse::<usize>()
                .map(|i| i + 1)
                .unwrap_or(0),
            None => 0,
        };

        let limit = first.max(0) as usize;
        let page: Vec<_> = all_alerts
            .into_iter()
            .skip(start_index)
            .take(limit)
            .collect();

        let edges: Vec<AlertEdge> = page
            .into_iter()
            .enumerate()
            .map(|(i, alert)| {
                let cursor = (start_index + i).to_string();
                AlertEdge {
                    node: AlertGql(alert),
                    cursor,
                }
            })
            .collect();

        let has_next_page = if let Some(last_edge) = edges.last() {
            last_edge
                .cursor
                .parse::<usize>()
                .map(|i| (i + 1) < total_count as usize)
                .unwrap_or(false)
        } else {
            false
        };

        let page_info = PageInfo {
            has_next_page,
            has_previous_page: start_index > 0,
            start_cursor: edges.first().map(|e| e.cursor.clone()),
            end_cursor: edges.last().map(|e| e.cursor.clone()),
        };

        Ok(AlertConnection {
            edges,
            page_info,
            total_count,
        })
    }

    /// Get the activity journal for a given date.
    async fn activity_journal(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
    ) -> Result<Vec<ActivitySlotGql>> {
        let user_id = ctx.data::<UserId>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;

        let slots =
            activity_tracking::get_activity_journal(activity_repo.as_ref(), *user_id, date)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(slots.into_iter().map(ActivitySlotGql).collect())
    }

    /// Get the currently active activity slot.
    async fn current_activity(&self, ctx: &Context<'_>) -> Result<Option<ActivitySlotGql>> {
        let user_id = ctx.data::<UserId>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;

        let slot = activity_tracking::get_current_activity(activity_repo.as_ref(), *user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(slot.map(ActivitySlotGql))
    }

    /// Get a weekly activity summary with daily totals and per-task breakdown.
    async fn weekly_activity_summary(
        &self,
        ctx: &Context<'_>,
        week_start: NaiveDate,
    ) -> Result<WeeklyActivitySummaryGql> {
        let user_id = ctx.data::<UserId>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;

        let summary = activity_reporting::get_weekly_activity_summary(
            activity_repo.as_ref(),
            task_repo.as_ref(),
            *user_id,
            week_start,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(WeeklyActivitySummaryGql(summary))
    }

    /// All non-dismissed tasks for the current user, projected to a lean
    /// payload for client-side fuzzy search. Unpaginated on purpose.
    async fn searchable_tasks(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Vec<crate::graphql::types::SearchableTaskGql>> {
        use std::collections::HashMap;
        use application::repositories::TaskFilter;
        use domain::types::{TrackingState, TagId, ProjectId};

        let user_id = *ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn application::repositories::TaskRepository>>()?;
        let tag_repo = ctx.data::<Arc<dyn application::repositories::TagRepository>>()?;
        let project_repo = ctx.data::<Arc<dyn application::repositories::ProjectRepository>>()?;

        let filter = TaskFilter {
            tracking_state: Some(vec![TrackingState::Inbox, TrackingState::Followed]),
            ..TaskFilter::empty()
        };

        let tasks = task_repo
            .find_by_user(user_id, &filter)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let tags = tag_repo
            .find_by_user(user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let tag_names: HashMap<TagId, String> =
            tags.into_iter().map(|t| (t.id, t.name)).collect();

        let projects = project_repo
            .find_by_user(user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let project_names: HashMap<ProjectId, String> =
            projects.into_iter().map(|p| (p.id, p.name)).collect();

        Ok(tasks
            .into_iter()
            .map(|task| {
                let project_name = task
                    .project_id
                    .and_then(|pid| project_names.get(&pid).cloned());
                let tag_names_vec: Vec<String> = task
                    .tags
                    .iter()
                    .filter_map(|tid| tag_names.get(tid).cloned())
                    .collect();
                crate::graphql::types::SearchableTaskGql {
                    task,
                    project_name,
                    tag_names: tag_names_vec,
                }
            })
            .collect())
    }

    /// List all active recurrence templates for the current user.
    async fn recurrence_templates(&self, ctx: &Context<'_>) -> Result<Vec<RecurrenceTemplateGql>> {
        let user_id = ctx.data::<UserId>()?;
        let rec_repo = ctx.data::<Arc<dyn RecurrenceRepository>>()?;

        let templates = rec_repo
            .find_active_by_user(*user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(templates.into_iter().map(RecurrenceTemplateGql).collect())
    }

    /// Get user configuration as a JSON-like list of key-value pairs.
    async fn configuration(&self, ctx: &Context<'_>) -> Result<serde_json::Value> {
        let user_id = ctx.data::<UserId>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;

        let pairs = configuration::get_all_config(config_repo.as_ref(), *user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let map: serde_json::Map<String, serde_json::Value> = pairs
            .into_iter()
            .map(|(k, v)| {
                let displayed = if is_secret_key(&k) && !v.is_empty() {
                    "********".to_string()
                } else {
                    v
                };
                (k, serde_json::Value::String(displayed))
            })
            .collect();

        Ok(serde_json::Value::Object(map))
    }

    /// Microsoft session status for the current user.
    ///
    /// Reads `microsoft.refresh_token` directly from the config store (never
    /// redacted here) so `authenticated` reflects the real stored value.
    async fn session(&self, ctx: &Context<'_>) -> Result<SessionGql> {
        let user_id = ctx.data::<UserId>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let refresh = config_repo
            .get(*user_id, "microsoft.refresh_token")
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let account = config_repo
            .get(*user_id, "microsoft.account")
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(SessionGql {
            authenticated: refresh.map(|s| !s.is_empty()).unwrap_or(false),
            account,
        })
    }

    /// List worklog entries for the authenticated user.
    ///
    /// If `filter.recurrenceId` is provided, returns all entries whose task belongs to
    /// that recurrence template — `taskIds` is ignored in this case.
    async fn worklog_entries(
        &self,
        ctx: &Context<'_>,
        filter: Option<WorklogEntryFilterInput>,
    ) -> Result<Vec<WorklogEntryGql>> {
        use domain::types::recurrence::RecurrenceTemplateId;

        let repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let user_id = *ctx.data::<UserId>()?;
        let f = filter.unwrap_or_default();
        let limit = f.limit.unwrap_or(0).max(0) as u32;
        let offset = f.offset.unwrap_or(0).max(0) as u32;

        if let Some(ref rec_id) = f.recurrence_id {
            let template_id = Uuid::parse_str(rec_id)
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            let entries = repo
                .find_by_recurrence(user_id, RecurrenceTemplateId(template_id), limit, offset)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            return Ok(entries.into_iter().map(WorklogEntryGql).collect());
        }

        let task_ids = match f.task_ids {
            Some(ids) => {
                let mut parsed = Vec::with_capacity(ids.len());
                for i in &ids {
                    parsed.push(
                        Uuid::parse_str(i)
                            .map_err(|e| async_graphql::Error::new(e.to_string()))?,
                    );
                }
                Some(parsed)
            }
            None => None,
        };
        let wf = WorklogFilter {
            task_ids,
            from: f.from,
            to: f.to,
            limit,
            offset,
        };
        let entries = worklog_uc::list_worklog_entries(repo.as_ref(), user_id, wf)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(entries.into_iter().map(WorklogEntryGql).collect())
    }

    /// List active Gryzzly catalog entries for the current user, optionally filtered by a
    /// name/project search string and a project-name exact filter, capped at `limit`.
    async fn gryzzly_tasks(
        &self,
        ctx: &Context<'_>,
        search: Option<String>,
        project_filter: Option<String>,
        #[graphql(default = 100)] limit: i32,
    ) -> Result<Vec<GryzzlyTaskGql>> {
        let repo = ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?;
        let user_id = *ctx.data::<UserId>()?;
        let rows = repo
            .list_active(user_id, search.as_deref(), project_filter.as_deref(), limit as i64)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(rows.into_iter().map(GryzzlyTaskGql::from).collect())
    }
}

/// Convert GraphQL TaskFilterInput to the application layer TaskFilter.
fn convert_task_filter(input: Option<TaskFilterInput>) -> TaskFilter {
    match input {
        None => TaskFilter::empty(),
        Some(f) => TaskFilter {
            status: f
                .status
                .map(|v| v.into_iter().map(|s| s.into()).collect()),
            source: f
                .source
                .map(|v| v.into_iter().map(|s| s.into()).collect()),
            project_id: f.project_id.and_then(|id| Uuid::parse_str(&id).ok()),
            assignee: f.assignee,
            deadline_before: f.deadline_before,
            deadline_after: f.deadline_after,
            tag_ids: f.tag_ids.map(|v| {
                v.into_iter()
                    .filter_map(|id| Uuid::parse_str(&id).ok())
                    .collect()
            }),
            tracking_state: f.tracking_state.map(|states| {
                states.into_iter().map(|s| s.into()).collect()
            }),
            source_id: f.source_id,
            title_contains: f.title_contains,
        },
    }
}

/// Returns true if a configuration key holds a secret that must never be
/// returned in plaintext over the API.
fn is_secret_key(key: &str) -> bool {
    key.ends_with(".token")
        || key.ends_with(".access_token")
        || key.ends_with(".refresh_token")
        || key.ends_with(".secret")
        || key.ends_with(".client_secret")
        || key.ends_with(".api_key")
        || key.ends_with(".password")
}
