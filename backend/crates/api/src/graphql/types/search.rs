use async_graphql::SimpleObject;
use chrono::NaiveDate;
use domain::rules::search::{SearchGroup, SearchHit};

use application::use_cases::search::SearchOutcome;

/// One search hit, reduced to what a caller needs to decide whether to drill in.
#[derive(SimpleObject)]
pub struct SearchHitGql {
    pub id: String,
    pub title: String,
    pub occurred_on: NaiveDate,
}

impl From<SearchHit> for SearchHitGql {
    fn from(hit: SearchHit) -> Self {
        Self {
            id: hit.id,
            title: hit.title,
            occurred_on: hit.occurred_on,
        }
    }
}

fn search_hits(group: &SearchGroup) -> Vec<SearchHitGql> {
    group.hits.iter().cloned().map(SearchHitGql::from).collect()
}

/// The cross-entity search (`aplan search`): tasks, worklog, meetings and
/// memories that match the same query, one already-capped group per entity.
/// Selection, matching and capping all happen in `domain`/`application` —
/// this type only carries their shape across the wire.
#[derive(SimpleObject)]
pub struct SearchGql {
    pub tasks: Vec<SearchHitGql>,
    /// How many tasks matched, before the group cap.
    pub task_total: i32,
    pub worklog: Vec<SearchHitGql>,
    /// How many worklog entries matched, before the group cap.
    pub worklog_total: i32,
    pub meetings: Vec<SearchHitGql>,
    /// How many meetings matched, before the group cap.
    pub meeting_total: i32,
    pub memories: Vec<SearchHitGql>,
    /// How many memories matched, before the group cap.
    pub memory_total: i32,
}

impl From<SearchOutcome> for SearchGql {
    fn from(outcome: SearchOutcome) -> Self {
        Self {
            tasks: search_hits(&outcome.tasks),
            task_total: outcome.tasks.total as i32,
            worklog: search_hits(&outcome.worklog),
            worklog_total: outcome.worklog.total as i32,
            meetings: search_hits(&outcome.meetings),
            meeting_total: outcome.meetings.total as i32,
            memories: search_hits(&outcome.memories),
            memory_total: outcome.memories.total as i32,
        }
    }
}
