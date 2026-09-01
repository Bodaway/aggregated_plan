use chrono::{DateTime, Datelike, Duration, NaiveTime, Utc, Weekday};
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{
    BreakEventRepository, BreakRuleRepository, ConfigRepository, MeetingRepository,
};
use crate::services::{Notification, NotificationOutcome, Notifier, SurfaceController};
use crate::time::{local_to_utc, resolve_tz, to_local};
use domain::rules::breaks::{decide, BreakTickInput, BusyPeriod, Candidate, Window};
use domain::types::*;

const KEY_ENABLED: &str = "aplan.breaks.enabled";
const KEY_GRACE: &str = "aplan.breaks.meeting_grace_minutes";
const KEY_SNOOZE: &str = "aplan.breaks.snooze_minutes";
const KEY_SHOW_AS: &str = "aplan.breaks.suppressing_show_as";
const KEY_LAST_TICK: &str = "aplan.breaks.last_tick";

const DEFAULT_GRACE_MINUTES: i64 = 3;
const DEFAULT_SNOOZE_MINUTES: i64 = 10;
/// Every status Outlook can put on a real appointment. The narrow `busy,oof` this used
/// to be let `tentative` and `free` meetings through, and both are ordinary meetings in
/// practice — an unanswered invitation is still attended, and recurring internal points
/// are routinely marked `free`. The asymmetry settles it: a break deferred to the end of
/// a meeting costs a few minutes, a popup during one costs the meeting. Users who want
/// the narrow reading set `aplan.breaks.suppressing_show_as` themselves.
const DEFAULT_SHOW_AS: &str = "busy,oof,tentative,free";

pub struct BreakTickDeps<'a> {
    pub rules: &'a dyn BreakRuleRepository,
    pub events: &'a dyn BreakEventRepository,
    pub meetings: &'a dyn MeetingRepository,
    pub config: &'a dyn ConfigRepository,
    pub notifier: &'a dyn Notifier,
    pub surface: &'a dyn SurfaceController,
    /// How often a running break re-reads its own row while waiting for the deadline.
    ///
    /// A parameter rather than a constant because it is the one knob that makes the
    /// wait testable: one second in production, a few milliseconds in a test. Nothing
    /// else about the wait is abstracted — the clock stays the real one, so what the
    /// tests exercise is the code that runs in production.
    pub session_poll: std::time::Duration,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BreakTickReport {
    pub fired: Option<BreakEventId>,
    pub deferred: usize,
    pub absorbed: usize,
    pub expired: usize,
    /// Breaks that were still open from a previous process and got closed on the way in.
    pub recovered: usize,
}

/// Read an integer configuration value, falling back to `default`.
///
/// Deliberately a local helper rather than a shared one: `use_cases/timesheet.rs` has its
/// own private `u32_key`, nested inside a function and not reachable from here. What must
/// stay shared is not the code but the **defaults** — 8 / 12 / 13 / 17 for the workday
/// bounds, identical to timesheet's — because a break engine and a timesheet that disagree
/// about when the workday starts would each be individually correct and jointly absurd.
async fn config_i64(
    config: &dyn ConfigRepository,
    user_id: UserId,
    key: &str,
    default: i64,
) -> Result<i64, AppError> {
    Ok(config
        .get(user_id, key)
        .await?
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default))
}

/// One entry of `general.working_days`.
///
/// The system's own spelling is the ISO number — Monday = 1 — written by the settings
/// screen and read by `use_cases/dashboard.rs`. Names are accepted as a tolerated alias
/// so a hand-edited configuration still works; nothing in the cockpit writes them.
fn parse_weekday(s: &str) -> Option<Weekday> {
    let s = s.trim();
    if let Ok(n) = s.parse::<u8>() {
        return match n {
            1 => Some(Weekday::Mon),
            2 => Some(Weekday::Tue),
            3 => Some(Weekday::Wed),
            4 => Some(Weekday::Thu),
            5 => Some(Weekday::Fri),
            6 => Some(Weekday::Sat),
            7 => Some(Weekday::Sun),
            _ => None,
        };
    }
    match s.to_lowercase().as_str() {
        "mon" | "monday" | "lundi" => Some(Weekday::Mon),
        "tue" | "tuesday" | "mardi" => Some(Weekday::Tue),
        "wed" | "wednesday" | "mercredi" => Some(Weekday::Wed),
        "thu" | "thursday" | "jeudi" => Some(Weekday::Thu),
        "fri" | "friday" | "vendredi" => Some(Weekday::Fri),
        "sat" | "saturday" | "samedi" => Some(Weekday::Sat),
        "sun" | "sunday" | "dimanche" => Some(Weekday::Sun),
        _ => None,
    }
}

