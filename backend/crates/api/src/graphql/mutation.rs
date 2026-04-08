use std::sync::Arc;

use async_graphql::{Context, MaybeUndefined, Object, Result, ID};
use domain::types::UserId;
use uuid::Uuid;

use application::repositories::*;
use application::services::*;
use application::use_cases::{activity_tracking, alerts, configuration, deduplication, priority, sync, task_management};
use infrastructure::connectors::jira::HttpJiraClient;
use infrastructure::connectors::outlook::client::GraphOutlookClient;

use super::types::*;

/// Root mutation type for the GraphQL schema.
#[derive(Default)]
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// No-op mutation placeholder. Returns true.
    async fn noop(&self) -> bool {
        true
    }

    /// Create a new personal task.
    async fn create_task(
        &self,
        ctx: &Context<'_>,
        input: CreateTaskInput,
    ) -> Result<TaskGql> {
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let today = chrono::Utc::now().date_naive();

        let app_input = convert_create_input(input)?;

        let task =
            task_management::create_personal_task(task_repo.as_ref(), *user_id, app_input, today)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Update an existing task.
    async fn update_task(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: UpdateTaskInput,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
        let today = chrono::Utc::now().date_naive();

        let app_input = convert_update_input(input)?;

        let task = task_management::update_task(task_repo.as_ref(), task_id, app_input, today)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Delete a task by ID. Returns true on success.
    async fn delete_task(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        task_management::delete_task(task_repo.as_ref(), task_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    /// Mark a task as completed.
    async fn complete_task(&self, ctx: &Context<'_>, id: ID) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let task = task_management::complete_task(task_repo.as_ref(), task_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Append a line of text to a task's user-owned `notes` field. Backs the
    /// activity-timer "quick note" feature: existing content is preserved and
    /// the new text is added on a new paragraph.
    async fn append_task_notes(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        text: String,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let task = task_management::append_to_task_notes(task_repo.as_ref(), id, &text)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Set the tracking state of a task (inbox/followed/dismissed).
    async fn set_tracking_state(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        state: TrackingStateGql,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let task = task_management::set_tracking_state(task_repo.as_ref(), id, state.into())
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Batch-set the tracking state for multiple tasks.
    async fn set_tracking_state_batch(
        &self,
        ctx: &Context<'_>,
        task_ids: Vec<ID>,
        state: TrackingStateGql,
    ) -> Result<Vec<TaskGql>> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let ids: Vec<Uuid> = task_ids
            .into_iter()
            .map(|id| {
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))
            })
            .collect::<Result<Vec<_>>>()?;

        let tasks =
            task_management::set_tracking_state_batch(task_repo.as_ref(), ids, state.into())
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(tasks.into_iter().map(TaskGql).collect())
    }

    /// Override the urgency level of a task (manual override).
    async fn update_priority(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        urgency: Option<UrgencyLevelGql>,
        impact: Option<ImpactLevelGql>,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let mut task: Option<domain::types::Task> = None;

        if let Some(u) = urgency {
            task = Some(
                priority::override_urgency(task_repo.as_ref(), id, u.into())
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?,
            );
        }

        if let Some(i) = impact {
            task = Some(
                priority::override_impact(task_repo.as_ref(), id, i.into())
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?,
            );
        }

        match task {
            Some(t) => Ok(TaskGql(t)),
            None => Err(async_graphql::Error::new(
                "At least one of urgency or impact must be provided",
            )),
        }
    }

    /// Reset urgency to auto-calculated based on deadline.
    async fn reset_urgency(&self, ctx: &Context<'_>, task_id: ID) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
        let today = chrono::Utc::now().date_naive();

        let task = priority::reset_urgency(task_repo.as_ref(), id, today)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Trigger a sync for a specific source (or all sources if not specified).
    /// Returns updated sync statuses.
    async fn force_sync(
        &self,
        ctx: &Context<'_>,
        source: Option<SourceGql>,
    ) -> Result<Vec<SyncStatusGql>> {
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;
        let project_repo = ctx.data::<Arc<dyn ProjectRepository>>()?;
        let sync_repo = ctx.data::<Arc<dyn SyncStatusRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;

        // Build clients dynamically from stored configuration.
        let jira_client: Option<Arc<dyn JiraClient>> = {
            let base_url = config_repo.get(*user_id, "jira.base_url").await.ok().flatten();
            let email = config_repo.get(*user_id, "jira.email").await.ok().flatten();
            let token = config_repo.get(*user_id, "jira.token").await.ok().flatten();
            match (base_url, email, token) {
                (Some(url), Some(em), Some(tok)) if !url.is_empty() && !em.is_empty() && !tok.is_empty() => {
                    Some(Arc::new(HttpJiraClient::new(url, em, tok)))
                }
                _ => None,
            }
        };
        let outlook_client: Option<Arc<dyn OutlookClient>> = {
            let token = config_repo.get(*user_id, "outlook.access_token").await.ok().flatten();
            match token {
                Some(tok) if !tok.is_empty() => Some(Arc::new(GraphOutlookClient::new(tok))),
                _ => None,
            }
        };
        let excel_client: Option<Arc<dyn ExcelClient>> = ctx
            .data::<Arc<dyn ExcelClient>>()
            .ok()
            .map(|c| c.clone());

        match source {
            Some(src) => {
                // Sync a single source.
                let domain_source: domain::types::Source = src.into();
                sync::sync_source(
                    domain_source,
                    task_repo.as_ref(),
                    meeting_repo.as_ref(),
                    project_repo.as_ref(),
                    sync_repo.as_ref(),
                    jira_client.as_deref(),
                    outlook_client.as_deref(),
                    excel_client.as_deref(),
                    config_repo.as_ref(),
                    *user_id,
                )
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            }
            None => {
                // Sync all sources.
                sync::sync_all(
                    jira_client.as_deref(),
                    outlook_client.as_deref(),
                    excel_client.as_deref(),
                    task_repo.as_ref(),
                    meeting_repo.as_ref(),
                    project_repo.as_ref(),
                    sync_repo.as_ref(),
                    config_repo.as_ref(),
                    *user_id,
                )
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            }
        }

        // Return all sync statuses.
        let statuses = sync_repo
            .find_by_user(*user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(statuses.into_iter().map(SyncStatusGql).collect())
    }

    // ─── Deduplication mutations ───

    /// Manually link two tasks.
    async fn link_tasks(
        &self,
        ctx: &Context<'_>,
        task_id_primary: ID,
        task_id_secondary: ID,
    ) -> Result<bool> {
        let task_link_repo = ctx.data::<Arc<dyn TaskLinkRepository>>()?;
        let primary = Uuid::parse_str(&task_id_primary)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
        let secondary = Uuid::parse_str(&task_id_secondary)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        deduplication::link_tasks(task_link_repo.as_ref(), primary, secondary)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    /// Unlink two tasks by removing their link.
    async fn unlink_tasks(&self, ctx: &Context<'_>, link_id: ID) -> Result<bool> {
        let task_link_repo = ctx.data::<Arc<dyn TaskLinkRepository>>()?;
        let id = Uuid::parse_str(&link_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid link ID: {}", e)))?;

        deduplication::unlink_tasks(task_link_repo.as_ref(), id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    /// Confirm or reject a deduplication suggestion.
    async fn confirm_deduplication(
        &self,
        ctx: &Context<'_>,
        task_id_primary: ID,
        task_id_secondary: ID,
        accept: bool,
    ) -> Result<bool> {
        let task_link_repo = ctx.data::<Arc<dyn TaskLinkRepository>>()?;
        let primary = Uuid::parse_str(&task_id_primary)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
        let secondary = Uuid::parse_str(&task_id_secondary)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        deduplication::confirm_suggestion(task_link_repo.as_ref(), primary, secondary, accept)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    // ─── Alert mutations ───

    /// Resolve an alert by ID.
    async fn resolve_alert(&self, ctx: &Context<'_>, id: ID) -> Result<AlertGql> {
        let alert_repo = ctx.data::<Arc<dyn AlertRepository>>()?;
        let alert_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid alert ID: {}", e)))?;

        let alert = alerts::resolve_alert(alert_repo.as_ref(), alert_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(AlertGql(alert))
    }

    // ─── Activity tracking mutations ───

    /// Start tracking a new activity. Stops the previous active slot (if any).
    async fn start_activity(
        &self,
        ctx: &Context<'_>,
        task_id: Option<ID>,
    ) -> Result<ActivitySlotGql> {
        let user_id = ctx.data::<UserId>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let now = chrono::Utc::now();

        let tid = match task_id {
            Some(id) => Some(
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?,
            ),
            None => None,
        };

        let slot =
            activity_tracking::start_activity(activity_repo.as_ref(), *user_id, tid, now)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(ActivitySlotGql(slot))
    }

    /// Stop the currently active activity tracking.
    async fn stop_activity(&self, ctx: &Context<'_>) -> Result<Option<ActivitySlotGql>> {
        let user_id = ctx.data::<UserId>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let now = chrono::Utc::now();

        let slot = activity_tracking::stop_activity(activity_repo.as_ref(), *user_id, now)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(slot.map(ActivitySlotGql))
    }

    /// Update an existing activity slot.
    async fn update_activity_slot(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: UpdateActivitySlotInput,
    ) -> Result<ActivitySlotGql> {
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let slot_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid slot ID: {}", e)))?;

        // Convert MaybeUndefined task_id:
        // Undefined => None (don't change), Null => Some(None) (clear), Value => Some(Some(id))
        let task_id = match input.task_id {
            MaybeUndefined::Value(tid) => {
                let parsed = Uuid::parse_str(&tid)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
                Some(Some(parsed))
            }
            MaybeUndefined::Null => Some(None),
            MaybeUndefined::Undefined => None,
        };

        let slot = activity_tracking::update_activity_slot(
            activity_repo.as_ref(),
            slot_id,
            task_id,
            input.start_time,
            input.end_time,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(ActivitySlotGql(slot))
    }

    /// Delete an activity slot by ID.
    async fn delete_activity_slot(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let slot_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid slot ID: {}", e)))?;

        activity_tracking::delete_activity_slot(activity_repo.as_ref(), slot_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    /// Create a manual activity slot with explicit start and end times.
    async fn create_activity_slot(
        &self,
        ctx: &Context<'_>,
        input: CreateActivitySlotInput,
    ) -> Result<ActivitySlotGql> {
        let user_id = ctx.data::<UserId>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;

        let task_id = match input.task_id {
            Some(id) => Some(
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?,
            ),
            None => None,
        };

        let slot = activity_tracking::create_manual_activity_slot(
            activity_repo.as_ref(),
            *user_id,
            input.start_time,
            input.end_time,
            task_id,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(ActivitySlotGql(slot))
    }

    // ─── Meeting-project association (Task 38) ───

    /// Update the project association of a meeting.
    async fn update_meeting_project(
        &self,
        ctx: &Context<'_>,
        meeting_id: ID,
        project_id: Option<ID>,
    ) -> Result<MeetingGql> {
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;

        let mid = Uuid::parse_str(&meeting_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid meeting ID: {}", e)))?;

        let pid = match project_id {
            Some(id) => Some(
                Uuid::parse_str(&id).map_err(|e| {
                    async_graphql::Error::new(format!("Invalid project ID: {}", e))
                })?,
            ),
            None => None,
        };

        let mut meeting = meeting_repo
            .find_by_id(mid)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| {
                async_graphql::Error::new(format!("Meeting {} not found", mid))
            })?;

        meeting.project_id = pid;
        meeting_repo
            .update(&meeting)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(MeetingGql(meeting))
    }

    // ─── Tag management mutations (Task 39) ───

    /// Create a new tag.
    async fn create_tag(
        &self,
        ctx: &Context<'_>,
        name: String,
        color: Option<String>,
    ) -> Result<TagGql> {
        let user_id = ctx.data::<UserId>()?;
        let tag_repo = ctx.data::<Arc<dyn TagRepository>>()?;

        let tag = configuration::create_tag(tag_repo.as_ref(), *user_id, name, color)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TagGql(tag))
    }

    /// Update an existing tag.
    async fn update_tag(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: Option<String>,
        color: Option<String>,
    ) -> Result<TagGql> {
        let tag_repo = ctx.data::<Arc<dyn TagRepository>>()?;
        let tag_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {}", e)))?;

        // Wrap color in Option<Option<String>> for the use case:
        // Some(color_value) means update, None means don't change.
        let color_update = color.map(|c| {
            if c.is_empty() {
                None
            } else {
                Some(c)
            }
        });

        let tag = configuration::update_tag(tag_repo.as_ref(), tag_id, name, color_update)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TagGql(tag))
    }

    /// Delete a tag by ID. Returns true on success.
    async fn delete_tag(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let tag_repo = ctx.data::<Arc<dyn TagRepository>>()?;
        let tag_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {}", e)))?;

        configuration::delete_tag(tag_repo.as_ref(), tag_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    // ─── Configuration mutations ───

    /// Update a configuration key-value pair.
    async fn update_configuration(
        &self,
        ctx: &Context<'_>,
        key: String,
        value: String,
    ) -> Result<bool> {
        let user_id = ctx.data::<UserId>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;

        configuration::set_config(config_repo.as_ref(), *user_id, &key, &value)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }
}

/// Convert GraphQL CreateTaskInput to application layer input.
fn convert_create_input(
    input: CreateTaskInput,
) -> Result<task_management::CreateTaskInput> {
    let project_id = match input.project_id {
        Some(id) => Some(
            Uuid::parse_str(&id)
                .map_err(|e| async_graphql::Error::new(format!("Invalid project ID: {}", e)))?,
        ),
        None => None,
    };

    let tag_ids: Vec<Uuid> = match input.tag_ids {
        Some(ids) => ids
            .into_iter()
            .map(|id| {
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {}", e)))
            })
            .collect::<Result<Vec<_>>>()?,
        None => vec![],
    };

    Ok(task_management::CreateTaskInput {
        title: input.title,
        description: input.description,
        notes: input.notes,
        project_id,
        deadline: input.deadline,
        planned_start: input.planned_start,
        planned_end: input.planned_end,
        estimated_hours: input.estimated_hours.map(|h| h as f32),
        impact: input.impact.map(|i| i.into()),
        urgency: input.urgency.map(|u| u.into()),
        tags: tag_ids,
    })
}

/// Convert GraphQL UpdateTaskInput to application layer input.
fn convert_update_input(
    input: UpdateTaskInput,
) -> Result<task_management::UpdateTaskInput> {
    let project_id = match input.project_id {
        Some(id) => Some(Some(
            Uuid::parse_str(&id)
                .map_err(|e| async_graphql::Error::new(format!("Invalid project ID: {}", e)))?,
        )),
        None => None,
    };

    let tag_ids = match input.tag_ids {
        Some(ids) => Some(
            ids.into_iter()
                .map(|id| {
                    Uuid::parse_str(&id)
                        .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {}", e)))
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        None => None,
    };

    Ok(task_management::UpdateTaskInput {
        title: input.title,
        description: input.description.map(Some),
        notes: input.notes.map(Some),
        project_id,
        deadline: input.deadline.map(Some),
        planned_start: match input.planned_start {
            MaybeUndefined::Value(dt) => Some(Some(dt)),
            MaybeUndefined::Null      => Some(None),
            MaybeUndefined::Undefined => None,
        },
        planned_end: input.planned_end.map(Some),
        estimated_hours: input.estimated_hours.map(|h| Some(h as f32)),
        status: input.status.map(|s| s.into()),
        impact: input.impact.map(|i| i.into()),
        urgency: input.urgency.map(|u| u.into()),
        tags: tag_ids,
        remaining_hours_override: match input.remaining_hours_override {
            Some(Some(h)) => Some(Some(h as f32)),
            Some(None) => Some(None),
            None => None,
        },
        estimated_hours_override: match input.estimated_hours_override {
            Some(Some(h)) => Some(Some(h as f32)),
            Some(None) => Some(None),
            None => None,
        },
    })
}
