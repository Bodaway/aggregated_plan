use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Timelike, Utc};
use domain::rules::project_mapping::{resolve_signal_project, ProjectResolution, RawSignal};
use domain::rules::reconstruction::{
    reconstruct_day, renormalize_lines, DayInputs, EditedLine, MeetingBlock, MeetingKind,
    ReconstructedDay, ReconstructionConfig, Signal, SignalKind,
};
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{
    AlertRepository, ConfigRepository, GryzzlyCatalogRepository, MeetingRepository,
    SignalMappingRepository, TaskRepository, TimesheetDraftRepository, WorklogFilter,
    WorklogRepository, WORKLOG_FILTER_MAX_LIMIT,
};
use crate::services::git_connector::{jira_key_in, GitConnector};
use crate::time::{local_window, resolve_tz, to_local};

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
    let (from_utc, to_utc) = local_window(tz, date, date);
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

const EOD_CATCHUP_CAP: usize = 7;
const DEFAULT_AUTO_RECONSTRUCT_HOUR: u32 = 18;

/// A step of the end-of-day pass, named so a tolerated failure says what it cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EodStep {
    /// Rebuilding and persisting one day's draft. Failing loses that day's work,
    /// so the watermark must not step over it.
    Reconstruction,
    /// The passive TimesheetReady alert. Ancillary: failing costs the alert only.
    ReadyAlert,
    /// Advancing the watermark. Failing costs a redundant re-run, not the work.
    Watermark,
}

impl std::fmt::Display for EodStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            EodStep::Reconstruction => "reconstruction",
            EodStep::ReadyAlert => "ready-alert",
            EodStep::Watermark => "watermark",
        };
        f.write_str(s)
    }
}

/// A step that failed without costing the pass the work it had already persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EodStepFailure {
    pub date: NaiveDate,
    pub step: EodStep,
    pub error: String,
}

/// What one end-of-day pass achieved, and what it had to tolerate to achieve it.
///
/// The pass used to be all-or-nothing: a single failing ancillary write discarded a
/// whole reconstruction. It now keeps what succeeded and reports the rest — tolerated,
/// never swallowed. The caller is expected to log every entry in `degraded` and to
/// treat a degraded pass as a failed attempt for back-off purposes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EodPassOutcome {
    /// Local dates reconstructed and persisted on this pass, ascending.
    pub processed: Vec<NaiveDate>,
    /// Steps that failed without aborting the pass.
    pub degraded: Vec<EodStepFailure>,
}

impl EodPassOutcome {
    /// A stable one-line rendering of everything that went wrong, so a caller can tell
    /// "the same failure again" from "a new failure" without printing either twice.
    pub fn degradation_signature(&self) -> String {
        self.degraded
            .iter()
            .map(|f| format!("{}@{}: {}", f.step, f.date, f.error))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// One end-of-day pass for `user_id` as of `now_utc`. Reconstructs each due local day
/// (persisting a draft; never clobbering validated/submitted), raises/settles a
/// TimesheetReady alert, and advances the `aplan.timesheet.last_auto_run` watermark.
/// NEVER submits to Gryzzly.
///
/// Only the configuration reads that decide *what is due* are fatal: past that point a
/// failing day or a failing alert degrades the outcome instead of discarding the pass.
#[allow(clippy::too_many_arguments)]
pub async fn run_eod_pass(
    worklog_repo: &dyn WorklogRepository,
    meeting_repo: &dyn MeetingRepository,
    task_repo: &dyn TaskRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    mapping_repo: &dyn SignalMappingRepository,
    config_repo: &dyn ConfigRepository,
    git: &dyn GitConnector,
    draft_repo: &dyn TimesheetDraftRepository,
    alert_repo: &dyn AlertRepository,
    user_id: UserId,
    now_utc: DateTime<Utc>,
) -> Result<EodPassOutcome, AppError> {
    let tz = resolve_tz(config_repo.get(user_id, "aplan.timezone").await?);
    let local_now = to_local(now_utc, tz);
    let local_today = local_now.date();
    let local_hour = local_now.time().hour();
    let trigger_hour = config_repo
        .get(user_id, "workday.auto_reconstruct_hour")
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_AUTO_RECONSTRUCT_HOUR);
    let last_auto_run = config_repo
        .get(user_id, "aplan.timesheet.last_auto_run")
        .await?
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());

