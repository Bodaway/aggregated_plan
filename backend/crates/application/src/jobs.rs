//! Scheduling policy for the long-lived background jobs: how long to wait after an
//! attempt, and whether that attempt deserves a log line.
//!
//! Pure by construction — no clock, no I/O, no `tracing`. The caller passes `now`,
//! then does the sleeping and the logging itself. That is what makes a back-off
//! curve and a suppression rule testable without waiting on wall-clock time.
//!
//! The problem it solves: a job that retries a permanent failure on a fixed short
//! interval prints the same line thousands of times a day, and a failure that has
//! lasted three weeks becomes indistinguishable from one that started a minute ago.

use std::time::Duration;

use chrono::{DateTime, Utc};

/// How aggressively a job backs off after failures, and when repetition stops being
/// a `warn` and becomes an `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Delay between attempts while healthy, and the first back-off step.
    pub base: Duration,
    /// Upper bound on the back-off delay, however long the streak runs.
    pub ceiling: Duration,
    /// Number of consecutive failures at which the log line escalates to `error`.
    pub escalate_after: u32,
    /// Once escalated, re-print the otherwise-suppressed line every N attempts, so a
    /// permanent failure stays visible without being shouted every attempt.
    pub reminder_every: u32,
}

impl RetryPolicy {
    /// The end-of-day reconstruction job: a pass every 5 minutes while healthy (the
    /// watermark, not the tick, decides when work is actually due), backing off to
    /// 30 minutes, escalating on the third consecutive failure and reminding every
    /// twelfth attempt (~6 h at the ceiling).
    pub const fn end_of_day() -> Self {
        Self {
            base: Duration::from_secs(5 * 60),
            ceiling: Duration::from_secs(30 * 60),
            escalate_after: 3,
            reminder_every: 12,
        }
    }

    /// The idle-session reaper: a pass every 15 minutes while healthy, backing off
    /// to 45 minutes. Slower than `end_of_day()`'s 5-minute base on purpose -- the
    /// default idle threshold is 12 hours, so a 5-minute tick would poll 288
    /// times a day for a boundary each session crosses exactly once; 15 minutes is
    /// still fine enough that a session going idle is picked up well inside the
    /// hour, at under a hundredth of the threshold it is checking against. Lower
    /// ceiling than the end-of-day job's for the same reason as before: a late
    /// reap only delays closing an already-idle session, not the timesheet
    /// reconstruction the end-of-day back-off is tuned to protect. Same escalation
    /// shape as `end_of_day()`: the third consecutive failure escalates, and an
    /// ongoing outage reminds every twelfth attempt.
    pub const fn session_reaper() -> Self {
        Self {
            base: Duration::from_secs(15 * 60),
            ceiling: Duration::from_secs(45 * 60),
            escalate_after: 3,
            reminder_every: 12,
        }
    }

    /// The break engine: a tick every 30 seconds while healthy, backing off to 5
    /// minutes.
    ///
    /// Far finer than `end_of_day()`'s 5-minute base because here the granularity of
    /// the tick is the granularity of every break: at a 5-minute tick a deferral armed
    /// for 09:53 lands somewhere in 09:53–09:58, which is exactly the sloppiness that
    /// makes a reminder feel arbitrary.
    pub const fn breaks() -> Self {
        Self {
            base: Duration::from_secs(30),
            ceiling: Duration::from_secs(5 * 60),
            escalate_after: 3,
            reminder_every: 12,
        }
    }
}

/// What one attempt produced, as far as the policy is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome<'a> {
    Succeeded,
    /// `signature` identifies the error so an unchanged one can be recognised as a
    /// repeat — and a changed one reported at once.
    Failed { signature: &'a str },
}

/// Severity the caller should use for the line it is being told to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Warn,
    Error,
}

/// The line the caller should print, with everything that makes it informative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEntry {
    /// A failure worth printing: the first one, a changed error, or the escalated
    /// reminder for a streak that is not ending.
    Failure {
        level: LogLevel,
        /// Length of the current failure streak, this attempt included.
        consecutive_failures: u32,
        /// Span since the streak started — "failing for 3w" is information.
        failing_for: chrono::Duration,
        /// Identical failures swallowed since the previous printed line.
        suppressed_repeats: u32,
    },
    /// The job came back after a streak. Worth exactly one line.
    Recovered {
        after_failures: u32,
        was_failing_for: chrono::Duration,
    },
}

