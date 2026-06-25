use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use rmcp::model::Content;
use rmcp::{ServerHandler, tool};
use serde::Deserialize;
use uuid::Uuid;

use application::repositories::*;
use application::use_cases::{
    activity_tracking, alerts, configuration, dashboard, priority, task_management,
};
use domain::types::*;

pub struct AggregatedPlanServer {
    task_repo: Arc<dyn TaskRepository>,
    meeting_repo: Arc<dyn MeetingRepository>,
    project_repo: Arc<dyn ProjectRepository>,
    activity_repo: Arc<dyn ActivitySlotRepository>,
    alert_repo: Arc<dyn AlertRepository>,
    tag_repo: Arc<dyn TagRepository>,
    sync_repo: Arc<dyn SyncStatusRepository>,
    config_repo: Arc<dyn ConfigRepository>,
    user_id: UserId,
}

impl AggregatedPlanServer {
    pub fn new(
        task_repo: Arc<dyn TaskRepository>,
        meeting_repo: Arc<dyn MeetingRepository>,
        project_repo: Arc<dyn ProjectRepository>,
        activity_repo: Arc<dyn ActivitySlotRepository>,
        alert_repo: Arc<dyn AlertRepository>,
        tag_repo: Arc<dyn TagRepository>,
        sync_repo: Arc<dyn SyncStatusRepository>,
        config_repo: Arc<dyn ConfigRepository>,
        user_id: UserId,
    ) -> Self {
        Self {
            task_repo,
            meeting_repo,
            project_repo,
            activity_repo,
            alert_repo,
            tag_repo,
            sync_repo,
            config_repo,
            user_id,
        }
    }

    fn today(&self) -> NaiveDate {
        Utc::now().date_naive()
    }

    fn json_content<T: serde::Serialize>(value: &T) -> Vec<Content> {
        let json = serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
        vec![Content::text(json)]
    }

    fn error_content(err: impl std::fmt::Display) -> Vec<Content> {
        vec![Content::text(format!("Error: {}", err))]
    }
}

// Helper deserialize types for tool parameters
#[derive(Deserialize)]
struct ListTasksParams {
    status: Option<String>,
    source: Option<String>,
    project_id: Option<String>,
    tracking_state: Option<String>,
}

#[derive(Deserialize)]
struct GetTaskParams {
    task_id: String,
}

#[derive(Deserialize)]
struct CreateTaskParams {
    title: String,
    description: Option<String>,
    project_id: Option<String>,
    deadline: Option<String>,
    planned_start: Option<String>,
    planned_end: Option<String>,
    estimated_hours: Option<f32>,
    impact: Option<String>,
    urgency: Option<String>,
}

#[derive(Deserialize)]
struct UpdateTaskParams {
    task_id: String,
    title: Option<String>,
    description: Option<String>,
    deadline: Option<String>,
    planned_start: Option<String>,
    planned_end: Option<String>,
    estimated_hours: Option<f32>,
    status: Option<String>,
    impact: Option<String>,
    urgency: Option<String>,
    remaining_hours_override: Option<f32>,
    estimated_hours_override: Option<f32>,
}

#[derive(Deserialize)]
struct CompleteTaskParams {
    task_id: String,
}

#[derive(Deserialize)]
struct DeleteTaskParams {
    task_id: String,
}

#[derive(Deserialize)]
struct SetTrackingStateParams {
    task_id: String,
    state: String,
}

#[derive(Deserialize)]
struct SetTrackingStateBatchParams {
    task_ids: Vec<String>,
    state: String,
}

#[derive(Deserialize)]
struct GetDashboardParams {
    date: Option<String>,
}

#[derive(Deserialize)]
struct GetWeeklyWorkloadParams {
    week_start: Option<String>,
}

#[derive(Deserialize)]
struct GetPriorityMatrixParams {}

#[derive(Deserialize)]
struct UpdatePriorityParams {
    task_id: String,
    urgency: Option<String>,
    impact: Option<String>,
}

