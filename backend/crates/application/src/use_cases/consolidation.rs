//! The deterministic machinery of the 17:30 consolidation (§6.2 of the design).
//!
//! The consolidation itself — reading the day's worklog and proposing durable
//! memories — runs as a **scheduled Claude Code session**, not here. That is a
//! deliberate boundary: the backend holds no LLM client, no API key and no prompt.
//! What lives in this module is only what that session drives:
//!
//! 1. **the read side of the watermark** — the entries nobody has consolidated yet;
//! 2. **the write side of the watermark** — stamping those entries, once the
//!    memories they produced are persisted;
//! 3. **the last-run timestamp**, so `aplan brief` can say a consolidation has gone
//!    quiet instead of failing silently.
//!
//! Two properties this module exists to protect:
//!
//! - The watermark is **per entry**, never a global timestamp cursor. A cursor
//!   would permanently skip any entry inserted later with an earlier `logged_at`,
//!   and nothing would ever report the loss.
//! - The marker is written **after** the writes it accounts for. A duplicate
//!   memory is recoverable through the rejection tombstones (§6.3); an entry
//!   skipped forever is not. Ordering the two the other way trades a recoverable
//!   failure for an unrecoverable one.

use chrono::{DateTime, Utc};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::{ConfigRepository, WorklogFilter, WorklogRepository};

// The one key name. `use_cases::brief` READS it to render "Dernière
// consolidation : …"; this module WRITES it. Importing rather than redeclaring is
// the point: a second constant with a near-identical name would make the brief
// report "jamais exécutée" forever while the job dutifully recorded every run.
pub use crate::use_cases::brief::CONSOLIDATION_LAST_RUN_KEY;

/// How many unconsolidated entries a run reads when the caller does not say.
/// Above a day's worth of atomic worklog entries, and below the repository cap so
/// a backlog is drained over several runs rather than truncated silently.
pub const CONSOLIDATION_BATCH_LIMIT: u32 = 200;

/// Outcome of stamping the watermark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkConsolidatedOutcome {
    /// How many ids the caller submitted.
    pub requested: usize,
    /// How many rows actually moved from unmarked to marked. Lower than
    /// `requested` when an id was already consolidated, or belongs to another
    /// user — both of which are silent no-ops by design, not errors.
    pub marked: u64,
    /// The timestamp written.
    pub consolidated_at: DateTime<Utc>,
}

/// The worklog entries the consolidation has never read (`consolidated_at IS
/// NULL`), oldest first.
///
/// `limit` of `0` means "the default"; the repository resolves and caps it through
/// [`WorklogFilter::effective_limit`].
pub async fn list_unconsolidated_entries(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    limit: u32,
) -> Result<Vec<WorklogEntry>, AppError> {
    let filter = WorklogFilter {
        limit: match limit {
            0 => CONSOLIDATION_BATCH_LIMIT,
            n => n,
        },
        ..WorklogFilter::default()
    };
    Ok(worklog_repo.list_unconsolidated(user_id, &filter).await?)
}

/// Stamp `consolidated_at` on the entries a run has finished with.
///
/// An empty list is a no-op, not an error: a run that found nothing to consolidate
/// must be able to finish cleanly, and returning an error there would train the
/// scheduled job to ignore its own failures.
pub async fn mark_entries_consolidated(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    ids: &[WorklogEntryId],
    now: DateTime<Utc>,
) -> Result<MarkConsolidatedOutcome, AppError> {
    if ids.is_empty() {
        return Ok(MarkConsolidatedOutcome {
            requested: 0,
            marked: 0,
            consolidated_at: now,
        });
    }
    let marked = worklog_repo.mark_consolidated(user_id, ids, now).await?;
    Ok(MarkConsolidatedOutcome {
        requested: ids.len(),
        marked,
        consolidated_at: now,
    })
}

