use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};

use application::errors::AppError;
use application::jobs::{
    humanize_duration, AttemptOutcome, JobHealth, LogEntry, LogLevel, RetryPolicy,
};
use application::repositories::{
    ActivitySlotRepository, AlertRepository, ConfigRepository, GryzzlyCatalogRepository,
    MeetingRepository, SessionRepository, SignalMappingRepository, TaskRepository,
    TimesheetDraftRepository, WorklogRepository,
};
use application::services::git_connector::GitConnector;
use application::use_cases::session_reaper::{reap_idle_sessions, ReapOutcome};
use application::use_cases::timesheet::run_eod_pass;
use domain::types::UserId;

/// Dependencies the end-of-day scheduler needs (Arc clones of the app's repos).
pub struct EodDeps {
    pub worklog_repo: Arc<dyn WorklogRepository>,
    pub meeting_repo: Arc<dyn MeetingRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub catalog_repo: Arc<dyn GryzzlyCatalogRepository>,
    pub mapping_repo: Arc<dyn SignalMappingRepository>,
    pub config_repo: Arc<dyn ConfigRepository>,
    pub git: Arc<dyn GitConnector>,
    pub draft_repo: Arc<dyn TimesheetDraftRepository>,
    pub alert_repo: Arc<dyn AlertRepository>,
}

/// Long-lived background task: run one end-of-day pass for `user_id`, then wait as long
/// as `application::jobs::RetryPolicy` says to before the next one — 5 minutes while
/// healthy, backing off to 30 minutes while it is not. Errors are logged, never fatal.
/// Idempotent via the last_auto_run watermark, which is what actually decides when work
/// is due; the tick only decides how often we ask.
///
/// This function owns exactly two things the policy cannot: the sleeping and the
/// `tracing` calls.
pub async fn run_eod_scheduler(deps: EodDeps, user_id: UserId) {
    let policy = RetryPolicy::end_of_day();
    let mut health = JobHealth::default();
    loop {
        let attempt = run_eod_pass(
            deps.worklog_repo.as_ref(),
            deps.meeting_repo.as_ref(),
            deps.task_repo.as_ref(),
            deps.catalog_repo.as_ref(),
            deps.mapping_repo.as_ref(),
            deps.config_repo.as_ref(),
            deps.git.as_ref(),
            deps.draft_repo.as_ref(),
            deps.alert_repo.as_ref(),
            user_id,
            Utc::now(),
        )
        .await;

        // A pass that kept its work but had to tolerate a broken step is not a success:
        // whatever it tolerated is still unfixed, so it must feed the back-off too.
        let failure = match &attempt {
            Ok(outcome) if outcome.degraded.is_empty() => None,
            Ok(outcome) => Some(outcome.degradation_signature()),
            Err(e) => Some(e.to_string()),
        };
        if let Ok(outcome) = &attempt {
            if !outcome.processed.is_empty() {
                tracing::info!(
                    dates = ?outcome.processed,
                    "end-of-day timesheet reconstruction completed"
                );
            }
        }

        let observed = match &failure {
            Some(signature) => AttemptOutcome::Failed { signature },
            None => AttemptOutcome::Succeeded,
        };
        let (next_health, decision) = health.observe(observed, Utc::now(), &policy);
        health = next_health;
        report("end-of-day timesheet reconstruction", decision.log, failure.as_deref(), decision.retry_in);

        tokio::time::sleep(decision.retry_in).await;
    }
}

/// Turn the policy's verdict into a journal line for `job`. The policy already
/// decided whether to speak and how loudly; this only decides the wording. Shared
/// between both scheduler loops -- the three message shapes below reproduce each
/// job's previous, job-specific wording byte-for-byte, so extracting this changed
/// no log output, only where it is written.
fn report(job: &str, entry: Option<LogEntry>, error: Option<&str>, retry_in: Duration) {
    let Some(entry) = entry else {
        return;
    };
    match entry {
        LogEntry::Failure { level, consecutive_failures, failing_for, suppressed_repeats } => {
            let error = error.unwrap_or("unknown");
            let failing_for = humanize_duration(failing_for);
            match level {
                LogLevel::Warn => tracing::warn!(
                    error,
                    consecutive_failures,
                    failing_for = %failing_for,
                    retry_in_s = retry_in.as_secs(),
                    "{job} failed"
                ),
                // The line a three-week-old outage needs: not "failed" for the
                // thousandth time, but for how long and how many times.
                LogLevel::Error => tracing::error!(
                    error,
                    consecutive_failures,
                    failing_for = %failing_for,
                    suppressed_repeats,
                    retry_in_s = retry_in.as_secs(),
                    "{job} has been failing for {failing_for} \
                     ({consecutive_failures} consecutive attempts) -- it will not fix itself"
                ),
            }
        }
        LogEntry::Recovered { after_failures, was_failing_for } => tracing::info!(
            after_failures,
            was_failing_for = %humanize_duration(was_failing_for),
            "{job} recovered"
        ),
    }
}

