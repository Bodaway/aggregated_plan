//! `aplan search` — one query, four entities.
//!
//! This layer only fetches, filters and pages. Which terms a query breaks into,
//! whether a haystack matches them and how a group gets capped all live in
//! `domain::rules::search`, so those rules are testable without a database (same
//! split as `use_cases::brief`).

use chrono::{DateTime, Duration, Utc};
use domain::rules::recall::{build_match_query, RecallContext, RecallWeights};
use domain::rules::search::{group_from, matches, parse_terms, SearchGroup, SearchHit};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::{
    MeetingRepository, TaskFilter, TaskRepository, WorklogFilter, WorklogRepository,
    WORKLOG_FILTER_MAX_LIMIT,
};
use crate::services::{MemoryRetriever, RecallQuery, RECALL_MAX_LIMIT};

/// How far the meeting search window reaches on each side of today.
/// `MeetingRepository` has no unbounded `list`, only a range query, so the
/// window must be picked explicitly — and 24 months covers every meeting the
/// store holds several times over. Reaches into the future as well as the
/// past: a scheduled meeting is a real search target, and a window that
/// stopped at today would make it structurally unreachable.
const MEETING_SEARCH_MONTHS: i64 = 24;

/// The worklog page size a search asks for: the server's own per-request
/// ceiling (`WorklogFilter::effective_limit`). Asking for more in one call gets
/// silently capped to this anyway.
const WORKLOG_SEARCH_PAGE_SIZE: u32 = WORKLOG_FILTER_MAX_LIMIT;

/// Stops the worklog paging loop from looping forever if a server ever ignored
/// `offset`. Fifty full pages is far past any real worklog (572 rows today), so
/// hitting it means the pagination contract broke, not that the user is
/// prolific. Mirrors `WORKLOG_MAX_PAGES` in `cli::commands`.
const WORKLOG_SEARCH_MAX_PAGES: usize = 50;

/// How many memories `recall_hits` asks the retriever for, deliberately
/// independent of `request.limit` (what `group_from` will actually show).
///
/// `RecallQuery::effective_limit` caps whatever is asked at `RECALL_MAX_LIMIT`
/// regardless, so asking for anything less costs nothing to avoid: it just
/// ties the count of memories *seen* to the count *displayed*, which makes
/// `group_from`'s `total` always equal to `hits.len()` and `hidden()`
/// structurally zero — the one group that could otherwise honestly announce a
/// truncation never can. Asking for the ceiling every time is the truest count
/// this call site can report without changing `MemoryRetriever`'s contract
/// (the SQL-level over-fetch and cap for re-ranking accuracy already live in
/// `SqliteMemoryRetriever`, unaffected by this).
const MEMORY_SEARCH_ASK: u32 = RECALL_MAX_LIMIT;

/// What the caller asks for.
pub struct SearchRequest {
    pub query: String,
    pub limit: usize,
}

/// One entity's group per matched entity, each already capped.
pub struct SearchOutcome {
    pub tasks: SearchGroup,
    pub worklog: SearchGroup,
    pub meetings: SearchGroup,
    pub memories: SearchGroup,
}

fn empty_outcome() -> SearchOutcome {
    SearchOutcome {
        tasks: group_from(Vec::new(), 0),
        worklog: group_from(Vec::new(), 0),
        meetings: group_from(Vec::new(), 0),
        memories: group_from(Vec::new(), 0),
    }
}