/// Record that a consolidation run happened, in the `configuration` table.
///
/// `sync_status` cannot carry this: its `source` column is under a closed `CHECK`
/// (`jira` / `outlook` / `excel` / `obsidian`), SQLite cannot `ALTER` a `CHECK`,
/// and widening `domain::Source` would leak a fake sync source into
/// `dashboard.syncStatuses`.
///
/// Written as RFC 3339 because that is what the brief parses back.
pub async fn record_consolidation_run(
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    at: DateTime<Utc>,
) -> Result<DateTime<Utc>, AppError> {
    config_repo
        .set(user_id, CONSOLIDATION_LAST_RUN_KEY, &at.to_rfc3339())
        .await?;
    Ok(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use domain::rules::brief::ConsolidationAge;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;
    use crate::repositories::WORKLOG_FILTER_DEFAULT_LIMIT;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-03T17:30:00+00:00")
            .expect("valid fixture")
            .with_timezone(&Utc)
    }

    // ─── Doubles ─────────────────────────────────────────────────────────────

    /// Records what the use case asked for, so the tests can pin the contract the
    /// SQLite implementation has to honour.
    #[derive(Default)]
    struct SpyWorklogRepo {
        entries: Mutex<Vec<(WorklogEntry, Option<DateTime<Utc>>)>>,
        limits_seen: Mutex<Vec<u32>>,
        marked_calls: Mutex<Vec<(Vec<WorklogEntryId>, DateTime<Utc>)>>,
    }

    impl SpyWorklogRepo {
        fn push(&self, entry: WorklogEntry, consolidated_at: Option<DateTime<Utc>>) {
            self.entries
                .lock()
                .expect("lock")
                .push((entry, consolidated_at));
        }
    }

    #[async_trait]
    impl WorklogRepository for SpyWorklogRepo {
        async fn create(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
            self.push(entry.clone(), None);
            Ok(())
        }
        async fn update(&self, _entry: &WorklogEntry) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn delete(
            &self,
            _id: WorklogEntryId,
            _user_id: UserId,
        ) -> Result<bool, RepositoryError> {
            Ok(false)
        }
        async fn find_by_id(
            &self,
            _id: WorklogEntryId,
            _user_id: UserId,
        ) -> Result<Option<WorklogEntry>, RepositoryError> {
            Ok(None)
        }
        async fn list(
            &self,
            _user_id: UserId,
            _filter: &WorklogFilter,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            Ok(vec![])
        }
        async fn find_by_recurrence(
            &self,
            _user_id: UserId,
            _template_id: domain::types::recurrence::RecurrenceTemplateId,
            _limit: u32,
            _offset: u32,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            Ok(vec![])
        }

        async fn list_unconsolidated(
            &self,
            user_id: UserId,
            filter: &WorklogFilter,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            // The double binds `effective_limit()` exactly as the SQL must.
            let limit = filter.effective_limit();
            self.limits_seen.lock().expect("lock").push(limit);
            let rows = self.entries.lock().expect("lock");
            let mut out: Vec<WorklogEntry> = rows
                .iter()
                .filter(|(entry, marker)| entry.user_id == user_id && marker.is_none())
                .map(|(entry, _)| entry.clone())
                .collect();
            out.sort_by_key(|entry| entry.logged_at);
            out.truncate(limit as usize);
            Ok(out)
        }

        async fn mark_consolidated(
            &self,
            user_id: UserId,
            ids: &[WorklogEntryId],
            at: DateTime<Utc>,
        ) -> Result<u64, RepositoryError> {
            self.marked_calls
                .lock()
                .expect("lock")
                .push((ids.to_vec(), at));
            let mut rows = self.entries.lock().expect("lock");
            let mut marked = 0u64;
            for (entry, marker) in rows.iter_mut() {
                if entry.user_id == user_id && ids.contains(&entry.id) && marker.is_none() {
                    *marker = Some(at);
                    marked += 1;
                }
            }
            Ok(marked)
        }
    }

    #[derive(Default)]
    struct FakeConfigRepo {
        map: Mutex<HashMap<String, String>>,
    }

    #[async_trait]
    impl ConfigRepository for FakeConfigRepo {
        async fn get(
            &self,
            _user_id: UserId,
            key: &str,
        ) -> Result<Option<String>, RepositoryError> {
            Ok(self.map.lock().expect("lock").get(key).cloned())
        }
        async fn get_all(
            &self,
            _user_id: UserId,
        ) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(self
                .map
                .lock()
                .expect("lock")
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }
        async fn set(
            &self,
            _user_id: UserId,
            key: &str,
            value: &str,
        ) -> Result<(), RepositoryError> {
            self.map
                .lock()
                .expect("lock")
                .insert(key.to_string(), value.to_string());
            Ok(())
        }
    }

    fn entry(user_id: UserId, body: &str, logged_at: DateTime<Utc>) -> WorklogEntry {
        WorklogEntry::new(user_id, Uuid::new_v4(), body.into(), logged_at, logged_at)
            .expect("valid fixture")
    }

    // ─── The read side of the watermark ──────────────────────────────────────

    #[tokio::test]
    async fn only_unconsolidated_entries_come_back() {
        let repo = SpyWorklogRepo::default();
        let uid = Uuid::new_v4();
        repo.push(entry(uid, "already read", now()), Some(now()));
        repo.push(entry(uid, "never read", now()), None);

        let out = list_unconsolidated_entries(&repo, uid, 0)
            .await
            .expect("lists");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].body, "never read");
    }

    /// Oldest first: the job is a catch-up, and a truncated page must leave the
    /// most RECENT entries behind, not the ones already overdue.
    #[tokio::test]
    async fn entries_come_back_oldest_first() {
        let repo = SpyWorklogRepo::default();
        let uid = Uuid::new_v4();
        repo.push(entry(uid, "newer", now()), None);
        repo.push(entry(uid, "older", now() - chrono::Duration::days(3)), None);

        let out = list_unconsolidated_entries(&repo, uid, 0)
            .await
            .expect("lists");
        assert_eq!(
            out.iter().map(|e| e.body.as_str()).collect::<Vec<_>>(),
            vec!["older", "newer"]
        );
    }

    #[tokio::test]
    async fn another_users_entries_are_never_returned() {
        let repo = SpyWorklogRepo::default();
        let mine = Uuid::new_v4();
        let theirs = Uuid::new_v4();
        repo.push(entry(theirs, "not mine", now()), None);

        assert!(list_unconsolidated_entries(&repo, mine, 0)
            .await
            .expect("lists")
            .is_empty());
    }

    /// A `0` limit must never reach SQL as `LIMIT 0`: the job would read "nothing
    /// left to consolidate" and stamp nothing, every evening, in silence.
    #[tokio::test]
    async fn a_zero_limit_resolves_to_the_batch_default_not_to_limit_zero() {
        let repo = SpyWorklogRepo::default();
        let uid = Uuid::new_v4();
        repo.push(entry(uid, "still there", now()), None);

        let out = list_unconsolidated_entries(&repo, uid, 0)
            .await
            .expect("lists");
        assert_eq!(out.len(), 1, "a default limit must not empty the page");
        assert_eq!(
            *repo.limits_seen.lock().expect("lock"),
            vec![CONSOLIDATION_BATCH_LIMIT]
        );
    }

    #[tokio::test]
    async fn an_oversized_limit_is_capped_by_the_filter() {
        let repo = SpyWorklogRepo::default();
        let uid = Uuid::new_v4();
        let _ = list_unconsolidated_entries(&repo, uid, u32::MAX).await;
        assert_eq!(
            *repo.limits_seen.lock().expect("lock"),
            vec![crate::repositories::WORKLOG_FILTER_MAX_LIMIT]
        );
    }

    #[tokio::test]
    async fn the_batch_default_stays_within_the_repository_cap() {
        assert!(CONSOLIDATION_BATCH_LIMIT <= WORKLOG_FILTER_DEFAULT_LIMIT);
    }

    // ─── The write side of the watermark ─────────────────────────────────────

    #[tokio::test]
    async fn marking_an_entry_removes_it_from_the_next_run() {
        let repo = SpyWorklogRepo::default();
        let uid = Uuid::new_v4();
        let keep = entry(uid, "keep", now() - chrono::Duration::hours(2));
        let done = entry(uid, "done", now());
        repo.push(keep.clone(), None);
        repo.push(done.clone(), None);

        let outcome = mark_entries_consolidated(&repo, uid, &[done.id], now())
            .await
            .expect("marks");
        assert_eq!(outcome.marked, 1);
        assert_eq!(outcome.requested, 1);
        assert_eq!(outcome.consolidated_at, now());

        let left = list_unconsolidated_entries(&repo, uid, 0)
            .await
            .expect("lists");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, keep.id);
    }

    /// Idempotent: re-marking the same entry moves no row, so a job that crashed
    /// after stamping and retried cannot rewrite the date.
    #[tokio::test]
    async fn re_marking_an_entry_moves_no_row() {
        let repo = SpyWorklogRepo::default();
        let uid = Uuid::new_v4();
        let e = entry(uid, "once", now());
        repo.push(e.clone(), None);

        assert_eq!(
            mark_entries_consolidated(&repo, uid, &[e.id], now())
                .await
                .expect("marks")
                .marked,
            1
        );
        let later = now() + chrono::Duration::days(1);
        assert_eq!(
            mark_entries_consolidated(&repo, uid, &[e.id], later)
                .await
                .expect("marks")
                .marked,
            0,
            "the first marking wins"
        );
    }

    #[tokio::test]
    async fn marking_cannot_reach_another_users_entry() {
        let repo = SpyWorklogRepo::default();
        let mine = Uuid::new_v4();
        let theirs = Uuid::new_v4();
        let e = entry(theirs, "not mine", now());
        repo.push(e.clone(), None);

        let outcome = mark_entries_consolidated(&repo, mine, &[e.id], now())
            .await
            .expect("marks");
        assert_eq!(outcome.marked, 0);
        assert_eq!(outcome.requested, 1, "the request is still reported in full");
    }

    /// A run that produced nothing must finish cleanly. An error here would teach
    /// the scheduled job that a non-zero exit is normal.
    #[tokio::test]
    async fn marking_nothing_is_a_no_op_and_never_touches_the_repository() {
        let repo = SpyWorklogRepo::default();
        let outcome = mark_entries_consolidated(&repo, Uuid::new_v4(), &[], now())
            .await
            .expect("no-op succeeds");
        assert_eq!(outcome.requested, 0);
        assert_eq!(outcome.marked, 0);
        assert!(
            repo.marked_calls.lock().expect("lock").is_empty(),
            "an empty id list must not issue an UPDATE"
        );
    }

    // ─── The last-run timestamp ──────────────────────────────────────────────

    #[tokio::test]
    async fn recording_a_run_writes_the_key_the_brief_reads() {
        let config = FakeConfigRepo::default();
        let uid = Uuid::new_v4();
        record_consolidation_run(&config, uid, now())
            .await
            .expect("records");
        assert_eq!(
            config
                .get(uid, CONSOLIDATION_LAST_RUN_KEY)
                .await
                .expect("reads"),
            Some(now().to_rfc3339())
        );
    }

    /// The regression that matters: the writer and the reader must agree on the key
    /// name AND on the format. Two constants, or one side writing a naive datetime,
    /// would leave the brief announcing "jamais exécutée" forever while every run
    /// dutifully recorded itself — the exact silent failure §6.2 asks the brief to
    /// expose.
    #[tokio::test]
    async fn what_the_run_records_is_what_the_brief_reads_back() {
        let config = FakeConfigRepo::default();
        let uid = Uuid::new_v4();

        let raw = config
            .get(uid, CONSOLIDATION_LAST_RUN_KEY)
            .await
            .expect("reads");
        assert_eq!(raw, None, "nothing recorded yet");

        record_consolidation_run(&config, uid, now())
            .await
            .expect("records");

        let stored = config
            .get(uid, CONSOLIDATION_LAST_RUN_KEY)
            .await
            .expect("reads")
            .expect("a value is present");
        let parsed = DateTime::parse_from_rfc3339(stored.trim())
            .expect("the brief parses this back with parse_from_rfc3339")
            .with_timezone(&Utc);
        assert_eq!(parsed, now());

        // And the age the brief would compute from it is "ran today", not stale.
        let age = ConsolidationAge::Ran {
            days_ago: (now() - parsed).num_days().max(0),
        };
        assert_eq!(age, ConsolidationAge::Ran { days_ago: 0 });
        assert!(!age.is_stale());
    }
}