    let targets = compute_target_dates(last_auto_run, local_today, local_hour, trigger_hour, EOD_CATCHUP_CAP);

    let mut outcome = EodPassOutcome::default();
    // The watermark may only advance over an unbroken prefix of reconstructed days:
    // stepping past a day that failed would drop it silently and forever.
    let mut watermark: Option<NaiveDate> = None;
    let mut prefix_intact = true;

    for date in &targets {
        if let Err(e) = reconstruct_timesheet(
            worklog_repo, meeting_repo, task_repo, catalog_repo, mapping_repo, config_repo,
            git, draft_repo, user_id, *date,
        )
        .await
        {
            // Days are independent: one bad day must not cost the others.
            outcome.degraded.push(EodStepFailure {
                date: *date,
                step: EodStep::Reconstruction,
                error: e.to_string(),
            });
            prefix_intact = false;
            continue;
        }
        outcome.processed.push(*date);
        if prefix_intact {
            watermark = Some(*date);
        }
        // Ancillary: a rejected alert write must not discard the draft just persisted.
        if let Err(e) =
            upsert_timesheet_ready_alert(alert_repo, draft_repo, user_id, *date, now_utc).await
        {
            outcome.degraded.push(EodStepFailure {
                date: *date,
                step: EodStep::ReadyAlert,
                error: e.to_string(),
            });
        }
    }

    if let Some(max) = watermark {
        if let Err(e) = config_repo
            .set(user_id, "aplan.timesheet.last_auto_run", &max.format("%Y-%m-%d").to_string())
            .await
        {
            outcome.degraded.push(EodStepFailure {
                date: max,
                step: EodStep::Watermark,
                error: e.to_string(),
            });
        }
    }

    Ok(outcome)
}

