use std::sync::Arc;

use async_graphql::{Context, InputObject, Object, SimpleObject, ID};
use chrono::NaiveDate;

use application::repositories::TaskRepository;
use application::use_cases::reattribution::TaskTimeChange;
use application::use_cases::slot_repair::{DateRepair, SlotRepairOutcome};

use super::task::TaskGql;

/// What `repairOrphanedSlots` is asked to sweep.
///
/// The range is required, both ends of it. There is no default: "everything" would
/// rewrite years of billing history on a typo, and "today" would never reach the
/// damage, which is always in the past by the time anyone sees it in `aplan journal`.
/// `confirm` defaults to `false`, so the default call previews and writes nothing.
#[derive(InputObject, Debug)]
pub struct RepairOrphanedSlotsInput {
    /// First local day of the range (inclusive).
    pub from: NaiveDate,
    /// Last local day of the range (inclusive).
    pub to: NaiveDate,
    /// Write. Absent or `false` previews.
    pub confirm: Option<bool>,
}

/// One local day's share of the repair.
#[derive(SimpleObject)]
pub struct DateRepairGql {
    pub date: NaiveDate,
    /// Unattributed projection slots dropped from this date.
    pub orphans_dropped: i32,
    /// What those orphans were worth — the hours that had fallen off every report.
    pub orphan_hours: f64,
    /// Slots of the rebuilt tasks dropped from this date: their own projection, which
    /// the rewrite replaces. Routine, not damage.
    pub slots_discarded: i32,
    /// Slots written back from the worklog on this date.
    pub slots_written: i32,
}

impl From<DateRepair> for DateRepairGql {
    fn from(repair: DateRepair) -> Self {
        Self {
            date: repair.date,
            orphans_dropped: repair.orphans_dropped as i32,
            orphan_hours: repair.orphan_hours,
            slots_discarded: repair.slots_discarded as i32,
            slots_written: repair.slots_written as i32,
        }
    }
}

/// One task the repair rebuilt, with its hours before and after.
///
/// Not [`super::worklog_entry::TaskTimeChangeGql`]: that type answers about tasks the
/// caller named itself, so an id is enough. Here the tasks were *discovered* — the
/// orphan lost the only pointer that said which they were — so the report has to name
/// them, or the operator is asked to confirm a rewrite of hours against a bare UUID.
pub struct RepairedTaskGql(pub TaskTimeChange);

#[Object]
impl RepairedTaskGql {
    async fn task_id(&self) -> ID {
        ID(self.0.task_id.to_string())
    }

    /// Hydrated task, so a report can name what it is about to rewrite. Null only if
    /// the task was deleted between the repair and this resolve.
    async fn task(&self, ctx: &Context<'_>) -> Option<TaskGql> {
        let repo = ctx.data::<Arc<dyn TaskRepository>>().ok()?;
        let task = repo.find_by_id(self.0.task_id).await.ok()??;
        Some(TaskGql(task))
    }

    /// Hours this task's own projection accounted for in the touched half-days.
    /// Excludes the orphans, which accounted for nobody's.
    async fn hours_before(&self) -> f64 {
        self.0.hours_before
    }

    /// Hours it accounts for once the half-days are rewritten from the worklog.
    async fn hours_after(&self) -> f64 {
        self.0.hours_after
    }
}

/// Result of `repairOrphanedSlots`.
///
/// Everything a reader needs to CHECK the repair rather than trust it: per date what
/// would be dropped and what would be written, and per task the hours before and
/// after. `applied: false` means nothing was written. An empty `dates` means the range
/// held no damage — a success, not a refusal, so the same call doubles as the
/// verification that a previous repair worked.
#[derive(SimpleObject)]
pub struct SlotRepairResultGql {
    pub applied: bool,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub dates: Vec<DateRepairGql>,
    pub tasks: Vec<RepairedTaskGql>,
    pub orphans_dropped: i32,
    /// Total hours the dropped orphans were holding. Compare against the tasks'
    /// `hoursAfter`: that is where the time went — unless a date shows orphans
    /// dropped and no slot written, which is time the worklog can no longer explain.
    pub orphan_hours: f64,
    pub slots_discarded: i32,
    pub slots_written: i32,
}

impl From<SlotRepairOutcome> for SlotRepairResultGql {
    fn from(outcome: SlotRepairOutcome) -> Self {
        Self {
            applied: outcome.applied,
            from: outcome.from,
            to: outcome.to,
            dates: outcome.dates.into_iter().map(DateRepairGql::from).collect(),
            tasks: outcome.tasks.into_iter().map(RepairedTaskGql).collect(),
            orphans_dropped: outcome.orphans_dropped as i32,
            orphan_hours: outcome.orphan_hours,
            slots_discarded: outcome.slots_discarded as i32,
            slots_written: outcome.slots_written as i32,
        }
    }
}