/// Today's working windows in UTC, from the workday configuration the rest of the
/// cockpit already uses.
///
/// This is where the timezone lives. `domain` gets UTC instants and never learns that
/// zones exist — the same split `use_cases/worklog.rs` uses for half-day projection.
pub async fn resolve_windows(
    config: &dyn ConfigRepository,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<Vec<Window>, AppError> {
    let tz = resolve_tz(config.get(user_id, "aplan.timezone").await?);
    let local_now = to_local(now, tz);
    let today = local_now.date();

    let days = config
        .get(user_id, "general.working_days")
        .await?
        .unwrap_or_else(|| "1,2,3,4,5".to_string());
    let working: Vec<Weekday> = days.split(',').filter_map(parse_weekday).collect();
    if working.is_empty() {
        // No window means no break can ever fire, and a tick with no window reports
        // success — which is exactly how a configuration this engine could not read
        // once passed for a quiet routine. Say the raw value out loud.
        tracing::warn!(
            configured = %days,
            "general.working_days yielded no recognised day: the break routine cannot fire"
        );
        return Ok(Vec::new());
    }
    if !working.contains(&today.weekday()) {
        tracing::debug!(configured = %days, "not a working day: no break windows today");
        return Ok(Vec::new());
    }

    let mut windows = Vec::new();
    for (start_key, end_key, default_start, default_end) in [
        ("workday.morning_start_hour", "workday.morning_end_hour", 8, 12),
        ("workday.afternoon_start_hour", "workday.afternoon_end_hour", 13, 17),
    ] {
        let start_h = config_i64(config, user_id, start_key, default_start).await?;
        let end_h = config_i64(config, user_id, end_key, default_end).await?;
        let (Some(start_t), Some(end_t)) = (
            NaiveTime::from_hms_opt(start_h.clamp(0, 23) as u32, 0, 0),
            NaiveTime::from_hms_opt(end_h.clamp(0, 23) as u32, 0, 0),
        ) else {
            continue;
        };
        if end_t <= start_t {
            continue;
        }
        windows.push(Window {
            start: local_to_utc(tz, today.and_time(start_t)),
            end: local_to_utc(tz, today.and_time(end_t)),
        });
    }
    if windows.is_empty() {
        // Today is a working day and still nothing came out: the hour keys are the
        // culprit. Same reason as above — an empty window list must never be quiet.
        tracing::warn!(
            configured = %days,
            "workday hours yielded no break window on a working day"
        );
    }
    Ok(windows)
}

/// Resolve today's UTC instant for every enabled `Daily` rule.
async fn resolve_daily_dues(
    config: &dyn ConfigRepository,
    user_id: UserId,
    rules: &[BreakRule],
    now: DateTime<Utc>,
) -> Result<Vec<(BreakRuleId, DateTime<Utc>)>, AppError> {
    let tz = resolve_tz(config.get(user_id, "aplan.timezone").await?);
    let today = to_local(now, tz).date();
    Ok(rules
        .iter()
        .filter_map(|r| r.cadence.at_time().map(|t| (r.id, local_to_utc(tz, today.and_time(t)))))
        .collect())
}

/// One pass of the break engine.
///
/// Never fails on a delivery problem: a notification that could not be shown leaves its
/// row unfired and pending. Such a row carries no `deferred_until`, so the expiry rule
/// in `decide` steps over it — it is the end-of-day sweep, when the last window closes,
/// that clears it. Harmless: an unfired pending row is invisible to the user and counts
/// on neither side of adherence until then.
pub async fn run_break_tick(
    deps: BreakTickDeps<'_>,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<BreakTickReport, AppError> {
    let mut report = BreakTickReport::default();
    recover_orphan_session(&deps, user_id, now, &mut report).await?;

    let since = match deps.config.get(user_id, KEY_LAST_TICK).await? {
        Some(s) => DateTime::parse_from_rfc3339(&s)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or(now),
        // First run ever: start the clock here rather than invent a backlog.
        None => now,
    };
    // Advance the watermark whatever happens below, including when the feature is off:
    // otherwise re-enabling after a week replays a week of dues in one tick.
    let advance = || async {
        deps.config
            .set(user_id, KEY_LAST_TICK, &now.to_rfc3339())
            .await
    };

    let enabled = deps
        .config
        .get(user_id, KEY_ENABLED)
        .await?
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);
    if !enabled {
        advance().await?;
        return Ok(report);
    }

    let rules = deps.rules.list_enabled(user_id).await?;
    let windows = resolve_windows(deps.config, user_id, now).await?;
    let daily_dues = resolve_daily_dues(deps.config, user_id, &rules, now).await?;
    let open = deps.events.list_open(user_id).await?;

    let grace_minutes = config_i64(deps.config, user_id, KEY_GRACE, DEFAULT_GRACE_MINUTES).await?;
    let show_as_filter = deps
        .config
        .get(user_id, KEY_SHOW_AS)
        .await?
        .unwrap_or_else(|| DEFAULT_SHOW_AS.to_string());
    let suppressing: Vec<String> = show_as_filter
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    // `MeetingRepository` exposes `find_by_user_and_range(user_id, start: NaiveDate,
    // end: NaiveDate)` — local calendar dates, not a UTC instant range. Breaks only ever
    // fire inside today's windows, so today's local date on both ends is the exact query.
    let tz = resolve_tz(deps.config.get(user_id, "aplan.timezone").await?);
    let today = to_local(now, tz).date();
    let busy: Vec<BusyPeriod> = deps
        .meetings
        .find_by_user_and_range(user_id, today, today)
        .await?
        .into_iter()
        .filter(|m| {
            m.show_as
                .as_deref()
                .map(|s| suppressing.contains(&s.to_lowercase()))
                .unwrap_or(false)
        })
        .map(|m| BusyPeriod {
            meeting_id: m.outlook_id.clone(),
            start: m.start_time,
            end: m.end_time,
        })
        .collect();

    let tick = decide(BreakTickInput {
        now,
        since,
        windows: &windows,
        rules: &rules,
        daily_dues: &daily_dues,
        busy: &busy,
        open: &open,
        grace: Duration::minutes(grace_minutes),
    });

    for id in &tick.expire {
        deps.events.set_outcome(*id, BreakOutcome::Expired, None).await?;
        report.expired += 1;
    }

    for absorbed in &tick.absorb {
        let id = match absorbed.candidate.event_id() {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4();
                deps.events
                    .create(&new_event(id, user_id, &absorbed.candidate, now, BreakOutcome::Absorbed))
                    .await?;
                report.absorbed += 1;
                continue;
            }
        };
        deps.events.set_outcome(id, BreakOutcome::Absorbed, None).await?;
        report.absorbed += 1;
    }

    for deferred in &tick.defer {
        let id = match deferred.candidate.event_id() {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4();
                deps.events
                    .create(&new_event(id, user_id, &deferred.candidate, now, BreakOutcome::Pending))
                    .await?;
                id
            }
        };
        deps.events
            .set_deferral(id, deferred.until, deferred.reason, deferred.meeting_id.as_deref())
            .await?;
        report.deferred += 1;
    }

    let Some(fire) = tick.fire else {
        advance().await?;
        return Ok(report);
    };

    let event_id = match fire.candidate.event_id() {
        Some(id) => id,
        None => {
            let id = Uuid::new_v4();
            deps.events
                .create(&new_event(id, user_id, &fire.candidate, now, BreakOutcome::Pending))
                .await?;
            id
        }
    };

    let Some(rule) = rules.iter().find(|r| r.id == fire.candidate.rule_id()) else {
        advance().await?;
        return Ok(report);
    };

    let notification = Notification {
        title: rule.label.clone(),
        body: rule.body.clone(),
        urgency: rule.urgency,
        icon: Some(icon_for(rule.kind).to_string()),
        expire_after: std::time::Duration::from_secs(rule.duration_seconds as u64 + 300),
        actions: actions_for(rule),
    };

    match deps.notifier.notify(notification).await {
        Ok(NotificationOutcome::NotShown) => {
            // Nothing reached a screen, so nothing fired: `fired_at` stays NULL and the
            // slot closes as `expired`, the one outcome excluded from both sides of
            // adherence. Calling it a dismissal — and therefore `ignored` — would let a
            // headless API report a user who ignores breaks they were never shown.
            deps.events.set_outcome(event_id, BreakOutcome::Expired, None).await?;
            report.expired += 1;
            tracing::info!(rule = %rule.label, "break not shown: no display available");
        }
        Ok(outcome) => {
            deps.events.mark_fired(event_id, now).await?;
            report.fired = Some(event_id);
            apply_outcome(&deps, user_id, event_id, rule, outcome, now).await?;
        }
        Err(e) => {
            // Books kept, no state invented: `fired_at` stays NULL and the expiry rule
            // clears the row at the rule's next natural due. Logged rather than
            // returned, because a daemon that is not there must not fail the tick —
            // but it must not be silent either, or a routine that stopped notifying
            // looks identical to a routine with nothing to say.
            tracing::warn!(error = %e, rule = %rule.label, "break notification not delivered");
        }
    }

    advance().await?;
    Ok(report)
}

/// The notification's buttons, in the order the user reads them.
///
/// *Plus tard* is offered only where the rule can carry it (`BreakRule::allows_snooze`):
/// below the hour a deferral re-queues on top of a grid that is already firing, so the
/// break is taken or it is not.
fn actions_for(rule: &BreakRule) -> Vec<(String, String)> {
    let mut actions = vec![("taken".to_string(), "Prendre la pause".to_string())];
    if rule.allows_snooze() {
        actions.push(("snoozed".to_string(), "Plus tard".to_string()));
    }
    actions.push(("skipped".to_string(), "Passer".to_string()));
    actions
}

fn icon_for(kind: BreakKind) -> &'static str {
    match kind {
        BreakKind::Visual => "eye",
        BreakKind::Posture => "user-available",
        BreakKind::Long => "appointment-soon",
        BreakKind::Strength => "weather-clear",
    }
}

fn new_event(
    id: BreakEventId,
    user_id: UserId,
    candidate: &Candidate,
    now: DateTime<Utc>,
    outcome: BreakOutcome,
) -> BreakEvent {
    BreakEvent {
        id,
        user_id,
        rule_id: candidate.rule_id(),
        due_at: candidate.due_at(),
        fired_at: None,
        deferred_until: None,
        defer_reason: None,
        suppressed_by_meeting_id: None,
        outcome,
        responded_at: if outcome == BreakOutcome::Pending { None } else { Some(now) },
        // No session yet: one only opens when the user presses the button.
        started_at: None,
        ends_at: None,
        created_at: now,
    }
}

/// Translate what the user pressed into stored state.
///
/// A snooze resolves the current slot and arms a fresh deferral, which is how it
/// re-enters `decide` without `decide` knowing snoozes exist — but only for a rule that
/// still offers the button. `actions_for` stopped drawing it below the hour; a
/// notification daemon replaying a stale action must not be able to press it anyway.
async fn apply_outcome(
    deps: &BreakTickDeps<'_>,
    user_id: UserId,
    event_id: BreakEventId,
    rule: &BreakRule,
    outcome: NotificationOutcome,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    let resolved = match &outcome {
        NotificationOutcome::Action(key) => {
            BreakOutcome::from_str(key).unwrap_or(BreakOutcome::Ignored)
        }
        NotificationOutcome::Dismissed => BreakOutcome::Ignored,
        NotificationOutcome::Expired => BreakOutcome::Ignored,
        // The caller intercepts this one before calling us, and records exactly this.
        // Kept here so the match stays exhaustive and cannot drift from that decision.
        NotificationOutcome::NotShown => BreakOutcome::Expired,
    };
    // Recorded as a deliberate refusal rather than dropped, because the user did answer;
    // what they cannot do on this cadence is defer. Falling through would arm the
    // follow-up below and resurrect the compounding the restriction exists to remove.
    let resolved = if resolved == BreakOutcome::Snoozed && !rule.allows_snooze() {
        tracing::debug!(rule = %rule.label, "snooze action on a rule that does not offer it: recorded as skipped");
        BreakOutcome::Skipped
    } else {
        resolved
    };

    // Pressing the button does not record a break, it starts one. `taken` is earned at
    // the deadline and nowhere else — that is the whole point of the session, and what
    // makes the adherence figure mean "breaks taken" rather than "popups dismissed
    // quickly". Every other answer resolves the slot here and now, as it always did.
    if resolved == BreakOutcome::Taken {
        return serve_break(deps, user_id, event_id, rule).await;
    }

    deps.events.set_outcome(event_id, resolved, Some(now)).await?;

    if resolved == BreakOutcome::Snoozed {
        let minutes =
            config_i64(deps.config, user_id, KEY_SNOOZE, DEFAULT_SNOOZE_MINUTES).await?;
        let follow_up = Uuid::new_v4();
        deps.events
            .create(&BreakEvent {
                id: follow_up,
                user_id,
                rule_id: rule.id,
                due_at: now,
                fired_at: None,
                deferred_until: Some(now + Duration::minutes(minutes)),
                defer_reason: Some(DeferReason::Snooze),
                suppressed_by_meeting_id: None,
                outcome: BreakOutcome::Pending,
                responded_at: None,
                started_at: None,
                ends_at: None,
                created_at: now,
            })
            .await?;
    }
    Ok(())
}