/// Search tasks, worklog, meetings and memories for the same query.
///
/// `now` is injected, not read from the clock here, for the same reason
/// `MemoryRetriever::search` takes it explicitly: recency decay in the recall
/// path must be deterministic and testable.
///
/// Nothing here fails on an empty query: a blank `request.query` returns four
/// empty groups rather than every task, worklog entry and meeting the user has
/// — 642 tasks and 572 worklog rows today would drown the caller and teach it
/// never to search again.
pub async fn search(
    task_repo: &dyn TaskRepository,
    worklog_repo: &dyn WorklogRepository,
    meeting_repo: &dyn MeetingRepository,
    memory_retriever: &dyn MemoryRetriever,
    user_id: UserId,
    request: SearchRequest,
    now: DateTime<Utc>,
) -> Result<SearchOutcome, AppError> {
    let terms = parse_terms(&request.query);
    if terms.is_empty() {
        return Ok(empty_outcome());
    }

    // Tasks: `find_by_user`, never the `tasks` GraphQL query and its `first: 50`.
    //
    // Title and description are matched as a single concatenated haystack, not
    // "all terms in the title OR all terms in the description": a memory is one
    // FTS5 document spanning title and body together, so a task must behave the
    // same way or the same query matches differently depending on which entity
    // it happens to hit — the exact defect `search` exists to remove.
    let mut tasks: Vec<SearchHit> = task_repo
        .find_by_user(user_id, &TaskFilter::empty())
        .await?
        .into_iter()
        .filter(|t| matches(&task_haystack(t), &terms))
        .map(|t| SearchHit {
            id: t.id.to_string(),
            title: t.title,
            occurred_on: t.updated_at.date_naive(),
        })
        .collect();
    tasks.sort_by(|a, b| b.occurred_on.cmp(&a.occurred_on));

    // Worklog: paged, on the precedent of `aplan show --worklog all`. Asking for
    // `u32::MAX` in one call would silently return the first 1000 rows and the
    // result would *look* complete.
    let mut worklog: Vec<SearchHit> = collect_worklog_pages(worklog_repo, user_id)
        .await?
        .into_iter()
        .filter(|e| matches(&e.body, &terms))
        .map(|e| SearchHit {
            id: e.id.to_string(),
            title: e.body.clone(),
            occurred_on: e.logged_at.date_naive(),
        })
        .collect();
    worklog.sort_by(|a, b| b.occurred_on.cmp(&a.occurred_on));

    let today = now.date_naive();
    let from = today - Duration::days(MEETING_SEARCH_MONTHS * 30);
    let to = today + Duration::days(MEETING_SEARCH_MONTHS * 30);
    let mut meetings: Vec<SearchHit> = meeting_repo
        .find_by_user_and_range(user_id, from, to)
        .await?
        .into_iter()
        .filter(|m| matches(&m.title, &terms))
        .map(|m| SearchHit {
            id: m.id.to_string(),
            title: m.title.clone(),
            occurred_on: m.start_time.date_naive(),
        })
        .collect();
    meetings.sort_by(|a, b| b.occurred_on.cmp(&a.occurred_on));

    // Memories keep the recall ordering: relevance, not recency. Do not re-sort.
    let memories = recall_hits(memory_retriever, user_id, &request.query, now).await?;

    Ok(SearchOutcome {
        tasks: group_from(tasks, request.limit),
        worklog: group_from(worklog, request.limit),
        meetings: group_from(meetings, request.limit),
        memories: group_from(memories, request.limit),
    })
}

/// Title and description joined into the one haystack a task is matched
/// against — see the comment at the call site for why this must not be two
/// separate `matches()` calls ORed together.
fn task_haystack(task: &Task) -> String {
    match task.description.as_deref() {
        Some(description) => format!("{} {description}", task.title),
        None => task.title.clone(),
    }
}

/// Walk `WorklogRepository::list` page by page until a short page proves there
/// is nothing left, exactly like `--worklog all` in `cli::commands::fetch_worklog`.
/// A single call for everything is not on the table: the server caps any one
/// request at [`WORKLOG_FILTER_MAX_LIMIT`] rows, so asking for more would
/// silently return one page and *look* complete.
async fn collect_worklog_pages(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
) -> Result<Vec<WorklogEntry>, AppError> {
    let mut all = Vec::new();
    for page in 0..WORKLOG_SEARCH_MAX_PAGES {
        let filter = WorklogFilter {
            task_ids: None,
            from: None,
            to: None,
            limit: WORKLOG_SEARCH_PAGE_SIZE,
            offset: WORKLOG_SEARCH_PAGE_SIZE * page as u32,
        };
        let batch = worklog_repo.list(user_id, &filter).await?;
        let full = batch.len() as u32 == WORKLOG_SEARCH_PAGE_SIZE;
        all.extend(batch);
        if !full {
            break;
        }
        // A full last page here means the loop hit `WORKLOG_SEARCH_MAX_PAGES`
        // without ever seeing a short page — the pagination contract broke.
        // Nothing to log to from this layer (no I/O below the domain/repository
        // boundary); the caller gets whatever was collected rather than a crash.
    }
    Ok(all)
}

