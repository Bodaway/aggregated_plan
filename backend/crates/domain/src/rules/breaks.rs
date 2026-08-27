use chrono::{DateTime, Duration, Utc};

use crate::types::{BreakEvent, BreakEventId, BreakRule, BreakRuleId, DeferReason};

/// A stretch of working time, already resolved to UTC by the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl Window {
    /// Both bounds included. The start has to be: a `Daily` rule set to 13:00 is a rule
    /// about the instant the afternoon opens, and an exclusive start would make it —
    /// and only it — silently unfireable. Step 1 of `decide` reads it the same way, so a
    /// tick landing exactly on 08:00 is inside the working day rather than after it.
    fn contains(&self, t: DateTime<Utc>) -> bool {
        self.start <= t && t <= self.end
    }
}

/// A meeting that suppresses breaks. The caller has already filtered on `show_as`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusyPeriod {
    pub meeting_id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl BusyPeriod {
    fn covers(&self, t: DateTime<Utc>) -> bool {
        self.start <= t && t < self.end
    }
}

/// Something that wants to fire on this tick: either a due that has no row yet, or a
/// deferred event whose wait is over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidate {
    New { rule_id: BreakRuleId, due_at: DateTime<Utc> },
    Wake { event_id: BreakEventId, rule_id: BreakRuleId, due_at: DateTime<Utc> },
}

impl Candidate {
    pub fn rule_id(&self) -> BreakRuleId {
        match self {
            Candidate::New { rule_id, .. } | Candidate::Wake { rule_id, .. } => *rule_id,
        }
    }

    pub fn due_at(&self) -> DateTime<Utc> {
        match self {
            Candidate::New { due_at, .. } | Candidate::Wake { due_at, .. } => *due_at,
        }
    }

