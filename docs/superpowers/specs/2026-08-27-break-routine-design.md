# Break routine: scheduled micro-breaks with meeting-aware suppression

**Date:** 2026-08-27
**Status:** Design — approved, ready for planning
**Author:** (brainstormed with Claude)

## Problem

The user has shoulder problems and works long uninterrupted stretches at a screen. The
ergonomics literature converges on two things:

- **Cadence.** INRS recommends 5 min per hour of intensive screen work (or 15 min per 2 h
  when less intensive), plus an active break every 30 min and a visual break every ~20 min.
  Cornell's 20-8-2 covers the same ground for static postural load.
- **Content.** Passive micro-breaks reduce general discomfort, but an EMG study found *no
  significant change in shoulder muscle-fibre cycling* from breaks alone. What moves the
  shoulder specifically is brief active loading: Andersen's RCT (2 min/day of elastic-band
  resistance training, 10 weeks) cut neck/shoulder pain intensity by 40% and quadrupled EMG
  relaxation gaps.

So a useful system needs **several distinct rhythms with distinct content**, not one timer.
It also needs to shut up during meetings — a reminder that fires while the user is talking
on a call is noise, and noise is what kills adherence to exactly this kind of routine.

aplan already knows the working windows (`workday.*`), the timezone (`aplan.timezone`), the
working days (`general.working_days`) and the day's meetings (synced from Outlook). It
already has the two patterns this needs: a long-lived background job (`run_eod_scheduler`
in `api/src/jobs.rs`, with `RetryPolicy`/`JobHealth`) and a desktop notification path
(`aplan-brief.service` → `notify-send`, delivered by swaync).

## Goals

1. A routine of several superposed cadences, each with its own interval, duration and content.
2. The user edits it in the React settings screen.
3. Desktop notification when a break is due, with actionable buttons.
4. No notification lands inside a meeting; it is deferred to the meeting's end.
5. Full trace of what happened, so adherence is measurable and the routine can be tuned.

## Non-goals

- No presence/idle detection. The clock is pure wall-clock inside the configured working
  windows. aplan knows tasks, not whether the user is at the keyboard, and inventing that
  signal was explicitly rejected.
- No CLI surface for editing the routine. Customisation is the React screen.
- No integration with the timesheet reconstruction. Breaks do not touch quarter arbitration.
- No day-preview timeline in the first cut (see *Deferred* below).

## Decisions taken during design

| Question | Decision | Why |
|---|---|---|
| What advances the clock | Pure wall-clock inside working windows | Predictable; presence detection was rejected as out of aplan's model |
| Collision with a meeting | Defer to meeting end + grace | A 1 h meeting must not cost two breaks; leaving a meeting is a good moment to move |
| Routine shape | N independent rules, each with its own content | Matches the evidence; adding a rhythm later does not touch the engine |
| Tracking | Full trace with user response | Without it there is no way to know whether the routine holds |
| Customisation surface | React settings screen | User's choice |
| Where the engine runs | In-API background job (approach A) | One moving part; state, decision and delivery in one place; mirrors `run_eod_scheduler` |

## Data model

Migration `019_create_break_rules.sql`.

### `break_rules`

One row per rhythm. This is what the React screen edits.

| column | type | notes |
|---|---|---|
| `id` | TEXT PK | UUID |
| `user_id` | TEXT NOT NULL | |
| `kind` | TEXT NOT NULL | `visual` \| `posture` \| `long` \| `strength` — drives icon and default labels |
| `label` | TEXT NOT NULL | notification title |
| `body` | TEXT NOT NULL | notification body: what to actually do |
| `cadence` | TEXT NOT NULL | `interval` \| `daily` |
| `interval_minutes` | INTEGER NULL | set iff `cadence = 'interval'` |
| `at_time` | TEXT NULL | `HH:MM`, set iff `cadence = 'daily'`, read in `aplan.timezone` |
| `duration_seconds` | INTEGER NOT NULL | announced break length |
| `priority` | INTEGER NOT NULL | breaks collision ties **and** orders the UI |
| `enabled` | INTEGER NOT NULL DEFAULT 1 | |
| `urgency` | TEXT NOT NULL DEFAULT 'normal' | passed through to `notify-send` |
| `created_at`, `updated_at` | TEXT NOT NULL | ISO 8601 |

Constraints:

```sql
CHECK (cadence IN ('interval','daily'))
CHECK (kind IN ('visual','posture','long','strength'))
CHECK (urgency IN ('low','normal','critical'))
CHECK ((cadence = 'interval' AND interval_minutes IS NOT NULL AND at_time IS NULL)
    OR (cadence = 'daily'    AND at_time         IS NOT NULL AND interval_minutes IS NULL))
CHECK (interval_minutes IS NULL OR interval_minutes > 0)
CHECK (duration_seconds > 0)
```

