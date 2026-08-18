use async_graphql::{Enum, SimpleObject, ID};
use chrono::NaiveDate;
use domain::rules::brief::{
    render_brief, Brief, BriefSection, BriefVariant, ConsolidationAge, DeadlineEntry, MemoryEntry,
};

/// Which brief was asked for.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum BriefVariantGql {
    /// The SessionStart injection: everything, with drill-down hints.
    Session,
    /// The 08:30 notification: today's deadlines, open commitments, queue size.
    Morning,
}

impl From<BriefVariantGql> for BriefVariant {
    fn from(v: BriefVariantGql) -> Self {
        match v {
            BriefVariantGql::Session => BriefVariant::Session,
            BriefVariantGql::Morning => BriefVariant::Morning,
        }
    }
}

impl From<BriefVariant> for BriefVariantGql {
    fn from(v: BriefVariant) -> Self {
        match v {
            BriefVariant::Session => BriefVariantGql::Session,
            BriefVariant::Morning => BriefVariantGql::Morning,
        }
    }
}

/// A task deadline as the brief shows it.
#[derive(SimpleObject)]
pub struct BriefDeadlineGql {
    pub title: String,
    /// Days from today. Negative means overdue.
    pub days_until: i32,
}

/// A memory the brief points at.
#[derive(SimpleObject)]
pub struct BriefMemoryGql {
    pub id: ID,
    /// The short handle rendered in the brief (`m:7c1`). `recall` accepts it.
    pub reference: String,
    pub title: String,
    pub stakeholders: Vec<String>,
    pub occurred_on: NaiveDate,
}

/// How long since the consolidation job last ran.
#[derive(SimpleObject)]
pub struct BriefConsolidationGql {
    /// `null` when no run was ever recorded — the normal case until lot 5 lands.
    pub days_ago: Option<i32>,
    /// Whether the brief warns about it. Never-run counts as stale.
    pub stale: bool,
}

/// The session brief.
///
/// `lines` is the rendering, capped at 40 lines by `domain::rules::brief` — the
/// cap lives there because that is where it can be tested. Clients that want to
/// lay it out themselves read the structured fields instead.
#[derive(SimpleObject)]
pub struct BriefGql {
    pub variant: BriefVariantGql,
    pub date: NaiveDate,
    pub lines: Vec<String>,
    pub deadlines: Vec<BriefDeadlineGql>,
    /// How many deadlines qualified, before the section cap.
    pub deadline_total: i32,
    /// Working rules. Rendered first, cut last.
    pub preferences: Vec<BriefMemoryGql>,
    /// How many qualified, before the section cap.
    pub preference_total: i32,
    pub commitments: Vec<BriefMemoryGql>,
    pub commitment_total: i32,
    pub decisions: Vec<BriefMemoryGql>,
    pub decision_total: i32,
    /// Whether the decisions were narrowed to the project in focus.
    pub decisions_scoped_to_project: bool,
    /// Memory candidates waiting in the validation queue.
    pub pending_count: i32,
    pub consolidation: BriefConsolidationGql,
}

fn memory_entries(section: &BriefSection<MemoryEntry>) -> Vec<BriefMemoryGql> {
    section
        .entries
        .iter()
        .map(|e| BriefMemoryGql {
            id: ID(e.id.to_string()),
            reference: e.reference.clone(),
            title: e.title.clone(),
            stakeholders: e.stakeholders.clone(),
            occurred_on: e.occurred_on,
        })
        .collect()
}

fn deadline_entries(section: &BriefSection<DeadlineEntry>) -> Vec<BriefDeadlineGql> {
    section
        .entries
        .iter()
        .map(|d: &DeadlineEntry| BriefDeadlineGql {
            title: d.title.clone(),
            days_until: d.days_until as i32,
        })
        .collect()
}

impl From<Brief> for BriefGql {
    fn from(brief: Brief) -> Self {
        // Rendered here so every client gets the same 40-line-capped text.
        let lines = render_brief(&brief);
        Self {
            variant: brief.variant.into(),
            date: brief.date,
            lines,
            deadlines: deadline_entries(&brief.deadlines),
            deadline_total: brief.deadlines.total as i32,
            preferences: memory_entries(&brief.preferences),
            preference_total: brief.preferences.total as i32,
            commitments: memory_entries(&brief.commitments),
            commitment_total: brief.commitments.total as i32,
            decisions: memory_entries(&brief.decisions),
            decision_total: brief.decisions.total as i32,
            decisions_scoped_to_project: brief.decisions_scoped_to_project,
            pending_count: brief.pending_count as i32,
            consolidation: BriefConsolidationGql {
                days_ago: match brief.consolidation {
                    ConsolidationAge::NeverRun => None,
                    ConsolidationAge::Ran { days_ago } => Some(days_ago as i32),
                },
                stale: brief.consolidation.is_stale(),
            },
        }
    }
}