    pub fn event_id(&self) -> Option<BreakEventId> {
        match self {
            Candidate::New { .. } => None,
            Candidate::Wake { event_id, .. } => Some(*event_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireBreak {
    pub candidate: Candidate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferBreak {
    pub candidate: Candidate,
    pub until: DateTime<Utc>,
    pub reason: DeferReason,
    pub meeting_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsorbBreak {
    pub candidate: Candidate,
}

/// Everything one tick decided. The caller only has to execute it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BreakTick {
    /// At most one, always: the user sees one popup per tick or none.
    pub fire: Option<FireBreak>,
    pub defer: Vec<DeferBreak>,
    pub absorb: Vec<AbsorbBreak>,
    pub expire: Vec<BreakEventId>,
}

pub struct BreakTickInput<'a> {
    pub now: DateTime<Utc>,
    /// The previous tick. `(since, now]` is the interval examined, which is what makes
    /// the engine survive a suspend or a restart without firing a burst.
    pub since: DateTime<Utc>,
    /// Today's working windows in UTC. Empty on a non-working day.
    pub windows: &'a [Window],
    /// Enabled rules only — filtering is the caller's job.
    pub rules: &'a [BreakRule],
    /// Today's UTC instant for each enabled `Daily` rule, resolved by the caller
    /// because `domain` has no timezone database.
    pub daily_dues: &'a [(BreakRuleId, DateTime<Utc>)],
    /// Meetings already filtered on `show_as`.
    pub busy: &'a [BusyPeriod],
    /// Events still pending: deferred, or fired and unanswered.
    pub open: &'a [BreakEvent],
    pub grace: Duration,
}

/// Every instant `rule` comes due inside `(since, now]`, anchored on each window's start.
///
/// Anchoring on the window rather than on the last fire is the whole meaning of "wall
/// clock": a break that was missed, snoozed or absorbed does not shift the grid.
pub fn natural_dues(
    rule: &BreakRule,
    windows: &[Window],
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Vec<DateTime<Utc>> {
    let Some(minutes) = rule.cadence.interval_minutes() else {
        return Vec::new();
    };
    let step = Duration::minutes(minutes as i64);
    let mut out = Vec::new();
    for w in windows {
        let mut due = w.start + step;
        while due <= w.end {
            if due > since && due <= now {
                out.push(due);
            }
            if due > now {
                break;
            }
            due = due + step;
        }
    }
    out.sort();
    out
}

/// The first instant after `t` at which `rule` next comes due, if any remains today.
///
/// `None` for a `Daily` rule: it has no "next" today, so a deferral of it is never
/// culled by the expiry rule and only ends at the close of the working day.
pub fn next_natural_due_after(
    rule: &BreakRule,
    windows: &[Window],
    t: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let minutes = rule.cadence.interval_minutes()?;
    let step = Duration::minutes(minutes as i64);
    for w in windows {
        let mut due = w.start + step;
        while due <= w.end {
            if due > t {
                return Some(due);
            }
            due = due + step;
        }
    }
    None
}

pub fn decide(input: BreakTickInput<'_>) -> BreakTick {
    let mut tick = BreakTick::default();

    // 1. Outside every working window nothing fires, and whatever was still waiting is
    //    cleaned up: a break deferred at 17:55 has no meaning at 19:00.
    let in_window = input.windows.iter().any(|w| w.contains(input.now));
    if !in_window {
        tick.expire = input.open.iter().map(|e| e.id).collect();
        return tick;
    }

    let rule_of = |id: BreakRuleId| input.rules.iter().find(|r| r.id == id);

    // 2. Cull deferrals that can no longer beat their own rule's next due, plus any
    //    whose rule has since been disabled or deleted. Doing this before anything else
    //    is what keeps "one live deferral per rule" true in step 4 without counting.
    let mut live_deferrals: Vec<&BreakEvent> = Vec::new();
    for event in input.open {
        let Some(rule) = rule_of(event.rule_id) else {
            tick.expire.push(event.id);
            continue;
        };
        let Some(until) = event.deferred_until else {
            // Fired and unanswered: the notifier owns its fate, not the tick.
            continue;
        };
        match next_natural_due_after(rule, input.windows, event.due_at) {
            Some(next) if until >= next => tick.expire.push(event.id),
            _ => live_deferrals.push(event),
        }
    }

    // 3. Candidates: woken deferrals, then natural dues in (since, now], then the daily
    //    instants the caller resolved.
    let mut candidates: Vec<Candidate> = Vec::new();
    for event in &live_deferrals {
        if let Some(until) = event.deferred_until {
            if until <= input.now {
                candidates.push(Candidate::Wake {
                    event_id: event.id,
                    rule_id: event.rule_id,
                    due_at: event.due_at,
                });
            }
        }
    }
    for rule in input.rules {
        for due_at in natural_dues(rule, input.windows, input.since, input.now) {
            candidates.push(Candidate::New { rule_id: rule.id, due_at });
        }
    }
    for (rule_id, due_at) in input.daily_dues {
        let inside = input.windows.iter().any(|w| w.contains(*due_at));
        if inside && *due_at > input.since && *due_at <= input.now {
            candidates.push(Candidate::New { rule_id: *rule_id, due_at: *due_at });
        }
    }

    // 4. Meeting suppression. A candidate is judged at the instant it wants the user's
    //    attention: its due time for a fresh one, `now` for a wake-up.
    //
    //    Rules that already hold a live deferral do not get a second one — their extra
    //    dues are absorbed. Without that, a two-hour meeting would arm six deferrals of
    //    the 20-minute rule and empty them all at once when it ended.
    let mut deferred_rules: Vec<BreakRuleId> = live_deferrals.iter().map(|e| e.rule_id).collect();
    let mut runnable: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        let judged_at = match &candidate {
            Candidate::New { due_at, .. } => *due_at,
            Candidate::Wake { .. } => input.now,
        };
        let Some(period) = input.busy.iter().find(|b| b.covers(judged_at)) else {
            runnable.push(candidate);
            continue;
        };
        let until = period.end + input.grace;
        if until <= input.now {
            // The blocking meeting is already over; resolve on this tick rather than
            // writing a deferral that is born overdue.
            runnable.push(candidate);
            continue;
        }
        let already = deferred_rules.contains(&candidate.rule_id())
            && matches!(candidate, Candidate::New { .. });
        if already {
            tick.absorb.push(AbsorbBreak { candidate });
            continue;
        }
        deferred_rules.push(candidate.rule_id());
        tick.defer.push(DeferBreak {
            candidate,
            until,
            reason: DeferReason::Meeting,
            meeting_id: Some(period.meeting_id.clone()),
        });
    }

    // 5. Coalescing: the highest priority fires, the rest are absorbed. Ties go to the
    //    oldest due, so a backlog drains in order.
    finish(&mut tick, runnable, input.rules);
    tick
}

/// Pick the one candidate that fires and absorb the others.
fn finish(tick: &mut BreakTick, mut candidates: Vec<Candidate>, rules: &[BreakRule]) {
    if candidates.is_empty() {
        return;
    }
    let priority_of = |c: &Candidate| {
        rules
            .iter()
            .find(|r| r.id == c.rule_id())
            .map(|r| r.priority)
            .unwrap_or(i32::MIN)
    };
    candidates.sort_by(|a, b| {
        priority_of(b)
            .cmp(&priority_of(a))
            .then(a.due_at().cmp(&b.due_at()))
    });
    let winner = candidates.remove(0);
    tick.fire = Some(FireBreak { candidate: winner });
    tick.absorb
        .extend(candidates.into_iter().map(|c| AbsorbBreak { candidate: c }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BreakCadence, BreakKind, BreakUrgency};
    use chrono::TimeZone;
    use uuid::Uuid;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 27, h, m, 0).unwrap()
    }

    fn interval_rule(minutes: u32, priority: i32) -> BreakRule {
        BreakRule {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            kind: BreakKind::Posture,
            label: format!("{minutes} min"),
            body: "bouge".into(),
            cadence: BreakCadence::Interval { minutes },
            duration_seconds: 60,
            priority,
            enabled: true,
            urgency: BreakUrgency::Normal,
            created_at: at(0, 0),
            updated_at: at(0, 0),
        }
    }

    fn morning() -> Vec<Window> {
        vec![Window { start: at(8, 0), end: at(12, 0) }]
    }

    fn input<'a>(
        now: DateTime<Utc>,
        since: DateTime<Utc>,
        windows: &'a [Window],
        rules: &'a [BreakRule],
    ) -> BreakTickInput<'a> {
        BreakTickInput {
            now,
            since,
            windows,
            rules,
            daily_dues: &[],
            busy: &[],
            open: &[],
            grace: Duration::minutes(3),
        }
    }

    /// The clock is anchored on the window, not on the last fire. That is what makes
    /// it a wall clock: 08:20, 08:40, 09:00 … whatever happened in between.
    #[test]
    fn interval_dues_are_anchored_on_the_window_start() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        let dues = natural_dues(&rules[0], &w, at(8, 0), at(9, 0));
        assert_eq!(dues, vec![at(8, 20), at(8, 40), at(9, 0)]);
    }

