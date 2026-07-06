use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use domain::rules::project_mapping::{resolve_signal_project, ProjectResolution, RawSignal};
use domain::rules::reconstruction::{
    reconstruct_day, renormalize_lines, DayInputs, EditedLine, MeetingBlock, MeetingKind,
    ReconstructedDay, ReconstructionConfig, Signal, SignalKind,
};
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{
    ConfigRepository, GryzzlyCatalogRepository, MeetingRepository, SignalMappingRepository,
    TaskRepository, TimesheetDraftRepository, WorklogFilter, WorklogRepository,
    WORKLOG_FILTER_MAX_LIMIT,
};
use crate::services::git_connector::{jira_key_in, GitConnector};
use crate::time::{local_day_bounds, resolve_tz, to_local};

/// Upper bound when scanning the Gryzzly catalog to derive the set of live project
/// ids. Set well above any realistic catalog size (the catalog is task-grained and
/// the set dedupes to distinct projects). FOLLOW-UP (Plan 2): replace with a dedicated
/// `distinct_active_project_ids` repo method so growth can never silently drop a project
/// and cause a false StaleMapping downgrade.
const CATALOG_SCAN_LIMIT: i64 = 100_000;

#[derive(Debug, Clone, Copy)]
pub enum DayOffScope {
    Full,
    Morning,
    Afternoon,
}

