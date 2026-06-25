use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::common::*;
use super::recurrence::RecurrenceTemplateId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub user_id: UserId,
    pub title: String,
    pub description: Option<String>,
    /// User-owned markdown notes. Never overwritten by Jira sync — distinct from `description`,
    /// which mirrors the Jira ticket body.
    pub notes: Option<String>,
    pub source: Source,
    pub source_id: Option<String>,
    pub jira_status: Option<String>,
    pub status: TaskStatus,
    pub project_id: Option<ProjectId>,
    pub assignee: Option<String>,
    /// Person this task is delegated to (free text). User-owned — never
    /// overwritten by Jira/Excel sync, unlike `assignee` which mirrors Jira.
    pub delegated_to: Option<String>,
    pub deadline: Option<NaiveDate>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f32>,
    pub urgency: UrgencyLevel,
    pub urgency_manual: bool,
    pub impact: ImpactLevel,
    pub tags: Vec<TagId>,
    pub tracking_state: TrackingState,
    pub jira_remaining_seconds: Option<i32>,
    pub jira_original_estimate_seconds: Option<i32>,
    pub jira_time_spent_seconds: Option<i32>,
    pub remaining_hours_override: Option<f32>,
    pub estimated_hours_override: Option<f32>,
    /// Links this task instance to its recurrence template, if any.
    pub recurrence_id: Option<RecurrenceTemplateId>,
    /// The occurrence slot this instance fills (the "planned date" for the recurrence).
    pub occurrence_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    /// Effective remaining hours: local override > Jira remaining > None
    pub fn effective_remaining_hours(&self) -> Option<f32> {
        self.remaining_hours_override
            .or(self.jira_remaining_seconds.map(|s| s as f32 / 3600.0))
    }

    /// Effective estimated hours: local override > Jira estimate > estimated_hours (personal tasks)
    pub fn effective_estimated_hours(&self) -> Option<f32> {
        self.estimated_hours_override
            .or(self.jira_original_estimate_seconds.map(|s| s as f32 / 3600.0))
            .or(self.estimated_hours)
    }

    /// Whether this task contributes to workload/hour-count aggregations (per-day and
    /// weekly totals). Done and Cancelled tasks are excluded: they retain their estimate
    /// (Jira original estimate / personal estimate) but no longer represent outstanding
    /// work, so counting them would inflate the totals. Blocked tasks still count — the
    /// work is not finished.
    pub fn counts_toward_workload(&self) -> bool {
        !matches!(self.status, TaskStatus::Done | TaskStatus::Cancelled)
    }

    /// A task is dashboard-visible if it has a planned_start or a deadline, or if it is an
    /// active (Todo/InProgress) followed task.
    pub fn is_dashboard_visible(&self) -> bool {
        if self.planned_start.is_some() || self.deadline.is_some() {
            return true;
        }
        matches!(self.status, TaskStatus::Todo | TaskStatus::InProgress)
            && self.tracking_state == TrackingState::Followed
    }
}

/// Choose the survivor and the loser from a deduplication pair.
///
/// Rules (in priority order):
/// 1. If exactly one task has `source == Jira`, that task is the survivor.
/// 2. Else if exactly one is dashboard-visible, that one is the survivor.
/// 3. Else task `a` (the "primary" / first argument) is the survivor.
///
/// Returns `(survivor, loser)`.
pub fn choose_dedup_survivor<'a>(a: &'a Task, b: &'a Task) -> (&'a Task, &'a Task) {
    let a_jira = a.source == Source::Jira;
    let b_jira = b.source == Source::Jira;

    // Rule 1: exactly one is Jira
    if a_jira && !b_jira {
        return (a, b);
    }
    if b_jira && !a_jira {
        return (b, a);
    }

    // Rule 2: exactly one is dashboard-visible
    let a_vis = a.is_dashboard_visible();
    let b_vis = b.is_dashboard_visible();
    if a_vis && !b_vis {
        return (a, b);
    }
    if b_vis && !a_vis {
        return (b, a);
    }

    // Rule 3: fallback — keep `a` as survivor
    (a, b)
}