/// Raise a single passive TimesheetReady alert for a day with a non-empty draft (deduped),
/// or resolve any stale one if the day is now validated/submitted or empty.
async fn upsert_timesheet_ready_alert(
    alert_repo: &dyn AlertRepository,
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
    now_utc: DateTime<Utc>,
) -> Result<(), AppError> {
    let draft = draft_repo.find_by_user_and_date(user_id, date).await?;
    let mut existing: Vec<Alert> = alert_repo
        .find_by_user(user_id, Some(false))
        .await?
        .into_iter()
        .filter(|a| a.alert_type == AlertType::TimesheetReady && a.date == date)
        .collect();

    let should_alert = matches!(
        &draft,
        Some(d) if d.total_hours > 0.0
            && !matches!(d.status, TimesheetStatus::Validated | TimesheetStatus::Submitted)
    );

    if should_alert {
        if existing.is_empty() {
            let d = draft.expect("checked Some above");
            let project_count = d.lines.iter().filter(|l| l.gryzzly_project_id.is_some()).count();
            let alert = Alert {
                id: Uuid::new_v4(),
                user_id,
                alert_type: AlertType::TimesheetReady,
                severity: AlertSeverity::Information,
                message: format!(
                    "Timesheet draft ready for {date} ({:.1}h across {project_count} project(s)) — review and copy into Gryzzly",
                    d.total_hours
                ),
                related_items: vec![],
                date,
                resolved: false,
                created_at: now_utc,
            };
            alert_repo.save(&alert).await?;
        }
    } else {
        // Day is validated/submitted/empty → settle any stale ready-alert.
        for a in existing.iter_mut() {
            a.resolved = true;
            alert_repo.update(a).await?;
        }
    }
    Ok(())
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
            let mut g = self.saved.lock().unwrap();
            g.retain(|existing| !(existing.user_id == d.user_id && existing.date == d.date));
            g.push(d.clone());
            Ok(())
        }
        async fn find_by_user_and_date(
            &self,
            u: UserId,
            d: NaiveDate,
        ) -> Result<Option<TimesheetDraft>, RepositoryError> {
            Ok(self
                .saved
                .lock()
                .unwrap()
                .iter()
                .find(|existing| existing.user_id == u && existing.date == d)
                .cloned())
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

    // ── Mock AlertRepository ──────────────────────────────────────────────────

    #[derive(Default)]
    struct MemAlert {
        saved: Mutex<Vec<Alert>>,
    }
    #[async_trait]
    impl AlertRepository for MemAlert {
        async fn find_by_id(&self, _id: domain::types::AlertId) -> Result<Option<Alert>, RepositoryError> {
            Ok(None)
        }
        async fn find_unresolved(&self, _u: UserId) -> Result<Vec<Alert>, RepositoryError> {
            Ok(self.saved.lock().unwrap().iter().filter(|a| !a.resolved).cloned().collect())
        }
        async fn find_by_user(&self, _u: UserId, resolved: Option<bool>) -> Result<Vec<Alert>, RepositoryError> {
            let all = self.saved.lock().unwrap().clone();
            Ok(match resolved {
                Some(r) => all.into_iter().filter(|a| a.resolved == r).collect(),
                None => all,
            })
        }
        async fn save(&self, a: &Alert) -> Result<(), RepositoryError> {
            self.saved.lock().unwrap().push(a.clone());
            Ok(())
        }
        async fn save_batch(&self, alerts: &[Alert]) -> Result<(), RepositoryError> {
            self.saved.lock().unwrap().extend_from_slice(alerts);
            Ok(())
        }
        async fn update(&self, a: &Alert) -> Result<(), RepositoryError> {
            let mut g = self.saved.lock().unwrap();
            if let Some(slot) = g.iter_mut().find(|x| x.id == a.id) {
                *slot = a.clone();
            }
            Ok(())
        }
        async fn delete_resolved(&self, _u: UserId) -> Result<u64, RepositoryError> {
            Ok(0)
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn utc(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        chrono::TimeZone::with_ymd_and_hms(&Utc, y, m, d, h, 0, 0).unwrap()
    }

    fn date_of(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// An alert repo that fails every read and write, the way the `alerts.alert_type`
    /// CHECK constraint did for weeks.
    struct BrokenAlert;

    impl BrokenAlert {
        fn boom<T>() -> Result<T, RepositoryError> {
            Err(RepositoryError::Database("(code: 275) CHECK constraint failed: alerts".into()))
        }
    }

    #[async_trait]
    impl AlertRepository for BrokenAlert {
        async fn find_by_id(&self, _id: domain::types::AlertId) -> Result<Option<Alert>, RepositoryError> {
            Self::boom()
        }
        async fn find_unresolved(&self, _u: UserId) -> Result<Vec<Alert>, RepositoryError> {
            Self::boom()
        }
        async fn find_by_user(&self, _u: UserId, _r: Option<bool>) -> Result<Vec<Alert>, RepositoryError> {
            Self::boom()
        }
        async fn save(&self, _a: &Alert) -> Result<(), RepositoryError> {
            Self::boom()
        }
        async fn save_batch(&self, _alerts: &[Alert]) -> Result<(), RepositoryError> {
            Self::boom()
        }
        async fn update(&self, _a: &Alert) -> Result<(), RepositoryError> {
            Self::boom()
        }
        async fn delete_resolved(&self, _u: UserId) -> Result<u64, RepositoryError> {
            Self::boom()
        }
    }

    /// A draft repo that refuses to persist exactly one date, to prove a mid-catch-up
    /// failure neither aborts the pass nor lets the watermark step over the lost day.
    struct FlakyDraft {
        inner: MemDraft,
        fail_on: NaiveDate,
    }

    #[async_trait]
    impl TimesheetDraftRepository for FlakyDraft {
        async fn upsert(&self, d: &TimesheetDraft) -> Result<(), RepositoryError> {
            if d.date == self.fail_on {
                return Err(RepositoryError::Database("draft upsert refused".into()));
            }
            self.inner.upsert(d).await
        }
        async fn find_by_user_and_date(
            &self,
            u: UserId,
            d: NaiveDate,
        ) -> Result<Option<TimesheetDraft>, RepositoryError> {
            self.inner.find_by_user_and_date(u, d).await
        }
        async fn set_status(
            &self,
            u: UserId,
            d: NaiveDate,
            s: TimesheetStatus,
        ) -> Result<(), RepositoryError> {
            self.inner.set_status(u, d, s).await
        }
    }

    /// A config repo that reads defaults but cannot write, so the watermark update fails
    /// while everything upstream of it succeeds.
    struct ReadOnlyConfig;

    #[async_trait]
    impl ConfigRepository for ReadOnlyConfig {
        async fn get(&self, _u: UserId, _key: &str) -> Result<Option<String>, RepositoryError> {
            Ok(None)
        }
        async fn get_all(&self, _u: UserId) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(vec![])
        }
        async fn set(&self, _u: UserId, _key: &str, _value: &str) -> Result<(), RepositoryError> {
            Err(RepositoryError::Database("configuration is read-only".into()))
        }
    }

    /// Empty-seeded Plan-1 mocks for the EOD job tests, in `run_eod_pass` arg order.
    #[allow(clippy::type_complexity)]
    fn eod_mocks() -> (MemWorklog, MemMeeting, MemTask, MemCatalog, MemMapping, MemConfig, MemGit, MemDraft) {
        (
            MemWorklog { entries: vec![] },
            MemMeeting,
            MemTask { task: make_task_with_project(make_user_id(), Uuid::new_v4(), "unused") },
            MemCatalog { entries: vec![] },
            MemMapping,
            MemConfig::default(),
            MemGit,
            MemDraft::default(),
        )
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
            project_status: None,
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
            session_id: None,
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
            session_id: None,
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
            session_id: None,
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

    // ── run_eod_pass ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn eod_before_trigger_processes_nothing() {
        // 09:00 UTC = 11:00 Paris, before the default 18:00 trigger, no watermark.
        let (worklog, meeting, task, catalog, mapping, config, git, draft) = eod_mocks();
        let alert = MemAlert::default();
        let outcome = run_eod_pass(
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert,
            make_user_id(), utc(2026, 6, 8, 9),
        )
        .await
        .unwrap();
        assert!(outcome.processed.is_empty());
        assert!(outcome.degraded.is_empty());
        assert!(alert.saved.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn eod_after_trigger_processes_today_and_advances_watermark() {
        // 20:00 UTC = 22:00 Paris, after trigger. Empty signals → draft total 0 → NO alert, but watermark advances.
        let (worklog, meeting, task, catalog, mapping, config, git, draft) = eod_mocks();
        let alert = MemAlert::default();
        let outcome = run_eod_pass(
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert,
            make_user_id(), utc(2026, 6, 8, 20),
        )
        .await
        .unwrap();
        assert_eq!(outcome.processed.len(), 1);
        assert!(outcome.degraded.is_empty(), "a clean pass reports no degradation");
        // watermark set to the local date (2026-06-08 Paris)
        assert_eq!(
            config.get(make_user_id(), "aplan.timesheet.last_auto_run").await.unwrap().as_deref(),
            Some("2026-06-08")
        );
        // empty day → no alert (total 0)
        assert!(alert.saved.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn eod_keeps_the_reconstruction_when_the_alert_step_fails() {
        // The real incident: `alerts.alert_type` rejected `timesheet_ready`, so an
        // ancillary write killed the whole pass — every 60s, for weeks.
        let (worklog, meeting, task, catalog, mapping, config, git, draft) = eod_mocks();
        let alert = BrokenAlert;
        let outcome = run_eod_pass(
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert,
            make_user_id(), utc(2026, 6, 8, 20),
        )
        .await
        .expect("an alert-table failure must not fail the whole pass");

        assert_eq!(outcome.processed, vec![date_of(2026, 6, 8)], "the day was reconstructed");
        assert!(
            draft.find_by_user_and_date(make_user_id(), date_of(2026, 6, 8)).await.unwrap().is_some(),
            "the persisted draft must survive the broken alert step"
        );
        assert_eq!(
            config.get(make_user_id(), "aplan.timesheet.last_auto_run").await.unwrap().as_deref(),
            Some("2026-06-08"),
            "the watermark must advance: the day's work is done and must not be redone forever"
        );
        // Tolerated, never swallowed.
        assert_eq!(outcome.degraded.len(), 1);
        assert_eq!(outcome.degraded[0].step, EodStep::ReadyAlert);
        assert_eq!(outcome.degraded[0].date, date_of(2026, 6, 8));
        assert!(outcome.degraded[0].error.contains("CHECK constraint"));
        assert!(!outcome.degradation_signature().is_empty());
    }

    #[tokio::test]
    async fn eod_watermark_never_steps_over_a_day_that_failed() {
        // Watermark 3 days back → catch up 06-06, 06-07, 06-08. Persisting 06-07 fails.
        let (worklog, meeting, task, catalog, mapping, _config, git, _draft) = eod_mocks();
        let config = MemConfig::with(&[("aplan.timesheet.last_auto_run", "2026-06-05")]);
        let draft = FlakyDraft { inner: MemDraft::default(), fail_on: date_of(2026, 6, 7) };
        let alert = MemAlert::default();

        let outcome = run_eod_pass(
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert,
            make_user_id(), utc(2026, 6, 8, 20),
        )
        .await
        .expect("one bad day must not abort the catch-up");

        assert_eq!(
            outcome.processed,
            vec![date_of(2026, 6, 6), date_of(2026, 6, 8)],
            "the days that could be reconstructed were, the broken one was skipped"
        );
        assert_eq!(
            config.get(make_user_id(), "aplan.timesheet.last_auto_run").await.unwrap().as_deref(),
            Some("2026-06-06"),
            "the watermark stops before the failed day so it is retried, not lost"
        );
        assert_eq!(outcome.degraded.len(), 1);
        assert_eq!(outcome.degraded[0].step, EodStep::Reconstruction);
        assert_eq!(outcome.degraded[0].date, date_of(2026, 6, 7));
    }

    #[tokio::test]
    async fn eod_reports_a_watermark_write_failure_without_pretending_to_succeed() {
        let (worklog, meeting, task, catalog, mapping, _config, git, draft) = eod_mocks();
        let config = ReadOnlyConfig;
        let alert = MemAlert::default();
        let outcome = run_eod_pass(
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert,
            make_user_id(), utc(2026, 6, 8, 20),
        )
        .await
        .expect("the reconstruction still happened");
        assert_eq!(outcome.processed, vec![date_of(2026, 6, 8)]);
        assert_eq!(outcome.degraded.len(), 1);
        assert_eq!(outcome.degraded[0].step, EodStep::Watermark);
    }

    // ── upsert_timesheet_ready_alert ────────────────────────────────────────

    #[tokio::test]
    async fn upsert_timesheet_ready_alert_creates_alert_once() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let draft_repo = MemDraft::default();
        let alert_repo = MemAlert::default();
        let now = Utc::now();

        let draft = TimesheetDraft {
            id: Uuid::new_v4(),
            user_id,
            date,
            status: TimesheetStatus::Draft,
            target_hours: 7.5,
            total_hours: 7.5,
            day_confidence: Confidence::High,
            blocks_json: None,
            lines: vec![TimesheetDraftLine {
                id: Uuid::new_v4(),
                gryzzly_project_id: Some("p1".to_string()),
                project_name: None,
                hours: 7.5,
                is_pinned: false,
                confidence: Confidence::High,
                source_refs: vec![],
            }],
            created_at: now,
            updated_at: now,
        };
        draft_repo.upsert(&draft).await.unwrap();

        upsert_timesheet_ready_alert(&alert_repo, &draft_repo, user_id, date, now)
            .await
            .unwrap();
        assert_eq!(alert_repo.saved.lock().unwrap().len(), 1, "expected exactly one alert");

        // Second call is deduped: no duplicate alert for the same unresolved day.
        upsert_timesheet_ready_alert(&alert_repo, &draft_repo, user_id, date, now)
            .await
            .unwrap();
        assert_eq!(alert_repo.saved.lock().unwrap().len(), 1, "must not duplicate the alert");
    }

    #[tokio::test]
    async fn upsert_timesheet_ready_alert_resolves_stale_alert_for_validated_day() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let draft_repo = MemDraft::default();
        let alert_repo = MemAlert::default();
        let now = Utc::now();

        let draft = TimesheetDraft {
            id: Uuid::new_v4(),
            user_id,
            date,
            status: TimesheetStatus::Validated,
            target_hours: 7.5,
            total_hours: 7.5,
            day_confidence: Confidence::High,
            blocks_json: None,
            lines: vec![],
            created_at: now,
            updated_at: now,
        };
        draft_repo.upsert(&draft).await.unwrap();

        let stale = Alert {
            id: Uuid::new_v4(),
            user_id,
            alert_type: AlertType::TimesheetReady,
            severity: AlertSeverity::Information,
            message: "stale".to_string(),
            related_items: vec![],
            date,
            resolved: false,
            created_at: now,
        };
        alert_repo.save(&stale).await.unwrap();

        upsert_timesheet_ready_alert(&alert_repo, &draft_repo, user_id, date, now)
            .await
            .unwrap();

        let saved = alert_repo.saved.lock().unwrap();
        assert_eq!(saved.len(), 1, "no new alert should be created for a validated day");
        assert!(saved[0].resolved, "the stale alert must be resolved");
    }
}