The cross-column `CHECK` is deliberate: the exclusivity of `interval_minutes` / `at_time` is
an invariant we do not want to entrust to application code alone.

Index: `(user_id, enabled)`.

### `break_events`

One row per due slot. This is the trace, and it is also what makes deferral survive an API
restart.

| column | type | notes |
|---|---|---|
| `id` | TEXT PK | UUID |
| `user_id` | TEXT NOT NULL | |
| `rule_id` | TEXT NOT NULL | REFERENCES `break_rules(id)` ON DELETE CASCADE |
| `due_at` | TEXT NOT NULL | the instant the cadence designated |
| `fired_at` | TEXT NULL | when the notification actually went out; NULL while deferred or on delivery failure |
| `deferred_until` | TEXT NULL | meeting end + grace, or snooze target |
| `defer_reason` | TEXT NULL | `meeting` \| `snooze` |
| `suppressed_by_meeting_id` | TEXT NULL | audit trail for "why didn't it fire" |
| `outcome` | TEXT NOT NULL | see below |
| `responded_at` | TEXT NULL | |
| `created_at` | TEXT NOT NULL | |

Outcomes:

| value | meaning |
|---|---|
| `pending` | created, not yet resolved (deferred, or fired and awaiting a response) |
| `taken` | user clicked *Pris* |
| `snoozed` | user clicked *Plus tard*; a follow-up deferral was armed |
| `skipped` | user clicked *Passer* — the break was deliberately declined |
| `ignored` | notification fired, closed without a choice |
| `absorbed` | collapsed by coalescing; the user never saw it |
| `expired` | could no longer usefully fire (see the expiry rule) |

`skipped` and `ignored` are kept distinct on purpose: systematically *ignoring* the 20-minute
break signals a badly tuned cadence, while explicitly *skipping* signals a badly timed one.
Those are two different fixes. `absorbed` counts neither for nor against adherence.

Constraints: `CHECK (outcome IN (...))`, `CHECK (defer_reason IS NULL OR defer_reason IN ('meeting','snooze'))`.
Indexes: `(user_id, rule_id, due_at)`, `(user_id, outcome)`.

### Configuration scalars

Single scalars go in the existing `configuration` table, not a table of their own:

| key | default | meaning |
|---|---|---|
| `aplan.breaks.enabled` | `true` | master switch |
| `aplan.breaks.meeting_grace_minutes` | `3` | delay after a meeting ends before a deferred break fires |
| `aplan.breaks.snooze_minutes` | `10` | *Plus tard* horizon |
| `aplan.breaks.suppressing_show_as` | `busy,oof` | which Outlook `show_as` values suppress |
| `aplan.breaks.last_tick` | — | written by the job; the `since` of the next tick |

### Seeded routine

The migration seeds four rules; the user edits them afterwards in the screen.

| kind | cadence | duration | body | priority |
|---|---|---|---|---|
| `visual` | every 20 min | 30 s | Regarder au loin, relâcher les épaules | 1 |
| `posture` | every 30 min | 2 min | Se lever, changer de posture, bouger | 2 |
| `long` | every 60 min | 5 min | Pause franche, hors écran | 3 |
| `strength` | daily 14:00 | 2 min | Renfo épaule/scapula à l'élastique | 4 |

Seeded `label`/`body` are French, matching the user-facing strings the user reads.

**These cadences overlap by construction.** At minute 60 all three `interval` rules are due
simultaneously. `priority` exists precisely for that: the engine fires at most one
notification per tick, the highest-priority one, and marks the rest `absorbed`. Without it
the user takes three pop-ups every hour and disables the whole thing within two days.

## Domain rule — `domain/src/rules/breaks.rs`

`domain` may only depend on chrono/serde/uuid/thiserror, and `chrono_tz` lives in
`application` (`application/src/time.rs`). So the **application resolves timezone, working
days and window boundaries**, and hands the domain UTC instants — the same split
`use_cases/worklog.rs` already uses.