/// Apply the "make-visible" transform to `survivor`, optionally inheriting
/// planning dates from `loser` when the survivor has none.
///
/// - Always sets `tracking_state = Followed`.
/// - If survivor has no `planned_start` AND no `deadline`, copies both from `loser`.
pub fn make_survivor_visible(mut survivor: Task, loser: &Task) -> Task {
    survivor.tracking_state = TrackingState::Followed;

    if survivor.planned_start.is_none() && survivor.deadline.is_none() {
        survivor.planned_start = loser.planned_start;
        survivor.planned_end = loser.planned_end;
        survivor.deadline = loser.deadline;
    }

    survivor
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_test_task() -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            title: "Test".to_string(),
            description: None,
            notes: None,
            source: Source::Jira,
            source_id: Some("PROJ-1".to_string()),
            jira_status: Some("In Progress".to_string()),
            status: TaskStatus::InProgress,
            project_id: None,
            assignee: None,
            delegated_to: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Medium,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn effective_remaining_hours_override_takes_precedence() {
        let mut task = make_test_task();
        task.jira_remaining_seconds = Some(7200);
        task.remaining_hours_override = Some(5.0);
        assert_eq!(task.effective_remaining_hours(), Some(5.0));
    }

    #[test]
    fn effective_remaining_hours_falls_back_to_jira() {
        let mut task = make_test_task();
        task.jira_remaining_seconds = Some(3600);
        assert_eq!(task.effective_remaining_hours(), Some(1.0));
    }

    #[test]
    fn effective_remaining_hours_none_when_no_data() {
        let task = make_test_task();
        assert_eq!(task.effective_remaining_hours(), None);
    }

    #[test]
    fn effective_estimated_hours_override_takes_precedence() {
        let mut task = make_test_task();
        task.jira_original_estimate_seconds = Some(14400);
        task.estimated_hours_override = Some(8.0);
        assert_eq!(task.effective_estimated_hours(), Some(8.0));
    }

    #[test]
    fn effective_estimated_hours_falls_back_to_jira() {
        let mut task = make_test_task();
        task.jira_original_estimate_seconds = Some(14400);
        assert_eq!(task.effective_estimated_hours(), Some(4.0));
    }

    #[test]
    fn effective_estimated_hours_falls_back_to_estimated_hours() {
        let mut task = make_test_task();
        task.estimated_hours = Some(3.5);
        assert_eq!(task.effective_estimated_hours(), Some(3.5));
    }

    #[test]
    fn effective_estimated_hours_none_when_no_data() {
        let task = make_test_task();
        assert_eq!(task.effective_estimated_hours(), None);
    }

    // ─── is_dashboard_visible ───

    #[test]
    fn dashboard_visible_when_has_deadline() {
        let mut task = make_test_task();
        task.source = Source::Personal;
        task.status = TaskStatus::Done;
        task.tracking_state = TrackingState::Inbox;
        task.deadline = Some(chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        assert!(task.is_dashboard_visible());
    }

    #[test]
    fn dashboard_visible_when_has_planned_start() {
        let mut task = make_test_task();
        task.source = Source::Personal;
        task.status = TaskStatus::Done;
        task.tracking_state = TrackingState::Inbox;
        task.planned_start = Some(Utc::now());
        assert!(task.is_dashboard_visible());
    }

    #[test]
    fn dashboard_visible_when_active_followed() {
        let mut task = make_test_task();
        task.source = Source::Personal;
        task.status = TaskStatus::InProgress;
        task.tracking_state = TrackingState::Followed;
        task.planned_start = None;
        task.deadline = None;
        assert!(task.is_dashboard_visible());
    }

    #[test]
    fn dashboard_not_visible_when_done_followed_no_dates() {
        let mut task = make_test_task();
        task.status = TaskStatus::Done;
        task.tracking_state = TrackingState::Followed;
        task.planned_start = None;
        task.deadline = None;
        assert!(!task.is_dashboard_visible());
    }

    #[test]
    fn dashboard_not_visible_when_inbox_no_dates() {
        let mut task = make_test_task();
        task.status = TaskStatus::Todo;
        task.tracking_state = TrackingState::Inbox;
        task.planned_start = None;
        task.deadline = None;
        assert!(!task.is_dashboard_visible());
    }

    // ─── counts_toward_workload ───

    #[test]
    fn counts_toward_workload_excludes_done() {
        let mut task = make_test_task();
        task.status = TaskStatus::Done;
        assert!(!task.counts_toward_workload());
    }

    #[test]
    fn counts_toward_workload_excludes_cancelled() {
        let mut task = make_test_task();
        task.status = TaskStatus::Cancelled;
        assert!(!task.counts_toward_workload());
    }

    #[test]
    fn counts_toward_workload_includes_blocked() {
        let mut task = make_test_task();
        task.status = TaskStatus::Blocked;
        assert!(task.counts_toward_workload());
    }

    #[test]
    fn counts_toward_workload_includes_todo() {
        let mut task = make_test_task();
        task.status = TaskStatus::Todo;
        assert!(task.counts_toward_workload());
    }

    #[test]
    fn counts_toward_workload_includes_in_progress() {
        let mut task = make_test_task();
        task.status = TaskStatus::InProgress;
        assert!(task.counts_toward_workload());
    }

    // ─── choose_dedup_survivor ───

    #[test]
    fn survivor_jira_wins_over_personal() {
        let mut a = make_test_task();
        a.source = Source::Personal;
        let mut b = make_test_task();
        b.source = Source::Jira;

        let (survivor, loser) = choose_dedup_survivor(&a, &b);
        assert_eq!(survivor.source, Source::Jira);
        assert_eq!(loser.source, Source::Personal);
    }

    #[test]
    fn survivor_jira_wins_when_a_is_jira() {
        let mut a = make_test_task();
        a.source = Source::Jira;
        let mut b = make_test_task();
        b.source = Source::Personal;

        let (survivor, loser) = choose_dedup_survivor(&a, &b);
        assert_eq!(survivor.id, a.id);
        assert_eq!(loser.id, b.id);
    }

    #[test]
    fn survivor_visible_wins_when_no_jira() {
        let mut a = make_test_task();
        a.source = Source::Personal;
        a.planned_start = None;
        a.deadline = None;
        a.status = TaskStatus::Todo;
        a.tracking_state = TrackingState::Inbox; // not visible

        let mut b = make_test_task();
        b.source = Source::Excel;
        b.deadline = Some(chrono::NaiveDate::from_ymd_opt(2025, 6, 1).unwrap()); // visible

        let (survivor, loser) = choose_dedup_survivor(&a, &b);
        assert_eq!(survivor.id, b.id);
        assert_eq!(loser.id, a.id);
    }

    #[test]
    fn survivor_fallback_to_a_when_both_identical_visibility() {
        let mut a = make_test_task();
        a.source = Source::Personal;
        a.planned_start = None;
        a.deadline = None;
        a.tracking_state = TrackingState::Inbox;

        let mut b = make_test_task();
        b.source = Source::Personal;
        b.planned_start = None;
        b.deadline = None;
        b.tracking_state = TrackingState::Inbox;

        let (survivor, loser) = choose_dedup_survivor(&a, &b);
        assert_eq!(survivor.id, a.id);
        assert_eq!(loser.id, b.id);
    }

    // ─── make_survivor_visible ───

    #[test]
    fn make_visible_always_sets_followed() {
        let mut survivor = make_test_task();
        survivor.tracking_state = TrackingState::Inbox;
        survivor.planned_start = Some(Utc::now());
        let loser = make_test_task();

        let result = make_survivor_visible(survivor, &loser);
        assert_eq!(result.tracking_state, TrackingState::Followed);
    }

    #[test]
    fn make_visible_copies_planning_when_survivor_unplanned() {
        let planned = Utc::now();
        let deadline = chrono::NaiveDate::from_ymd_opt(2025, 12, 31).unwrap();

        let mut survivor = make_test_task();
        survivor.planned_start = None;
        survivor.deadline = None;
        survivor.tracking_state = TrackingState::Inbox;

        let mut loser = make_test_task();
        loser.planned_start = Some(planned);
        loser.deadline = Some(deadline);

        let result = make_survivor_visible(survivor, &loser);
        assert_eq!(result.tracking_state, TrackingState::Followed);
        assert_eq!(result.planned_start, Some(planned));
        assert_eq!(result.deadline, Some(deadline));
    }

    #[test]
    fn make_visible_does_not_overwrite_existing_planning() {
        let survivor_planned = Utc::now();
        let loser_planned = survivor_planned + chrono::Duration::days(10);

        let mut survivor = make_test_task();
        survivor.planned_start = Some(survivor_planned);
        survivor.deadline = None; // has planned_start so not "unplanned"

        let mut loser = make_test_task();
        loser.planned_start = Some(loser_planned);

        let result = make_survivor_visible(survivor, &loser);
        // planned_start was set — should NOT be overwritten
        assert_eq!(result.planned_start, Some(survivor_planned));
    }
}