/// Serve the break: open the session, raise the surface, hold until the deadline.
///
/// The tick stays on this row for the whole break, which is deliberate — it is the
/// property that makes it impossible for one break to ring during another.
async fn serve_break(
    deps: &BreakTickDeps<'_>,
    user_id: UserId,
    event_id: BreakEventId,
    rule: &BreakRule,
) -> Result<(), AppError> {
    // Read the clock here rather than take the tick's `now`. The tick started before
    // the notification went out, and `notify-send --action` implies `--wait`: by the
    // time the button is pressed, that instant can be minutes old. Anchoring on it
    // would serve a break short by exactly the user's hesitation — and, on a
    // thirty-second visual break answered late, a break already over when it opens.
    let started_at = Utc::now();
    let ends_at = started_at + Duration::seconds(rule.duration_seconds as i64);
    deps.events.start_session(event_id, started_at, ends_at).await?;
    show_surface(deps).await;

    // The wait is a separate call, and its result is held rather than propagated,
    // because the surface has to come back down on **every** exit — the failing one
    // included. A `?` inside the loop jumped straight over the `hide` below, and a
    // transient database error then left the pause screen filling the display until the
    // next tick: thirty seconds to five minutes of frozen countdown, over an error that
    // lasted a millisecond.
    let waited = wait_out_break(deps, user_id, event_id, rule, ends_at).await;
    hide_surface(deps).await;
    waited?;

    announce_end(deps, rule).await;
    Ok(())
}

/// How many polls the wait may make before it gives up on its deadline.
///
/// The equivalent of twice the break's own length: a wait that has not seen its deadline
/// after twice the break has a clock problem, not a slow break.
///
/// The floor on the poll interval is what keeps the answer finite for a caller that
/// passes zero, and it sits at the shortest interval anything here actually polls at:
/// five milliseconds in the tests, one second in production. Below that is not a poll
/// interval, it is a spin. So the floor never distorts a real budget — it only stops a
/// division from turning into an unbounded loop.
fn max_wait_polls(duration_seconds: u32, poll: std::time::Duration) -> u32 {
    const MIN_POLL_SECONDS: f64 = 0.005;
    let poll = poll.as_secs_f64().max(MIN_POLL_SECONDS);
    ((2.0 * f64::from(duration_seconds)) / poll)
        .ceil()
        .clamp(1.0, f64::from(u32::MAX)) as u32
}

/// Hold the tick on this row until the break is over.
///
/// Three ways out: the deadline arrives, which writes `taken`; another writer has taken
/// the row over, which is `endBreak` having recorded `abandoned`; or the poll budget
/// runs out.
///
/// That third one exists because `ends_at` is an absolute instant. A clock stepped
/// backwards — an NTP correction, a resume from suspend — makes `now >= ends_at`
/// unreachable for the whole offset, and this loop holds the scheduler for exactly that
/// long, which is to say for as long as the correction was large. Exhausting the budget
/// is treated as reaching the deadline, because by then the break has had at least its
/// prescribed time on any clock worth believing.
async fn wait_out_break(
    deps: &BreakTickDeps<'_>,
    user_id: UserId,
    event_id: BreakEventId,
    rule: &BreakRule,
    ends_at: DateTime<Utc>,
) -> Result<(), AppError> {
    for _ in 0..max_wait_polls(rule.duration_seconds, deps.session_poll) {
        tokio::time::sleep(deps.session_poll).await;
        let now = Utc::now();
        if now >= ends_at {
            deps.events
                .set_outcome(event_id, BreakOutcome::Taken, Some(now))
                .await?;
            return Ok(());
        }
        // Re-read rather than trust the deadline alone: `endBreak` can have written
        // `abandoned` while we slept. The row is no longer active, or a different one
        // is — either way this tick has stopped being its owner, so it stops waiting
        // and, above all, writes nothing over what the other writer recorded.
        match deps.events.find_active(user_id).await? {
            Some(active) if active.id == event_id => continue,
            _ => return Ok(()),
        }
    }
    tracing::warn!(
        event = %event_id,
        rule = %rule.label,
        "break wait exhausted its poll budget without reaching its deadline: the clock moved"
    );
    deps.events
        .set_outcome(event_id, BreakOutcome::Taken, Some(Utc::now()))
        .await?;
    Ok(())
}

/// A `pending` row carrying a session that no tick is serving: the API went down in
/// the middle of a break.
///
/// **Only a session whose deadline has passed is recovered**, and it is recovered as
/// `taken`: the countdown was launched and it ran to its end on its own, with or
/// without a process watching it. A session still counting down is left alone and comes
/// back here at a later tick, once the deadline is behind it.
///
/// The waiting is what makes this safe, because the premise that would justify claiming
/// it — "the scheduler is a single loop, so a tick that is running proves no tick is
/// waiting on this row" — holds per *process*, not per *database*. A development
/// `cargo run -p api` beside the installed service is two processes over one file, which
/// happens on this machine; the second one's first tick would see a perfectly healthy
/// break as an orphan, write `abandoned` over it and pull the overlay off the screen
/// mid-countdown.
///
/// Deferring costs at most one tick and removes two more nuisances on the way: a restart
/// during a break no longer books an `abandoned` — which counts against adherence, so a
/// deployment used to dirty the statistic — and an NTP correction stepping the clock
/// backwards no longer manufactures one either.
///
/// A row with `started_at` and no `ends_at` is the one incoherent case: `start_session`
/// writes both instants or neither, so nothing legitimate produces it. It cannot be
/// waited on and it cannot be dated, so it is closed on sight, and `abandoned` is all
/// that is left to call it.
async fn recover_orphan_session(
    deps: &BreakTickDeps<'_>,
    user_id: UserId,
    now: DateTime<Utc>,
    report: &mut BreakTickReport,
) -> Result<(), AppError> {
    let Some(active) = deps.events.find_active(user_id).await? else {
        return Ok(());
    };
    let outcome = match active.ends_at {
        Some(ends_at) if now < ends_at => return Ok(()),
        Some(_) => BreakOutcome::Taken,
        None => BreakOutcome::Abandoned,
    };
    deps.events.set_outcome(active.id, outcome, Some(now)).await?;
    hide_surface(deps).await;
    report.recovered += 1;
    tracing::info!(event = %active.id, outcome = outcome.as_str(), "closed a break session left open by a previous run");
    Ok(())
}

/// Raise the surface. A failure is logged, never propagated: a break without its
/// overlay is a degraded break, and refusing to run one over a compositor that would
/// not answer would turn a small loss into a total one.
async fn show_surface(deps: &BreakTickDeps<'_>) {
    if let Err(e) = deps.surface.show().await {
        tracing::warn!(error = %e, "break surface could not be shown");
    }
}

/// Lower the surface. Same treatment as `show_surface`, and for a stronger reason: the
/// break is already recorded by the time this runs, and losing the tick here would
/// leave the scheduler down over a cosmetic failure.
async fn hide_surface(deps: &BreakTickDeps<'_>) {
    if let Err(e) = deps.surface.hide().await {
        tracing::warn!(error = %e, "break surface could not be hidden");
    }
}

