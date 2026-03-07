use std::sync::Arc;

use async_graphql::{Context, Object, Result, ID};
use chrono::NaiveDate;
use domain::types::UserId;
use uuid::Uuid;

use application::repositories::*;
use application::use_cases::{dashboard, priority, task_management};

use super::types::*;

/// Root query type for the GraphQL schema.
#[derive(Default)]
pub struct QueryRoot;

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
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;
        let alert_repo = ctx.data::<Arc<dyn AlertRepository>>()?;
        let sync_repo = ctx.data::<Arc<dyn SyncStatusRepository>>()?;

        let data = dashboard::get_daily_dashboard(
            task_repo.as_ref(),
            meeting_repo.as_ref(),
            alert_repo.as_ref(),
            sync_repo.as_ref(),
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

        let data = dashboard::get_weekly_workload(
            task_repo.as_ref(),
            meeting_repo.as_ref(),
            *user_id,
            week_start,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(WeeklyWorkloadGql::from(data))
    }

    /// Get the Eisenhower priority matrix for the current user.
    async fn priority_matrix(&self, ctx: &Context<'_>) -> Result<PriorityMatrixGql> {
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
        },
    }
}
