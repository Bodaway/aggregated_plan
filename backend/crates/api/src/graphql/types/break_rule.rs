use async_graphql::{Enum, InputObject, SimpleObject, ID};

use domain::types::{BreakCadence, BreakKind, BreakRule, BreakUrgency};

/// Longest break a rule may ask for, in seconds. One hour: past any break worth the
/// name, and short enough that a mistyped value cannot park the break scheduler — the
/// tick waits on the notification inline, for `duration_seconds + 300`.
pub const MAX_DURATION_SECONDS: i32 = 3600;

/// GraphQL enum mirroring `domain::types::BreakKind`. Drives which icon and seeded
/// copy the notification daemon and the settings screen render.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum BreakKindGql {
    Visual,
    Posture,
    Long,
    Strength,
}

impl From<BreakKind> for BreakKindGql {
    fn from(k: BreakKind) -> Self {
        match k {
            BreakKind::Visual => BreakKindGql::Visual,
            BreakKind::Posture => BreakKindGql::Posture,
            BreakKind::Long => BreakKindGql::Long,
            BreakKind::Strength => BreakKindGql::Strength,
        }
    }
}

impl From<BreakKindGql> for BreakKind {
    fn from(k: BreakKindGql) -> Self {
        match k {
            BreakKindGql::Visual => BreakKind::Visual,
            BreakKindGql::Posture => BreakKind::Posture,
            BreakKindGql::Long => BreakKind::Long,
            BreakKindGql::Strength => BreakKind::Strength,
        }
    }
}

/// GraphQL enum mirroring `domain::types::BreakUrgency`. Passed straight through to
/// the notification daemon.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum BreakUrgencyGql {
    Low,
    Normal,
    Critical,
}

impl From<BreakUrgency> for BreakUrgencyGql {
    fn from(u: BreakUrgency) -> Self {
        match u {
            BreakUrgency::Low => BreakUrgencyGql::Low,
            BreakUrgency::Normal => BreakUrgencyGql::Normal,
            BreakUrgency::Critical => BreakUrgencyGql::Critical,
        }
    }
}

impl From<BreakUrgencyGql> for BreakUrgency {
    fn from(u: BreakUrgencyGql) -> Self {
        match u {
            BreakUrgencyGql::Low => BreakUrgency::Low,
            BreakUrgencyGql::Normal => BreakUrgency::Normal,
            BreakUrgencyGql::Critical => BreakUrgency::Critical,
        }
    }
}

/// Which of the two `BreakCadence` shapes a rule carries. Exposed only so the
/// settings form knows which of `intervalMinutes` / `atTime` to show; the actual
/// XOR is enforced by the sum type on the way back in, not by this enum.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum BreakCadenceGql {
    Interval,
    Daily,
}

/// GraphQL projection of one break rule. This is what the settings screen lists.
#[derive(SimpleObject)]
pub struct BreakRuleGql {
    pub id: ID,
    pub kind: BreakKindGql,
    pub label: String,
    pub body: String,
    pub cadence: BreakCadenceGql,
    pub interval_minutes: Option<i32>,
    /// `HH:MM` in the user's timezone.
    pub at_time: Option<String>,
    pub duration_seconds: i32,
    pub priority: i32,
    pub enabled: bool,
    pub urgency: BreakUrgencyGql,
}

impl From<BreakRule> for BreakRuleGql {
    fn from(rule: BreakRule) -> Self {
        let (cadence, interval_minutes, at_time) = match rule.cadence {
            BreakCadence::Interval { minutes } => {
                (BreakCadenceGql::Interval, Some(minutes as i32), None)
            }
            BreakCadence::Daily { at } => {
                (BreakCadenceGql::Daily, None, Some(at.format("%H:%M").to_string()))
            }
        };
        BreakRuleGql {
            id: ID(rule.id.to_string()),
            kind: rule.kind.into(),
            label: rule.label,
            body: rule.body,
            cadence,
            interval_minutes,
            at_time,
            duration_seconds: rule.duration_seconds as i32,
            priority: rule.priority,
            enabled: rule.enabled,
            urgency: rule.urgency.into(),
        }
    }
}