#[derive(Deserialize)]
struct ResetUrgencyParams {
    task_id: String,
}

#[derive(Deserialize)]
struct StartActivityParams {
    task_id: Option<String>,
}

#[derive(Deserialize)]
struct GetActivityJournalParams {
    date: Option<String>,
}

#[derive(Deserialize)]
struct GetAlertsParams {
    resolved: Option<bool>,
}

#[derive(Deserialize)]
struct ResolveAlertParams {
    alert_id: String,
}

#[derive(Deserialize)]
struct SetConfigurationParams {
    key: String,
    value: String,
}

fn parse_uuid(s: &str) -> Result<Uuid, String> {
    Uuid::parse_str(s).map_err(|e| format!("Invalid UUID '{}': {}", s, e))
}

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date '{}' (expected YYYY-MM-DD): {}", s, e))
}

fn parse_datetime(s: &str) -> Result<chrono::DateTime<Utc>, String> {
    s.parse::<chrono::DateTime<Utc>>()
        .map_err(|e| format!("Invalid datetime '{}' (expected RFC 3339): {}", s, e))
}

fn parse_status(s: &str) -> Result<TaskStatus, String> {
    match s.to_lowercase().as_str() {
        "todo" => Ok(TaskStatus::Todo),
        "in_progress" | "inprogress" => Ok(TaskStatus::InProgress),
        "done" => Ok(TaskStatus::Done),
        "blocked" => Ok(TaskStatus::Blocked),
        _ => Err(format!("Invalid status '{}'. Use: todo, in_progress, done, blocked", s)),
    }
}

fn parse_source(s: &str) -> Result<Source, String> {
    match s.to_lowercase().as_str() {
        "jira" => Ok(Source::Jira),
        "excel" => Ok(Source::Excel),
        "obsidian" => Ok(Source::Obsidian),
        "personal" => Ok(Source::Personal),
        "outlook" => Ok(Source::Outlook),
        "gryzzly" => Ok(Source::Gryzzly),
        _ => Err(format!("Invalid source '{}'. Use: jira, excel, obsidian, personal, outlook, gryzzly", s)),
    }
}

fn parse_urgency(s: &str) -> Result<UrgencyLevel, String> {
    match s.to_lowercase().as_str() {
        "low" => Ok(UrgencyLevel::Low),
        "medium" => Ok(UrgencyLevel::Medium),
        "high" => Ok(UrgencyLevel::High),
        "critical" => Ok(UrgencyLevel::Critical),
        _ => Err(format!("Invalid urgency '{}'. Use: low, medium, high, critical", s)),
    }
}

fn parse_impact(s: &str) -> Result<ImpactLevel, String> {
    match s.to_lowercase().as_str() {
        "low" => Ok(ImpactLevel::Low),
        "medium" => Ok(ImpactLevel::Medium),
        "high" => Ok(ImpactLevel::High),
        "critical" => Ok(ImpactLevel::Critical),
        _ => Err(format!("Invalid impact '{}'. Use: low, medium, high, critical", s)),
    }
}

fn parse_tracking_state(s: &str) -> Result<TrackingState, String> {
    s.parse::<TrackingState>()
}

