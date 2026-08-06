use chrono::{DateTime, Utc};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::SessionRepository;

/// A bind, and the task the session was on before it.
///
/// The previous task travels back to the caller instead of being flushed here: this
/// crate must not decide when billing-relevant time is materialized, and plan 2 is
/// what teaches the flush to be idempotent. Until then the CLI flushes it exactly as
/// `aplan start` does today, so behaviour is unchanged.
pub struct BindOutcome {
    pub session: Session,
    pub previous_task: Option<TaskId>,
}

/// Point a session at `task_id`, creating it if this is its first bind.
///
/// A bind is also a tracking decision: a session that was `off` and is now given a
/// task is tracking again, because the only way to get here is the user asking for it.
pub async fn bind_session(
    repo: &dyn SessionRepository,
    user_id: UserId,
    id: &str,
    task_id: TaskId,
    label: Option<String>,
    now: DateTime<Utc>,
) -> Result<BindOutcome, AppError> {
    match repo.find_by_id(id, user_id).await? {
        Some(mut existing) => {
            let previous_task = existing.task_id.filter(|prev| *prev != task_id);
            existing.task_id = Some(task_id);
            existing.mode = SessionMode::Tracking;
            // A bind is a request to work, and a session the user is binding is
            // alive by definition. Leaving `ended_at` set would make this call
            // report success while changing nothing — worse than refusing,
            // because `end`'s own guard then makes the id unrecoverable.
            existing.ended_at = None;
            existing.last_seen_at = now;
            if label.is_some() {
                existing.label = label;
            }
            repo.upsert(&existing).await?;
            Ok(BindOutcome {
                session: existing,
                previous_task,
            })
        }
        None => {
            let session = Session::tracking(id.to_string(), user_id, task_id, label, now)?;
            repo.upsert(&session).await?;
            Ok(BindOutcome {
                session,
                previous_task: None,
            })
        }
    }
}

/// Record what a session was told to do. `Off` also clears the task: a stale
/// `task_id` on an opted-out session is exactly the state that let a re-fired hook
/// claim to be tracking something the user had declined.
pub async fn set_session_mode(
    repo: &dyn SessionRepository,
    user_id: UserId,
    id: &str,
    mode: SessionMode,
    label: Option<String>,
    now: DateTime<Utc>,
) -> Result<Session, AppError> {
    let mut session = match repo.find_by_id(id, user_id).await? {
        Some(existing) => existing,
        None => Session::off(id.to_string(), user_id, label.clone(), now)?,
    };
    session.mode = mode;
    if mode == SessionMode::Off {
        session.task_id = None;
    }
    session.last_seen_at = now;
    if label.is_some() {
        session.label = label;
    }
    repo.upsert(&session).await?;
    Ok(session)
}

/// Close a session. `Ok(None)` means there was nothing open to close.
pub async fn end_session(
    repo: &dyn SessionRepository,
    user_id: UserId,
    id: &str,
    now: DateTime<Utc>,
) -> Result<Option<Session>, AppError> {
    if !repo.end(id, user_id, now).await? {
        return Ok(None);
    }
    Ok(repo.find_by_id(id, user_id).await?)
}

pub async fn list_open_sessions(
    repo: &dyn SessionRepository,
    user_id: UserId,
) -> Result<Vec<Session>, AppError> {
    Ok(repo.list_open(user_id).await?)
}