/// Input carried by `createBreakRule` / `updateBreakRule`: the same fields as
/// `BreakRuleGql` minus `id`, which the mutation resolves on its own.
#[derive(InputObject)]
pub struct BreakRuleInput {
    pub kind: BreakKindGql,
    pub label: String,
    pub body: String,
    pub cadence: BreakCadenceGql,
    pub interval_minutes: Option<i32>,
    pub at_time: Option<String>,
    pub duration_seconds: i32,
    pub priority: i32,
    pub enabled: bool,
    pub urgency: BreakUrgencyGql,
}

/// One rule's adherence over a window — one row per rule in `breakStats`.
#[derive(SimpleObject)]
pub struct BreakRuleStatsGql {
    pub rule_id: async_graphql::ID,
    pub label: String,
    pub taken: i32,
    pub snoozed: i32,
    pub skipped: i32,
    pub ignored: i32,
    pub absorbed: i32,
    pub expired: i32,
    /// `taken / seen`, or `null` when nothing was seen. Absorbed and expired slots are
    /// excluded from both sides: the user never had the chance to answer them, so
    /// counting them would drown a real signal in scheduling noise.
    pub adherence: Option<f64>,
}

/// The adherence statistics panel, one row per rule, over the queried window.
#[derive(SimpleObject)]
pub struct BreakStatsGql {
    pub per_rule: Vec<BreakRuleStatsGql>,
}

impl BreakRuleInput {
    /// Reject the shapes the database would reject anyway, but with a message a form
    /// can display. The CHECK stays as the backstop, not as the user experience.
    pub fn to_cadence(&self) -> Result<BreakCadence, String> {
        match (self.cadence, self.interval_minutes, self.at_time.as_deref()) {
            (BreakCadenceGql::Interval, Some(m), None) if m > 0 => {
                Ok(BreakCadence::Interval { minutes: m as u32 })
            }
            (BreakCadenceGql::Interval, _, Some(_)) => {
                Err("une règle par intervalle ne peut pas porter d'heure fixe".into())
            }
            (BreakCadenceGql::Interval, _, None) => {
                Err("intervalMinutes est requis et doit être positif".into())
            }
            (BreakCadenceGql::Daily, None, Some(t)) => chrono::NaiveTime::parse_from_str(t, "%H:%M")
                .map(|at| BreakCadence::Daily { at })
                .map_err(|_| "atTime doit être au format HH:MM".to_string()),
            (BreakCadenceGql::Daily, Some(_), _) => {
                Err("une règle quotidienne ne peut pas porter d'intervalle".into())
            }
            (BreakCadenceGql::Daily, None, None) => Err("atTime est requis".into()),
        }
    }

    /// Reject a duration outside `1..=MAX_DURATION_SECONDS` before it ever reaches an
    /// `as u32` cast: a negative `i32` wraps to a huge unsigned value that the
    /// database's `CHECK (duration_seconds > 0)` cannot see, and `run_break_tick` would
    /// then compute an `expire_after` that never fires — a notification, and its
    /// `notify-send` child process, that never expire.
    ///
    /// The upper bound closes the same hole from the other side, and the database has
    /// no opinion on it either: the tick awaits `notify` inline, so a slipped zero on
    /// an otherwise valid duration would park the scheduler on one notification for
    /// days — no further breaks, no error. An hour is far past any plausible break and
    /// well short of that. Shared by both mutations so neither can bypass it.
    pub fn validated_duration_seconds(&self) -> Result<u32, String> {
        match self.duration_seconds {
            d if d <= 0 => Err("durationSeconds doit être positif".into()),
            d if d > MAX_DURATION_SECONDS => Err(format!(
                "durationSeconds ne peut pas dépasser {MAX_DURATION_SECONDS} secondes"
            )),
            d => Ok(d as u32),
        }
    }
}