/// Dependencies the idle-session reaper needs (Arc clones of the app's repos).
///
/// Separate from `EodDeps` on purpose: the end-of-day pass never reads
/// `session_repo` or `activity_repo`, and widening `EodDeps` with fields it never
/// touches would blur what each job actually depends on.
pub struct SessionReaperDeps {
    pub session_repo: Arc<dyn SessionRepository>,
    pub worklog_repo: Arc<dyn WorklogRepository>,
    pub activity_repo: Arc<dyn ActivitySlotRepository>,
    pub config_repo: Arc<dyn ConfigRepository>,
}

/// Configuration key for how many hours a session may sit quiet before the reaper
/// closes it.
const SESSION_IDLE_TIMEOUT_KEY: &str = "aplan.session_idle_timeout_hours";

/// Fallback used when the key above is unset, holds a value that fails to parse,
/// or holds a value outside `SESSION_IDLE_TIMEOUT_RANGE`. A corrupt value must not
/// make the reaper close every open session on the next tick, so a bad read falls
/// back here rather than erroring.
const DEFAULT_SESSION_IDLE_TIMEOUT_HOURS: i64 = 12;

/// Sane bounds for a parsed threshold, applied after parsing so an out-of-range
/// value is treated the same as an unparseable one. Below `1`: `0` or a negative
/// value parses fine, so the earlier "unparseable" fallback never sees it, and
/// `idle_before = now - hours` would land at or after `now` -- `list_idle_open`
/// would then return every open session, including the one asking right now.
/// Above `8760` (one year): still a valid `i64`, but `chrono::Duration::hours`
/// panics once `hours * 3_600_000` overflows an `i64` millisecond count -- past
/// roughly 2.56e12 hours (`i64::MAX / 1_000 / 3_600`, verified against chrono
/// 0.4.44's `TimeDelta::try_hours`/`try_seconds`) -- and that panic would kill this
/// job for the rest of the process, silently, since it runs inside a dropped
/// `tokio::spawn` handle.
const SESSION_IDLE_TIMEOUT_RANGE: std::ops::RangeInclusive<i64> = 1..=8760;

/// How many hours a session may go quiet before the reaper closes it, read fresh
/// every pass so a live `updateConfiguration` takes effect on the next tick
/// without a restart.
async fn idle_timeout_hours(
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
) -> Result<i64, AppError> {
    let hours = config_repo
        .get(user_id, SESSION_IDLE_TIMEOUT_KEY)
        .await?
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|hours| SESSION_IDLE_TIMEOUT_RANGE.contains(hours))
        .unwrap_or(DEFAULT_SESSION_IDLE_TIMEOUT_HOURS);
    Ok(hours)
}