```rust
pub struct Window { pub start: DateTime<Utc>, pub end: DateTime<Utc> }
pub struct BusyPeriod { pub meeting_id: String, pub start: DateTime<Utc>, pub end: DateTime<Utc> }

pub struct BreakTickInput<'a> {
    pub now: DateTime<Utc>,
    pub since: DateTime<Utc>,        // previous tick; (since, now] is the window examined
    pub windows: &'a [Window],       // today's working windows, UTC; empty on a non-working day
    pub rules: &'a [BreakRule],      // enabled only
    pub busy: &'a [BusyPeriod],      // meetings already filtered on show_as
    pub open: &'a [BreakEvent],      // pending / deferred
    pub grace: Duration,
    /// Today's UTC instant for each enabled `Daily` rule. Resolved by the caller,
    /// because `domain` has no timezone database.
    pub daily_dues: &'a [(BreakRuleId, DateTime<Utc>)],
}

pub struct BreakTick {
    pub fire: Option<FireBreak>,        // at most one
    pub defer: Vec<DeferBreak>,
    pub absorb: Vec<AbsorbBreak>,
    pub expire: Vec<BreakEventId>,
}

pub fn decide(input: BreakTickInput<'_>) -> BreakTick
```

Decision order:

1. **Outside every window** → nothing fires and everything still open expires (end-of-day cleanup).
2. **Expiry.** Cull every deferral that can no longer fire before its own rule's next natural
   due, plus any whose rule was disabled or deleted. This is what prevents accumulation without
   counting deferrals: after a 1 h meeting the deferred *visual* break erases itself because
   the next one is 4 minutes away, while the deferred *hourly* break survives and fires.
3. **Natural dues.** For each `interval` rule, the instants `window_start + k × interval`
   falling in `(since, now]`. The anchor is the **window**, not the last fire — that is what
   makes it wall-clock: 09:20, 09:40, 10:00…, and the afternoon window restarts from its own
   anchor. For each `daily` rule, its `at_time` instant if it falls in `(since, now]`.
   The first due of a window is `window_start + interval`, never `window_start` itself.
4. **Wake-ups.** Surviving open events whose `deferred_until <= now` join the candidate set.
5. **Meeting suppression.** Any candidate whose instant falls inside a `BusyPeriod` is
   deferred to `meeting_end + grace`. A wake-up that lands inside another meeting
   (back-to-back calls) is simply re-deferred onto the new one. A rule that already holds a
   live deferral does not get a second one — its extra dues in the same tick are absorbed.
6. **Coalescing.** Among what remains, fire the **highest priority, exactly one**. The rest
   become `absorbed`.

Note the position of expiry: it runs at step 2, **before** the candidate set is built, not
after suppression. That ordering is load-bearing rather than incidental — the
one-live-deferral-per-rule guard in step 4 reads the surviving set, so culling afterwards
would let a stale deferral suppress a fresh due that should have replaced it.

Two properties fall out of this shape for free:

- **Suspend- and restart-resilience.** `(since, now]` may span two hours after a laptop
  suspend; the six missed dues are computed, five are absorbed and one fires. No catch-up burst.
- **Snooze is not a special case.** *Plus tard* writes a deferral at `now + snooze` with
  `defer_reason = 'snooze'` instead of `'meeting'` and re-enters the same path, expiry included.
  `decide` therefore takes no snooze parameter: the use case writes the deferral, and
  `decide` only ever meets it later as an ordinary wake-up.

## Application — `application/src/use_cases/breaks.rs` and the notifier trait

### Notifier trait (`application/src/services/notifier.rs`)

```rust
pub struct Notification {
    pub title: String,
    pub body: String,
    pub urgency: Urgency,
    pub icon: Option<String>,
    pub expire_after: Duration,
    pub actions: Vec<(String, String)>,   // (key, label)
}

pub enum NotificationOutcome { Action(String), Dismissed, Expired }

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, n: Notification) -> Result<NotificationOutcome, AppError>;
}
```

### `run_break_tick`

Loads config, enabled rules, the day's meetings and open events; resolves the working windows
via `time.rs`; calls `decide`; persists everything the tick produced; then hands the `fire` to
the notifier. The notifier's answer writes `outcome` + `responded_at` back onto the
`break_events` row.