/// Read the reconstruction config from the key-value store (with defaults).
pub async fn load_reconstruction_config(
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
) -> Result<ReconstructionConfig, AppError> {
    async fn f64_key(c: &dyn ConfigRepository, u: UserId, k: &str, d: f64) -> f64 {
        c.get(u, k).await.ok().flatten().and_then(|s| s.parse().ok()).unwrap_or(d)
    }
    async fn u32_key(c: &dyn ConfigRepository, u: UserId, k: &str, d: u32) -> u32 {
        c.get(u, k).await.ok().flatten().and_then(|s| s.parse().ok()).unwrap_or(d)
    }
    let rounding_minutes = f64_key(config_repo, user_id, "gryzzly.rounding_minutes", 15.0).await;
    Ok(ReconstructionConfig {
        morning: (
            u32_key(config_repo, user_id, "workday.morning_start_hour", 8).await,
            u32_key(config_repo, user_id, "workday.morning_end_hour", 12).await,
        ),
        afternoon: (
            u32_key(config_repo, user_id, "workday.afternoon_start_hour", 13).await,
            u32_key(config_repo, user_id, "workday.afternoon_end_hour", 17).await,
        ),
        daily_target_hours: f64_key(config_repo, user_id, "workday.daily_target_hours", 7.5).await,
        rounding_hours: (rounding_minutes / 60.0).max(f64::EPSILON),
        min_signal_hours: f64_key(config_repo, user_id, "timesheet.min_signal_hours", 2.0).await,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn reconstruct_timesheet(
    worklog_repo: &dyn WorklogRepository,
    meeting_repo: &dyn MeetingRepository,
    task_repo: &dyn TaskRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    mapping_repo: &dyn SignalMappingRepository,
    config_repo: &dyn ConfigRepository,
    git: &dyn GitConnector,
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
) -> Result<ReconstructedDay, AppError> {
    let tz = resolve_tz(config_repo.get(user_id, "aplan.timezone").await?);
    let (from_utc, to_utc) = local_day_bounds(date, tz);
    let cfg = load_reconstruction_config(config_repo, user_id).await?;

    let rules = mapping_repo.list_enabled(user_id).await?;
    let live_project_ids: HashSet<String> = catalog_repo
        .list_active(user_id, None, None, CATALOG_SCAN_LIMIT)
        .await?
        .into_iter()
        .map(|e| e.gryzzly_project_id)
        .collect();

    // ---- Worklog signals ----
    let wl = worklog_repo
        .list(
            user_id,
            &WorklogFilter { task_ids: None, from: Some(from_utc), to: Some(to_utc), limit: WORKLOG_FILTER_MAX_LIMIT, offset: 0 },
        )
        .await?;
    let mut signals: Vec<Signal> = Vec::new();
    for e in &wl {
        let task = task_repo.find_by_id(e.task_id).await?;
        let raw = RawSignal::Worklog {
            task_gryzzly_project_id: task.as_ref().and_then(|t| t.gryzzly_project_id.clone()),
        };
        let project = mapped_or_none(&raw, &rules, &live_project_ids);
        signals.push(Signal {
            at: to_local(e.logged_at, tz),
            gryzzly_project_id: project,
            kind: SignalKind::Log,
            label: truncate(&e.body, 60),
            source_ref: format!("wl:{}", e.id),
        });
    }

    // ---- Git commit signals ----
    let repos = split_repos(config_repo.get(user_id, "git.repos").await?);
    if !repos.is_empty() {
        let commits = git.commits_between(&repos, from_utc, to_utc).await?;
        for c in &commits {
            // Prefer a Jira key match to a task; else fall back to repo/branch rules.
            let mut project = None;
            if let Some(key) = jira_key_in(&c.message).or_else(|| jira_key_in(&c.branch)) {
                if let Some(t) = task_repo.find_by_source(user_id, Source::Jira, &key).await? {
                    project = t.gryzzly_project_id.clone().filter(|p| live_project_ids.contains(p));
                }
            }
            if project.is_none() {
                let raw = RawSignal::Commit { repo_path: c.repo_path.clone(), branch: c.branch.clone() };
                project = mapped_or_none(&raw, &rules, &live_project_ids);
            }
            signals.push(Signal {
                at: to_local(c.committed_at, tz),
                gryzzly_project_id: project,
                kind: SignalKind::Commit,
                label: truncate(&c.message, 60),
                source_ref: format!("git:{}:{}", c.repo_path, c.committed_at.to_rfc3339()),
            });
        }
    }

    // ---- Meeting anchors ----
    let meetings_raw = meeting_repo.find_by_user_and_date(user_id, date).await?;
    let mut meetings: Vec<MeetingBlock> = Vec::new();
    for m in &meetings_raw {
        let kind = if is_out_of_office(m) { MeetingKind::OutOfOffice } else { MeetingKind::Work };
        let project = if matches!(kind, MeetingKind::Work) {
            let raw = RawSignal::Meeting {
                subject: m.title.clone(),
                organizer: meeting_organizer(m),
                internal_project_id: m.project_id.map(|p| p.to_string()),
            };
            mapped_or_none(&raw, &rules, &live_project_ids)
        } else {
            None
        };
        meetings.push(MeetingBlock {
            start: to_local(m.start_time, tz),
            end: to_local(m.end_time, tz),
            gryzzly_project_id: project,
            kind,
            title: m.title.clone(),
            source_ref: format!("mtg:{}", m.id),
        });
    }

    let day = reconstruct_day(&DayInputs { date, meetings, signals }, &cfg);

    // Persist as a draft, but NEVER clobber a validated/submitted day.
    if let Some(existing) = draft_repo.find_by_user_and_date(user_id, date).await? {
        if matches!(existing.status, TimesheetStatus::Validated | TimesheetStatus::Submitted | TimesheetStatus::DayOff) {
            return Ok(day);
        }
    }
    let draft = to_draft(user_id, &day, cfg.daily_target_hours, TimesheetStatus::Draft).await?;
    draft_repo.upsert(&draft).await?;
    Ok(day)
}

/// Build a persistable draft from a reconstructed day. Lines carry project_name: None —
/// name resolution is deferred to the GraphQL/CLI layer (Plan 2) via list_active.
async fn to_draft(
    user_id: UserId,
    day: &ReconstructedDay,
    target_hours: f64,
    status: TimesheetStatus,
) -> Result<TimesheetDraft, AppError> {
    let now = Utc::now();
    let mut lines: Vec<TimesheetDraftLine> = Vec::new();
    for a in &day.allocations {
        lines.push(TimesheetDraftLine {
            id: Uuid::new_v4(),
            gryzzly_project_id: Some(a.gryzzly_project_id.clone()),
            project_name: None,
            hours: a.hours,
            is_pinned: false,
            confidence: a.confidence,
            source_refs: a.source_refs.clone(),
        });
    }
    if day.unattributed_hours > 0.0 {
        lines.push(TimesheetDraftLine {
            id: Uuid::new_v4(),
            gryzzly_project_id: None,
            project_name: None,
            hours: day.unattributed_hours,
            is_pinned: false,
            confidence: Confidence::Low,
            source_refs: vec![],
        });
    }
    let blocks_json = serde_json::to_string(
        &day.blocks
            .iter()
            .map(|b| {
                serde_json::json!({
                    "start": b.start.to_string(),
                    "end": b.end.to_string(),
                    "gryzzlyProjectId": b.gryzzly_project_id,
                    "kind": format!("{:?}", b.kind),
                    "hours": b.hours,
                    "sourceRefs": b.source_refs,
                })
            })
            .collect::<Vec<_>>(),
    )
    .ok();
    Ok(TimesheetDraft {
        id: Uuid::new_v4(),
        user_id,
        date: day.date,
        status,
        target_hours,
        total_hours: day.total_hours,
        day_confidence: day.day_confidence,
        blocks_json,
        lines,
        created_at: now,
        updated_at: now,
    })
}

/// Persist user edits: re-normalize (pinned frozen), store, keep status=draft.
#[allow(clippy::too_many_arguments)]
pub async fn save_timesheet_draft(
    draft_repo: &dyn TimesheetDraftRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    date: NaiveDate,
    edited: Vec<EditedLine>,
) -> Result<(), AppError> {
    let cfg = load_reconstruction_config(config_repo, user_id).await?;
    // Pinned lines are frozen; if the user pins more than the target, the total can't
    // honor the invariant — reject rather than silently persist an over-target day.
    let pinned_total: f64 = edited.iter().filter(|l| l.is_pinned).map(|l| l.hours).sum();
    if pinned_total > cfg.daily_target_hours + 1e-9 {
        return Err(AppError::Validation(format!(
            "pinned hours ({pinned_total}) exceed the daily target ({})",
            cfg.daily_target_hours
        )));
    }
    let normalized = renormalize_lines(&edited, cfg.daily_target_hours, cfg.rounding_hours);
    let now = Utc::now();
    let lines = normalized
        .into_iter()
        .map(|l| TimesheetDraftLine {
            id: Uuid::new_v4(),
            gryzzly_project_id: l.gryzzly_project_id,
            project_name: None,
            hours: l.hours,
            is_pinned: l.is_pinned,
            confidence: Confidence::High,
            source_refs: vec![],
        })
        .collect::<Vec<_>>();
    let total: f64 = lines.iter().map(|l| l.hours).sum();
    let existing = draft_repo.find_by_user_and_date(user_id, date).await?;
    let draft = TimesheetDraft {
        id: existing.as_ref().map(|d| d.id).unwrap_or_else(Uuid::new_v4),
        user_id,
        date,
        status: TimesheetStatus::Draft,
        target_hours: cfg.daily_target_hours,
        total_hours: total,
        day_confidence: existing.map(|d| d.day_confidence).unwrap_or(Confidence::Medium),
        blocks_json: None,
        lines,
        created_at: now,
        updated_at: now,
    };
    draft_repo.upsert(&draft).await?;
    Ok(())
}

pub async fn validate_timesheet(
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
) -> Result<(), AppError> {
    draft_repo.set_status(user_id, date, TimesheetStatus::Validated).await?;
    Ok(())
}

pub async fn mark_day_off(
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
    _scope: DayOffScope,
) -> Result<(), AppError> {
    // v1: full-day off. (Half-day scoping refines total_hours in a later iteration.)
    let now = Utc::now();
    let draft = TimesheetDraft {
        id: Uuid::new_v4(),
        user_id,
        date,
        status: TimesheetStatus::DayOff,
        target_hours: 0.0,
        total_hours: 0.0,
        day_confidence: Confidence::High,
        blocks_json: None,
        lines: vec![],
        created_at: now,
        updated_at: now,
    };
    draft_repo.upsert(&draft).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn learn_mapping(
    mapping_repo: &dyn SignalMappingRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    user_id: UserId,
    kind: MappingKind,
    pattern: String,
    branch_pattern: Option<String>,
    gryzzly_project_id: String,
    now: DateTime<Utc>,
) -> Result<SignalMapping, AppError> {
    // Validate the target project against the live catalog + fetch its display name.
    let name = catalog_repo
        .list_active(user_id, None, None, CATALOG_SCAN_LIMIT)
        .await?
        .into_iter()
        .find(|e| e.gryzzly_project_id == gryzzly_project_id)
        .map(|e| e.project_name);
    if name.is_none() {
        return Err(AppError::Validation(format!(
            "unknown or inactive Gryzzly project: {gryzzly_project_id}"
        )));
    }
    let mapping = SignalMapping {
        id: Uuid::new_v4(),
        user_id,
        kind,
        pattern,
        branch_pattern,
        gryzzly_project_id,
        gryzzly_project_name: name,
        is_enabled: true,
        created_at: now,
        updated_at: now,
    };
    mapping_repo.upsert(&mapping).await?;
    Ok(mapping)
}

fn mapped_or_none(
    raw: &RawSignal,
    rules: &[SignalMapping],
    live: &HashSet<String>,
) -> Option<String> {
    match resolve_signal_project(raw, rules, live) {
        ProjectResolution::Mapped { gryzzly_project_id, .. } => Some(gryzzly_project_id),
        ProjectResolution::Unmapped { .. } => None,
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

fn split_repos(v: Option<String>) -> Vec<String> {
    v.map(|s| {
        s.split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// A meeting is out-of-office if Outlook marked it `oof` or its title looks like leave.
fn is_out_of_office(m: &Meeting) -> bool {
    let show_as = m.show_as.as_deref().unwrap_or("").to_lowercase();
    if show_as == "oof" {
        return true;
    }
    let t = m.title.to_lowercase();
    ["congé", "conge", "vacances", "pto", "ooo", "out of office", "absent"]
        .iter()
        .any(|kw| t.contains(kw))
}

fn meeting_organizer(_m: &Meeting) -> Option<String> {
    // Meeting schema has no organizer column today; return None until added.
    None
}

/// The local dates the end-of-day job should (re)process on this tick, ascending.
///
/// - Every missed local date STRICTLY after `last_auto_run` and STRICTLY before `local_today`
///   (catch-up for days the machine was off) — but only when a watermark exists; with no
///   watermark we never backfill history.
/// - Plus `local_today` itself, IFF `local_hour >= trigger_hour` AND today isn't already the
///   watermark (so today is processed at most once per day, not every tick).
/// - Capped to the most recent `cap` dates (avoid reconstructing months after a long absence).
pub fn compute_target_dates(
    last_auto_run: Option<NaiveDate>,
    local_today: NaiveDate,
    local_hour: u32,
    trigger_hour: u32,
    cap: usize,
) -> Vec<NaiveDate> {
    let mut dates = Vec::new();
    if let Some(last) = last_auto_run {
        let mut d = match last.succ_opt() {
            Some(n) => n,
            None => return dates,
        };
        while d < local_today {
            dates.push(d);
            d = match d.succ_opt() {
                Some(n) => n,
                None => break,
            };
        }
    }
    let already_ran_today = last_auto_run == Some(local_today);
    if !already_ran_today && local_hour >= trigger_hour {
        dates.push(local_today);
    }
    if dates.len() > cap {
        dates = dates.split_off(dates.len() - cap);
    }
    dates
}

#[cfg(test)]
mod eod_target_tests {
    use super::compute_target_dates;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn no_watermark_before_trigger_is_empty() {
        assert!(compute_target_dates(None, d(2026, 6, 8), 9, 18, 7).is_empty());
    }

    #[test]
    fn no_watermark_after_trigger_is_today_only() {
        assert_eq!(compute_target_dates(None, d(2026, 6, 8), 18, 18, 7), vec![d(2026, 6, 8)]);
    }

    #[test]
    fn caught_up_after_trigger_is_today() {
        assert_eq!(
            compute_target_dates(Some(d(2026, 6, 7)), d(2026, 6, 8), 20, 18, 7),
            vec![d(2026, 6, 8)]
        );
    }

    #[test]
    fn missed_days_are_caught_up_plus_today() {
        assert_eq!(
            compute_target_dates(Some(d(2026, 6, 5)), d(2026, 6, 8), 20, 18, 7),
            vec![d(2026, 6, 6), d(2026, 6, 7), d(2026, 6, 8)]
        );
    }

    #[test]
    fn missed_days_caught_up_even_before_trigger_but_not_today() {
        assert_eq!(
            compute_target_dates(Some(d(2026, 6, 5)), d(2026, 6, 8), 9, 18, 7),
            vec![d(2026, 6, 6), d(2026, 6, 7)]
        );
    }

    #[test]
    fn already_ran_today_is_empty() {
        assert!(compute_target_dates(Some(d(2026, 6, 8)), d(2026, 6, 8), 20, 18, 7).is_empty());
    }

    #[test]
    fn catch_up_is_capped_to_most_recent() {
        let out = compute_target_dates(Some(d(2026, 5, 1)), d(2026, 6, 8), 20, 18, 7);
        assert_eq!(out.len(), 7);
        assert_eq!(*out.last().unwrap(), d(2026, 6, 8));
        assert_eq!(*out.first().unwrap(), d(2026, 6, 2)); // last 7: Jun 2..Jun 8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use domain::types::{
        GryzzlyCatalogEntry, MeetingId, ProjectId, Source, Task, TaskId, TaskStatus,
        WorklogEntryId,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;
    use crate::repositories::task_repository::TaskFilter;
    use crate::repositories::WorklogFilter;
    use crate::services::git_connector::GitCommit;
    use domain::types::recurrence::RecurrenceTemplateId;

    // ── Mock ConfigRepository ─────────────────────────────────────────────────

    #[derive(Default)]
    struct MemConfig {
        map: Mutex<HashMap<String, String>>,
    }

    impl MemConfig {
        fn with(pairs: &[(&str, &str)]) -> Self {
            let mut m = HashMap::new();
            for (k, v) in pairs {
                m.insert(k.to_string(), v.to_string());
            }
            Self { map: Mutex::new(m) }
        }
    }

    #[async_trait]
    impl ConfigRepository for MemConfig {
        async fn get(&self, _u: UserId, key: &str) -> Result<Option<String>, RepositoryError> {
            Ok(self.map.lock().unwrap().get(key).cloned())
        }
        async fn get_all(&self, _u: UserId) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(vec![])
        }
        async fn set(&self, _u: UserId, key: &str, value: &str) -> Result<(), RepositoryError> {
            self.map.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
    }

    // ── Mock TimesheetDraftRepository ─────────────────────────────────────────

    #[derive(Default)]
    struct MemDraft {
        saved: Mutex<Vec<TimesheetDraft>>,
    }

    #[async_trait]
    impl TimesheetDraftRepository for MemDraft {
        async fn upsert(&self, d: &TimesheetDraft) -> Result<(), RepositoryError> {
            self.saved.lock().unwrap().push(d.clone());
            Ok(())
        }
        async fn find_by_user_and_date(
            &self,
            _u: UserId,
            _d: NaiveDate,
        ) -> Result<Option<TimesheetDraft>, RepositoryError> {
            Ok(None)
        }
        async fn set_status(
            &self,
            _u: UserId,
            _d: NaiveDate,
            _s: TimesheetStatus,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    // ── Mock WorklogRepository ────────────────────────────────────────────────

    struct MemWorklog {
        entries: Vec<WorklogEntry>,
    }

    #[async_trait]
    impl WorklogRepository for MemWorklog {
        async fn list(
            &self,
            _user_id: UserId,
            _filter: &WorklogFilter,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            Ok(self.entries.clone())
        }
        async fn create(&self, _e: &WorklogEntry) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn update(&self, _e: &WorklogEntry) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn delete(&self, _id: WorklogEntryId, _uid: UserId) -> Result<bool, RepositoryError> {
            unimplemented!()
        }
        async fn find_by_id(
            &self,
            _id: WorklogEntryId,
            _uid: UserId,
        ) -> Result<Option<WorklogEntry>, RepositoryError> {
            unimplemented!()
        }
        async fn find_by_recurrence(
            &self,
            _user_id: UserId,
            _template_id: RecurrenceTemplateId,
            _limit: u32,
            _offset: u32,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            unimplemented!()
        }
    }

    // ── Mock MeetingRepository ────────────────────────────────────────────────

    #[derive(Default)]
    struct MemMeeting;

    #[async_trait]
    impl MeetingRepository for MemMeeting {
        async fn find_by_user_and_date(
            &self,
            _u: UserId,
            _d: NaiveDate,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(vec![])
        }
        async fn find_by_id(&self, _id: MeetingId) -> Result<Option<Meeting>, RepositoryError> {
            unimplemented!()
        }
        async fn update(&self, _m: &Meeting) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn find_by_user_and_range(
            &self,
            _u: UserId,
            _s: NaiveDate,
            _e: NaiveDate,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            unimplemented!()
        }
        async fn upsert_batch(&self, _ms: &[Meeting]) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn delete_stale(
            &self,
            _u: UserId,
            _ids: &[String],
        ) -> Result<u64, RepositoryError> {
            unimplemented!()
        }
        async fn find_by_project(
            &self,
            _u: UserId,
            _pid: ProjectId,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            unimplemented!()
        }
    }

    // ── Mock TaskRepository ───────────────────────────────────────────────────

    struct MemTask {
        task: Task,
    }

    #[async_trait]
    impl TaskRepository for MemTask {
        async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
            if self.task.id == id {
                Ok(Some(self.task.clone()))
            } else {
                Ok(None)
            }
        }
        async fn save(&self, _t: &Task) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn find_by_user(
            &self,
            _u: UserId,
            _f: &TaskFilter,
        ) -> Result<Vec<Task>, RepositoryError> {
            unimplemented!()
        }
        async fn find_by_source(
            &self,
            _u: UserId,
            _s: Source,
            _sid: &str,
        ) -> Result<Option<Task>, RepositoryError> {
            Ok(None)
        }
        async fn find_by_date_range(
            &self,
            _u: UserId,
            _s: chrono::NaiveDate,
            _e: chrono::NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            unimplemented!()
        }
        async fn find_planned_before(
            &self,
            _u: UserId,
            _b: chrono::NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            unimplemented!()
        }
        async fn save_batch(&self, _ts: &[Task]) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn delete(&self, _id: TaskId) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn delete_stale_by_source(
            &self,
            _u: UserId,
            _s: Source,
            _keep: &[String],
        ) -> Result<u64, RepositoryError> {
            unimplemented!()
        }
    }

    // ── Mock GryzzlyCatalogRepository ─────────────────────────────────────────

    struct MemCatalog {
        entries: Vec<GryzzlyCatalogEntry>,
    }

    #[async_trait]
    impl GryzzlyCatalogRepository for MemCatalog {
        async fn list_active(
            &self,
            _u: UserId,
            _search: Option<&str>,
            _project_filter: Option<&str>,
            _limit: i64,
        ) -> Result<Vec<GryzzlyCatalogEntry>, RepositoryError> {
            Ok(self.entries.clone())
        }
        async fn upsert(&self, _e: &GryzzlyCatalogEntry) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn soft_prune_missing(
            &self,
            _u: UserId,
            _keep: &[String],
        ) -> Result<u64, RepositoryError> {
            unimplemented!()
        }
        async fn find_by_gryzzly_task_id(
            &self,
            _u: UserId,
            _gid: &str,
        ) -> Result<Option<GryzzlyCatalogEntry>, RepositoryError> {
            unimplemented!()
        }
    }

    // ── Mock SignalMappingRepository ──────────────────────────────────────────

    #[derive(Default)]
    struct MemMapping;

    #[async_trait]
    impl SignalMappingRepository for MemMapping {
        async fn list_enabled(
            &self,
            _u: UserId,
        ) -> Result<Vec<SignalMapping>, RepositoryError> {
            Ok(vec![])
        }
        async fn upsert(&self, _m: &SignalMapping) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn set_enabled(
            &self,
            _id: SignalMappingId,
            _enabled: bool,
        ) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn delete(&self, _id: SignalMappingId) -> Result<(), RepositoryError> {
            unimplemented!()
        }
    }

    // ── Mock GitConnector ─────────────────────────────────────────────────────

    #[derive(Default)]
    struct MemGit;

    #[async_trait]
    impl GitConnector for MemGit {
        async fn commits_between(
            &self,
            _repos: &[String],
            _from: chrono::DateTime<Utc>,
            _to: chrono::DateTime<Utc>,
        ) -> Result<Vec<GitCommit>, AppError> {
            Ok(vec![])
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn make_task_with_project(user_id: UserId, task_id: TaskId, project_id: &str) -> Task {
        Task {
            id: task_id,
            user_id,
            title: "Test task".to_string(),
            description: None,
            notes: None,
            status: TaskStatus::Todo,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            project_id: None,
            assignee: None,
            delegated_to: None,
            urgency: domain::types::common::UrgencyLevel::Medium,
            urgency_manual: false,
            impact: domain::types::common::ImpactLevel::Medium,
            tags: vec![],
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            estimated_hours_override: None,
            remaining_hours_override: None,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            tracking_state: domain::types::TrackingState::Inbox,
            recurrence_id: None,
            occurrence_date: None,
            gryzzly_task_id: Some("gt1".to_string()),
            gryzzly_project_id: Some(project_id.to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_catalog_entry(user_id: UserId, project_id: &str) -> GryzzlyCatalogEntry {
        GryzzlyCatalogEntry {
            id: Uuid::new_v4(),
            user_id,
            gryzzly_task_id: "gt1".to_string(),
            name: "Task 1".to_string(),
            gryzzly_project_id: project_id.to_string(),
            project_name: "Project One".to_string(),
            customer_name: None,
            is_active: true,
            last_synced_at: Utc::now(),
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn reconstruct_high_signal_day_persists_draft_summing_to_target() {
        let user_id = make_user_id();
        // Use a fixed date: 2026-06-08 (Monday), workday in Europe/Paris
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();

        // The worklog entry is at 09:00 UTC (within the 08:00-12:00 UTC morning window)
        let logged_at = chrono::DateTime::parse_from_rfc3339("2026-06-08T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);

        let task_id: TaskId = Uuid::new_v4();

        let worklog_entry = WorklogEntry {
            id: Uuid::new_v4(),
            user_id,
            task_id,
            body: "Implemented feature AP-42".to_string(),
            logged_at,
            created_at: logged_at,
            updated_at: logged_at,
        };

        let worklog_repo = MemWorklog { entries: vec![worklog_entry] };
        let meeting_repo = MemMeeting;
        let task_repo = MemTask { task: make_task_with_project(user_id, task_id, "p1") };
        let catalog_repo = MemCatalog {
            entries: vec![make_catalog_entry(user_id, "p1")],
        };
        let mapping_repo = MemMapping;
        // Config: UTC timezone so day bounds are straightforward; defaults for workday
        let config_repo = MemConfig::with(&[("aplan.timezone", "UTC")]);
        let git = MemGit;
        let draft_repo = MemDraft::default();

        let day = reconstruct_timesheet(
            &worklog_repo,
            &meeting_repo,
            &task_repo,
            &catalog_repo,
            &mapping_repo,
            &config_repo,
            &git,
            &draft_repo,
            user_id,
            date,
        )
        .await
        .expect("reconstruct_timesheet should succeed");

        // The reconstruction engine fills the full workday target.
        assert!(
            (day.total_hours - 7.5).abs() < 1e-9,
            "expected total_hours=7.5, got {}",
            day.total_hours
        );

        // The draft must have been persisted exactly once.
        let saved = draft_repo.saved.lock().unwrap();
        assert_eq!(saved.len(), 1, "expected exactly one upsert to draft repo");
        assert_eq!(saved[0].date, date);
        assert_eq!(saved[0].user_id, user_id);
    }

    #[tokio::test]
    async fn save_draft_rejects_over_pinned() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let draft_repo = MemDraft::default();
        let config_repo = MemConfig::default(); // daily_target_hours defaults to 7.5

        // Pin 8 hours — exceeds 7.5 target
        let edited = vec![
            EditedLine { gryzzly_project_id: Some("p1".into()), hours: 8.0, is_pinned: true },
        ];

        let result = save_timesheet_draft(&draft_repo, &config_repo, user_id, date, edited).await;
        assert!(result.is_err(), "should reject over-pinned hours");
        match result {
            Err(AppError::Validation(_)) => {}
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn validate_timesheet_calls_set_status() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let draft_repo = MemDraft::default();

        // validate_timesheet just calls set_status — no error expected on our no-op mock
        let result = validate_timesheet(&draft_repo, user_id, date).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn mark_day_off_persists_day_off_draft() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let draft_repo = MemDraft::default();

        mark_day_off(&draft_repo, user_id, date, DayOffScope::Full)
            .await
            .expect("mark_day_off should succeed");

        let saved = draft_repo.saved.lock().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].status, TimesheetStatus::DayOff);
        assert_eq!(saved[0].total_hours, 0.0);
        assert_eq!(saved[0].target_hours, 0.0);
    }

    #[tokio::test]
    async fn learn_mapping_rejects_unknown_project() {
        let user_id = make_user_id();
        let mapping_repo = MemMapping;
        // Catalog has project "p1" but we request "p-unknown"
        let catalog_repo = MemCatalog {
            entries: vec![make_catalog_entry(user_id, "p1")],
        };

        let result = learn_mapping(
            &mapping_repo,
            &catalog_repo,
            user_id,
            MappingKind::RepoPath,
            "/some/repo".to_string(),
            None,
            "p-unknown".to_string(),
            Utc::now(),
        )
        .await;

        assert!(result.is_err());
        match result {
            Err(AppError::Validation(_)) => {}
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reconstruct_never_clobbers_validated_draft() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();

        // Draft repo that returns an already-validated draft
        struct ValidatedDraft;
        #[async_trait]
        impl TimesheetDraftRepository for ValidatedDraft {
            async fn upsert(&self, _d: &TimesheetDraft) -> Result<(), RepositoryError> {
                panic!("upsert must NOT be called when draft is already Validated")
            }
            async fn find_by_user_and_date(
                &self,
                _u: UserId,
                _d: NaiveDate,
            ) -> Result<Option<TimesheetDraft>, RepositoryError> {
                Ok(Some(TimesheetDraft {
                    id: Uuid::new_v4(),
                    user_id: _u,
                    date: _d,
                    status: TimesheetStatus::Validated,
                    target_hours: 7.5,
                    total_hours: 7.5,
                    day_confidence: Confidence::High,
                    blocks_json: None,
                    lines: vec![],
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                }))
            }
            async fn set_status(
                &self,
                _u: UserId,
                _d: NaiveDate,
                _s: TimesheetStatus,
            ) -> Result<(), RepositoryError> {
                Ok(())
            }
        }

        let task_id: TaskId = Uuid::new_v4();
        let logged_at = chrono::DateTime::parse_from_rfc3339("2026-06-08T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let worklog_entry = WorklogEntry {
            id: Uuid::new_v4(),
            user_id,
            task_id,
            body: "work".to_string(),
            logged_at,
            created_at: logged_at,
            updated_at: logged_at,
        };

        let result = reconstruct_timesheet(
            &MemWorklog { entries: vec![worklog_entry] },
            &MemMeeting,
            &MemTask { task: make_task_with_project(user_id, task_id, "p1") },
            &MemCatalog { entries: vec![make_catalog_entry(user_id, "p1")] },
            &MemMapping,
            &MemConfig::with(&[("aplan.timezone", "UTC")]),
            &MemGit,
            &ValidatedDraft,
            user_id,
            date,
        )
        .await;

        // Should succeed (returns the day) without calling upsert
        assert!(result.is_ok(), "reconstruct should succeed even when draft is validated");
    }

    #[tokio::test]
    async fn reconstruct_does_not_clobber_day_off_draft() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();

        // Draft repo that returns an already-DayOff draft
        struct DayOffDraft;
        #[async_trait]
        impl TimesheetDraftRepository for DayOffDraft {
            async fn upsert(&self, _d: &TimesheetDraft) -> Result<(), RepositoryError> {
                panic!("upsert must NOT be called when draft is already DayOff")
            }
            async fn find_by_user_and_date(
                &self,
                _u: UserId,
                _d: NaiveDate,
            ) -> Result<Option<TimesheetDraft>, RepositoryError> {
                Ok(Some(TimesheetDraft {
                    id: Uuid::new_v4(),
                    user_id: _u,
                    date: _d,
                    status: TimesheetStatus::DayOff,
                    target_hours: 0.0,
                    total_hours: 0.0,
                    day_confidence: Confidence::High,
                    blocks_json: None,
                    lines: vec![],
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                }))
            }
            async fn set_status(
                &self,
                _u: UserId,
                _d: NaiveDate,
                _s: TimesheetStatus,
            ) -> Result<(), RepositoryError> {
                Ok(())
            }
        }

        // Signals ARE present, so reconstruction would produce a non-empty day if it
        // overwrote the persisted draft.
        let task_id: TaskId = Uuid::new_v4();
        let logged_at = chrono::DateTime::parse_from_rfc3339("2026-06-08T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let worklog_entry = WorklogEntry {
            id: Uuid::new_v4(),
            user_id,
            task_id,
            body: "work".to_string(),
            logged_at,
            created_at: logged_at,
            updated_at: logged_at,
        };

        let result = reconstruct_timesheet(
            &MemWorklog { entries: vec![worklog_entry] },
            &MemMeeting,
            &MemTask { task: make_task_with_project(user_id, task_id, "p1") },
            &MemCatalog { entries: vec![make_catalog_entry(user_id, "p1")] },
            &MemMapping,
            &MemConfig::with(&[("aplan.timezone", "UTC")]),
            &MemGit,
            &DayOffDraft,
            user_id,
            date,
        )
        .await;

        // Should succeed (returns the day) without calling upsert, i.e. the persisted
        // DayOff draft is left untouched.
        assert!(result.is_ok(), "reconstruct should succeed even when draft is day-off");
    }
}