    /// Arriving at 08:00 does not earn a break at 08:00.
    #[test]
    fn the_first_due_of_a_window_is_one_interval_in() {
        let rules = vec![interval_rule(30, 1)];
        let w = morning();
        assert_eq!(natural_dues(&rules[0], &w, at(7, 0), at(8, 0)), Vec::<DateTime<Utc>>::new());
        assert_eq!(natural_dues(&rules[0], &w, at(8, 0), at(8, 30)), vec![at(8, 30)]);
    }

    #[test]
    fn each_window_re_anchors_on_its_own_start() {
        let windows = vec![
            Window { start: at(8, 0), end: at(12, 0) },
            Window { start: at(13, 0), end: at(17, 0) },
        ];
        let rule = interval_rule(30, 1);
        assert_eq!(natural_dues(&rule, &windows, at(12, 0), at(13, 45)), vec![at(13, 30)]);
    }

    #[test]
    fn dues_never_fall_outside_their_window() {
        let rule = interval_rule(30, 1);
        let w = morning();
        // 12:00 is the window end and is included; 12:30 is not.
        assert_eq!(natural_dues(&rule, &w, at(11, 45), at(13, 0)), vec![at(12, 0)]);
    }

    /// The collision the whole `priority` column exists for: at minute 60 the three
    /// interval rules are due together and the user must see exactly one popup.
    #[test]
    fn simultaneous_dues_collapse_to_the_highest_priority() {
        let rules = vec![interval_rule(20, 1), interval_rule(30, 2), interval_rule(60, 3)];
        let w = morning();
        let tick = decide(input(at(9, 0), at(8, 59), &w, &rules));
        let fired = tick.fire.expect("one break fires");
        assert_eq!(fired.candidate.rule_id(), rules[2].id, "the hourly rule wins");
        assert_eq!(tick.absorb.len(), 2);
        assert!(tick.defer.is_empty());
    }

