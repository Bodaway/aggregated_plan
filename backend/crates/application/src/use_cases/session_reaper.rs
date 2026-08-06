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
/// A failure on one session — flushing it, advancing its watermark, or closing it — is
/// logged here and skipped, never propagated: the reaper runs unattended, and one
/// wedged session must not stop every later one from being flushed for the rest of the
/// day.
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
                Ok(outcome) => {
                    slots_written += outcome.slots_written;
                    // Advance the watermark before attempting to close. `end` can
                    // still fail below, and once its failure no longer aborts the
                    // pass, a session whose close keeps failing must not have its
                    // whole history re-selected and rebuilt on every later tick.
                    if let Err(e) = session_repo
                        .set_last_flush(&session.id, user_id, outcome.active_since)
                        .await
                    {
                        tracing::warn!(
                            session = %session.id,
                            "reaper could not advance the flush watermark, leaving it open: {e}"
                        );
                        continue;
                    }
                }
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
        match session_repo.end(&session.id, user_id, now).await {
            Ok(true) => reaped += 1,
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(
                    session = %session.id,
                    "reaper could not close an idle session after flushing it: {e}"
                );
                // The flush (and its watermark) already succeeded, so nothing is
                // lost: the session stays open and a later pass gets another
                // chance to close it.
                continue;
            }
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
    ///
    /// No `#[derive(Default)]`: a default-constructed instance would have no
    /// `failing_task` to fail on, so any caller reaching for `::default()` by
    /// habit would get a repository that never fails — silently defeating the one
    /// thing this type exists for. Constructed only through `::new`.
    struct FailingWorklogRepo {
        inner: FakeRepo,
        failing_task: TaskId,
    }

    impl FailingWorklogRepo {
        fn new(failing_task: TaskId) -> Self {
            Self {
                inner: FakeRepo::default(),
                failing_task,
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
            if filter.task_ids.as_deref() == Some(std::slice::from_ref(&self.failing_task)) {
                return Err(RepositoryError::Database(
                    "wedged: this task's worklog cannot be read".into(),
                ));
            }
            self.inner.list(user_id, filter).await
        }
    }

    /// Delegates every `SessionRepository` method to a real in-memory store except
    /// `end`, which always fails. The failure-injection point Important 2 and the
    /// watermark ruling both need: a session whose close never succeeds must still
    /// let a later session flush, and must not have its own window rebuilt again on
    /// a second pass. Mirrors `api/graphql/tests.rs`'s `FailingTouchSessionRepository`
    /// — same shape, different verb.
    struct FailingEndSessionRepository(InMemorySessionRepository);

    #[async_trait]
    impl SessionRepository for FailingEndSessionRepository {
        async fn find_by_id(
            &self,
            id: &str,
            user_id: UserId,
        ) -> Result<Option<Session>, RepositoryError> {
            self.0.find_by_id(id, user_id).await
        }
        async fn upsert(&self, session: &Session) -> Result<(), RepositoryError> {
            self.0.upsert(session).await
        }
        async fn list_open(&self, user_id: UserId) -> Result<Vec<Session>, RepositoryError> {
            self.0.list_open(user_id).await
        }
        async fn list_idle_open(
            &self,
            user_id: UserId,
            idle_before: DateTime<Utc>,
        ) -> Result<Vec<Session>, RepositoryError> {
            self.0.list_idle_open(user_id, idle_before).await
        }
        async fn touch(
            &self,
            id: &str,
            user_id: UserId,
            at: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            self.0.touch(id, user_id, at).await
        }
        async fn set_last_flush(
            &self,
            id: &str,
            user_id: UserId,
            at: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            self.0.set_last_flush(id, user_id, at).await
        }
        async fn end(
            &self,
            _id: &str,
            _user_id: UserId,
            _at: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            Err(RepositoryError::Database(
                "end always fails in this test double".to_string(),
            ))
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
    ///
    /// `wedged` is seen *before* `healthy` (`t(2)` vs `t(3)`), so `list_idle_open`'s
    /// oldest-first order processes `wedged` first. That ordering is what makes this
    /// fixture actually test resilience: with `healthy` seen first instead, `reaped
    /// == 1` would hold identically whether a later failure aborted the pass or not,
    /// because there would be no session left after it to fail to reach.
    async fn fakes_with_two_idle_one_failing(
    ) -> (InMemorySessionRepository, FailingWorklogRepo, FakeActivityRepo, FakeConfigRepo) {
        let session_repo = InMemorySessionRepository::default();
        session_repo
            .upsert(
                &Session::tracking("wedged".into(), user_id(), other_task_id(), None, t(2))
                    .unwrap(),
            )
            .await
            .unwrap();
        session_repo
            .upsert(
                &Session::tracking("healthy".into(), user_id(), task_id(), None, t(3)).unwrap(),
            )
            .await
            .unwrap();

        let worklog = FailingWorklogRepo::new(other_task_id());
        let entry = WorklogEntry::new(user_id(), task_id(), "did work".into(), t(9), t(9)).unwrap();
        worklog.create(&entry).await.unwrap();

        (session_repo, worklog, FakeActivityRepo::default(), FakeConfigRepo::default())
    }

    /// Two idle sessions on two tasks; the session store's `end` always fails.
    async fn fakes_with_two_idle_sessions_and_failing_end(
    ) -> (FailingEndSessionRepository, FakeRepo, FakeActivityRepo, FakeConfigRepo) {
        let inner = InMemorySessionRepository::default();
        inner
            .upsert(&Session::tracking("first".into(), user_id(), task_id(), None, t(2)).unwrap())
            .await
            .unwrap();
        inner
            .upsert(
                &Session::tracking("second".into(), user_id(), other_task_id(), None, t(3))
                    .unwrap(),
            )
            .await
            .unwrap();
        let session_repo = FailingEndSessionRepository(inner);

        let worklog = FakeRepo::default();
        worklog
            .create(&WorklogEntry::new(user_id(), task_id(), "a".into(), t(5), t(5)).unwrap())
            .await
            .unwrap();
        worklog
            .create(&WorklogEntry::new(user_id(), other_task_id(), "b".into(), t(6), t(6)).unwrap())
            .await
            .unwrap();

        (session_repo, worklog, FakeActivityRepo::default(), FakeConfigRepo::default())
    }

    /// One idle session with an entry, on a session store whose `end` always fails.
    async fn fakes_with_idle_session_and_failing_end(
    ) -> (FailingEndSessionRepository, FakeRepo, FakeActivityRepo, FakeConfigRepo) {
        let inner = InMemorySessionRepository::default();
        inner
            .upsert(
                &Session::tracking("wedged-close".into(), user_id(), task_id(), None, t(2))
                    .unwrap(),
            )
            .await
            .unwrap();
        let session_repo = FailingEndSessionRepository(inner);

        let worklog = FakeRepo::default();
        worklog
            .create(&WorklogEntry::new(user_id(), task_id(), "did work".into(), t(5), t(5)).unwrap())
            .await
            .unwrap();

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
        assert!(
            worklog.list_calls.lock().unwrap().is_empty(),
            "a taskless session must never even attempt a flush"
        );
    }

    #[tokio::test]
    async fn one_sessions_flush_failure_does_not_block_the_others() {
        // The reaper runs unattended. `wedged` is seen before `healthy`, so if a
        // flush failure aborted the pass instead of being skipped, `healthy` would
        // never be reached at all — `reaped == 1` alone cannot tell the two apart,
        // since there would be nothing left to reach either way. Asserting the flush
        // actually happened, and that `wedged` was left open rather than lost, is
        // what makes this test able to fail against a `break`.
        let (session_repo, worklog, activity, config) = fakes_with_two_idle_one_failing().await;

        let outcome = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();

        assert_eq!(outcome.reaped, 1, "the healthy session was still closed");
        assert!(
            outcome.slots_written >= 1,
            "healthy, processed after the failure, was still flushed"
        );
        let wedged = session_repo.find_by_id("wedged", user_id()).await.unwrap().unwrap();
        assert!(
            wedged.ended_at.is_none(),
            "the wedged session stays open for a later retry, not lost"
        );
    }

    #[tokio::test]
    async fn a_failing_close_does_not_block_a_later_sessions_flush() {
        // `first`'s `end` fails, but `second` must still be reached and flushed —
        // the same resilience property as a flush failure, one step later in the
        // pipeline.
        let (session_repo, worklog, activity, config) =
            fakes_with_two_idle_sessions_and_failing_end().await;

        let outcome = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();

        assert_eq!(outcome.reaped, 0, "`end` always fails in this double");
        assert_eq!(
            outcome.slots_written, 2,
            "both sessions were flushed despite `end` failing on the first"
        );
    }

    #[tokio::test]
    async fn a_failing_close_still_advances_the_watermark_so_a_second_pass_finds_nothing_new() {
        // Without the watermark advancing before `end` is attempted, a session
        // whose close keeps failing would have its entire history re-selected and
        // rebuilt on every tick, forever. The second pass here must find nothing
        // left to materialize, proving the window moved even though the row never
        // actually closed.
        let (session_repo, worklog, activity, config) =
            fakes_with_idle_session_and_failing_end().await;

        let first = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();
        assert_eq!(first.slots_written, 1, "the entry is materialized on the first pass");

        let second = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(20),
        )
        .await
        .unwrap();

        assert_eq!(
            second.slots_written, 0,
            "the watermark moved past the entry, so the same window is not rebuilt again"
        );
    }
}
