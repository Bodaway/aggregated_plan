use async_graphql::{SimpleObject, ID};
use chrono::{DateTime, Utc};

use domain::types::{BreakEvent, BreakRule};

use super::break_rule::BreakKindGql;

/// The break being served right now — what the HUD overlay renders instead of its
/// grid.
///
/// `kind`, `label` and `body` are read off the **rule**, not off the event: the event
/// records only that a break opened and when it is due to end, so rewording a rule
/// changes what the next overlay says without migrating anything. `endsAt` is the
/// frozen deadline, not `startedAt + durationSeconds` recomputed here — backend and
/// HUD read one absolute instant, and retuning the rule mid-break cannot lengthen it.
#[derive(SimpleObject)]
pub struct ActiveBreakGql {
    pub event_id: ID,
    pub kind: BreakKindGql,
    pub label: String,
    pub body: String,
    pub started_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

impl ActiveBreakGql {
    /// Pair a running event with the rule that asked for it.
    ///
    /// `None` when the row carries no `started_at` / `ends_at` pair. `find_active`
    /// already filters on `started_at`, so such a row is incoherent rather than
    /// merely absent: it describes a session with no deadline, which nothing can
    /// close and nothing can count down to. Reporting nothing beats reporting a
    /// countdown built on an invented instant.
    pub fn from_parts(event: &BreakEvent, rule: &BreakRule) -> Option<Self> {
        Some(ActiveBreakGql {
            event_id: ID(event.id.to_string()),
            kind: rule.kind.into(),
            label: rule.label.clone(),
            body: rule.body.clone(),
            started_at: event.started_at?,
            ends_at: event.ends_at?,
        })
    }
}