/// One reap pass: turn the configured threshold into a cutoff, then hand off to
/// `reap_idle_sessions`. Kept apart from the loop below so the scheduler reads
/// like `run_eod_scheduler`'s: one `attempt`, one policy decision, one sleep.
async fn run_reap_pass(
    deps: &SessionReaperDeps,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<ReapOutcome, AppError> {
    let hours = idle_timeout_hours(deps.config_repo.as_ref(), user_id).await?;
    let idle_before = now - chrono::Duration::hours(hours);
    reap_idle_sessions(
        deps.session_repo.as_ref(),
        deps.worklog_repo.as_ref(),
        deps.activity_repo.as_ref(),
        deps.config_repo.as_ref(),
        user_id,
        idle_before,
        now,
    )
    .await
}

/// Long-lived background task: close every session that has gone quiet longer
/// than the configured idle threshold, then wait as long as
/// `RetryPolicy::session_reaper()` says to before the next pass.
///
/// Deliberately its own loop with its own `JobHealth`, not folded into
/// `run_eod_scheduler`'s: that job's back-off exists to stop hammering a broken
/// git/Gryzzly integration, and a reaper failure has nothing to do with either, so
/// it must never feed that signal. A pass's error is logged and the loop keeps
/// running -- the same non-propagating shape `run_eod_scheduler` already uses --
/// so neither job's schedule can be aborted by the other's failure.
pub async fn run_session_reaper_scheduler(deps: SessionReaperDeps, user_id: UserId) {
    let policy = RetryPolicy::session_reaper();
    let mut health = JobHealth::default();
    loop {
        let attempt = run_reap_pass(&deps, user_id, Utc::now()).await;

        if let Ok(outcome) = &attempt {
            if outcome.reaped > 0 || outcome.slots_written > 0 {
                tracing::info!(
                    reaped = outcome.reaped,
                    slots_written = outcome.slots_written,
                    "idle-session reap completed"
                );
            }
        }

        let failure = attempt.as_ref().err().map(ToString::to_string);
        let observed = match &failure {
            Some(signature) => AttemptOutcome::Failed { signature },
            None => AttemptOutcome::Succeeded,
        };
        let (next_health, decision) = health.observe(observed, Utc::now(), &policy);
        health = next_health;
        report("idle-session reap", decision.log, failure.as_deref(), decision.retry_in);

        tokio::time::sleep(decision.retry_in).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use application::errors::RepositoryError;
    use async_trait::async_trait;
    use uuid::Uuid;

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    /// A `ConfigRepository` double that actually stores what it is given, keyed by
    /// `(user_id, key)`. Plan 2 found a stub that discarded every `set` and always
    /// read back empty, which made assertions like these three unobservable; this
    /// one's `get` reads from the same map `set` writes into, not a constant, so a
    /// regression back to that shape would show up as these tests failing.
    #[derive(Default)]
    struct StubConfigRepository {
        values: Mutex<HashMap<(UserId, String), String>>,
    }

    #[async_trait]
    impl ConfigRepository for StubConfigRepository {
        async fn get(
            &self,
            user_id: UserId,
            key: &str,
        ) -> Result<Option<String>, RepositoryError> {
            Ok(self.values.lock().unwrap().get(&(user_id, key.to_string())).cloned())
        }
        async fn set(
            &self,
            user_id: UserId,
            key: &str,
            value: &str,
        ) -> Result<(), RepositoryError> {
            self.values
                .lock()
                .unwrap()
                .insert((user_id, key.to_string()), value.to_string());
            Ok(())
        }
        async fn get_all(&self, user_id: UserId) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(self
                .values
                .lock()
                .unwrap()
                .iter()
                .filter(|((uid, _), _)| *uid == user_id)
                .map(|((_, k), v)| (k.clone(), v.clone()))
                .collect())
        }
    }

    #[tokio::test]
    async fn the_idle_threshold_defaults_to_twelve_hours() {
        let config = StubConfigRepository::default();
        assert_eq!(idle_timeout_hours(&config, user_id()).await.unwrap(), 12);
    }

    #[tokio::test]
    async fn the_idle_threshold_is_read_from_configuration() {
        let config = StubConfigRepository::default();
        config.set(user_id(), "aplan.session_idle_timeout_hours", "3").await.unwrap();
        assert_eq!(idle_timeout_hours(&config, user_id()).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn an_unparseable_threshold_falls_back_to_the_default() {
        // A corrupt value must not make the reaper close every session immediately.
        let config = StubConfigRepository::default();
        config.set(user_id(), "aplan.session_idle_timeout_hours", "soon").await.unwrap();
        assert_eq!(idle_timeout_hours(&config, user_id()).await.unwrap(), 12);
    }

    #[tokio::test]
    async fn a_zero_threshold_falls_back_to_the_default() {
        // "0" parses fine, so it never reaches the unparseable fallback above. Left
        // unfiltered, idle_before == now and every open session -- including the one
        // asking right now -- would be reaped on the very next tick.
        let config = StubConfigRepository::default();
        config.set(user_id(), "aplan.session_idle_timeout_hours", "0").await.unwrap();
        assert_eq!(idle_timeout_hours(&config, user_id()).await.unwrap(), 12);
    }

    #[tokio::test]
    async fn a_negative_threshold_falls_back_to_the_default() {
        let config = StubConfigRepository::default();
        config.set(user_id(), "aplan.session_idle_timeout_hours", "-5").await.unwrap();
        assert_eq!(idle_timeout_hours(&config, user_id()).await.unwrap(), 12);
    }

    #[tokio::test]
    async fn a_threshold_too_large_for_a_duration_falls_back_to_the_default() {
        // i64::MAX parses fine as a threshold but is many orders of magnitude past
        // the ~2.56e12-hour bound `chrono::Duration::hours` panics beyond -- left
        // unfiltered here, `run_reap_pass` would panic on it downstream instead of
        // this function returning a merely-wrong number. See this test's assertion
        // for what this function alone can prove; the report explains the rest.
        let config = StubConfigRepository::default();
        config
            .set(user_id(), "aplan.session_idle_timeout_hours", &i64::MAX.to_string())
            .await
            .unwrap();
        assert_eq!(idle_timeout_hours(&config, user_id()).await.unwrap(), 12);
    }
}