#[tool(tool_box)]
impl AggregatedPlanServer {
    #[tool(description = "List tasks with optional filters. Returns task details including title, status, urgency, impact, deadline, tracking state, and time estimates.")]
    async fn list_tasks(
        &self,
        #[tool(param)]
        #[tool(description = "Filter by status: todo, in_progress, done, blocked")]
        status: Option<String>,
        #[tool(param)]
        #[tool(description = "Filter by source: jira, excel, obsidian, personal, outlook")]
        source: Option<String>,
        #[tool(param)]
        #[tool(description = "Filter by project UUID")]
        project_id: Option<String>,
        #[tool(param)]
        #[tool(description = "Filter by tracking state: inbox, followed, dismissed")]
        tracking_state: Option<String>,
    ) -> Vec<Content> {
        let filter = TaskFilter {
            status: match status {
                Some(s) => match parse_status(&s) {
                    Ok(st) => Some(vec![st]),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            source: match source {
                Some(s) => match parse_source(&s) {
                    Ok(src) => Some(vec![src]),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            project_id: match project_id {
                Some(s) => match parse_uuid(&s) {
                    Ok(id) => Some(id),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            tracking_state: match tracking_state {
                Some(s) => match parse_tracking_state(&s) {
                    Ok(ts) => Some(vec![ts]),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            ..TaskFilter::empty()
        };

        match task_management::get_tasks(self.task_repo.as_ref(), self.user_id, &filter).await {
            Ok(tasks) => Self::json_content(&tasks),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Get a single task by its UUID. Returns full task details.")]
    async fn get_task(
        &self,
        #[tool(param)]
        #[tool(description = "Task UUID")]
        task_id: String,
    ) -> Vec<Content> {
        let id = match parse_uuid(&task_id) {
            Ok(id) => id,
            Err(e) => return Self::error_content(e),
        };
        match task_management::get_task(self.task_repo.as_ref(), id).await {
            Ok(Some(task)) => Self::json_content(&task),
            Ok(None) => Self::error_content(format!("Task {} not found", task_id)),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Create a new personal task. Returns the created task. Urgency is auto-calculated from deadline if not provided.")]
    async fn create_task(
        &self,
        #[tool(param)]
        #[tool(description = "Task title (required)")]
        title: String,
        #[tool(param)]
        #[tool(description = "Task description")]
        description: Option<String>,
        #[tool(param)]
        #[tool(description = "Project UUID to associate with")]
        project_id: Option<String>,
        #[tool(param)]
        #[tool(description = "Deadline date (YYYY-MM-DD)")]
        deadline: Option<String>,
        #[tool(param)]
        #[tool(description = "Planned start datetime (RFC 3339, e.g. 2026-03-14T09:00:00Z)")]
        planned_start: Option<String>,
        #[tool(param)]
        #[tool(description = "Planned end datetime (RFC 3339)")]
        planned_end: Option<String>,
        #[tool(param)]
        #[tool(description = "Estimated hours to complete")]
        estimated_hours: Option<f32>,
        #[tool(param)]
        #[tool(description = "Impact level: low, medium, high, critical (default: medium)")]
        impact: Option<String>,
        #[tool(param)]
        #[tool(description = "Urgency level: low, medium, high, critical (auto-calculated from deadline if omitted)")]
        urgency: Option<String>,
    ) -> Vec<Content> {
        let input = task_management::CreateTaskInput {
            title,
            description,
            notes: None,
            project_id: match project_id {
                Some(s) => match parse_uuid(&s) {
                    Ok(id) => Some(id),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            deadline: match deadline {
                Some(s) => match parse_date(&s) {
                    Ok(d) => Some(d),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            planned_start: match planned_start {
                Some(s) => match parse_datetime(&s) {
                    Ok(dt) => Some(dt),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            planned_end: match planned_end {
                Some(s) => match parse_datetime(&s) {
                    Ok(dt) => Some(dt),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            estimated_hours,
            impact: match impact {
                Some(s) => match parse_impact(&s) {
                    Ok(i) => Some(i),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            urgency: match urgency {
                Some(s) => match parse_urgency(&s) {
                    Ok(u) => Some(u),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            tags: vec![],
        };

        match task_management::create_personal_task(
            self.task_repo.as_ref(),
            self.user_id,
            input,
            self.today(),
        )
        .await
        {
            Ok(task) => Self::json_content(&task),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Update an existing task. Only provided fields are changed. Returns the updated task.")]
    async fn update_task(
        &self,
        #[tool(param)]
        #[tool(description = "Task UUID")]
        task_id: String,
        #[tool(param)]
        #[tool(description = "New title")]
        title: Option<String>,
        #[tool(param)]
        #[tool(description = "New description")]
        description: Option<String>,
        #[tool(param)]
        #[tool(description = "New deadline (YYYY-MM-DD), or empty string to clear")]
        deadline: Option<String>,
        #[tool(param)]
        #[tool(description = "New planned start (RFC 3339), or empty string to clear")]
        planned_start: Option<String>,
        #[tool(param)]
        #[tool(description = "New planned end (RFC 3339), or empty string to clear")]
        planned_end: Option<String>,
        #[tool(param)]
        #[tool(description = "New estimated hours")]
        estimated_hours: Option<f32>,
        #[tool(param)]
        #[tool(description = "New status: todo, in_progress, done, blocked")]
        status: Option<String>,
        #[tool(param)]
        #[tool(description = "New impact: low, medium, high, critical")]
        impact: Option<String>,
        #[tool(param)]
        #[tool(description = "New urgency: low, medium, high, critical (sets manual override)")]
        urgency: Option<String>,
        #[tool(param)]
        #[tool(description = "Override remaining hours (local override, takes priority over Jira)")]
        remaining_hours_override: Option<f32>,
        #[tool(param)]
        #[tool(description = "Override estimated hours (local override, takes priority over Jira)")]
        estimated_hours_override: Option<f32>,
    ) -> Vec<Content> {
        let id = match parse_uuid(&task_id) {
            Ok(id) => id,
            Err(e) => return Self::error_content(e),
        };

        let input = task_management::UpdateTaskInput {
            title,
            description: description.map(|d| if d.is_empty() { None } else { Some(d) }),
            notes: None,
            project_id: None,
            deadline: match deadline {
                Some(s) if s.is_empty() => Some(None),
                Some(s) => match parse_date(&s) {
                    Ok(d) => Some(Some(d)),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            planned_start: match planned_start {
                Some(s) if s.is_empty() => Some(None),
                Some(s) => match parse_datetime(&s) {
                    Ok(dt) => Some(Some(dt)),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            planned_end: match planned_end {
                Some(s) if s.is_empty() => Some(None),
                Some(s) => match parse_datetime(&s) {
                    Ok(dt) => Some(Some(dt)),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            estimated_hours: estimated_hours.map(Some),
            status: match status {
                Some(s) => match parse_status(&s) {
                    Ok(st) => Some(st),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            impact: match impact {
                Some(s) => match parse_impact(&s) {
                    Ok(i) => Some(i),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            urgency: match urgency {
                Some(s) => match parse_urgency(&s) {
                    Ok(u) => Some(u),
                    Err(e) => return Self::error_content(e),
                },
                None => None,
            },
            tags: None,
            remaining_hours_override: remaining_hours_override.map(Some),
            estimated_hours_override: estimated_hours_override.map(Some),
            delegated_to: None,
        };

        match task_management::update_task(self.task_repo.as_ref(), id, input, self.today()).await {
            Ok(task) => Self::json_content(&task),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Mark a task as completed (status = Done).")]
    async fn complete_task(
        &self,
        #[tool(param)]
        #[tool(description = "Task UUID")]
        task_id: String,
    ) -> Vec<Content> {
        let id = match parse_uuid(&task_id) {
            Ok(id) => id,
            Err(e) => return Self::error_content(e),
        };
        match task_management::complete_task(self.task_repo.as_ref(), id).await {
            Ok(task) => Self::json_content(&task),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Delete a task by UUID.")]
    async fn delete_task(
        &self,
        #[tool(param)]
        #[tool(description = "Task UUID")]
        task_id: String,
    ) -> Vec<Content> {
        let id = match parse_uuid(&task_id) {
            Ok(id) => id,
            Err(e) => return Self::error_content(e),
        };
        match task_management::delete_task(self.task_repo.as_ref(), id).await {
            Ok(()) => vec![Content::text("Task deleted successfully")],
            Err(e) => Self::error_content(e),
        }
    }

    // ─── Triage ───

    #[tool(description = "Set the tracking state of a task (inbox, followed, or dismissed). Used for triaging incoming tasks.")]
    async fn set_tracking_state(
        &self,
        #[tool(param)]
        #[tool(description = "Task UUID")]
        task_id: String,
        #[tool(param)]
        #[tool(description = "Tracking state: inbox, followed, dismissed")]
        state: String,
    ) -> Vec<Content> {
        let id = match parse_uuid(&task_id) {
            Ok(id) => id,
            Err(e) => return Self::error_content(e),
        };
        let ts = match parse_tracking_state(&state) {
            Ok(ts) => ts,
            Err(e) => return Self::error_content(e),
        };
        match task_management::set_tracking_state(self.task_repo.as_ref(), id, ts).await {
            Ok(task) => Self::json_content(&task),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Batch-set tracking state for multiple tasks at once.")]
    async fn set_tracking_state_batch(
        &self,
        #[tool(param)]
        #[tool(description = "List of task UUIDs")]
        task_ids: Vec<String>,
        #[tool(param)]
        #[tool(description = "Tracking state: inbox, followed, dismissed")]
        state: String,
    ) -> Vec<Content> {
        let ids: Vec<Uuid> = match task_ids.iter().map(|s| parse_uuid(s)).collect::<Result<Vec<_>, _>>() {
            Ok(ids) => ids,
            Err(e) => return Self::error_content(e),
        };
        let ts = match parse_tracking_state(&state) {
            Ok(ts) => ts,
            Err(e) => return Self::error_content(e),
        };
        match task_management::set_tracking_state_batch(self.task_repo.as_ref(), ids, ts).await {
            Ok(tasks) => Self::json_content(&tasks),
            Err(e) => Self::error_content(e),
        }
    }

    // ─── Dashboard ───

    #[tool(description = "Get the daily dashboard: tasks for the week, meetings, unresolved alerts, sync statuses, and weekly workload. Defaults to today.")]
    async fn get_dashboard(
        &self,
        #[tool(param)]
        #[tool(description = "Date to view (YYYY-MM-DD, defaults to today)")]
        date: Option<String>,
    ) -> Vec<Content> {
        let d = match date {
            Some(s) => match parse_date(&s) {
                Ok(d) => d,
                Err(e) => return Self::error_content(e),
            },
            None => self.today(),
        };
        match dashboard::get_daily_dashboard(
            self.task_repo.as_ref(),
            self.meeting_repo.as_ref(),
            self.alert_repo.as_ref(),
            self.sync_repo.as_ref(),
            self.config_repo.as_ref(),
            self.user_id,
            d,
        )
        .await
        {
            Ok(data) => Self::json_content(&data),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Get weekly workload breakdown with half-day slots showing meetings, tasks, and capacity. Defaults to current week.")]
    async fn get_weekly_workload(
        &self,
        #[tool(param)]
        #[tool(description = "Week start date, Monday (YYYY-MM-DD). Defaults to current week.")]
        week_start: Option<String>,
    ) -> Vec<Content> {
        let ws = match week_start {
            Some(s) => match parse_date(&s) {
                Ok(d) => d,
                Err(e) => return Self::error_content(e),
            },
            None => dashboard::week_start_for(self.today()),
        };
        match dashboard::get_weekly_workload(
            self.task_repo.as_ref(),
            self.meeting_repo.as_ref(),
            self.config_repo.as_ref(),
            self.user_id,
            ws,
        )
        .await
        {
            Ok(data) => Self::json_content(&data),
            Err(e) => Self::error_content(e),
        }
    }

    // ─── Priority ───

    #[tool(description = "Get the Eisenhower priority matrix. Returns tasks grouped into 4 quadrants: urgent_important, important, urgent, neither. Only includes followed, non-done tasks.")]
    async fn get_priority_matrix(&self) -> Vec<Content> {
        match priority::get_priority_matrix(self.task_repo.as_ref(), self.user_id, self.today())
            .await
        {
            Ok(data) => Self::json_content(&data),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Override urgency and/or impact for a task. Sets manual override flag. At least one of urgency or impact must be provided.")]
    async fn update_priority(
        &self,
        #[tool(param)]
        #[tool(description = "Task UUID")]
        task_id: String,
        #[tool(param)]
        #[tool(description = "New urgency: low, medium, high, critical")]
        urgency: Option<String>,
        #[tool(param)]
        #[tool(description = "New impact: low, medium, high, critical")]
        impact: Option<String>,
    ) -> Vec<Content> {
        let id = match parse_uuid(&task_id) {
            Ok(id) => id,
            Err(e) => return Self::error_content(e),
        };

        let mut task_result = None;

        if let Some(u) = urgency {
            let level = match parse_urgency(&u) {
                Ok(l) => l,
                Err(e) => return Self::error_content(e),
            };
            match priority::override_urgency(self.task_repo.as_ref(), id, level).await {
                Ok(t) => task_result = Some(t),
                Err(e) => return Self::error_content(e),
            }
        }

        if let Some(i) = impact {
            let level = match parse_impact(&i) {
                Ok(l) => l,
                Err(e) => return Self::error_content(e),
            };
            match priority::override_impact(self.task_repo.as_ref(), id, level).await {
                Ok(t) => task_result = Some(t),
                Err(e) => return Self::error_content(e),
            }
        }

        match task_result {
            Some(task) => Self::json_content(&task),
            None => Self::error_content("At least one of urgency or impact must be provided"),
        }
    }

    #[tool(description = "Reset urgency to auto-calculated value based on deadline proximity. Clears manual override.")]
    async fn reset_urgency(
        &self,
        #[tool(param)]
        #[tool(description = "Task UUID")]
        task_id: String,
    ) -> Vec<Content> {
        let id = match parse_uuid(&task_id) {
            Ok(id) => id,
            Err(e) => return Self::error_content(e),
        };
        match priority::reset_urgency(self.task_repo.as_ref(), id, self.today()).await {
            Ok(task) => Self::json_content(&task),
            Err(e) => Self::error_content(e),
        }
    }

    // ─── Activity Tracking ───

    #[tool(description = "Start tracking an activity. Automatically stops any currently active activity. Half-day (morning/afternoon) is determined from current time.")]
    async fn start_activity(
        &self,
        #[tool(param)]
        #[tool(description = "Optional task UUID to associate this activity with")]
        task_id: Option<String>,
    ) -> Vec<Content> {
        let tid = match task_id {
            Some(s) => match parse_uuid(&s) {
                Ok(id) => Some(id),
                Err(e) => return Self::error_content(e),
            },
            None => None,
        };
        match activity_tracking::start_activity(
            self.activity_repo.as_ref(),
            self.user_id,
            tid,
            Utc::now(),
        )
        .await
        {
            Ok(slot) => Self::json_content(&slot),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Stop the currently active activity tracker. Returns the stopped slot, or null if nothing was active.")]
    async fn stop_activity(&self) -> Vec<Content> {
        match activity_tracking::stop_activity(
            self.activity_repo.as_ref(),
            self.user_id,
            Utc::now(),
        )
        .await
        {
            Ok(Some(slot)) => Self::json_content(&slot),
            Ok(None) => vec![Content::text("No active activity to stop")],
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Get the currently running activity slot, if any.")]
    async fn get_current_activity(&self) -> Vec<Content> {
        match activity_tracking::get_current_activity(self.activity_repo.as_ref(), self.user_id)
            .await
        {
            Ok(Some(slot)) => Self::json_content(&slot),
            Ok(None) => vec![Content::text("No activity currently running")],
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Get the activity journal (all tracked slots) for a specific date. Defaults to today.")]
    async fn get_activity_journal(
        &self,
        #[tool(param)]
        #[tool(description = "Date to view (YYYY-MM-DD, defaults to today)")]
        date: Option<String>,
    ) -> Vec<Content> {
        let d = match date {
            Some(s) => match parse_date(&s) {
                Ok(d) => d,
                Err(e) => return Self::error_content(e),
            },
            None => self.today(),
        };
        match activity_tracking::get_activity_journal(self.activity_repo.as_ref(), self.user_id, d)
            .await
        {
            Ok(slots) => Self::json_content(&slots),
            Err(e) => Self::error_content(e),
        }
    }

    // ─── Alerts ───

    #[tool(description = "Get alerts for the user. Can filter by resolved status.")]
    async fn get_alerts(
        &self,
        #[tool(param)]
        #[tool(description = "Filter by resolved status: true=resolved only, false=unresolved only, omit=all")]
        resolved: Option<bool>,
    ) -> Vec<Content> {
        match alerts::get_alerts(self.alert_repo.as_ref(), self.user_id, resolved).await {
            Ok(alert_list) => Self::json_content(&alert_list),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Mark an alert as resolved.")]
    async fn resolve_alert(
        &self,
        #[tool(param)]
        #[tool(description = "Alert UUID")]
        alert_id: String,
    ) -> Vec<Content> {
        let id = match parse_uuid(&alert_id) {
            Ok(id) => id,
            Err(e) => return Self::error_content(e),
        };
        match alerts::resolve_alert(self.alert_repo.as_ref(), id).await {
            Ok(alert) => Self::json_content(&alert),
            Err(e) => Self::error_content(e),
        }
    }

    // ─── Configuration ───

    #[tool(description = "Get all configuration key-value pairs.")]
    async fn get_configuration(&self) -> Vec<Content> {
        match configuration::get_all_config(self.config_repo.as_ref(), self.user_id).await {
            Ok(config) => {
                let map: std::collections::HashMap<&str, &str> =
                    config.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                Self::json_content(&map)
            }
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Set a configuration key-value pair. Common keys: general.working_hours, general.working_days, jira.url, jira.project_keys")]
    async fn set_configuration(
        &self,
        #[tool(param)]
        #[tool(description = "Configuration key (e.g. 'general.working_hours')")]
        key: String,
        #[tool(param)]
        #[tool(description = "Configuration value")]
        value: String,
    ) -> Vec<Content> {
        match configuration::set_config(self.config_repo.as_ref(), self.user_id, &key, &value).await
        {
            Ok(()) => vec![Content::text(format!("Configuration set: {} = {}", key, value))],
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "List all tags.")]
    async fn list_tags(&self) -> Vec<Content> {
        match configuration::get_tags(self.tag_repo.as_ref(), self.user_id).await {
            Ok(tags) => Self::json_content(&tags),
            Err(e) => Self::error_content(e),
        }
    }

    #[tool(description = "Get sync status for all configured external sources (Jira, Outlook, Excel).")]
    async fn get_sync_status(&self) -> Vec<Content> {
        match self.sync_repo.find_by_user(self.user_id).await {
            Ok(statuses) => Self::json_content(&statuses),
            Err(e) => Self::error_content(e),
        }
    }

    // ─── Projects ───

    #[tool(description = "List all projects.")]
    async fn list_projects(&self) -> Vec<Content> {
        match self.project_repo.find_by_user(self.user_id).await {
            Ok(projects) => Self::json_content(&projects),
            Err(e) => Self::error_content(e),
        }
    }
}

#[tool(tool_box)]
impl ServerHandler for AggregatedPlanServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            instructions: Some("Aggregated Plan - Tech Lead cockpit for task management, activity tracking, and workload planning. Provides access to tasks (from Jira, Excel, or personal), meetings, priority matrix, activity journal, alerts, and configuration.".into()),
            ..Default::default()
        }
    }
}
