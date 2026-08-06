use chrono::{DateTime, Utc};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::{
    ActivitySlotRepository, ConfigRepository, SessionRepository, WorklogRepository,
};
use crate::use_cases::worklog::materialize_worklog_time;

/// What one reaping pass did.
pub struct ReapOutcome {
    pub reaped: u32,
    pub slots_written: u32,
}

/// Close every session that has gone quiet, materializing its time first.
///
/// The order matters and is the whole point: flush, then close. Closing first would
/// leave the session's entries with no window that will ever select them again — the
/// row is closed, so no later `aplan log` can revive it — and the time would be lost.
///
/// A failure on one session is logged by the caller and skipped, never propagated: the
/// reaper runs unattended, and one wedged session must not stop every later one from
/// being flushed for the rest of the day.
pub async fn reap_idle_sessions(
    session_repo: &dyn SessionRepository,
    worklog_repo: &dyn WorklogRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    idle_before: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<ReapOutcome, AppError> {
    let idle = session_repo.list_idle_open(user_id, idle_before).await?;
    let mut reaped = 0u32;
    let mut slots_written = 0u32;

    for session in idle {
        // A session with no task has nothing to materialize — `mode = off`, or a bind
        // that never happened. Closing it is still right.
        if let Some(task_id) = session.task_id {
            match materialize_worklog_time(
                worklog_repo,
                activity_repo,
                config_repo,
                user_id,
                task_id,
                session.flush_window_start(),
                now,
            )
            .await
            {
                Ok(outcome) => slots_written += outcome.slots_written,
                Err(e) => {
                    tracing::warn!(
                        session = %session.id,
                        "reaper could not flush an idle session, leaving it open: {e}"
                    );
                    // Leave it open on purpose: an unflushed session that stays open can
                    // be flushed by the next pass, whereas one closed without its flush
                    // has lost its time for good.
                    continue;
                }
            }
        }
        if session_repo.end(&session.id, user_id, now).await? {
            reaped += 1;
        }
    }

    Ok(ReapOutcome { reaped, slots_written })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use uuid::Uuid;

    use crate::errors::RepositoryError;
    use crate::repositories::WorklogFilter;
    use crate::use_cases::session_tracking::tests::InMemorySessionRepository;
    use crate::use_cases::worklog::tests::{FakeActivityRepo, FakeConfigRepo, FakeRepo};
    use domain::types::WorklogEntry;

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }
    fn task_id() -> TaskId {
        Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").unwrap()
    }
    fn other_task_id() -> TaskId {
        Uuid::parse_str("00000000-0000-0000-0000-0000000000bb").unwrap()
    }
    fn t(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, h, 0, 0).unwrap()
    }

    /// Delegates every `WorklogRepository` method to a real fake except `list`,
    /// which errors for one named task. None of the existing fakes can express a
    /// failure — `FakeRepo::list` always succeeds — and this is the one situation
    /// the fourth test needs: one session's flush failing without touching another.
    #[derive(Default)]
    struct FailingWorklogRepo {
        inner: FakeRepo,
        failing_task: Option<TaskId>,
    }

    impl FailingWorklogRepo {
        fn new(failing_task: TaskId) -> Self {
            Self {
                inner: FakeRepo::default(),
                failing_task: Some(failing_task),
            }
        }
    }

    #[async_trait]
    impl WorklogRepository for FailingWorklogRepo {
        async fn create(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
            self.inner.create(entry).await
        }
        async fn update(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
            self.inner.update(entry).await
        }
        async fn delete(
            &self,
            id: domain::types::WorklogEntryId,
            user_id: UserId,
        ) -> Result<bool, RepositoryError> {
            self.inner.delete(id, user_id).await
        }
        async fn find_by_id(
            &self,
            id: domain::types::WorklogEntryId,
            user_id: UserId,
        ) -> Result<Option<WorklogEntry>, RepositoryError> {
            self.inner.find_by_id(id, user_id).await
        }
        async fn find_by_recurrence(
            &self,
            user_id: UserId,
            template_id: domain::types::recurrence::RecurrenceTemplateId,
            limit: u32,
            offset: u32,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            self.inner
                .find_by_recurrence(user_id, template_id, limit, offset)
                .await
        }
        async fn list(
            &self,
            user_id: UserId,
            filter: &WorklogFilter,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            if filter.task_ids.as_deref() == Some(std::slice::from_ref(
                self.failing_task.as_ref().expect("set by FailingWorklogRepo::new"),
            )) {
                return Err(RepositoryError::Database(
                    "wedged: this task's worklog cannot be read".into(),
                ));
            }
            self.inner.list(user_id, filter).await
        }
    }

    /// An idle session with an entry inside its flush window.
    async fn fakes_with_idle_session(
    ) -> (InMemorySessionRepository, FakeRepo, FakeActivityRepo, FakeConfigRepo) {
        let session_repo = InMemorySessionRepository::default();
        session_repo
            .upsert(&Session::tracking("stale".into(), user_id(), task_id(), None, t(2)).unwrap())
            .await
            .unwrap();

        let worklog = FakeRepo::default();
        let entry = WorklogEntry::new(user_id(), task_id(), "did work".into(), t(9), t(9)).unwrap();
        worklog.create(&entry).await.unwrap();

        (session_repo, worklog, FakeActivityRepo::default(), FakeConfigRepo::default())
    }

    /// A session seen after `idle_before` — not the reaper's business.
    async fn fakes_with_fresh_session(
    ) -> (InMemorySessionRepository, FakeRepo, FakeActivityRepo, FakeConfigRepo) {
        let session_repo = InMemorySessionRepository::default();
        session_repo
            .upsert(&Session::tracking("fresh".into(), user_id(), task_id(), None, t(11)).unwrap())
            .await
            .unwrap();
        (session_repo, FakeRepo::default(), FakeActivityRepo::default(), FakeConfigRepo::default())
    }

    /// An idle session that never bound a task — `mode = off`.
    async fn fakes_with_idle_untracked_session(
    ) -> (InMemorySessionRepository, FakeRepo, FakeActivityRepo, FakeConfigRepo) {
        let session_repo = InMemorySessionRepository::default();
        session_repo
            .upsert(&Session::off("untracked".into(), user_id(), None, t(2)).unwrap())
            .await
            .unwrap();
        (session_repo, FakeRepo::default(), FakeActivityRepo::default(), FakeConfigRepo::default())
    }

    /// Two idle sessions on two tasks; one task's worklog read is wedged.
    async fn fakes_with_two_idle_one_failing(
    ) -> (InMemorySessionRepository, FailingWorklogRepo, FakeActivityRepo, FakeConfigRepo) {
        let session_repo = InMemorySessionRepository::default();
        session_repo
            .upsert(
                &Session::tracking("healthy".into(), user_id(), task_id(), None, t(2)).unwrap(),
            )
            .await
            .unwrap();
        session_repo
            .upsert(
                &Session::tracking("wedged".into(), user_id(), other_task_id(), None, t(3))
                    .unwrap(),
            )
            .await
            .unwrap();

        let worklog = FailingWorklogRepo::new(other_task_id());
        (session_repo, worklog, FakeActivityRepo::default(), FakeConfigRepo::default())
    }

    #[tokio::test]
    async fn reaping_flushes_the_sessions_own_task_and_closes_it() {
        // An idle session with an entry in its window: the reaper must materialize
        // that time before closing, or it is lost the moment the row is closed.
        let (session_repo, worklog, activity, config) = fakes_with_idle_session().await;

        let outcome = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();

        assert_eq!(outcome.reaped, 1);
        assert!(outcome.slots_written >= 1, "the idle session's time was materialized");
        let row = session_repo.find_by_id("stale", user_id()).await.unwrap().unwrap();
        assert!(row.ended_at.is_some(), "the session is closed");
    }

    #[tokio::test]
    async fn reaping_leaves_a_fresh_session_alone() {
        let (session_repo, worklog, activity, config) = fakes_with_fresh_session().await;

        let outcome = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();

        assert_eq!(outcome.reaped, 0);
        let row = session_repo.find_by_id("fresh", user_id()).await.unwrap().unwrap();
        assert!(row.ended_at.is_none());
    }

    #[tokio::test]
    async fn a_session_with_no_task_is_closed_without_flushing() {
        // `mode = off`, or a bind that never happened: there is nothing to materialize,
        // and asking the flush for a `None` task would be a bug, not a no-op.
        let (session_repo, worklog, activity, config) = fakes_with_idle_untracked_session().await;

        let outcome = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();

        assert_eq!(outcome.reaped, 1);
        assert_eq!(outcome.slots_written, 0);
    }

    #[tokio::test]
    async fn one_sessions_flush_failure_does_not_block_the_others() {
        // The reaper runs unattended. If it aborted on the first failure, one wedged
        // session would keep every later one from ever being flushed.
        let (session_repo, worklog, activity, config) = fakes_with_two_idle_one_failing().await;

        let outcome = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();

        assert_eq!(outcome.reaped, 1, "the healthy session was still closed");
    }
}
