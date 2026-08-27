use async_graphql::{Enum, InputObject, SimpleObject, ID};

use domain::types::{BreakCadence, BreakKind, BreakRule, BreakUrgency};

/// GraphQL enum mirroring `domain::types::BreakKind`. Drives which icon and seeded
/// copy the notification daemon and the settings screen render.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum GqlBreakKind {
    Visual,
    Posture,
    Long,
    Strength,
}

impl From<BreakKind> for GqlBreakKind {
    fn from(k: BreakKind) -> Self {
        match k {
            BreakKind::Visual => GqlBreakKind::Visual,
            BreakKind::Posture => GqlBreakKind::Posture,
            BreakKind::Long => GqlBreakKind::Long,
            BreakKind::Strength => GqlBreakKind::Strength,
        }
    }
}

impl From<GqlBreakKind> for BreakKind {
    fn from(k: GqlBreakKind) -> Self {
        match k {
            GqlBreakKind::Visual => BreakKind::Visual,
            GqlBreakKind::Posture => BreakKind::Posture,
            GqlBreakKind::Long => BreakKind::Long,
            GqlBreakKind::Strength => BreakKind::Strength,
        }
    }
}

/// GraphQL enum mirroring `domain::types::BreakUrgency`. Passed straight through to
/// the notification daemon.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum GqlBreakUrgency {
    Low,
    Normal,
    Critical,
}

impl From<BreakUrgency> for GqlBreakUrgency {
    fn from(u: BreakUrgency) -> Self {
        match u {
            BreakUrgency::Low => GqlBreakUrgency::Low,
            BreakUrgency::Normal => GqlBreakUrgency::Normal,
            BreakUrgency::Critical => GqlBreakUrgency::Critical,
        }
    }
}

impl From<GqlBreakUrgency> for BreakUrgency {
    fn from(u: GqlBreakUrgency) -> Self {
        match u {
            GqlBreakUrgency::Low => BreakUrgency::Low,
            GqlBreakUrgency::Normal => BreakUrgency::Normal,
            GqlBreakUrgency::Critical => BreakUrgency::Critical,
        }
    }
}

/// Which of the two `BreakCadence` shapes a rule carries. Exposed only so the
/// settings form knows which of `intervalMinutes` / `atTime` to show; the actual
/// XOR is enforced by the sum type on the way back in, not by this enum.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum GqlBreakCadence {
    Interval,
    Daily,
}

/// GraphQL projection of one break rule. This is what the settings screen lists.
#[derive(SimpleObject)]
pub struct GqlBreakRule {
    pub id: ID,
    pub kind: GqlBreakKind,
    pub label: String,
    pub body: String,
    pub cadence: GqlBreakCadence,
    pub interval_minutes: Option<i32>,
    /// `HH:MM` in the user's timezone.
    pub at_time: Option<String>,
    pub duration_seconds: i32,
    pub priority: i32,
    pub enabled: bool,
    pub urgency: GqlBreakUrgency,
}

impl From<BreakRule> for GqlBreakRule {
    fn from(rule: BreakRule) -> Self {
        let (cadence, interval_minutes, at_time) = match rule.cadence {
            BreakCadence::Interval { minutes } => {
                (GqlBreakCadence::Interval, Some(minutes as i32), None)
            }
            BreakCadence::Daily { at } => {
                (GqlBreakCadence::Daily, None, Some(at.format("%H:%M").to_string()))
            }
        };
        GqlBreakRule {
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
/// `GqlBreakRule` minus `id`, which the mutation resolves on its own.
#[derive(InputObject)]
pub struct BreakRuleInput {
    pub kind: GqlBreakKind,
    pub label: String,
    pub body: String,
    pub cadence: GqlBreakCadence,
    pub interval_minutes: Option<i32>,
    pub at_time: Option<String>,
    pub duration_seconds: i32,
    pub priority: i32,
    pub enabled: bool,
    pub urgency: GqlBreakUrgency,
}

impl BreakRuleInput {
    /// Reject the shapes the database would reject anyway, but with a message a form
    /// can display. The CHECK stays as the backstop, not as the user experience.
    pub fn to_cadence(&self) -> Result<BreakCadence, String> {
        match (self.cadence, self.interval_minutes, self.at_time.as_deref()) {
            (GqlBreakCadence::Interval, Some(m), None) if m > 0 => {
                Ok(BreakCadence::Interval { minutes: m as u32 })
            }
            (GqlBreakCadence::Interval, _, Some(_)) => {
                Err("une règle par intervalle ne peut pas porter d'heure fixe".into())
            }
            (GqlBreakCadence::Interval, _, None) => {
                Err("intervalMinutes est requis et doit être positif".into())
            }
            (GqlBreakCadence::Daily, None, Some(t)) => chrono::NaiveTime::parse_from_str(t, "%H:%M")
                .map(|at| BreakCadence::Daily { at })
                .map_err(|_| "atTime doit être au format HH:MM".to_string()),
            (GqlBreakCadence::Daily, Some(_), _) => {
                Err("une règle quotidienne ne peut pas porter d'intervalle".into())
            }
            (GqlBreakCadence::Daily, None, None) => Err("atTime est requis".into()),
        }
    }

    /// Reject a non-positive duration before it ever reaches an `as u32` cast: a
    /// negative `i32` wraps to a huge unsigned value that the database's
    /// `CHECK (duration_seconds > 0)` cannot see, and `run_break_tick` would then
    /// compute an `expire_after` that never fires — a notification, and its
    /// `notify-send` child process, that never expire. Shared by both mutations so
    /// neither can bypass it.
    pub fn validated_duration_seconds(&self) -> Result<u32, String> {
        if self.duration_seconds > 0 {
            Ok(self.duration_seconds as u32)
        } else {
            Err("durationSeconds doit être positif".into())
        }
    }
}