/// The policy's verdict on one attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptDecision {
    /// How long the caller should wait before attempting again.
    pub retry_in: Duration,
    /// `None` means stay quiet: this is a repeat that was already reported.
    pub log: Option<LogEntry>,
}

/// The failure streak a job is currently in. `default()` is healthy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobHealth {
    consecutive_failures: u32,
    /// When the current streak started; `None` while healthy.
    failing_since: Option<DateTime<Utc>>,
    /// Signature of the last failure, to tell "again" from "something new".
    last_error: Option<String>,
    /// Identical failures swallowed since the last printed line.
    suppressed_repeats: u32,
}

impl JobHealth {
    /// Length of the current failure streak (0 while healthy).
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Fold one attempt outcome into the streak, yielding the new state and what the
    /// caller should do about it. Total function, no interior mutability, no clock.
    pub fn observe(
        self,
        outcome: AttemptOutcome<'_>,
        now: DateTime<Utc>,
        policy: &RetryPolicy,
    ) -> (Self, AttemptDecision) {
        match outcome {
            AttemptOutcome::Succeeded => {
                let log = self.failing_since.map(|since| LogEntry::Recovered {
                    after_failures: self.consecutive_failures,
                    was_failing_for: now - since,
                });
                (Self::default(), AttemptDecision { retry_in: policy.base, log })
            }
            AttemptOutcome::Failed { signature } => {
                let consecutive = self.consecutive_failures.saturating_add(1);
                let failing_since = self.failing_since.unwrap_or(now);
                let escalated = consecutive >= policy.escalate_after;

                let is_first = consecutive == 1;
                let error_changed = self.last_error.as_deref() != Some(signature);
                let reminder_due = escalated
                    && policy.reminder_every > 0
                    && (consecutive - policy.escalate_after).is_multiple_of(policy.reminder_every);
                let speak = is_first || error_changed || reminder_due;

                let log = speak.then(|| LogEntry::Failure {
                    level: if escalated { LogLevel::Error } else { LogLevel::Warn },
                    consecutive_failures: consecutive,
                    failing_for: now - failing_since,
                    suppressed_repeats: self.suppressed_repeats,
                });

                let next = Self {
                    consecutive_failures: consecutive,
                    failing_since: Some(failing_since),
                    last_error: Some(signature.to_string()),
                    suppressed_repeats: if speak { 0 } else { self.suppressed_repeats.saturating_add(1) },
                };
                (next, AttemptDecision { retry_in: backoff_delay(consecutive, policy), log })
            }
        }
    }
}

/// `base * 2^(failures - 1)`, capped at `ceiling`. Saturating throughout: a streak of
/// thousands must not wrap the shift around into a tight retry loop.
pub fn backoff_delay(consecutive_failures: u32, policy: &RetryPolicy) -> Duration {
    let ceiling_ms = policy.ceiling.as_millis();
    let Some(shift) = consecutive_failures.checked_sub(1) else {
        return policy.base;
    };
    if shift >= u128::BITS {
        return policy.ceiling;
    }
    let factor = 1u128 << shift;
    let millis = policy.base.as_millis().saturating_mul(factor).min(ceiling_ms);
    Duration::from_millis(u64::try_from(millis).unwrap_or(u64::MAX))
}