/// Run `raw_query` through the existing recall path and reduce each
/// `ScoredMemory` to a `SearchHit`, without touching the order recall already
/// produced: relevance, not recency, is what a memory search means.
///
/// Always asks for [`MEMORY_SEARCH_ASK`], not whatever the caller will show —
/// see its doc comment for why the group's cap must not double as the ask.
///
/// Mirrors `use_cases::memory::search_memories`, which is the only other place
/// raw user input becomes an FTS5 `MATCH` expression.
async fn recall_hits(
    memory_retriever: &dyn MemoryRetriever,
    user_id: UserId,
    raw_query: &str,
    now: DateTime<Utc>,
) -> Result<Vec<SearchHit>, AppError> {
    let query = RecallQuery {
        match_query: build_match_query(raw_query)?,
        context: RecallContext::default(),
        include_history: false,
        weights: RecallWeights::default(),
        limit: MEMORY_SEARCH_ASK,
    };
    let scored = memory_retriever.search(user_id, &query, now).await?;
    Ok(scored
        .into_iter()
        .map(|s| SearchHit {
            id: s.memory.id.to_string(),
            title: s.memory.title,
            occurred_on: s.memory.occurred_at.date_naive(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{NaiveDate, TimeZone};
    use domain::rules::recall::ScoredMemory;
    use domain::rules::search::SEARCH_MAX_PER_GROUP;
    use domain::types::recurrence::RecurrenceTemplateId;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;

    // ─── Doubles ─────────────────────────────────────────────────────────

    #[derive(Default)]
    struct MemTaskRepo {
        tasks: Mutex<Vec<Task>>,
    }

    #[async_trait]
    impl TaskRepository for MemTaskRepo {
        async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
            Ok(self.tasks.lock().expect("lock").iter().find(|t| t.id == id).cloned())
        }

        async fn find_by_user(
            &self,
            user_id: UserId,
            _filter: &TaskFilter,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .expect("lock")
                .iter()
                .filter(|t| t.user_id == user_id)
                .cloned()
                .collect())
        }

        async fn find_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _source_id: &str,
        ) -> Result<Option<Task>, RepositoryError> {
            Ok(None)
        }

        async fn find_by_date_range(
            &self,
            _user_id: UserId,
            _start: NaiveDate,
            _end: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(vec![])
        }

        async fn find_planned_before(
            &self,
            _user_id: UserId,
            _before_date: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(vec![])
        }

        async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
            self.tasks.lock().expect("lock").push(task.clone());
            Ok(())
        }

        async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError> {
            self.tasks.lock().expect("lock").extend_from_slice(tasks);
            Ok(())
        }

        async fn delete(&self, id: TaskId) -> Result<(), RepositoryError> {
            self.tasks.lock().expect("lock").retain(|t| t.id != id);
            Ok(())
        }

        async fn delete_stale_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _keep_ids: &[String],
        ) -> Result<u64, RepositoryError> {
            Ok(0)
        }
    }

    /// On the exact pattern of `MemTaskRepo`: `list` mirrors the real
    /// implementation's `ORDER BY logged_at DESC, created_at DESC` plus
    /// `effective_limit`/`offset`, since the paging loop this fixture backs
    /// depends on that contract.
    #[derive(Default)]
    struct MemWorklogRepo {
        entries: Mutex<Vec<WorklogEntry>>,
    }

    #[async_trait]
    impl WorklogRepository for MemWorklogRepo {
        async fn create(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
            self.entries.lock().expect("lock").push(entry.clone());
            Ok(())
        }

        async fn update(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
            let mut entries = self.entries.lock().expect("lock");
            if let Some(existing) = entries.iter_mut().find(|e| e.id == entry.id) {
                *existing = entry.clone();
            }
            Ok(())
        }

        async fn delete(
            &self,
            id: WorklogEntryId,
            _user_id: UserId,
        ) -> Result<bool, RepositoryError> {
            let mut entries = self.entries.lock().expect("lock");
            let before = entries.len();
            entries.retain(|e| e.id != id);
            Ok(entries.len() != before)
        }

        async fn find_by_id(
            &self,
            id: WorklogEntryId,
            _user_id: UserId,
        ) -> Result<Option<WorklogEntry>, RepositoryError> {
            Ok(self.entries.lock().expect("lock").iter().find(|e| e.id == id).cloned())
        }

        async fn list(
            &self,
            user_id: UserId,
            filter: &WorklogFilter,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            let mut matching: Vec<WorklogEntry> = self
                .entries
                .lock()
                .expect("lock")
                .iter()
                .filter(|e| e.user_id == user_id)
                .filter(|e| match &filter.task_ids {
                    None => true,
                    Some(ids) => ids.contains(&e.task_id),
                })
                .filter(|e| filter.from.is_none_or(|from| e.logged_at >= from))
                .filter(|e| filter.to.is_none_or(|to| e.logged_at < to))
                .cloned()
                .collect();
            matching.sort_by(|a, b| {
                b.logged_at.cmp(&a.logged_at).then(b.created_at.cmp(&a.created_at))
            });
            let offset = filter.offset as usize;
            let limit = filter.effective_limit() as usize;
            Ok(matching.into_iter().skip(offset).take(limit).collect())
        }

        async fn find_by_recurrence(
            &self,
            _user_id: UserId,
            _template_id: RecurrenceTemplateId,
            _limit: u32,
            _offset: u32,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            Ok(vec![])
        }
    }

    #[derive(Default)]
    struct MemMeetingRepo {
        meetings: Mutex<Vec<Meeting>>,
    }

    #[async_trait]
    impl MeetingRepository for MemMeetingRepo {
        async fn find_by_id(&self, id: MeetingId) -> Result<Option<Meeting>, RepositoryError> {
            Ok(self.meetings.lock().expect("lock").iter().find(|m| m.id == id).cloned())
        }

        async fn update(&self, meeting: &Meeting) -> Result<(), RepositoryError> {
            let mut meetings = self.meetings.lock().expect("lock");
            if let Some(existing) = meetings.iter_mut().find(|m| m.id == meeting.id) {
                *existing = meeting.clone();
            }
            Ok(())
        }

        async fn find_by_user_and_date(
            &self,
            _user_id: UserId,
            _date: NaiveDate,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(vec![])
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
                .expect("lock")
                .iter()
                .filter(|m| m.user_id == user_id)
                .filter(|m| {
                    let day = m.start_time.date_naive();
                    day >= start && day <= end
                })
                .cloned()
                .collect())
        }

        async fn upsert_batch(&self, meetings: &[Meeting]) -> Result<(), RepositoryError> {
            self.meetings.lock().expect("lock").extend_from_slice(meetings);
            Ok(())
        }

        async fn delete_stale(
            &self,
            _user_id: UserId,
            _current_outlook_ids: &[String],
        ) -> Result<u64, RepositoryError> {
            Ok(0)
        }

        async fn find_by_project(
            &self,
            _user_id: UserId,
            _project_id: ProjectId,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(vec![])
        }
    }

    /// Stands in for the FTS5-backed retriever: records the `match_query` it was
    /// asked for, and returns exactly the `ScoredMemory` list it is told to,
    /// unsorted, so a test can catch this layer re-sorting it.
    #[derive(Default)]
    struct MemMemoryRetriever {
        results: Mutex<Vec<ScoredMemory>>,
        seen_queries: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl MemoryRetriever for MemMemoryRetriever {
        async fn search(
            &self,
            _user_id: UserId,
            query: &RecallQuery,
            _now: DateTime<Utc>,
        ) -> Result<Vec<ScoredMemory>, RepositoryError> {
            self.seen_queries.lock().expect("lock").push(query.match_query.clone());
            Ok(self.results.lock().expect("lock").clone())
        }
    }

    // ─── Fixtures ────────────────────────────────────────────────────────

    fn uid() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("valid uuid")
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 18, 8, 30, 0).single().expect("valid instant")
    }

    fn task_titled(title: &str) -> Task {
        task_titled_at(title, now())
    }

    fn task_with_description(title: &str, description: &str) -> Task {
        Task {
            description: Some(description.to_string()),
            ..task_titled(title)
        }
    }

    fn task_titled_at(title: &str, updated_at: DateTime<Utc>) -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: uid(),
            title: title.to_string(),
            description: None,
            notes: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            delegated_to: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Low,
            tags: vec![],
            tracking_state: TrackingState::Followed,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            recurrence_id: None,
            occurrence_date: None,
            gryzzly_task_id: None,
            gryzzly_project_id: None,
            created_at: now(),
            updated_at,
        }
    }

    fn worklog_entry(body: &str, logged_at: DateTime<Utc>) -> WorklogEntry {
        WorklogEntry::new(uid(), Uuid::new_v4(), body.to_string(), logged_at, now())
            .expect("valid fixture")
    }

    fn meeting_titled(title: &str, start_time: DateTime<Utc>) -> Meeting {
        Meeting {
            id: Uuid::new_v4(),
            user_id: uid(),
            title: title.to_string(),
            start_time,
            end_time: start_time + chrono::Duration::hours(1),
            location: None,
            participants: vec![],
            project_id: None,
            outlook_id: Uuid::new_v4().to_string(),
            show_as: None,
            created_at: now(),
        }
    }

    struct Fixture {
        tasks: MemTaskRepo,
        worklog: MemWorklogRepo,
        meetings: MemMeetingRepo,
        memories: MemMemoryRetriever,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                tasks: MemTaskRepo::default(),
                worklog: MemWorklogRepo::default(),
                meetings: MemMeetingRepo::default(),
                memories: MemMemoryRetriever::default(),
            }
        }

        async fn search(&self, request: SearchRequest) -> SearchOutcome {
            search(
                &self.tasks,
                &self.worklog,
                &self.meetings,
                &self.memories,
                uid(),
                request,
                now(),
            )
            .await
            .expect("search ran")
        }
    }

    fn request(query: &str) -> SearchRequest {
        SearchRequest {
            query: query.to_string(),
            limit: SEARCH_MAX_PER_GROUP,
        }
    }

    // ─── Tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn search_groups_hits_by_entity() {
        let f = Fixture::new();
        f.tasks.save(&task_titled("Réunion WAF eActions")).await.expect("saved");
        f.tasks.save(&task_titled("Sans rapport")).await.expect("saved");

        let out = f.search(request("waf")).await;

        assert_eq!(out.tasks.total, 1, "une seule tâche porte le terme");
        assert_eq!(out.tasks.hits[0].title, "Réunion WAF eActions");
        assert!(out.worklog.hits.is_empty());
    }

    #[tokio::test]
    async fn search_folds_accents_on_tasks_like_it_does_on_memories() {
        let f = Fixture::new();
        f.tasks.save(&task_titled("Fenêtre de maintenance")).await.expect("saved");

        let out = f.search(request("fenetre")).await;

        assert_eq!(out.tasks.total, 1, "sans pliage, ce terme ne trouverait rien");
    }

    #[tokio::test]
    async fn task_terms_can_span_title_and_description() {
        let f = Fixture::new();
        // "waf" only in the title, "eactions" only in the description: a task
        // must match on the concatenation of the two, exactly like a memory
        // matches across its title and body as one FTS5 document.
        f.tasks
            .save(&task_with_description("Réunion WAF", "Suivi eActions"))
            .await
            .expect("saved");

        let out = f.search(request("waf eactions")).await;

        assert_eq!(
            out.tasks.total, 1,
            "the terms are split across title and description, not both in either one alone"
        );
    }

    #[tokio::test]
    async fn an_empty_query_returns_nothing_rather_than_everything() {
        let f = Fixture::new();
        f.tasks.save(&task_titled("Réunion WAF eActions")).await.expect("saved");

        let out = f.search(request("   ")).await;

        assert_eq!(out.tasks.total, 0, "une requête vide ne devient jamais un dump");
        assert!(
            f.memories.seen_queries.lock().expect("lock").is_empty(),
            "an empty query must not even reach the recall path"
        );
    }

    #[tokio::test]
    async fn tasks_worklog_and_meetings_are_sorted_newest_first() {
        let f = Fixture::new();
        let old = now() - chrono::Duration::days(30);
        let recent = now() - chrono::Duration::days(1);

        f.tasks.save(&task_titled_at("waf ancienne", old)).await.expect("saved");
        f.tasks.save(&task_titled_at("waf récente", recent)).await.expect("saved");
        f.worklog.create(&worklog_entry("Ticket waf ancien", old)).await.expect("created");
        f.worklog.create(&worklog_entry("Ticket waf récent", recent)).await.expect("created");
        f.meetings
            .upsert_batch(&[meeting_titled("Revue waf ancienne", old), meeting_titled("Revue waf récente", recent)])
            .await
            .expect("upserted");

        let out = f.search(request("waf")).await;

        assert_eq!(out.tasks.hits[0].title, "waf récente");
        assert_eq!(out.worklog.hits[0].title, "Ticket waf récent");
        assert_eq!(out.meetings.hits[0].title, "Revue waf récente");
    }

    #[tokio::test]
    async fn a_scheduled_meeting_is_reachable() {
        let f = Fixture::new();
        // Before the fix the window ran from `today - 24 months` to `today`:
        // a meeting scheduled next month was structurally unreachable.
        let scheduled = now() + chrono::Duration::days(30);
        f.meetings
            .upsert_batch(&[meeting_titled("Revue waf planifiée", scheduled)])
            .await
            .expect("upserted");

        let out = f.search(request("waf")).await;

        assert_eq!(out.meetings.total, 1, "a future meeting must be searchable, not just past ones");
    }

    #[tokio::test]
    async fn worklog_search_pages_past_the_server_ceiling() {
        let f = Fixture::new();
        // One more row than a single page: if the use case asked for one page
        // only, the oldest (and only non-matching-by-position) row would be
        // silently dropped and this test would see one hit short.
        let total_rows = WORKLOG_FILTER_MAX_LIMIT as i64 + 1;
        for i in 0..total_rows {
            let logged_at = now() - chrono::Duration::minutes(i);
            f.worklog.create(&worklog_entry("waf incident", logged_at)).await.expect("created");
        }

        let out = f.search(SearchRequest { query: "waf".to_string(), limit: usize::MAX }).await;

        assert_eq!(
            out.worklog.total as i64, total_rows,
            "a single-page read would silently look complete at {WORKLOG_FILTER_MAX_LIMIT}"
        );
    }

    #[tokio::test]
    async fn memory_hits_keep_the_recall_order_unsorted() {
        let f = Fixture::new();
        let weak = ScoredMemory {
            memory: memory_named("Décision faible pertinence", now() - chrono::Duration::days(1)),
            score: 0.1,
        };
        let strong = ScoredMemory {
            memory: memory_named("Décision forte pertinence", now() - chrono::Duration::days(400)),
            score: 0.9,
        };
        // Deliberately handed back weak-first, and the older one scored higher:
        // a recency re-sort here would flip it back, a relevance re-sort would
        // also flip it. Neither may happen in this layer.
        *f.memories.results.lock().expect("lock") = vec![weak.clone(), strong.clone()];

        let out = f.search(request("décision")).await;

        assert_eq!(out.memories.hits[0].title, weak.memory.title);
        assert_eq!(out.memories.hits[1].title, strong.memory.title);
        assert_eq!(
            f.memories.seen_queries.lock().expect("lock").last(),
            Some(&"\"décision\"*".to_string()),
            "the raw query must reach recall as a built FTS5 match expression"
        );
    }

    #[tokio::test]
    async fn memories_group_announces_its_truncation() {
        let f = Fixture::new();
        // More matches than `SEARCH_MAX_PER_GROUP` (the request's limit): if
        // this use case asked the retriever for only what it will show, the
        // retriever would hand back exactly that many and `total` could never
        // exceed `hits.len()` — `hidden()` would be structurally zero, the bug
        // this test exists to catch.
        let matches: Vec<ScoredMemory> = (0..(SEARCH_MAX_PER_GROUP + 3))
            .map(|i| ScoredMemory {
                memory: memory_named(&format!("Décision {i}"), now() - chrono::Duration::days(i as i64)),
                score: 1.0 - (i as f64 * 0.01),
            })
            .collect();
        *f.memories.results.lock().expect("lock") = matches;

        let out = f.search(request("décision")).await;

        assert_eq!(out.memories.total, SEARCH_MAX_PER_GROUP + 3);
        assert_eq!(out.memories.hits.len(), SEARCH_MAX_PER_GROUP);
        assert_eq!(
            out.memories.hidden(),
            3,
            "a memories group with more matches than the cap must report them as hidden, not silently drop them"
        );
    }

    fn memory_named(title: &str, occurred_at: DateTime<Utc>) -> Memory {
        Memory::new(
            uid(),
            NewMemory {
                kind: MemoryKind::Decision,
                title: title.to_string(),
                body: None,
                occurred_at: Some(occurred_at),
                source: MemorySource::ClaudeSession,
                source_ref: None,
                status: MemoryStatus::Active,
                proposed_supersedes: None,
                project_id: None,
                task_id: None,
                stakeholders: vec![],
            },
            now(),
        )
        .expect("valid fixture")
    }
}