/// What this session logs against — or a refusal.
///
/// The refusal is the feature. Falling back to the human's pointer when a session
/// declines tracking is how a Claude ends up reporting work on a task the user
/// explicitly opted out of.
pub async fn resolve_session_target(
    repo: &dyn SessionRepository,
    user_id: UserId,
    id: &str,
    now: DateTime<Utc>,
) -> Result<TaskId, AppError> {
    let session = repo
        .find_by_id(id, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("session {id}")))?;

    let target = session.target().map_err(|refusal| match refusal {
        SessionTargetRefusal::NotTracked => AppError::Validation(format!(
            "session {id} is not tracked (aplan logging is off for it)"
        )),
        SessionTargetRefusal::NoTask => {
            AppError::Validation(format!("session {id} has no task bound"))
        }
        SessionTargetRefusal::Ended => {
            AppError::Validation(format!("session {id} has ended"))
        }
    })?;

    repo.touch(id, user_id, now).await?;
    Ok(target)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;

    /// Reused as-is by `session_reaper`'s test module: one more copy would make a
    /// fifth implementor of a trait the reaper's brief counts at exactly four.
    #[derive(Default)]
    pub(crate) struct InMemorySessionRepository {
        rows: Mutex<Vec<Session>>,
    }

    #[async_trait]
    impl SessionRepository for InMemorySessionRepository {
        async fn find_by_id(
            &self,
            id: &str,
            user_id: UserId,
        ) -> Result<Option<Session>, RepositoryError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.id == id && s.user_id == user_id)
                .cloned())
        }

        async fn upsert(&self, session: &Session) -> Result<(), RepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            match rows.iter_mut().find(|s| s.id == session.id) {
                Some(existing) => {
                    existing.task_id = session.task_id;
                    existing.mode = session.mode;
                    existing.label = session.label.clone();
                    existing.last_seen_at = session.last_seen_at;
                    existing.ended_at = session.ended_at;
                }
                None => rows.push(session.clone()),
            }
            Ok(())
        }

        async fn list_open(&self, user_id: UserId) -> Result<Vec<Session>, RepositoryError> {
            let mut open: Vec<Session> = self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.user_id == user_id && s.is_open())
                .cloned()
                .collect();
            open.sort_by(|a, b| b.last_seen_at.cmp(&a.last_seen_at));
            Ok(open)
        }

        async fn list_idle_open(
            &self,
            user_id: UserId,
            idle_before: DateTime<Utc>,
        ) -> Result<Vec<Session>, RepositoryError> {
            let mut idle: Vec<Session> = self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.user_id == user_id && s.is_open() && s.last_seen_at < idle_before)
                .cloned()
                .collect();
            idle.sort_by(|a, b| a.last_seen_at.cmp(&b.last_seen_at));
            Ok(idle)
        }

        async fn touch(
            &self,
            id: &str,
            user_id: UserId,
            at: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            match rows
                .iter_mut()
                .find(|s| s.id == id && s.user_id == user_id && s.is_open())
            {
                Some(s) => {
                    s.last_seen_at = at;
                    Ok(true)
                }
                None => Ok(false),
            }
        }

        async fn set_last_flush(
            &self,
            id: &str,
            user_id: UserId,
            at: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            match rows.iter_mut().find(|s| s.id == id && s.user_id == user_id) {
                Some(s) => {
                    s.last_flush_at = Some(at);
                    Ok(true)
                }
                None => Ok(false),
            }
        }

        async fn end(
            &self,
            id: &str,
            user_id: UserId,
            at: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            match rows.iter_mut().find(|s| s.id == id && s.user_id == user_id) {
                Some(s) if s.is_open() => {
                    s.ended_at = Some(at);
                    Ok(true)
                }
                _ => Ok(false),
            }
        }
    }

    fn uid() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }
    fn t(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, h, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn binding_an_unknown_session_creates_it_tracking() {
        let repo = InMemorySessionRepository::default();
        let task = Uuid::new_v4();

        let out = bind_session(&repo, uid(), "s1", task, Some("/home/mbt/x".into()), t(9))
            .await
            .unwrap();

        assert_eq!(out.session.task_id, Some(task));
        assert_eq!(out.session.mode, SessionMode::Tracking);
        assert!(out.previous_task.is_none(), "nothing to flush on a first bind");
    }

    #[tokio::test]
    async fn rebinding_reports_the_task_to_flush_and_keeps_started_at() {
        let repo = InMemorySessionRepository::default();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        bind_session(&repo, uid(), "s1", first, None, t(9)).await.unwrap();

        let out = bind_session(&repo, uid(), "s1", second, None, t(11)).await.unwrap();

        assert_eq!(out.session.task_id, Some(second));
        assert_eq!(
            out.previous_task,
            Some(first),
            "the caller has to flush what the session was on"
        );
        assert_eq!(
            out.session.started_at,
            t(9),
            "a rebind is the same session; plan 2 anchors its window here"
        );
        assert_eq!(out.session.last_seen_at, t(11));
    }

    #[tokio::test]
    async fn rebinding_to_the_same_task_reports_nothing_to_flush() {
        let repo = InMemorySessionRepository::default();
        let task = Uuid::new_v4();
        bind_session(&repo, uid(), "s1", task, None, t(9)).await.unwrap();

        let out = bind_session(&repo, uid(), "s1", task, None, t(11)).await.unwrap();

        assert!(out.previous_task.is_none());
    }

    #[tokio::test]
    async fn binding_revives_a_session_that_was_off() {
        // The user answered "ne pas tracker", then changed their mind mid-session.
        let repo = InMemorySessionRepository::default();
        set_session_mode(&repo, uid(), "s1", SessionMode::Off, None, t(9))
            .await
            .unwrap();

        let task = Uuid::new_v4();
        let out = bind_session(&repo, uid(), "s1", task, None, t(10)).await.unwrap();

        assert_eq!(out.session.mode, SessionMode::Tracking);
        assert_eq!(out.session.target(), Ok(task));
    }

    #[tokio::test]
    async fn binding_revives_a_session_that_has_ended() {
        // The bug this pins: `session end` then `session bind` reported success
        // and changed nothing, because the existing-row arm never touched
        // `ended_at`. That poisons the id forever, since `end`'s own guard
        // requires an open row to close. The only way to reach a bind is the
        // user asking to work, so reviving is what a bind means — same
        // reasoning as `binding_revives_a_session_that_was_off` above, just for
        // `ended_at` instead of `mode`.
        let repo = InMemorySessionRepository::default();
        bind_session(&repo, uid(), "s1", Uuid::new_v4(), None, t(9))
            .await
            .unwrap();
        end_session(&repo, uid(), "s1", t(12)).await.unwrap();

        let task = Uuid::new_v4();
        let out = bind_session(&repo, uid(), "s1", task, None, t(14)).await.unwrap();

        assert!(out.session.ended_at.is_none(), "a bind must reopen the session");
        assert_eq!(out.session.mode, SessionMode::Tracking);
        assert_eq!(out.session.target(), Ok(task));
    }

    #[tokio::test]
    async fn setting_a_session_off_clears_its_task() {
        // Leaving a stale task_id behind would let any later code path resolve a
        // target for a session the user opted out of.
        let repo = InMemorySessionRepository::default();
        bind_session(&repo, uid(), "s1", Uuid::new_v4(), None, t(9))
            .await
            .unwrap();

        let s = set_session_mode(&repo, uid(), "s1", SessionMode::Off, None, t(10))
            .await
            .unwrap();

        assert_eq!(s.mode, SessionMode::Off);
        assert!(s.task_id.is_none());
        assert_eq!(s.target(), Err(SessionTargetRefusal::NotTracked));
    }

    #[tokio::test]
    async fn resolving_a_target_refuses_an_off_session_instead_of_falling_back() {
        let repo = InMemorySessionRepository::default();
        set_session_mode(&repo, uid(), "s1", SessionMode::Off, None, t(9))
            .await
            .unwrap();

        let err = resolve_session_target(&repo, uid(), "s1", t(10)).await.unwrap_err();

        assert!(
            matches!(err, AppError::Validation(ref m) if m.contains("not tracked")),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn resolving_a_target_reports_an_unknown_session_as_not_found() {
        let repo = InMemorySessionRepository::default();
        let err = resolve_session_target(&repo, uid(), "ghost", t(10)).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn resolving_a_target_bumps_last_seen() {
        // last_seen_at is what the idle reaper reads. A session that logs is a
        // session that is alive, so resolution is the natural heartbeat.
        let repo = InMemorySessionRepository::default();
        let task = Uuid::new_v4();
        bind_session(&repo, uid(), "s1", task, None, t(9)).await.unwrap();

        let resolved = resolve_session_target(&repo, uid(), "s1", t(15)).await.unwrap();

        assert_eq!(resolved, task);
        let after = repo.find_by_id("s1", uid()).await.unwrap().unwrap();
        assert_eq!(after.last_seen_at, t(15));
    }

    #[tokio::test]
    async fn ending_is_idempotent_and_keeps_the_first_instant() {
        let repo = InMemorySessionRepository::default();
        bind_session(&repo, uid(), "s1", Uuid::new_v4(), None, t(9))
            .await
            .unwrap();

        let first = end_session(&repo, uid(), "s1", t(17)).await.unwrap();
        assert_eq!(first.unwrap().ended_at, Some(t(17)));

        let second = end_session(&repo, uid(), "s1", t(19)).await.unwrap();
        assert!(second.is_none(), "a second end is a no-op");
        let row = repo.find_by_id("s1", uid()).await.unwrap().unwrap();
        assert_eq!(row.ended_at, Some(t(17)));
    }

    #[tokio::test]
    async fn listing_shows_only_open_sessions_most_recent_first() {
        let repo = InMemorySessionRepository::default();
        bind_session(&repo, uid(), "s1", Uuid::new_v4(), None, t(9)).await.unwrap();
        bind_session(&repo, uid(), "s2", Uuid::new_v4(), None, t(11)).await.unwrap();
        bind_session(&repo, uid(), "s3", Uuid::new_v4(), None, t(10)).await.unwrap();
        end_session(&repo, uid(), "s3", t(12)).await.unwrap();

        let open = list_open_sessions(&repo, uid()).await.unwrap();

        let ids: Vec<&str> = open.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["s2", "s1"]);
    }
}