/// A span rendered for a human reading a journal: `3w 0d`, `2d 4h`, `4h 12m`, `45m`,
/// `45s`. Coarse on purpose — the point is "how long has this been broken", not precision.
pub fn humanize_duration(span: chrono::Duration) -> String {
    let secs = span.num_seconds().max(0);
    let (weeks, rest) = (secs / 604_800, secs % 604_800);
    let (days, rest) = (rest / 86_400, rest % 86_400);
    let (hours, rest) = (rest / 3_600, rest % 3_600);
    let minutes = rest / 60;
    if weeks > 0 {
        format!("{weeks}w {days}d")
    } else if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn policy() -> RetryPolicy {
        RetryPolicy {
            base: Duration::from_secs(300),
            ceiling: Duration::from_secs(1800),
            escalate_after: 3,
            reminder_every: 12,
        }
    }

    /// A fixed instant plus `minutes`, so elapsed spans are exact.
    fn at(minutes: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 8, 18, 0, 0).unwrap() + chrono::Duration::minutes(minutes)
    }

    /// Drive `n` identical failures from healthy, one per minute. Returns the state
    /// after the streak plus the decision the last failure produced.
    fn streak(n: u32, signature: &str) -> (JobHealth, AttemptDecision) {
        let p = policy();
        let mut health = JobHealth::default();
        let mut decision = AttemptDecision { retry_in: p.base, log: None };
        for i in 1..=n {
            let (next, d) = health.observe(AttemptOutcome::Failed { signature }, at(i as i64), &p);
            health = next;
            decision = d;
        }
        (health, decision)
    }

    #[test]
    fn consecutive_failures_double_the_delay() {
        assert_eq!(streak(1, "boom").1.retry_in, Duration::from_secs(300));
        assert_eq!(streak(2, "boom").1.retry_in, Duration::from_secs(600));
        assert_eq!(streak(3, "boom").1.retry_in, Duration::from_secs(1200));
    }

    #[test]
    fn backoff_stops_at_the_ceiling() {
        assert_eq!(streak(4, "boom").1.retry_in, Duration::from_secs(1800));
        assert_eq!(streak(9, "boom").1.retry_in, Duration::from_secs(1800));
        // A streak long enough to overflow a naive `base << n` must still be capped.
        assert_eq!(
            backoff_delay(4_021, &policy()),
            Duration::from_secs(1800),
            "a three-week streak must not wrap around into a tight loop"
        );
    }

    #[test]
    fn success_resets_the_delay_to_base() {
        let (failing, _) = streak(5, "boom");
        let (healthy, decision) = failing.observe(AttemptOutcome::Succeeded, at(6), &policy());
        assert_eq!(decision.retry_in, Duration::from_secs(300));
        assert_eq!(healthy, JobHealth::default(), "a success must forget the streak");
        // ...and the next failure starts the curve over from the base.
        let (_, after) = healthy.observe(AttemptOutcome::Failed { signature: "boom" }, at(7), &policy());
        assert_eq!(after.retry_in, Duration::from_secs(300));
    }

    #[test]
    fn first_failure_warns() {
        let (_, decision) = streak(1, "boom");
        match decision.log {
            Some(LogEntry::Failure { level, consecutive_failures, suppressed_repeats, .. }) => {
                assert_eq!(level, LogLevel::Warn);
                assert_eq!(consecutive_failures, 1);
                assert_eq!(suppressed_repeats, 0);
            }
            other => panic!("the first failure must be logged, got {other:?}"),
        }
    }

    #[test]
    fn identical_repeat_is_suppressed() {
        let p = policy();
        let (health, _) = streak(1, "boom");
        let (_, decision) = health.observe(AttemptOutcome::Failed { signature: "boom" }, at(2), &p);
        assert!(decision.log.is_none(), "an unchanged error must not be reprinted");
    }

    #[test]
    fn changed_error_logs_immediately() {
        let p = policy();
        let (health, _) = streak(2, "boom");
        let (_, decision) = health.observe(AttemptOutcome::Failed { signature: "different" }, at(3), &p);
        assert!(
            decision.log.is_some(),
            "a new error is news even mid-streak, and must not be suppressed as a repeat"
        );
    }

    #[test]
    fn escalation_carries_the_count_and_how_long_it_has_been_failing() {
        let p = policy();
        // Two failures at minute 1 and 2, then the third three weeks later.
        let (health, _) = streak(2, "boom");
        let three_weeks_later = at(1) + chrono::Duration::days(21);
        let (_, decision) =
            health.observe(AttemptOutcome::Failed { signature: "boom" }, three_weeks_later, &p);
        match decision.log {
            Some(LogEntry::Failure { level, consecutive_failures, failing_for, suppressed_repeats }) => {
                assert_eq!(level, LogLevel::Error, "the threshold must escalate past warn");
                assert_eq!(consecutive_failures, 3);
                assert_eq!(failing_for.num_days(), 21, "the line must say how long it has been broken");
                assert_eq!(suppressed_repeats, 1, "and how many repeats it swallowed");
            }
            other => panic!("the escalation must be logged, got {other:?}"),
        }
    }

    #[test]
    fn an_escalated_streak_reminds_periodically_rather_than_never() {
        let p = policy();
        let mut health = JobHealth::default();
        let mut logged = Vec::new();
        for i in 1..=40u32 {
            let (next, d) =
                health.observe(AttemptOutcome::Failed { signature: "boom" }, at(i as i64), &p);
            health = next;
            if d.log.is_some() {
                logged.push(i);
            }
        }
        // First failure, the escalation at the threshold, then one reminder per
        // `reminder_every` attempts — never silent forever, never once a minute.
        assert_eq!(logged, vec![1, 3, 15, 27, 39]);
    }

    #[test]
    fn recovery_reports_the_streak_it_ended() {
        let p = policy();
        let (health, _) = streak(4, "boom");
        let (_, decision) = health.observe(AttemptOutcome::Succeeded, at(1) + chrono::Duration::hours(2), &p);
        match decision.log {
            Some(LogEntry::Recovered { after_failures, was_failing_for }) => {
                assert_eq!(after_failures, 4);
                assert_eq!(was_failing_for.num_hours(), 2);
            }
            other => panic!("coming back after an outage deserves a line, got {other:?}"),
        }
    }

    #[test]
    fn a_healthy_run_says_nothing() {
        let (_, decision) = JobHealth::default().observe(AttemptOutcome::Succeeded, at(0), &policy());
        assert!(decision.log.is_none(), "the happy path must not chatter");
    }

    #[test]
    fn a_permanent_failure_costs_dozens_of_lines_a_day_not_thousands() {
        let p = RetryPolicy::end_of_day();
        let mut health = JobHealth::default();
        let mut elapsed = Duration::ZERO;
        let day = Duration::from_secs(24 * 3600);
        let (mut attempts, mut lines) = (0u32, 0u32);
        let mut clock = at(0);
        while elapsed < day {
            let (next, d) = health.observe(AttemptOutcome::Failed { signature: "boom" }, clock, &p);
            health = next;
            attempts += 1;
            if d.log.is_some() {
                lines += 1;
            }
            elapsed += d.retry_in;
            clock += chrono::Duration::from_std(d.retry_in).expect("retry delay fits in chrono");
        }
        assert!(attempts <= 50, "backoff must cut 1440 attempts/day down to dozens, got {attempts}");
        assert!(lines <= 6, "suppression must keep the journal readable, got {lines} lines");
        assert!(lines >= 2, "but it must never go completely silent, got {lines} lines");
    }

    /// Breaks need a much finer tick than the end-of-day pass: a 5-minute tick would
    /// place a 20-minute break anywhere in a 5-minute band, and the deferral wake-up
    /// would inherit the same slop.
    #[test]
    fn break_policy_ticks_every_thirty_seconds_while_healthy() {
        assert_eq!(backoff_delay(0, &RetryPolicy::breaks()), Duration::from_secs(30));
    }

    #[test]
    fn break_policy_backs_off_to_five_minutes() {
        let p = RetryPolicy::breaks();
        assert_eq!(backoff_delay(1, &p), Duration::from_secs(30));
        assert_eq!(backoff_delay(3, &p), Duration::from_secs(120));
        // base * 2^(n-1) saturates at the ceiling from the fifth failure on.
        assert_eq!(backoff_delay(5, &p), Duration::from_secs(300));
        assert_eq!(backoff_delay(50, &p), Duration::from_secs(300));
    }

    #[test]
    fn humanized_spans_read_like_a_human_wrote_them() {
        assert_eq!(humanize_duration(chrono::Duration::seconds(45)), "45s");
        assert_eq!(humanize_duration(chrono::Duration::minutes(45)), "45m");
        assert_eq!(humanize_duration(chrono::Duration::minutes(252)), "4h 12m");
        assert_eq!(humanize_duration(chrono::Duration::hours(52)), "2d 4h");
        assert_eq!(humanize_duration(chrono::Duration::days(21)), "3w 0d");
        assert_eq!(humanize_duration(chrono::Duration::seconds(-5)), "0s");
    }
}