Because `-A` implies `--wait`, `notify()` blocks until the user answers. The tick
**awaits it inline** and simply starts late: detaching it would mean the spawned task
owning repository clones and writing back concurrently with the next tick, which buys
nothing here — `decide` anchors on the wall clock, so a late tick delays dues without
losing any. `expire_after` (the rule's duration plus five minutes) bounds the block and
kills the child, otherwise an untouched notification leaks a process until evening.

## Infrastructure

- `infrastructure/src/notify/notify_send.rs` — `NotifySendNotifier`, a `tokio::process::Command`
  running `notify-send --app-name=aplan -A taken=Pris -A snoozed="Plus tard" -A skipped=Passer`,
  reading the chosen action key from stdout. Verified available: `notify-send 0.8.8`, swaync 0.12.6.
- `NullNotifier` — used in tests, and selected at wiring time when D-Bus is unreachable
  (no graphical session). Records the event and stays silent rather than erroring every 30 s.
- `BreakRuleRepository` / `BreakEventRepository` — traits in `application/src/repositories.rs`,
  SQLite implementations in `infrastructure`, runtime `sqlx::query` per house convention,
  `sqlx::Error` → `RepositoryError::Database(e.to_string())`.

## API job — `api/src/jobs.rs`

`run_break_scheduler(deps, user_id)`, twin of `run_eod_scheduler`: 30 s tick while healthy,
backing off to 5 min while not, via a new `RetryPolicy::breaks()` and the existing `JobHealth`.
Wired in `main.rs` next to the other schedulers.

`since` is persisted in `aplan.breaks.last_tick`. On the very first run `since = now` — no
catch-up of a fictional backlog.

## GraphQL

- Queries: `breakRules`, `breakStats(from, to)` (per rule: fired / taken / snoozed / skipped /
  ignored, plus an adherence rate).
- Mutations: `createBreakRule`, `updateBreakRule`, `deleteBreakRule`, modelled on the
  `createTask` / `updateTask` / `deleteTask` triple.
- The configuration scalars go through the **existing** configuration mutation. `SettingsPage`
  already has `CONFIG_KEYS` and its `SaveButton`; that path is not duplicated.

## Frontend

`SettingsPage.tsx` is already 29.6 KB, so the new weight goes into its own files rather than
growing it further:

- `components/breaks/BreakRoutineSettings.tsx` — master switch, the four scalars, the rule
  list, add/delete, and a 30-day stats panel.
- `components/breaks/BreakRuleRow.tsx` — one rule: enable, cadence, interval or time, duration,
  priority, label, body.
- `hooks/use-break-rules.ts` — urql queries/mutations, alongside the existing hooks.

`SettingsPage` gains three lines, reusing the existing collapsible section component:

```tsx
<SettingsSection title="Pauses" icon={<PauseIcon />}>
  <BreakRoutineSettings />
</SettingsSection>
```

The rest of that page is not refactored — out of scope.

## Testing

TDD: tests before production code.

| Level | What | How |
|---|---|---|
| `domain::rules::breaks` | All decision logic | Table-driven pure tests, inline `#[cfg(test)]`: hourly collision, meeting spanning several dues, back-to-back meetings, suspend gap, day rollover, non-working day, expired snooze, first-due-is-not-window-start |
| `use_cases::breaks` | Persisted transitions | In-memory SQLite + a fake `Notifier` recording calls and replaying scripted answers: `pending → deferred → fired → taken`; a double tick must not double anything |
| SQLite repos | CRUD + the cross-column `CHECK` | In-memory SQLite; the database must reject an `interval` rule carrying an `at_time` |
| `notify-send` adapter | Argument construction and action parsing | Two **pure** functions extracted — `command_args(&Notification) -> Vec<String>` and stdout → `NotificationOutcome` — tested directly. The `spawn` stays a three-line shell, untested |
| GraphQL | CRUD round-trip and stats | Added to `graphql/tests.rs`, which already has the pattern |
| React | `BreakRoutineSettings` | Vitest + RTL on the `MemoryPage.test.tsx` model: enable/disable, and interval-XOR-daily validation |

## Error handling

A tick never dies.

- **Delivery failure** (D-Bus absent, `notify-send` errors): the event is recorded, `fired_at`
  stays NULL, and the expiry rule cleans it up at the next natural due. No new state, no retry
  loop — the existing machinery suffices.
- **Tick failure**: traced, never fatal, and it feeds the `JobHealth` that backs the cadence off
  to 5 min. Exactly the EOD job's behaviour.
- **Master switch off**: the tick still runs and still advances `last_tick`. Otherwise, on
  re-enabling, `(since, now]` would span three days and produce an absurd catch-up.

## Spec maintenance

Per the project CLAUDE.md, `SPEC_FONCTIONNELLE.md` and `SPEC_TECHNIQUE.md` are updated in the
same commit as the code: new tables, new configuration keys, new GraphQL surface, new
background job.

## Deferred (not in this plan)

**Day-preview timeline.** A `breakSchedule(date)` query projecting a fictional day through the
pure `decide`, rendered as a timeline with natural dues, meetings hatched over them, and what
would actually fire. It would make tuning legible — the hourly collision would be visible at a
glance — and it is nearly free given `decide` is pure. But the routine works without it.