/// Say out loud that the break is over.
///
/// The overlay disappearing is a mute signal, and the whole principle of a break is
/// that the user is not looking at the screen. **No actions**, and that is not a
/// detail: an action is what puts `notify-send` into `--wait`, so a button here would
/// hold the scheduler until somebody clicked it.
async fn announce_end(deps: &BreakTickDeps<'_>, rule: &BreakRule) {
    let notification = Notification {
        title: "Pause terminée".to_string(),
        body: rule.label.clone(),
        urgency: BreakUrgency::Low,
        icon: Some(icon_for(rule.kind).to_string()),
        expire_after: std::time::Duration::from_secs(10),
        actions: Vec::new(),
    };
    if let Err(e) = deps.notifier.notify(notification).await {
        tracing::warn!(error = %e, rule = %rule.label, "end-of-break notification not delivered");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::RepositoryError;
    use crate::services::{NullNotifier, NullSurface};
    use chrono::{NaiveDate, TimeZone};
    use std::sync::Mutex;

    /// A few milliseconds, so a one-second break costs a test a second and not a minute.
    const TEST_POLL: std::time::Duration = std::time::Duration::from_millis(5);

    /// Records the calls instead of talking to a compositor, and can be told to fail
    /// `show` so the "no overlay, still a break" path is exercised for real.
    #[derive(Default)]
    struct FakeSurface {
        calls: Mutex<Vec<&'static str>>,
        fails_to_show: Mutex<bool>,
    }

    #[async_trait::async_trait]
    impl SurfaceController for FakeSurface {
        async fn show(&self) -> Result<(), AppError> {
            self.calls.lock().unwrap().push("show");
            if *self.fails_to_show.lock().unwrap() {
                return Err(AppError::Internal("no compositor".into()));
            }
            Ok(())
        }

        async fn hide(&self) -> Result<(), AppError> {
            self.calls.lock().unwrap().push("hide");
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeNotifier {
        sent: Mutex<Vec<Notification>>,
        answer: Mutex<Option<NotificationOutcome>>,
        // Not part of the brief's literal skeleton: needed so
        // `a_notifier_error_leaves_the_event_unfired_but_does_not_fail_the_tick` can force
        // a delivery failure without a second Notifier implementation.
        always_errors: Mutex<bool>,
    }

    #[async_trait::async_trait]
    impl Notifier for FakeNotifier {
        async fn notify(&self, n: Notification) -> Result<NotificationOutcome, AppError> {
            if *self.always_errors.lock().unwrap() {
                return Err(AppError::Internal("notifier unreachable".into()));
            }
            self.sent.lock().unwrap().push(n);
            Ok(self
                .answer
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(NotificationOutcome::Dismissed))
        }
    }

    /// Mutex<Vec<_>> fake: mirrors `BreakRuleRepository` one vector operation at a time.
    #[derive(Default)]
    struct InMemoryBreakRuleRepository {
        rules: Mutex<Vec<BreakRule>>,
    }

    #[async_trait::async_trait]
    impl BreakRuleRepository for InMemoryBreakRuleRepository {
        async fn list(&self, user_id: UserId) -> Result<Vec<BreakRule>, RepositoryError> {
            Ok(self
                .rules
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.user_id == user_id)
                .cloned()
                .collect())
        }

        async fn list_enabled(&self, user_id: UserId) -> Result<Vec<BreakRule>, RepositoryError> {
            Ok(self
                .rules
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.user_id == user_id && r.enabled)
                .cloned()
                .collect())
        }

        async fn get(
            &self,
            user_id: UserId,
            id: BreakRuleId,
        ) -> Result<Option<BreakRule>, RepositoryError> {
            Ok(self
                .rules
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.user_id == user_id && r.id == id)
                .cloned())
        }

        async fn create(&self, rule: &BreakRule) -> Result<(), RepositoryError> {
            self.rules.lock().unwrap().push(rule.clone());
            Ok(())
        }

        async fn update(&self, rule: &BreakRule) -> Result<(), RepositoryError> {
            let mut rules = self.rules.lock().unwrap();
            if let Some(existing) = rules.iter_mut().find(|r| r.id == rule.id) {
                *existing = rule.clone();
            }
            Ok(())
        }

        async fn delete(&self, user_id: UserId, id: BreakRuleId) -> Result<(), RepositoryError> {
            self.rules
                .lock()
                .unwrap()
                .retain(|r| !(r.user_id == user_id && r.id == id));
            Ok(())
        }
    }

    /// Mutex<Vec<_>> fake: mirrors `BreakEventRepository` one vector operation at a time.
    #[derive(Default)]
    struct InMemoryBreakEventRepository {
        events: Mutex<Vec<BreakEvent>>,
        /// Set to true to have the next lookup that finds a running session write
        /// `abandoned` on it first — what the `endBreak` mutation does from another
        /// task. Scripted rather than raced from a spawned task, so the test observes
        /// one exact interleaving instead of whichever one the scheduler produced.
        abandon_next_lookup: Mutex<bool>,
        /// What the row's outcome was at the instant the session opened. The tick does
        /// not return until the break is over, so this is the only way to observe that
        /// pressing the button did *not* record `taken`.
        outcome_when_session_opened: Mutex<Option<BreakOutcome>>,
        /// Set to true to make every lookup fail once a session has opened — a database
        /// that goes away in the middle of a break. Gated on the session rather than on
        /// a counter, because the tick opens with an orphan check that reads the same
        /// method and must still be able to answer.
        fails_lookups_once_a_session_is_open: Mutex<bool>,
    }

    impl InMemoryBreakEventRepository {
        fn all(&self) -> Vec<BreakEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl BreakEventRepository for InMemoryBreakEventRepository {
        async fn list_open(&self, user_id: UserId) -> Result<Vec<BreakEvent>, RepositoryError> {
            Ok(self
                .events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.user_id == user_id && e.outcome == BreakOutcome::Pending)
                .cloned()
                .collect())
        }

        async fn create(&self, event: &BreakEvent) -> Result<(), RepositoryError> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }

        async fn set_outcome(
            &self,
            id: BreakEventId,
            outcome: BreakOutcome,
            responded_at: Option<DateTime<Utc>>,
        ) -> Result<(), RepositoryError> {
            let mut events = self.events.lock().unwrap();
            if let Some(event) = events.iter_mut().find(|e| e.id == id) {
                event.outcome = outcome;
                event.responded_at = responded_at;
            }
            Ok(())
        }

        async fn set_deferral(
            &self,
            id: BreakEventId,
            until: DateTime<Utc>,
            reason: DeferReason,
            meeting_id: Option<&str>,
        ) -> Result<(), RepositoryError> {
            let mut events = self.events.lock().unwrap();
            if let Some(event) = events.iter_mut().find(|e| e.id == id) {
                event.deferred_until = Some(until);
                event.defer_reason = Some(reason);
                event.suppressed_by_meeting_id = meeting_id.map(|s| s.to_string());
            }
            Ok(())
        }

        async fn mark_fired(
            &self,
            id: BreakEventId,
            fired_at: DateTime<Utc>,
        ) -> Result<(), RepositoryError> {
            let mut events = self.events.lock().unwrap();
            if let Some(event) = events.iter_mut().find(|e| e.id == id) {
                event.fired_at = Some(fired_at);
            }
            Ok(())
        }

        async fn find_active(
            &self,
            user_id: UserId,
        ) -> Result<Option<BreakEvent>, RepositoryError> {
            if *self.fails_lookups_once_a_session_is_open.lock().unwrap()
                && self.outcome_when_session_opened.lock().unwrap().is_some()
            {
                return Err(RepositoryError::Database("connection lost".into()));
            }
            let mut events = self.events.lock().unwrap();
            let Some(event) = events.iter_mut().find(|e| {
                e.user_id == user_id
                    && e.outcome == BreakOutcome::Pending
                    && e.started_at.is_some()
            }) else {
                return Ok(None);
            };
            // Read after the lookup, not before: the tick opens with an orphan check,
            // and a flag spent on that empty lookup would never reach the wait.
            let abandon = {
                let mut flag = self.abandon_next_lookup.lock().unwrap();
                let armed = *flag;
                *flag = false;
                armed
            };
            if abandon {
                event.outcome = BreakOutcome::Abandoned;
                event.responded_at = Some(Utc::now());
                return Ok(None);
            }
            Ok(Some(event.clone()))
        }

        /// The conjunction the SQL applies, applied here too — a fake that wrote
        /// blindly would hide the very race the compare-and-swap exists for.
        async fn abandon_if_running(
            &self,
            user_id: UserId,
            id: BreakEventId,
            responded_at: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            let mut events = self.events.lock().unwrap();
            let Some(event) = events.iter_mut().find(|e| {
                e.id == id
                    && e.user_id == user_id
                    && e.outcome == BreakOutcome::Pending
                    && e.started_at.is_some()
            }) else {
                return Ok(false);
            };
            event.outcome = BreakOutcome::Abandoned;
            event.responded_at = Some(responded_at);
            Ok(true)
        }

        async fn start_session(
            &self,
            id: BreakEventId,
            started_at: DateTime<Utc>,
            ends_at: DateTime<Utc>,
        ) -> Result<(), RepositoryError> {
            let mut events = self.events.lock().unwrap();
            if let Some(event) = events.iter_mut().find(|e| e.id == id) {
                event.started_at = Some(started_at);
                event.ends_at = Some(ends_at);
                *self.outcome_when_session_opened.lock().unwrap() = Some(event.outcome);
            }
            Ok(())
        }

        async fn counts_between(
            &self,
            user_id: UserId,
            from: DateTime<Utc>,
            to: DateTime<Utc>,
        ) -> Result<Vec<(BreakRuleId, BreakOutcome, i64)>, RepositoryError> {
            let events = self.events.lock().unwrap();
            let mut counts: Vec<(BreakRuleId, BreakOutcome, i64)> = Vec::new();
            for event in events
                .iter()
                .filter(|e| e.user_id == user_id && e.due_at >= from && e.due_at < to)
            {
                match counts
                    .iter_mut()
                    .find(|(rule_id, outcome, _)| *rule_id == event.rule_id && *outcome == event.outcome)
                {
                    Some(entry) => entry.2 += 1,
                    None => counts.push((event.rule_id, event.outcome, 1)),
                }
            }
            Ok(counts)
        }
    }

    /// Mutex<Vec<_>> fake: mirrors `ConfigRepository` one vector operation at a time.
    #[derive(Default)]
    struct InMemoryConfigRepository {
        values: Mutex<Vec<(UserId, String, String)>>,
    }

    #[async_trait::async_trait]
    impl ConfigRepository for InMemoryConfigRepository {
        async fn get(&self, user_id: UserId, key: &str) -> Result<Option<String>, RepositoryError> {
            Ok(self
                .values
                .lock()
                .unwrap()
                .iter()
                .find(|(u, k, _)| *u == user_id && k == key)
                .map(|(_, _, v)| v.clone()))
        }

        async fn get_all(&self, user_id: UserId) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(self
                .values
                .lock()
                .unwrap()
                .iter()
                .filter(|(u, _, _)| *u == user_id)
                .map(|(_, k, v)| (k.clone(), v.clone()))
                .collect())
        }

        async fn set(&self, user_id: UserId, key: &str, value: &str) -> Result<(), RepositoryError> {
            let mut values = self.values.lock().unwrap();
            match values.iter_mut().find(|(u, k, _)| *u == user_id && k == key) {
                Some(entry) => entry.2 = value.to_string(),
                None => values.push((user_id, key.to_string(), value.to_string())),
            }
            Ok(())
        }
    }

    /// Mutex<Vec<_>> fake: mirrors `MeetingRepository` one vector operation at a time.
    #[derive(Default)]
    struct InMemoryMeetingRepository {
        meetings: Mutex<Vec<Meeting>>,
    }

    #[async_trait::async_trait]
    impl MeetingRepository for InMemoryMeetingRepository {
        async fn find_by_id(&self, id: MeetingId) -> Result<Option<Meeting>, RepositoryError> {
            Ok(self.meetings.lock().unwrap().iter().find(|m| m.id == id).cloned())
        }

        async fn update(&self, meeting: &Meeting) -> Result<(), RepositoryError> {
            let mut meetings = self.meetings.lock().unwrap();
            if let Some(existing) = meetings.iter_mut().find(|m| m.id == meeting.id) {
                *existing = meeting.clone();
            }
            Ok(())
        }

        async fn find_by_user_and_date(
            &self,
            user_id: UserId,
            date: NaiveDate,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(self
                .meetings
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m.user_id == user_id && m.start_time.date_naive() == date)
                .cloned()
                .collect())
        }

        async fn find_by_user_and_range(
            &self,
            user_id: UserId,
            start: NaiveDate,
            end: NaiveDate,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(self
                .meetings
                .lock()
                .unwrap()
                .iter()
                .filter(|m| {
                    m.user_id == user_id
                        && m.start_time.date_naive() >= start
                        && m.start_time.date_naive() <= end
                })
                .cloned()
                .collect())
        }

        async fn upsert_batch(&self, meetings: &[Meeting]) -> Result<(), RepositoryError> {
            let mut all = self.meetings.lock().unwrap();
            for meeting in meetings {
                match all.iter_mut().find(|m| m.outlook_id == meeting.outlook_id) {
                    Some(existing) => *existing = meeting.clone(),
                    None => all.push(meeting.clone()),
                }
            }
            Ok(())
        }

        async fn delete_stale(
            &self,
            user_id: UserId,
            current_outlook_ids: &[String],
        ) -> Result<u64, RepositoryError> {
            if current_outlook_ids.is_empty() {
                return Ok(0);
            }
            let mut all = self.meetings.lock().unwrap();
            let before = all.len();
            all.retain(|m| m.user_id != user_id || current_outlook_ids.contains(&m.outlook_id));
            Ok((before - all.len()) as u64)
        }

        async fn find_by_project(
            &self,
            user_id: UserId,
            project_id: ProjectId,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(self
                .meetings
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m.user_id == user_id && m.project_id == Some(project_id))
                .cloned()
                .collect())
        }
    }

    /// Wires the four fakes and one seeded rule, and gives each test a small, named
    /// vocabulary (`tick`, `windows`, `set_config`, …) instead of repeating the wiring.
    struct Fixture {
        rules: InMemoryBreakRuleRepository,
        events: InMemoryBreakEventRepository,
        meetings: InMemoryMeetingRepository,
        config: InMemoryConfigRepository,
        notifier: FakeNotifier,
        surface: FakeSurface,
        user_id: UserId,
        /// The wait's poll interval, mutable so a test can starve the wait of wall
        /// clock without starving it of iterations.
        session_poll: Mutex<std::time::Duration>,
    }

    impl Fixture {
        async fn new() -> Self {
            let user_id = Uuid::new_v4();
            let config = InMemoryConfigRepository::default();
            for (key, value) in [
                ("aplan.timezone", "Europe/Paris"),
                // ISO numbers, Monday = 1 — what the settings screen writes and what
                // the live database holds. The fixture used to seed day names, which
                // no part of the cockpit produces, and that is precisely how a
                // configuration the engine could not read went unnoticed.
                ("general.working_days", "1,2,3,4,5"),
                ("workday.morning_start_hour", "8"),
                ("workday.morning_end_hour", "12"),
                ("workday.afternoon_start_hour", "13"),
                ("workday.afternoon_end_hour", "17"),
                (KEY_ENABLED, "true"),
                (KEY_GRACE, "3"),
                (KEY_SNOOZE, "10"),
                // KEY_SHOW_AS is deliberately left unset: seeding it pinned the fixture
                // to a list the product does not ship, and that is exactly how a default
                // that ignored half the calendar went unnoticed. Tests that care about
                // the narrowing set it themselves.
            ] {
                config.set(user_id, key, value).await.unwrap();
            }

            let rules = InMemoryBreakRuleRepository::default();
            let now = Utc::now();
            rules
                .create(&BreakRule {
                    id: Uuid::new_v4(),
                    user_id,
                    kind: BreakKind::Posture,
                    label: "Pause posture".into(),
                    body: "Leve-toi et bouge un peu.".into(),
                    cadence: BreakCadence::Interval { minutes: 30 },
                    // One second: a break the tests can actually sit through. Every
                    // test that runs one to its deadline pays this in wall clock.
                    duration_seconds: 1,
                    priority: 2,
                    enabled: true,
                    urgency: BreakUrgency::Normal,
                    created_at: now,
                    updated_at: now,
                })
                .await
                .unwrap();

            Fixture {
                rules,
                events: InMemoryBreakEventRepository::default(),
                meetings: InMemoryMeetingRepository::default(),
                config,
                notifier: FakeNotifier::default(),
                surface: FakeSurface::default(),
                user_id,
                session_poll: Mutex::new(TEST_POLL),
            }
        }

        fn deps(&self) -> BreakTickDeps<'_> {
            self.deps_with(&self.notifier, &self.surface)
        }

        /// Same wiring with someone else's notifier or surface, so a test can run the
        /// tick against the real `NullNotifier` / `NullSurface` rather than a fake that
        /// imitates them.
        fn deps_with<'a>(
            &'a self,
            notifier: &'a dyn Notifier,
            surface: &'a dyn SurfaceController,
        ) -> BreakTickDeps<'a> {
            BreakTickDeps {
                rules: &self.rules,
                events: &self.events,
                meetings: &self.meetings,
                config: &self.config,
                notifier,
                surface,
                session_poll: *self.session_poll.lock().unwrap(),
            }
        }

        fn set_poll(&self, poll: std::time::Duration) {
            *self.session_poll.lock().unwrap() = poll;
        }

        async fn tick(&self, now: DateTime<Utc>) -> Result<BreakTickReport, AppError> {
            run_break_tick(self.deps(), self.user_id, now).await
        }

        async fn windows(&self, now: DateTime<Utc>) -> Result<Vec<Window>, AppError> {
            resolve_windows(&self.config, self.user_id, now).await
        }

        /// Swap the seeded 30-minute rule for one of a chosen cadence, so a test can
        /// pick which side of the snooze boundary (`BreakRule::allows_snooze`) it
        /// exercises without re-wiring the four fakes.
        fn replace_rule(&self, cadence: BreakCadence) -> BreakRuleId {
            let id = Uuid::new_v4();
            let now = Utc::now();
            *self.rules.rules.lock().unwrap() = vec![BreakRule {
                id,
                user_id: self.user_id,
                kind: BreakKind::Posture,
                label: "Pause posture".into(),
                body: "Leve-toi et bouge un peu.".into(),
                cadence,
                duration_seconds: 1,
                priority: 2,
                enabled: true,
                urgency: BreakUrgency::Normal,
                created_at: now,
                updated_at: now,
            }];
            id
        }

        fn action_keys(&self) -> Vec<String> {
            self.notifier
                .sent
                .lock()
                .unwrap()
                .last()
                .map(|n| n.actions.iter().map(|(key, _)| key.clone()).collect())
                .unwrap_or_default()
        }

        async fn set_config(&self, key: &str, value: &str) {
            self.config.set(self.user_id, key, value).await.unwrap();
        }

        async fn set_last_tick(&self, at: DateTime<Utc>) {
            self.set_config(KEY_LAST_TICK, &at.to_rfc3339()).await;
        }

        async fn last_tick(&self) -> Option<DateTime<Utc>> {
            self.config
                .get(self.user_id, KEY_LAST_TICK)
                .await
                .unwrap()
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc))
        }

        async fn all_events(&self) -> Vec<BreakEvent> {
            self.events.all()
        }

        fn notifier_always_errors(&self) {
            *self.notifier.always_errors.lock().unwrap() = true;
        }

        fn surface_calls(&self) -> Vec<&'static str> {
            self.surface.calls.lock().unwrap().clone()
        }

        /// Stretch the break so the deadline cannot be the reason the wait ended.
        fn set_duration(&self, seconds: u32) {
            for rule in self.rules.rules.lock().unwrap().iter_mut() {
                rule.duration_seconds = seconds;
            }
        }

        /// Seed a session left open by a process that is no longer here. `ends_at` is
        /// optional because a row without one is its own case: a session that can
        /// never end.
        async fn seed_orphan_session(
            &self,
            started_at: DateTime<Utc>,
            ends_at: Option<DateTime<Utc>>,
        ) -> BreakEventId {
            let id = Uuid::new_v4();
            let rule_id = self.rules.rules.lock().unwrap()[0].id;
            self.events
                .create(&BreakEvent {
                    id,
                    user_id: self.user_id,
                    rule_id,
                    due_at: started_at,
                    fired_at: Some(started_at),
                    deferred_until: None,
                    defer_reason: None,
                    suppressed_by_meeting_id: None,
                    outcome: BreakOutcome::Pending,
                    responded_at: None,
                    started_at: Some(started_at),
                    ends_at,
                    created_at: started_at,
                })
                .await
                .unwrap();
            id
        }

        async fn event(&self, id: BreakEventId) -> BreakEvent {
            self.all_events().await.into_iter().find(|e| e.id == id).unwrap()
        }

        async fn add_meeting(
            &self,
            outlook_id: &str,
            start: DateTime<Utc>,
            end: DateTime<Utc>,
            show_as: Option<&str>,
        ) {
            self.meetings
                .upsert_batch(&[Meeting {
                    id: Uuid::new_v4(),
                    user_id: self.user_id,
                    title: "Meeting".into(),
                    start_time: start,
                    end_time: end,
                    location: None,
                    participants: vec![],
                    project_id: None,
                    outlook_id: outlook_id.to_string(),
                    show_as: show_as.map(|s| s.to_string()),
                    created_at: start,
                }])
                .await
                .unwrap();
        }
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 27, h, m, 0).unwrap()
    }

    /// The happy path end to end: a due arrives, a row is written, the notification goes
    /// out, and the user's answer lands back on the row.
    #[tokio::test]
    async fn a_fired_break_is_recorded_and_the_answer_is_written_back() {
        let fixture = Fixture::new().await;                    // seeds one 30-min rule
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("taken".into()));
        fixture.set_last_tick(at(8, 29)).await;

        let report = fixture.tick(at(8, 30)).await.unwrap();

        assert!(report.fired.is_some());
        // Two notifications now: the break itself, then the one that says it is over.
        assert_eq!(fixture.notifier.sent.lock().unwrap().len(), 2);
        let events = fixture.all_events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].outcome, BreakOutcome::Taken);
        assert!(events[0].fired_at.is_some());
        assert!(events[0].responded_at.is_some());
    }

    /// Pressing the button starts a break; it does not record one. The tick only
    /// returns once the break is over, so the row's state *at that instant* is read
    /// from the snapshot the repository fake takes when the session opens.
    #[tokio::test]
    async fn pressing_the_button_opens_a_session_and_leaves_the_outcome_pending() {
        let fixture = Fixture::new().await;
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("taken".into()));
        fixture.set_last_tick(at(8, 29)).await;

        fixture.tick(at(8, 30)).await.unwrap();

        assert_eq!(
            *fixture.events.outcome_when_session_opened.lock().unwrap(),
            Some(BreakOutcome::Pending),
            "`taken` is earned at the deadline, never at the click"
        );
        let event = &fixture.all_events().await[0];
        let started_at = event.started_at.expect("the session was opened");
        let ends_at = event.ends_at.expect("the deadline was frozen");
        assert_eq!(ends_at - started_at, Duration::seconds(1)); // the rule's duration
        assert_eq!(fixture.surface_calls().first(), Some(&"show"));
    }

    /// The label changed because the button no longer means "I took one", it means
    /// "start one now". The key is what is stored and must not move with it.
    #[tokio::test]
    async fn the_take_button_invites_the_break_rather_than_reporting_it() {
        let fixture = Fixture::new().await;
        fixture.set_last_tick(at(8, 29)).await;
        fixture.tick(at(8, 30)).await.unwrap();
        let sent = fixture.notifier.sent.lock().unwrap();
        assert_eq!(sent[0].actions[0], ("taken".into(), "Prendre la pause".into()));
    }

    /// Reaching the deadline is the only thing that earns `taken`, and it is what takes
    /// the overlay back down.
    #[tokio::test]
    async fn reaching_the_deadline_records_taken_and_lowers_the_surface() {
        let fixture = Fixture::new().await;
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("taken".into()));
        fixture.set_last_tick(at(8, 29)).await;

        fixture.tick(at(8, 30)).await.unwrap();

        let event = &fixture.all_events().await[0];
        assert_eq!(event.outcome, BreakOutcome::Taken);
        assert!(event.responded_at.is_some());
        assert_eq!(fixture.surface_calls(), vec!["show", "hide"]);
        // No button on the closing notification: an action is what puts `notify-send`
        // into `--wait`, and this one must never hold the scheduler.
        let sent = fixture.notifier.sent.lock().unwrap();
        assert_eq!(sent.last().unwrap().title, "Pause terminée");
        assert!(sent.last().unwrap().actions.is_empty());
        assert_eq!(sent.last().unwrap().urgency, BreakUrgency::Low);
    }

    /// "J'y retourne" reaches the row through `endBreak`, not through this tick. The
    /// wait must stop and, above all, must not write `taken` over what was recorded.
    #[tokio::test]
    async fn an_abandon_written_during_the_wait_ends_it_without_overwriting_it() {
        let fixture = Fixture::new().await;
        fixture.set_duration(600); // long enough that the deadline cannot be the exit
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("taken".into()));
        *fixture.events.abandon_next_lookup.lock().unwrap() = true;
        fixture.set_last_tick(at(8, 29)).await;

        fixture.tick(at(8, 30)).await.unwrap();

        let event = &fixture.all_events().await[0];
        assert_eq!(event.outcome, BreakOutcome::Abandoned);
        assert!(event.responded_at.is_some());
        assert_eq!(fixture.surface_calls(), vec!["show", "hide"]);
    }

    /// A clock that steps backwards — an NTP correction, a resume from suspend — makes
    /// `now >= ends_at` unreachable for the whole offset, and the tick holding the wait
    /// holds the entire scheduler for exactly that long. So the loop is bounded by its
    /// own iterations as well as by the deadline, and exhausting the budget is treated
    /// as reaching the deadline.
    ///
    /// The clock cannot be moved from a test, so the budget is spent from the other
    /// side: a poll interval of zero burns iterations far faster than the budget assumes,
    /// since it is floored at five milliseconds and `sleep(0)` costs about one. Eight
    /// hundred polls of roughly a millisecond, against a deadline two seconds away — so
    /// a two-second break that ends inside 1.5 s ended on its budget and on nothing
    /// else. Measured at 0.86 s.
    #[tokio::test]
    async fn the_wait_is_bounded_by_its_iterations_not_only_by_its_deadline() {
        let fixture = Fixture::new().await;
        fixture.set_duration(2);
        fixture.set_poll(std::time::Duration::ZERO);
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("taken".into()));
        fixture.set_last_tick(at(8, 29)).await;

        let started = std::time::Instant::now();
        fixture.tick(at(8, 30)).await.unwrap();
        let elapsed = started.elapsed();

        assert!(
            elapsed < std::time::Duration::from_millis(1500),
            "the wait must end on its iteration budget, not on its deadline (took {elapsed:?})"
        );
        assert_eq!(fixture.all_events().await[0].outcome, BreakOutcome::Taken);
        assert_eq!(fixture.surface_calls(), vec!["show", "hide"]);
    }

    /// The wait can fail: a transient database error in the middle of a break. The error
    /// must still reach the caller — nothing here can honestly report success — and the
    /// overlay must still come down. Left up, it outlives the failure by a whole tick
    /// interval, which is thirty seconds to five minutes of a frozen countdown filling
    /// the screen.
    #[tokio::test]
    async fn a_database_error_during_the_wait_still_lowers_the_surface() {
        let fixture = Fixture::new().await;
        fixture.set_duration(600); // long enough that the deadline cannot be the exit
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("taken".into()));
        *fixture.events.fails_lookups_once_a_session_is_open.lock().unwrap() = true;
        fixture.set_last_tick(at(8, 29)).await;

        let report = fixture.tick(at(8, 30)).await;

        assert!(report.is_err(), "the failure must reach the caller, not be swallowed");
        assert_eq!(
            fixture.surface_calls(),
            vec!["show", "hide"],
            "the overlay comes down on every exit, including the failing one"
        );
    }

    /// The API went down mid-break and the countdown ran out while it was away. The
    /// break happened: nothing about a restart makes it not have happened.
    #[tokio::test]
    async fn an_orphan_session_past_its_deadline_is_recovered_as_taken() {
        let fixture = Fixture::new().await;
        let orphan = fixture.seed_orphan_session(at(8, 0), Some(at(8, 5))).await;
        fixture.set_last_tick(at(8, 30)).await; // nothing new is due this tick

        let report = fixture.tick(at(8, 30)).await.unwrap();

        assert_eq!(report.recovered, 1);
        assert_eq!(fixture.event(orphan).await.outcome, BreakOutcome::Taken);
        assert!(fixture.event(orphan).await.responded_at.is_some());
        assert_eq!(fixture.surface_calls(), vec!["hide"]);
    }

    /// A session whose deadline is still ahead belongs to whoever opened it, and this
    /// tick is not necessarily that process: "a single loop" is true of one process, not
    /// of one database, and a development `cargo run -p api` beside the installed
    /// service is two. Claiming it would abandon a live break and pull the overlay off
    /// the screen mid-countdown.
    #[tokio::test]
    async fn a_session_still_counting_down_is_left_to_its_owner() {
        let fixture = Fixture::new().await;
        fixture.set_config(KEY_ENABLED, "false").await;
        let running = fixture.seed_orphan_session(at(8, 29), Some(at(9, 0))).await;

        let report = fixture.tick(at(8, 30)).await.unwrap();

        assert_eq!(report.recovered, 0);
        assert_eq!(fixture.event(running).await.outcome, BreakOutcome::Pending);
        assert!(
            fixture.surface_calls().is_empty(),
            "an overlay somebody else raised is not this tick's to lower"
        );
    }

    /// The same row, one tick later, with the deadline now behind it: this is where it
    /// gets closed, and as `taken` — the countdown was launched and ran to its end.
    /// Deferring the recovery costs at most one tick and never mislabels a break.
    ///
    /// Run with the routine switched off, which is also how this asserts that recovery
    /// happens before anything else the tick does — a break left open by a restart must
    /// not stay open just because the user disabled breaks in the meantime.
    #[tokio::test]
    async fn the_same_session_is_recovered_once_its_deadline_has_passed() {
        let fixture = Fixture::new().await;
        fixture.set_config(KEY_ENABLED, "false").await;
        let running = fixture.seed_orphan_session(at(8, 29), Some(at(9, 0))).await;

        assert_eq!(fixture.tick(at(8, 30)).await.unwrap().recovered, 0);
        assert_eq!(fixture.tick(at(9, 1)).await.unwrap().recovered, 1);

        assert_eq!(fixture.event(running).await.outcome, BreakOutcome::Taken);
        assert_eq!(fixture.surface_calls(), vec!["hide"]);
    }

    /// A session with no deadline describes a break that can never end — `start_session`
    /// writes both instants or neither, so this is a row nothing coherent produced. It
    /// cannot be waited on and it cannot be dated, so it is closed on sight, and
    /// `abandoned` is the only thing left to call it.
    #[tokio::test]
    async fn a_session_with_no_deadline_is_recovered_as_abandoned() {
        let fixture = Fixture::new().await;
        fixture.set_config(KEY_ENABLED, "false").await;
        let dangling = fixture.seed_orphan_session(at(8, 29), None).await;

        let report = fixture.tick(at(8, 30)).await.unwrap();

        assert_eq!(report.recovered, 1);
        assert_eq!(fixture.event(dangling).await.outcome, BreakOutcome::Abandoned);
        assert_eq!(fixture.surface_calls(), vec!["hide"]);
    }

    /// Deferring or refusing resolves the slot on the spot. There is no break to serve,
    /// so there is nothing to put on screen.
    #[tokio::test]
    async fn a_snooze_opens_no_session_and_shows_nothing() {
        let fixture = Fixture::new().await;
        fixture.replace_rule(BreakCadence::Interval { minutes: 60 });
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("snoozed".into()));
        fixture.set_last_tick(at(7, 59)).await;

        fixture.tick(at(8, 0)).await.unwrap();

        let slot = fixture
            .all_events()
            .await
            .into_iter()
            .find(|e| e.outcome == BreakOutcome::Snoozed)
            .expect("the slot was snoozed");
        assert!(slot.started_at.is_none());
        assert!(slot.ends_at.is_none());
        assert!(fixture.surface_calls().is_empty());
    }

    #[tokio::test]
    async fn a_skip_opens_no_session_and_shows_nothing() {
        let fixture = Fixture::new().await;
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("skipped".into()));
        fixture.set_last_tick(at(8, 29)).await;

        fixture.tick(at(8, 30)).await.unwrap();

        let event = &fixture.all_events().await[0];
        assert_eq!(event.outcome, BreakOutcome::Skipped);
        assert!(event.started_at.is_none());
        assert!(fixture.surface_calls().is_empty());
    }

    /// No compositor, no overlay — and still a break. The countdown belongs to the
    /// backend, so a surface that will not open costs the user their visual and
    /// nothing else.
    #[tokio::test]
    async fn a_surface_that_cannot_be_shown_still_serves_the_break_to_its_end() {
        let fixture = Fixture::new().await;
        *fixture.surface.fails_to_show.lock().unwrap() = true;
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("taken".into()));
        fixture.set_last_tick(at(8, 29)).await;

        let report = fixture.tick(at(8, 30)).await;

        assert!(report.is_ok(), "a missing overlay must not fail the tick");
        assert_eq!(fixture.all_events().await[0].outcome, BreakOutcome::Taken);
        assert_eq!(fixture.surface_calls(), vec!["show", "hide"]);
    }

    /// The headless wiring is a whole substitution, not just a type that compiles: a
    /// break served with the real `NullSurface` still runs and still records.
    ///
    /// The empty `surface_calls` is the load-bearing assertion, and not about the
    /// production code: it proves the tick really ran against `NullSurface` and not
    /// against the fixture's own fake. Without it the test passes just as well when
    /// the substitution never happens, which is exactly how it used to.
    #[tokio::test]
    async fn a_break_served_without_any_surface_is_still_recorded() {
        let fixture = Fixture::new().await;
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("taken".into()));
        fixture.set_last_tick(at(8, 29)).await;

        let report = run_break_tick(
            fixture.deps_with(&fixture.notifier, &NullSurface),
            fixture.user_id,
            at(8, 30),
        )
        .await
        .unwrap();

        assert!(
            fixture.surface_calls().is_empty(),
            "the tick must have run against NullSurface, not the fixture's fake"
        );
        let events = fixture.all_events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(report.fired, Some(events[0].id));
        assert_eq!(events[0].outcome, BreakOutcome::Taken);
        assert!(events[0].fired_at.is_some(), "the break reached the user");
        assert!(events[0].responded_at.is_some(), "and the user answered it");
    }

    /// Dismissing without choosing is `ignored`, not `skipped`: the distinction is the
    /// whole reason both outcomes exist.
    #[tokio::test]
    async fn a_dismissed_notification_is_recorded_as_ignored() {
        let fixture = Fixture::new().await;
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Dismissed);
        fixture.set_last_tick(at(8, 29)).await;
        fixture.tick(at(8, 30)).await.unwrap();
        assert_eq!(fixture.all_events().await[0].outcome, BreakOutcome::Ignored);
    }

    /// "Plus tard" resolves the current slot and arms a fresh deferral, which is how a
    /// snooze re-enters `decide` without being a special case there.
    ///
    /// On an hourly rule, because that is now the only kind that offers the button: a
    /// ten-minute deferral off an hourly grid still lands fifty minutes clear of the
    /// rule's own next due.
    #[tokio::test]
    async fn a_snooze_resolves_the_slot_and_arms_a_new_deferral() {
        let fixture = Fixture::new().await;
        fixture.replace_rule(BreakCadence::Interval { minutes: 60 });
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("snoozed".into()));
        fixture.set_last_tick(at(7, 59)).await;
        fixture.tick(at(8, 0)).await.unwrap();
        let events = fixture.all_events().await;
        assert_eq!(events.len(), 2, "the snoozed slot plus its follow-up");
        let follow_up = events.iter().find(|e| e.outcome == BreakOutcome::Pending).unwrap();
        assert_eq!(follow_up.defer_reason, Some(DeferReason::Snooze));
        assert_eq!(follow_up.deferred_until, Some(at(8, 10)));   // snooze_minutes = 10
    }

    /// A break that repeats more often than hourly is taken or skipped, nothing else:
    /// the deferral it used to offer re-queued on top of a grid already firing, and
    /// that compounding is what made the routine unusable on its first afternoon.
    #[tokio::test]
    async fn a_sub_hourly_rule_offers_no_deferral_button() {
        let fixture = Fixture::new().await;
        fixture.replace_rule(BreakCadence::Interval { minutes: 15 });
        fixture.set_last_tick(at(8, 29)).await;
        fixture.tick(at(8, 30)).await.unwrap();
        assert_eq!(fixture.action_keys(), vec!["taken", "skipped"]);
    }

    #[tokio::test]
    async fn an_hourly_rule_still_offers_all_three_buttons() {
        let fixture = Fixture::new().await;
        fixture.replace_rule(BreakCadence::Interval { minutes: 60 });
        fixture.set_last_tick(at(7, 59)).await;
        fixture.tick(at(8, 0)).await.unwrap();
        assert_eq!(fixture.action_keys(), vec!["taken", "snoozed", "skipped"]);
    }

    /// The button is gone from the notification, but a notification daemon replaying a
    /// stale action can still send its key. Honouring it would resurrect exactly the
    /// behaviour just removed, so the answer is recorded as a deliberate skip and no
    /// follow-up is armed.
    #[tokio::test]
    async fn a_stale_snooze_action_on_a_short_cadence_is_recorded_as_skipped() {
        let fixture = Fixture::new().await;
        fixture.replace_rule(BreakCadence::Interval { minutes: 15 });
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("snoozed".into()));
        fixture.set_last_tick(at(8, 29)).await;
        fixture.tick(at(8, 30)).await.unwrap();

        let events = fixture.all_events().await;
        assert_eq!(events.len(), 1, "no follow-up deferral may be created");
        assert_eq!(events[0].outcome, BreakOutcome::Skipped);
        assert!(events[0].responded_at.is_some());
        assert!(
            !events.iter().any(|e| e.defer_reason == Some(DeferReason::Snooze)),
            "a snooze deferral must not survive the restriction"
        );
    }

    /// The tick is the only writer of `last_tick`, and it must advance it even when it
    /// decided nothing — otherwise re-enabling after a pause replays days of dues.
    #[tokio::test]
    async fn the_tick_advances_last_tick_even_when_disabled() {
        let fixture = Fixture::new().await;
        fixture.set_config("aplan.breaks.enabled", "false").await;
        fixture.set_last_tick(at(8, 0)).await;
        let report = fixture.tick(at(10, 0)).await.unwrap();
        assert!(report.fired.is_none());
        assert!(fixture.all_events().await.is_empty());
        assert_eq!(fixture.last_tick().await, Some(at(10, 0)));
    }

    /// A first-ever run must not invent a backlog.
    #[tokio::test]
    async fn a_missing_last_tick_starts_the_clock_at_now() {
        let fixture = Fixture::new().await;
        let report = fixture.tick(at(11, 0)).await.unwrap();
        assert!(report.fired.is_none());
        assert_eq!(fixture.last_tick().await, Some(at(11, 0)));
    }

    /// Running the same tick twice must not double anything.
    #[tokio::test]
    async fn a_repeated_tick_is_inert() {
        let fixture = Fixture::new().await;
        fixture.set_last_tick(at(8, 29)).await;
        fixture.tick(at(8, 30)).await.unwrap();
        let before = fixture.all_events().await.len();
        fixture.tick(at(8, 30)).await.unwrap();
        assert_eq!(fixture.all_events().await.len(), before);
    }

    /// Delivery failure keeps the books and lets the expiry rule clean up; it must not
    /// fail the tick.
    #[tokio::test]
    async fn a_notifier_error_leaves_the_event_unfired_but_does_not_fail_the_tick() {
        let fixture = Fixture::new().await;
        fixture.notifier_always_errors();
        fixture.set_last_tick(at(8, 29)).await;
        let report = fixture.tick(at(8, 30)).await;
        assert!(report.is_ok());
        let events = fixture.all_events().await;
        assert_eq!(events[0].outcome, BreakOutcome::Pending);
        assert!(events[0].fired_at.is_none());
    }

    /// A calendar entry is a meeting whatever Outlook thinks of the user's availability.
    /// `tentative` marks an invitation that has not been answered, not one that will not
    /// be attended: the weekly the user sits through every Monday is `tentative`.
    #[tokio::test]
    async fn a_tentative_meeting_suppresses() {
        let fixture = Fixture::new().await;
        fixture.add_meeting("m1", at(8, 20), at(9, 0), Some("tentative")).await;
        fixture.set_last_tick(at(8, 29)).await;
        let report = fixture.tick(at(8, 30)).await.unwrap();
        assert!(report.fired.is_none());
        assert_eq!(report.deferred, 1);
    }

    /// `free` reads as "does not block my calendar", which recurring internal meetings
    /// are routinely marked. It said nothing about whether the user is in the room, and
    /// taking it as permission to fire is what put a popup on screen mid-meeting.
    #[tokio::test]
    async fn a_free_meeting_suppresses() {
        let fixture = Fixture::new().await;
        fixture.add_meeting("m1", at(8, 20), at(9, 0), Some("free")).await;
        fixture.set_last_tick(at(8, 29)).await;
        let report = fixture.tick(at(8, 30)).await.unwrap();
        assert!(report.fired.is_none());
        assert_eq!(report.deferred, 1);
    }

    /// Only meetings whose show_as is in the configured list suppress.
    #[tokio::test]
    async fn a_meeting_with_an_unlisted_show_as_does_not_suppress() {
        let fixture = Fixture::new().await;
        fixture
            .add_meeting("m1", at(8, 20), at(9, 0), Some("workingElsewhere"))
            .await;
        fixture.set_last_tick(at(8, 29)).await;
        let report = fixture.tick(at(8, 30)).await.unwrap();
        assert!(report.fired.is_some());
    }

    /// The list stays a knob. The default is wide because a missed break costs less than
    /// a popup during a client call, but narrowing it back down still decides the tick.
    #[tokio::test]
    async fn a_narrowed_show_as_list_lets_a_tentative_meeting_through() {
        let fixture = Fixture::new().await;
        fixture.set_config(KEY_SHOW_AS, "busy").await;
        fixture.add_meeting("m1", at(8, 20), at(9, 0), Some("tentative")).await;
        fixture.set_last_tick(at(8, 29)).await;
        let report = fixture.tick(at(8, 30)).await.unwrap();
        assert!(report.fired.is_some());
    }

    #[tokio::test]
    async fn a_busy_meeting_suppresses_and_defers() {
        let fixture = Fixture::new().await;
        fixture.add_meeting("m1", at(8, 20), at(9, 0), Some("busy")).await;
        fixture.set_last_tick(at(8, 29)).await;
        let report = fixture.tick(at(8, 30)).await.unwrap();
        assert!(report.fired.is_none());
        assert_eq!(report.deferred, 1);
        let events = fixture.all_events().await;
        assert_eq!(events[0].deferred_until, Some(at(9, 3)));   // grace = 3
        assert_eq!(events[0].suppressed_by_meeting_id.as_deref(), Some("m1"));
    }

    /// Windows come from the existing workday config, read in the user's timezone.
    #[tokio::test]
    async fn windows_come_from_the_workday_config_in_local_time() {
        let fixture = Fixture::new().await;   // Europe/Paris, 08-12 and 13-17 local
        let windows = fixture.windows(at(10, 0)).await.unwrap();
        assert_eq!(windows.len(), 2);
        // August in Paris is UTC+2.
        assert_eq!(windows[0].start, at(6, 0));
        assert_eq!(windows[0].end, at(10, 0));
        assert_eq!(windows[1].start, at(11, 0));
        assert_eq!(windows[1].end, at(15, 0));
    }

    /// The number form is the real one: it is what the settings screen writes and what
    /// the database holds. Read it wrong and there are no windows, nothing fires, and
    /// the tick still reports success — the whole routine dead and looking healthy.
    #[tokio::test]
    async fn iso_numbered_working_days_yield_windows_on_a_working_day() {
        let fixture = Fixture::new().await;
        // 2026-08-27 is a Thursday: ISO 4.
        fixture.set_config("general.working_days", "1,2,4").await;
        assert_eq!(fixture.windows(at(10, 0)).await.unwrap().len(), 2);

        fixture.set_config("general.working_days", "1,2,5").await;
        assert!(fixture.windows(at(10, 0)).await.unwrap().is_empty());
    }

    /// Day names are a tolerated alias for a hand-edited configuration. Nothing in the
    /// cockpit writes them, so this is the compatibility path, not the main one.
    #[tokio::test]
    async fn day_names_are_accepted_as_an_alias() {
        let fixture = Fixture::new().await;
        fixture.set_config("general.working_days", "mon,tue,wed,thu,fri").await;
        assert_eq!(fixture.windows(at(10, 0)).await.unwrap().len(), 2);
    }

    /// A day not in `general.working_days` has no windows, so nothing can fire.
    #[tokio::test]
    async fn a_non_working_day_yields_no_windows() {
        let fixture = Fixture::new().await;
        // 2026-08-27 is a Thursday (4); make it a Friday-only config instead.
        fixture.set_config("general.working_days", "5").await;
        assert!(fixture.windows(at(10, 0)).await.unwrap().is_empty());
    }

    /// A value in no recognised spelling parses to nothing, which is indistinguishable
    /// from a day off — except that it is never right. It yields no windows, and the
    /// `warn` beside this branch is what keeps that from passing for a quiet routine.
    #[tokio::test]
    async fn an_unreadable_working_days_value_yields_no_windows() {
        let fixture = Fixture::new().await;
        fixture.set_config("general.working_days", "lun;mar;jeu").await;
        assert!(fixture.windows(at(10, 0)).await.unwrap().is_empty());
    }

    /// A headless run must not invent user behaviour. `NullNotifier` shows nothing, so
    /// the slot was never seen: it stays unfired and closes as `expired`, the one
    /// outcome counted on neither side of adherence. `ignored` would count.
    #[tokio::test]
    async fn a_break_that_could_not_be_shown_is_expired_not_ignored() {
        let fixture = Fixture::new().await;
        fixture.set_last_tick(at(8, 29)).await;

        let report = run_break_tick(
            fixture.deps_with(&NullNotifier, &fixture.surface),
            fixture.user_id,
            at(8, 30),
        )
        .await
        .unwrap();

        assert!(report.fired.is_none(), "nothing reached a screen, so nothing fired");
        assert_eq!(report.expired, 1);
        let events = fixture.all_events().await;
        assert_eq!(events.len(), 1);
        assert!(events[0].fired_at.is_none(), "fired_at must stay NULL");
        assert_eq!(events[0].outcome, BreakOutcome::Expired);
        assert!(!events[0].outcome.counts_towards_adherence());
    }
}
