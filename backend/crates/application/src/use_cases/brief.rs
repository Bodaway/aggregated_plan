//! `aplan brief` — assembling what a Claude session is handed at startup.
//!
//! This layer only fetches. What goes in the brief, in which order, and how it is
//! cut down to the line ceiling all live in `domain::rules::brief`, so those rules
//! are testable without a database.

use chrono::{DateTime, NaiveDate, Utc};
use domain::rules::brief::{compose_brief, Brief, BriefInput, BriefVariant};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::{
    ActivitySlotRepository, ConfigRepository, MemoryListFilter, MemoryRepository, TaskFilter,
    TaskRepository,
};

/// Where §6.2 of the design puts the consolidation watermark: the `configuration`
/// key/value table. `sync_status` cannot carry it — its `source` column is under a
/// closed `CHECK` that rejects anything outside jira/outlook/excel/obsidian.
pub const CONSOLIDATION_LAST_RUN_KEY: &str = "memory.consolidation.last_run";

/// How many memories the brief scans. Section counts are therefore exact up to
/// this number, which is far above the few hundred durable memories a year the
/// design sizes for.
pub const BRIEF_SCAN_LIMIT: u32 = 200;

/// What the caller asks for.
#[derive(Debug, Clone, Copy)]
pub struct BriefRequest {
    pub variant: BriefVariant,
    /// Project in focus. When `None`, it is derived from the task currently being
    /// tracked, so a session that started with `aplan start` gets its own project's
    /// decisions without saying so twice.
    pub project_id: Option<ProjectId>,
    /// Local "today", for the deadline countdowns.
    pub today: NaiveDate,
    pub now: DateTime<Utc>,
}

/// Build the brief.
///
/// Nothing here fails on missing data: an absent consolidation watermark, an
/// unparseable one, or no tracked task all degrade to a well-defined brief. The
/// consolidation job is lot 5 and does not exist yet, so "never run" is the
/// normal case today and must not look like a crash.
pub async fn build_brief(
    task_repo: &dyn TaskRepository,
    memory_repo: &dyn MemoryRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    request: BriefRequest,
) -> Result<Brief, AppError> {
    // Open tasks only. The domain re-checks the status, so this filter is an
    // optimisation (the store holds ~600 rows), not the rule.
    let tasks = task_repo
        .find_by_user(
            user_id,
            &TaskFilter {
                status: Some(vec![
                    TaskStatus::Todo,
                    TaskStatus::InProgress,
                    TaskStatus::Blocked,
                ]),
                ..TaskFilter::empty()
            },
        )
        .await?;

    // One pass over the active memories: the domain splits them by kind. Two
    // kind-filtered queries would buy nothing at this scale.
    let memories = memory_repo
        .list(
            user_id,
            &MemoryListFilter {
                status: Some(vec![MemoryStatus::Active]),
                include_invalidated: false,
                project_id: None,
                limit: BRIEF_SCAN_LIMIT,
                offset: 0,
            },
        )
        .await?;

    let pending = memory_repo
        .list(
            user_id,
            &MemoryListFilter {
                status: Some(vec![MemoryStatus::Pending]),
                include_invalidated: false,
                project_id: None,
                limit: BRIEF_SCAN_LIMIT,
                offset: 0,
            },
        )
        .await?;

    let current_project = match request.project_id {
        Some(project) => Some(project),
        None => current_project_of(task_repo, activity_repo, user_id).await?,
    };

    let last_consolidation = last_consolidation_run(config_repo, user_id).await;

    Ok(compose_brief(&BriefInput {
        variant: request.variant,
        today: request.today,
        now: request.now,
        tasks: &tasks,
        memories: &memories,
        current_project,
        pending_count: pending.len(),
        last_consolidation,
    }))
}

/// The project of the task being tracked right now, if any.
async fn current_project_of(
    task_repo: &dyn TaskRepository,
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
) -> Result<Option<ProjectId>, AppError> {
    let Some(slot) = activity_repo.find_active(user_id).await? else {
        return Ok(None);
    };
    let Some(task_id) = slot.task_id else {
        return Ok(None);
    };
    Ok(task_repo
        .find_by_id(task_id)
        .await?
        .and_then(|task| task.project_id))
}