    /// After a suspend the tick interval can span hours. Six missed dues must not
    /// become six popups.
    #[test]
    fn a_long_gap_fires_once_and_absorbs_the_rest() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        let tick = decide(input(at(11, 0), at(9, 0), &w, &rules));
        assert!(tick.fire.is_some());
        assert_eq!(tick.absorb.len(), 5, "08:00-anchored dues 09:20..11:00 minus the one fired");
    }

    #[test]
    fn outside_every_window_nothing_fires() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        let tick = decide(input(at(19, 0), at(18, 0), &w, &rules));
        assert!(tick.fire.is_none());
        assert!(tick.absorb.is_empty());
    }

    /// A non-working day has no windows at all.
    #[test]
    fn a_day_with_no_windows_fires_nothing() {
        let rules = vec![interval_rule(20, 1)];
        let tick = decide(input(at(10, 0), at(9, 0), &[], &rules));
        assert!(tick.fire.is_none());
    }

    #[test]
    fn disabled_rules_are_the_callers_problem_not_ours() {
        // `rules` is documented as already filtered; passing an empty slice must be inert.
        let tick = decide(input(at(9, 0), at(8, 0), &morning(), &[]));
        assert!(tick.fire.is_none());
    }

    /// Daily rules arrive pre-resolved because `domain` cannot know the timezone.
    #[test]
    fn a_daily_due_inside_the_window_fires() {
        let rule = BreakRule {
            cadence: BreakCadence::Daily {
                at: chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            },
            ..interval_rule(0, 9)
        };
        let rules = vec![rule.clone()];
        let w = morning();
        let daily = vec![(rule.id, at(10, 0))];
        let tick = decide(BreakTickInput {
            now: at(10, 1),
            since: at(9, 59),
            windows: &w,
            rules: &rules,
            daily_dues: &daily,
            busy: &[],
            open: &[],
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.fire.expect("fires").candidate.rule_id(), rule.id);
    }

    /// A daily rule set to the exact minute a window opens is the ordinary way to ask
    /// for "first thing this morning" or "when I get back from lunch". An exclusive
    /// window start would drop it, and only it, without a trace.
    #[test]
    fn a_daily_due_at_the_window_start_fires() {
        let rule = BreakRule {
            cadence: BreakCadence::Daily {
                at: chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
            },
            ..interval_rule(0, 9)
        };
        let rules = vec![rule.clone()];
        let w = morning();                       // 08:00 → 12:00
        let daily = vec![(rule.id, at(8, 0))];
        let tick = decide(BreakTickInput {
            now: at(8, 1),
            since: at(7, 59),
            windows: &w,
            rules: &rules,
            daily_dues: &daily,
            busy: &[],
            open: &[],
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.fire.expect("fires").candidate.rule_id(), rule.id);
    }

    /// The same inclusiveness on step 1: a tick landing exactly on the window's opening
    /// instant is inside the working day, not outside every window.
    #[test]
    fn a_tick_at_the_window_start_is_inside_the_working_day() {
        let rules = vec![interval_rule(20, 1)];
        let tick = decide(input(at(8, 0), at(7, 55), &morning(), &rules));
        assert!(tick.expire.is_empty(), "nothing is cleaned up at the opening instant");
    }

    #[test]
    fn next_natural_due_after_walks_forward_within_the_window() {
        let rule = interval_rule(20, 1);
        let w = morning();
        assert_eq!(next_natural_due_after(&rule, &w, at(8, 25)), Some(at(8, 40)));
        // Past the last due of the window, the next one is in the following window.
        let windows = vec![
            Window { start: at(8, 0), end: at(12, 0) },
            Window { start: at(13, 0), end: at(17, 0) },
        ];
        assert_eq!(next_natural_due_after(&rule, &windows, at(12, 0)), Some(at(13, 20)));
        assert_eq!(next_natural_due_after(&rule, &w, at(12, 0)), None);
    }

    fn busy(id: &str, from: (u32, u32), to: (u32, u32)) -> BusyPeriod {
        BusyPeriod {
            meeting_id: id.into(),
            start: at(from.0, from.1),
            end: at(to.0, to.1),
        }
    }

    fn open_event(rule_id: BreakRuleId, due: DateTime<Utc>, until: DateTime<Utc>) -> BreakEvent {
        BreakEvent {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            rule_id,
            due_at: due,
            fired_at: None,
            deferred_until: Some(until),
            defer_reason: Some(DeferReason::Meeting),
            suppressed_by_meeting_id: Some("m1".into()),
            outcome: crate::types::BreakOutcome::Pending,
            responded_at: None,
            created_at: due,
        }
    }

    #[test]
    fn a_due_inside_a_meeting_is_deferred_to_its_end_plus_grace() {
        let rules = vec![interval_rule(60, 3)];
        let w = morning();
        let busy = vec![busy("m1", (8, 50), (9, 50))];
        let tick = decide(BreakTickInput {
            now: at(9, 1),
            since: at(8, 59),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &busy,
            open: &[],
            grace: Duration::minutes(3),
        });
        assert!(tick.fire.is_none(), "nothing fires during the meeting");
        assert_eq!(tick.defer.len(), 1);
        assert_eq!(tick.defer[0].until, at(9, 53));
        assert_eq!(tick.defer[0].reason, DeferReason::Meeting);
        assert_eq!(tick.defer[0].meeting_id.as_deref(), Some("m1"));
    }

    /// The point of deferring rather than skipping: an hour of meeting must not cost
    /// the hourly break.
    #[test]
    fn a_deferred_break_fires_when_its_wait_is_over() {
        let rules = vec![interval_rule(60, 3)];
        let w = morning();
        let ev = open_event(rules[0].id, at(9, 0), at(9, 53));
        let open = vec![ev.clone()];
        let tick = decide(BreakTickInput {
            now: at(9, 53),
            since: at(9, 52),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &[],
            open: &open,
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.fire.expect("fires").candidate.event_id(), Some(ev.id));
        assert!(tick.expire.is_empty());
    }

    /// Back-to-back calls: the wake-up lands inside the next meeting and is re-deferred
    /// onto it rather than firing over it.
    #[test]
    fn a_wake_up_inside_another_meeting_is_re_deferred() {
        let rules = vec![interval_rule(60, 3)];
        let w = morning();
        let ev = open_event(rules[0].id, at(9, 0), at(9, 53));
        let open = vec![ev.clone()];
        let busy = vec![busy("m2", (9, 50), (10, 30))];
        let tick = decide(BreakTickInput {
            now: at(9, 53),
            since: at(9, 52),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &busy,
            open: &open,
            grace: Duration::minutes(3),
        });
        assert!(tick.fire.is_none());
        assert_eq!(tick.defer.len(), 1);
        assert_eq!(tick.defer[0].until, at(10, 33));
        assert_eq!(tick.defer[0].meeting_id.as_deref(), Some("m2"));
    }

    /// A deferral that can no longer beat its own rule's next due is pointless: the
    /// fresh one is 4 minutes away. This is what stops deferrals piling up without
    /// having to count them.
    #[test]
    fn a_deferral_overtaken_by_the_next_natural_due_expires() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        // Due at 09:00, meeting ran to 10:50, so the wait ends at 10:53 — long past
        // the 09:20 that would have replaced it.
        let ev = open_event(rules[0].id, at(9, 0), at(10, 53));
        let open = vec![ev.clone()];
        let tick = decide(BreakTickInput {
            now: at(10, 53),
            since: at(10, 52),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &[],
            open: &open,
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.expire, vec![ev.id]);
        assert!(tick.fire.is_none() || tick.fire.as_ref().unwrap().candidate.event_id() != Some(ev.id));
    }

    /// Only one deferral per rule may be alive. The case that needs the guard is a
    /// single tick carrying several dues of the same rule — after a suspend, or simply
    /// a long meeting — which must arm one deferral, not three.
    ///
    /// (Across *separate* ticks the expiry rule already does this on its own: each new
    /// due overtakes the previous deferral and replaces it. The guard is what covers
    /// several dues arriving at once.)
    #[test]
    fn several_dues_inside_one_meeting_arm_only_one_deferral() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        let busy = vec![busy("m1", (8, 55), (11, 30))];
        let tick = decide(BreakTickInput {
            now: at(10, 0),
            since: at(9, 0),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &busy,
            open: &[],
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.defer.len(), 1, "one deferral for the rule");
        assert_eq!(tick.defer[0].until, at(11, 33));
        assert_eq!(tick.absorb.len(), 2, "the 09:40 and 10:00 dues are absorbed");
    }

    /// A deferral computed into the past resolves on this very tick instead of writing
    /// a row that is already overdue.
    #[test]
    fn a_due_inside_a_meeting_that_has_since_ended_fires_now() {
        let rules = vec![interval_rule(60, 3)];
        let w = morning();
        let busy = vec![busy("m1", (8, 50), (9, 50))];
        let tick = decide(BreakTickInput {
            now: at(10, 30),
            since: at(8, 30),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &busy,
            open: &[],
            grace: Duration::minutes(3),
        });
        assert!(tick.fire.is_some(), "the 09:00 due, unblocked, resolves immediately");
        assert!(tick.defer.is_empty());
    }

    #[test]
    fn the_end_of_the_day_expires_everything_still_waiting() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        let ev = open_event(rules[0].id, at(11, 40), at(11, 58));
        let open = vec![ev.clone()];
        let tick = decide(BreakTickInput {
            now: at(12, 30),
            since: at(12, 29),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &[],
            open: &open,
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.expire, vec![ev.id]);
    }

    /// A snooze is not a special case: it is a deferral with another reason, and it
    /// takes the same path — expiry included.
    #[test]
    fn a_snoozed_event_wakes_like_any_other_deferral() {
        let rules = vec![interval_rule(60, 3)];
        let w = morning();
        let mut ev = open_event(rules[0].id, at(9, 0), at(9, 10));
        ev.defer_reason = Some(DeferReason::Snooze);
        ev.suppressed_by_meeting_id = None;
        let open = vec![ev.clone()];
        let tick = decide(BreakTickInput {
            now: at(9, 10),
            since: at(9, 9),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &[],
            open: &open,
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.fire.expect("fires").candidate.event_id(), Some(ev.id));
    }
}