/// Read the consolidation watermark. A missing key, an unreadable store or a
/// value that is not a timestamp all mean "never run": the brief's job is to make
/// a dead consolidation visible, not to fail alongside it.
async fn last_consolidation_run(
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
) -> Option<DateTime<Utc>> {
    config_repo
        .get(user_id, CONSOLIDATION_LAST_RUN_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|raw| {
            DateTime::parse_from_rfc3339(raw.trim())
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use domain::rules::brief::ConsolidationAge;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;

    // ─── Doubles ─────────────────────────────────────────────────────────

    #[derive(Default)]
    struct MemTaskRepo {
        tasks: Mutex<Vec<Task>>,
    }

    #[async_trait]
    impl TaskRepository for MemTaskRepo {
        async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .expect("lock")
                .iter()
                .find(|t| t.id == id)
                .cloned())
        }

        async fn find_by_user(
            &self,
            user_id: UserId,
            filter: &TaskFilter,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .expect("lock")
                .iter()
                .filter(|t| t.user_id == user_id)
                .filter(|t| match &filter.status {
                    None => true,
                    Some(wanted) => wanted.contains(&t.status),
                })
                .cloned()
                .collect())
        }

        async fn find_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _source_id: &str,
        ) -> Result<Option<Task>, RepositoryError> {
            Ok(None)
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
            self.tasks.lock().expect("lock").push(task.clone());
            Ok(())
        }

        async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError> {
            self.tasks.lock().expect("lock").extend_from_slice(tasks);
            Ok(())
        }

        async fn delete(&self, id: TaskId) -> Result<(), RepositoryError> {
            self.tasks.lock().expect("lock").retain(|t| t.id != id);
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

    #[derive(Default)]
    struct MemMemoryRepo {
        rows: Mutex<Vec<Memory>>,
        /// Records the limits the use case bound, so `LIMIT 0` cannot come back.
        seen_limits: Mutex<Vec<u32>>,
    }

    #[async_trait]
    impl MemoryRepository for MemMemoryRepo {
        async fn create(&self, memory: &Memory) -> Result<(), RepositoryError> {
            self.rows.lock().expect("lock").push(memory.clone());
            Ok(())
        }

        async fn find_by_id(
            &self,
            id: MemoryId,
            user_id: UserId,
        ) -> Result<Option<Memory>, RepositoryError> {
            Ok(self
                .rows
                .lock()
                .expect("lock")
                .iter()
                .find(|m| m.id == id && m.user_id == user_id)
                .cloned())
        }

        async fn list(
            &self,
            user_id: UserId,
            filter: &MemoryListFilter,
        ) -> Result<Vec<Memory>, RepositoryError> {
            self.seen_limits
                .lock()
                .expect("lock")
                .push(filter.effective_limit());
            Ok(self
                .rows
                .lock()
                .expect("lock")
                .iter()
                .filter(|m| m.user_id == user_id)
                .filter(|m| match &filter.status {
                    None => true,
                    Some(wanted) => wanted.contains(&m.status),
                })
                .filter(|m| filter.include_invalidated || m.invalidated_at.is_none())
                .take(filter.effective_limit() as usize)
                .cloned()
                .collect())
        }

        async fn update(&self, _memory: &Memory) -> Result<(), RepositoryError> {
            Ok(())
        }

        async fn apply_merge(
            &self,
            _survivor: &Memory,
            _discarded: MemoryId,
            _user_id: UserId,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }

        async fn apply_supersession(
            &self,
            _invalidated: &Memory,
            _successor: &Memory,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }

        async fn existing_source_refs(
            &self,
            _user_id: UserId,
            _prefix: &str,
        ) -> Result<Vec<String>, RepositoryError> {
            Ok(vec![])
        }

        async fn supersession_chain(
            &self,
            _user_id: UserId,
            _from: MemoryId,
        ) -> Result<Vec<MemoryId>, RepositoryError> {
            Ok(vec![])
        }
    }

    #[derive(Default)]
    struct MemActivityRepo {
        active: Mutex<Option<ActivitySlot>>,
    }

    #[async_trait]
    impl ActivitySlotRepository for MemActivityRepo {
        async fn find_by_id(
            &self,
            _id: ActivitySlotId,
        ) -> Result<Option<ActivitySlot>, RepositoryError> {
            Ok(None)
        }

        async fn find_by_user_and_date(
            &self,
            _user_id: UserId,
            _date: NaiveDate,
        ) -> Result<Vec<ActivitySlot>, RepositoryError> {
            Ok(vec![])
        }

        async fn find_active(
            &self,
            _user_id: UserId,
        ) -> Result<Option<ActivitySlot>, RepositoryError> {
            Ok(self.active.lock().expect("lock").clone())
        }

        async fn save(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> {
            Ok(())
        }

        async fn update(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> {
            Ok(())
        }

        async fn find_by_user_and_date_range(
            &self,
            _user_id: UserId,
            _start: NaiveDate,
            _end: NaiveDate,
        ) -> Result<Vec<ActivitySlot>, RepositoryError> {
            Ok(vec![])
        }

        async fn delete(&self, _id: ActivitySlotId) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemConfigRepo {
        values: Mutex<HashMap<String, String>>,
        /// When set, every read fails — a broken store must not break the brief.
        broken: bool,
    }

    #[async_trait]
    impl ConfigRepository for MemConfigRepo {
        async fn get(
            &self,
            _user_id: UserId,
            key: &str,
        ) -> Result<Option<String>, RepositoryError> {
            if self.broken {
                return Err(RepositoryError::Database("store is down".into()));
            }
            Ok(self.values.lock().expect("lock").get(key).cloned())
        }

        async fn get_all(
            &self,
            _user_id: UserId,
        ) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(vec![])
        }

        async fn set(
            &self,
            _user_id: UserId,
            key: &str,
            value: &str,
        ) -> Result<(), RepositoryError> {
            self.values
                .lock()
                .expect("lock")
                .insert(key.to_string(), value.to_string());
            Ok(())
        }
    }

    // ─── Fixtures ────────────────────────────────────────────────────────

    fn uid() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("valid uuid")
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 3, 8, 30, 0)
            .single()
            .expect("valid instant")
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 3).expect("valid date")
    }

    fn request() -> BriefRequest {
        BriefRequest {
            variant: BriefVariant::Session,
            project_id: None,
            today: today(),
            now: now(),
        }
    }

    fn task(title: &str, deadline: Option<NaiveDate>) -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: uid(),
            title: title.to_string(),
            description: None,
            notes: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            delegated_to: None,
            deadline,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Low,
            tags: vec![],
            tracking_state: TrackingState::Followed,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            recurrence_id: None,
            occurrence_date: None,
            gryzzly_task_id: None,
            gryzzly_project_id: None,
            created_at: now(),
            updated_at: now(),
        }
    }

    fn memory(kind: MemoryKind, title: &str, status: MemoryStatus) -> Memory {
        Memory {
            id: Uuid::new_v4(),
            user_id: uid(),
            kind,
            title: title.to_string(),
            body: None,
            occurred_at: now() - chrono::Duration::days(10),
            recorded_at: now(),
            invalidated_at: None,
            superseded_by: None,
            source: MemorySource::ClaudeSession,
            source_ref: None,
            status,
            project_id: None,
            task_id: None,
            stakeholders: vec![],
        }
    }

    struct Fixture {
        tasks: MemTaskRepo,
        memories: MemMemoryRepo,
        activity: MemActivityRepo,
        config: MemConfigRepo,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                tasks: MemTaskRepo::default(),
                memories: MemMemoryRepo::default(),
                activity: MemActivityRepo::default(),
                config: MemConfigRepo::default(),
            }
        }

        async fn brief(&self, request: BriefRequest) -> Brief {
            build_brief(
                &self.tasks,
                &self.memories,
                &self.activity,
                &self.config,
                uid(),
                request,
            )
            .await
            .expect("the brief must not fail")
        }
    }

    // ─── Tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_brief_gathers_deadlines_commitments_decisions_and_the_queue() {
        let f = Fixture::new();
        f.tasks
            .save(&task("Cartier certificat", Some(today() + chrono::Duration::days(42))))
            .await
            .expect("saved");
        f.tasks
            .save(&task("Test uppercase kind", Some(today())))
            .await
            .expect("saved");
        f.memories
            .create(&memory(
                MemoryKind::Commitment,
                "Répondre à Pierre",
                MemoryStatus::Active,
            ))
            .await
            .expect("created");
        f.memories
            .create(&memory(
                MemoryKind::Decision,
                "Wave 0 limitée",
                MemoryStatus::Active,
            ))
            .await
            .expect("created");
        f.memories
            .create(&memory(MemoryKind::Fact, "un candidat", MemoryStatus::Pending))
            .await
            .expect("created");

        let brief = f.brief(request()).await;

        assert_eq!(brief.deadlines.total, 1, "the fixture task is filtered out");
        assert_eq!(brief.deadlines.entries[0].title, "Cartier certificat");
        assert_eq!(brief.commitments.total, 1);
        assert_eq!(brief.decisions.total, 1);
        assert_eq!(brief.pending_count, 1);
    }

    #[tokio::test]
    async fn closed_tasks_are_left_out_of_the_query() {
        let f = Fixture::new();
        let mut done = task("terminée", Some(today()));
        done.status = TaskStatus::Done;
        f.tasks.save(&done).await.expect("saved");
        f.tasks
            .save(&task("en cours", Some(today())))
            .await
            .expect("saved");

        let brief = f.brief(request()).await;
        assert_eq!(brief.deadlines.total, 1);
        assert_eq!(brief.deadlines.entries[0].title, "en cours");
    }

    /// `MemoryListFilter::default()` carries `limit: 0`, which used to emit
    /// `LIMIT 0` and silently return nothing. The brief must never bind a raw 0.
    #[tokio::test]
    async fn the_scan_never_asks_for_zero_rows() {
        let f = Fixture::new();
        f.brief(request()).await;
        let limits = f.memories.seen_limits.lock().expect("lock").clone();
        assert_eq!(limits, vec![BRIEF_SCAN_LIMIT, BRIEF_SCAN_LIMIT]);
        assert!(limits.iter().all(|&l| l > 0));
    }

    #[tokio::test]
    async fn a_missing_consolidation_key_reads_as_never_run() {
        let f = Fixture::new();
        let brief = f.brief(request()).await;
        assert_eq!(brief.consolidation, ConsolidationAge::NeverRun);
    }

    #[tokio::test]
    async fn the_consolidation_age_comes_from_the_configuration_table() {
        let f = Fixture::new();
        f.config
            .set(
                uid(),
                CONSOLIDATION_LAST_RUN_KEY,
                &(now() - chrono::Duration::days(19)).to_rfc3339(),
            )
            .await
            .expect("stored");
        let brief = f.brief(request()).await;
        assert_eq!(brief.consolidation, ConsolidationAge::Ran { days_ago: 19 });
    }

    #[tokio::test]
    async fn a_garbage_or_unreadable_watermark_degrades_to_never_run() {
        let f = Fixture::new();
        f.config
            .set(uid(), CONSOLIDATION_LAST_RUN_KEY, "hier soir")
            .await
            .expect("stored");
        assert_eq!(f.brief(request()).await.consolidation, ConsolidationAge::NeverRun);

        let broken = Fixture {
            config: MemConfigRepo {
                broken: true,
                ..MemConfigRepo::default()
            },
            ..Fixture::new()
        };
        assert_eq!(
            broken.brief(request()).await.consolidation,
            ConsolidationAge::NeverRun,
            "a broken config store must not break the brief"
        );
    }

    #[tokio::test]
    async fn the_project_in_focus_is_derived_from_the_tracked_task() {
        let f = Fixture::new();
        let project = Uuid::new_v4();
        let mut tracked = task("la tâche suivie", None);
        tracked.project_id = Some(project);
        let tracked_id = tracked.id;
        f.tasks.save(&tracked).await.expect("saved");
        *f.activity.active.lock().expect("lock") = Some(ActivitySlot {
            id: Uuid::new_v4(),
            user_id: uid(),
            task_id: Some(tracked_id),
            start_time: now(),
            end_time: None,
            half_day: HalfDay::Morning,
            date: today(),
            created_at: now(),
        });

        let mut mine = memory(MemoryKind::Decision, "du projet suivi", MemoryStatus::Active);
        mine.project_id = Some(project);
        f.memories.create(&mine).await.expect("created");
        f.memories
            .create(&memory(
                MemoryKind::Decision,
                "d'un autre projet",
                MemoryStatus::Active,
            ))
            .await
            .expect("created");

        let brief = f.brief(request()).await;
        assert!(brief.decisions_scoped_to_project);
        assert_eq!(brief.decisions.total, 1);
        assert_eq!(brief.decisions.entries[0].title, "du projet suivi");
    }

    #[tokio::test]
    async fn an_explicit_project_wins_over_the_tracked_task() {
        let f = Fixture::new();
        let asked = Uuid::new_v4();
        let tracked_project = Uuid::new_v4();
        let mut tracked = task("la tâche suivie", None);
        tracked.project_id = Some(tracked_project);
        let tracked_id = tracked.id;
        f.tasks.save(&tracked).await.expect("saved");
        *f.activity.active.lock().expect("lock") = Some(ActivitySlot {
            id: Uuid::new_v4(),
            user_id: uid(),
            task_id: Some(tracked_id),
            start_time: now(),
            end_time: None,
            half_day: HalfDay::Morning,
            date: today(),
            created_at: now(),
        });
        let mut asked_decision = memory(MemoryKind::Decision, "demandée", MemoryStatus::Active);
        asked_decision.project_id = Some(asked);
        f.memories.create(&asked_decision).await.expect("created");

        let brief = f
            .brief(BriefRequest {
                project_id: Some(asked),
                ..request()
            })
            .await;
        assert_eq!(brief.decisions.entries[0].title, "demandée");
    }

    #[tokio::test]
    async fn no_tracked_task_means_no_project_scope() {
        let f = Fixture::new();
        f.memories
            .create(&memory(
                MemoryKind::Decision,
                "sans projet",
                MemoryStatus::Active,
            ))
            .await
            .expect("created");
        let brief = f.brief(request()).await;
        assert!(!brief.decisions_scoped_to_project);
        assert_eq!(brief.decisions.total, 1);
    }

    #[tokio::test]
    async fn the_morning_variant_asks_for_the_morning_brief() {
        let f = Fixture::new();
        f.tasks
            .save(&task("demain", Some(today() + chrono::Duration::days(1))))
            .await
            .expect("saved");
        f.tasks
            .save(&task("aujourd'hui", Some(today())))
            .await
            .expect("saved");
        let brief = f
            .brief(BriefRequest {
                variant: BriefVariant::Morning,
                ..request()
            })
            .await;
        assert_eq!(brief.variant, BriefVariant::Morning);
        assert_eq!(brief.deadlines.total, 1);
        assert_eq!(brief.deadlines.entries[0].title, "aujourd'hui");
    }

    #[tokio::test]
    async fn a_superseded_memory_never_reaches_the_brief() {
        let f = Fixture::new();
        let mut superseded = memory(MemoryKind::Decision, "périmée", MemoryStatus::Active);
        superseded.invalidated_at = Some(now());
        superseded.superseded_by = Some(Uuid::new_v4());
        f.memories.create(&superseded).await.expect("created");
        let brief = f.brief(request()).await;
        assert!(brief.decisions.is_empty());
    }
}
