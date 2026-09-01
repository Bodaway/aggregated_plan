use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Timelike, Utc};
use domain::rules::project_mapping::{resolve_signal_project, ProjectResolution, RawSignal};
use domain::rules::presence::{
    build_lanes, EvidenceKind, EvidencePoint, EvidenceSpan, Lane, LaneKey,
};
use domain::rules::quarters::{allocate_day, quarters, DayPin};
use domain::rules::reconstruction::{
    windows_of, OutsideWork, ProjectAllocation, ReconstructedDay, ReconstructionConfig,
    UnresolvedSignal,
};
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{
    ActivitySlotRepository, AlertRepository, ConfigRepository, GryzzlyCatalogRepository, MeetingRepository,
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

/// Local minutes from midnight — the unit the presence lanes and quarters work in.
fn mins(dt: NaiveDateTime) -> i64 {
    dt.time().hour() as i64 * 60 + dt.time().minute() as i64
}

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

/// Rebuild the day from its evidence and persist it as a draft.
///
/// Gathering is unchanged from the carry-forward engine — worklog entries, git commits,
/// meetings, all resolved to a Gryzzly project the same way. What changed is everything
/// after: instead of slicing the day into one project at a time, evidence becomes
/// OVERLAPPING per-task lanes, and each quarter-day is shared out among the lanes present
/// in it. Concurrency survives to the screen, and the user arbitrates it.
#[allow(clippy::too_many_arguments)]
pub async fn reconstruct_timesheet(
    worklog_repo: &dyn WorklogRepository,
    meeting_repo: &dyn MeetingRepository,
    task_repo: &dyn TaskRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    mapping_repo: &dyn SignalMappingRepository,
    config_repo: &dyn ConfigRepository,
    activity_repo: &dyn ActivitySlotRepository,
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

    let mut points: Vec<EvidencePoint> = Vec::new();
    let mut spans: Vec<EvidenceSpan> = Vec::new();
    let mut unresolved: Vec<UnresolvedSignal> = Vec::new();
    let mut ooo: Vec<(i64, i64)> = Vec::new();

    // ---- Worklog entries ----
    let wl = worklog_repo
        .list(
            user_id,
            &WorklogFilter { task_ids: None, from: Some(from_utc), to: Some(to_utc), limit: WORKLOG_FILTER_MAX_LIMIT, offset: 0 },
        )
        .await?;
    for e in &wl {
        let task = task_repo.find_by_id(e.task_id).await?;
        let raw = RawSignal::Worklog {
            task_gryzzly_project_id: task.as_ref().and_then(|t| t.gryzzly_project_id.clone()),
        };
        let project = mapped_or_none(&raw, &rules, &live_project_ids);
        let at = to_local(e.logged_at, tz);
        if project.is_none() {
            unresolved.push(UnresolvedSignal {
                source_ref: format!("wl:{}", e.id),
                label: truncate(&e.body, 60),
                at,
            });
        }
        points.push(EvidencePoint {
            at,
            lane: LaneKey::Task(e.task_id),
            // The owning task is already loaded to resolve the project — its title is
            // what tells the reader WHAT the time was, so carry it, don't re-query it.
            label: task.as_ref().map(|t| t.title.clone()).unwrap_or_else(|| "tâche inconnue".into()),
            gryzzly_project_id: project,
            kind: EvidenceKind::Log,
        });
    }

    // ---- Git commits ----
    let repos = split_repos(config_repo.get(user_id, "git.repos").await?);
    if !repos.is_empty() {
        let commits = git.commits_between(&repos, from_utc, to_utc).await?;
        for c in &commits {
            // Prefer a Jira key match to a task; else fall back to repo/branch rules.
            let mut project = None;
            let mut lane = None;
            let mut label = None;
            if let Some(key) = jira_key_in(&c.message).or_else(|| jira_key_in(&c.branch)) {
                if let Some(t) = task_repo.find_by_source(user_id, Source::Jira, &key).await? {
                    project = t.gryzzly_project_id.clone().filter(|p| live_project_ids.contains(p));
                    lane = Some(LaneKey::Task(t.id));
                    label = Some(t.title.clone());
                }
            }
            if project.is_none() {
                let raw = RawSignal::Commit { repo_path: c.repo_path.clone(), branch: c.branch.clone() };
                project = mapped_or_none(&raw, &rules, &live_project_ids);
            }
            let at = to_local(c.committed_at, tz);
            let source_ref = format!("git:{}:{}", c.repo_path, c.committed_at.to_rfc3339());
            if project.is_none() {
                unresolved.push(UnresolvedSignal {
                    source_ref: source_ref.clone(),
                    label: truncate(&c.message, 60),
                    at,
                });
            }
            points.push(EvidencePoint {
                at,
                // A commit that matched no Jira key has no task to be arbitrated under,
                // but it is still evidence of work — it gets a lane of its own, per repo,
                // rather than being dropped the way the old engine dropped it.
                lane: lane.unwrap_or_else(|| LaneKey::Source(format!("repo:{}", c.repo_path))),
                label: label.unwrap_or_else(|| c.repo_path.clone()),
                gryzzly_project_id: project,
                kind: EvidenceKind::Commit,
            });
        }
    }

    // ---- Meetings: measured spans, and out-of-office ranges ----
    let meetings_raw = meeting_repo.find_by_user_and_date(user_id, date).await?;
    for m in &meetings_raw {
        let (start, end) = (to_local(m.start_time, tz), to_local(m.end_time, tz));
        if is_out_of_office(m) {
            ooo.push((mins(start), mins(end)));
            continue;
        }
        let raw = RawSignal::Meeting {
            subject: m.title.clone(),
            organizer: meeting_organizer(m),
            internal_project_id: m.project_id.map(|p| p.to_string()),
        };
        spans.push(EvidenceSpan {
            start,
            end,
            lane: LaneKey::Source(format!("mtg:{}", m.id)),
            label: m.title.clone(),
            gryzzly_project_id: mapped_or_none(&raw, &rules, &live_project_ids),
            kind: EvidenceKind::Meeting,
        });
    }

    // ---- Hand-run activity slots ----
    //
    // Only `manual` ones. A `worklog`-sourced slot is a projection of the very entries
    // already gathered above, so counting it too would weight that lane twice.
    let slots = activity_repo.find_by_user_and_date(user_id, date).await?;
    for s in slots.iter().filter(|s| !s.source.is_projection()) {
        let (Some(task_id), Some(end)) = (s.task_id, s.end_time) else { continue };
        let task = task_repo.find_by_id(task_id).await?;
        let raw = RawSignal::Worklog {
            task_gryzzly_project_id: task.as_ref().and_then(|t| t.gryzzly_project_id.clone()),
        };
        spans.push(EvidenceSpan {
            start: to_local(s.start_time, tz),
            end: to_local(end, tz),
            lane: LaneKey::Task(task_id),
            label: task.as_ref().map(|t| t.title.clone()).unwrap_or_else(|| "tâche inconnue".into()),
            gryzzly_project_id: mapped_or_none(&raw, &rules, &live_project_ids),
            kind: EvidenceKind::ManualSlot,
        });
    }

    let lanes = build_lanes(&points, &spans, &windows_of(&cfg));

    // A validated or submitted day is finished: read its pins, but never write over it.
    let existing = draft_repo.find_by_user_and_date(user_id, date).await?;
    let frozen = existing.as_ref().is_some_and(|d| {
        matches!(d.status, TimesheetStatus::Validated | TimesheetStatus::Submitted | TimesheetStatus::DayOff)
    });
    let pins = pins_of(existing.as_ref());

    let day = build_day(date, &lanes, &pins, &ooo, unresolved, &cfg);
    if frozen {
        return Ok(day);
    }
    persist_day(draft_repo, user_id, &day, &cfg, TimesheetStatus::Draft).await?;
    Ok(day)
}

/// The pins recorded on a persisted draft: the shares the user set by hand.
fn pins_of(draft: Option<&TimesheetDraft>) -> Vec<DayPin> {
    draft
        .map(|d| {
            d.shares
                .iter()
                .filter(|s| s.is_pinned)
                .filter_map(|s| {
                    LaneKey::parse(&s.lane_key).map(|lane| DayPin {
                        quarter_index: s.quarter_index,
                        lane,
                        hours: s.hours,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Allocate the quarters and roll the shares up into per-project lines.
fn build_day(
    date: NaiveDate,
    lanes: &[Lane],
    pins: &[DayPin],
    ooo: &[(i64, i64)],
    unresolved: Vec<UnresolvedSignal>,
    cfg: &ReconstructionConfig,
) -> ReconstructedDay {
    // A day with no evidence INSIDE the windows is not a day to declare. Without this
    // guard every untouched day — a weekend, a holiday with no calendar entry — would
    // report four unattributed quarters and eight hours nobody worked. Evidence that
    // fell outside the windows still travels in `outside_workday`, so an evening-only
    // day says "here is what I saw, outside your hours" instead of inventing a day.
    if !lanes.iter().any(|l| !l.intervals.is_empty()) && ooo.is_empty() {
        return ReconstructedDay {
            date,
            allocations: vec![],
            unattributed_hours: 0.0,
            unresolved,
            total_hours: 0.0,
            day_confidence: Confidence::Low,
            lanes: lanes.to_vec(),
            quarters: vec![],
            outside_workday: outside_work_of(lanes),
        };
    }

    let alloc = allocate_day(lanes, pins, ooo, cfg);

    // Roll shares up by project. Two tasks on the same Gryzzly project merge here — the
    // normal case, not an error: the lanes stay separate on screen so the merge is
    // visible before it reaches the declaration.
    let mut by_project: BTreeMap<Option<String>, (f64, Vec<String>, Confidence)> = BTreeMap::new();
    for q in &alloc.quarters {
        for s in &q.shares {
            let e = by_project
                .entry(s.gryzzly_project_id.clone())
                .or_insert((0.0, Vec::new(), Confidence::High));
            e.0 += s.hours;
            let key = s.lane.as_key();
            if !e.1.contains(&key) {
                e.1.push(key);
            }
            if confidence_rank(q.confidence) < confidence_rank(e.2) {
                e.2 = q.confidence;
            }
        }
    }

    let mut allocations = Vec::new();
    let mut unattributed_hours = 0.0;
    for (project, (hours, refs, confidence)) in by_project {
        match project {
            Some(gryzzly_project_id) => allocations.push(ProjectAllocation {
                gryzzly_project_id,
                hours,
                confidence,
                source_refs: refs,
            }),
            None => unattributed_hours += hours,
        }
    }

    ReconstructedDay {
        date,
        allocations,
        unattributed_hours,
        unresolved,
        total_hours: alloc.total_hours,
        day_confidence: alloc.day_confidence,
        lanes: lanes.to_vec(),
        quarters: alloc.quarters,
        outside_workday: outside_work_of(lanes),
    }
}

fn outside_work_of(lanes: &[Lane]) -> Vec<OutsideWork> {
    lanes
        .iter()
        .filter(|l| l.outside_minutes > 0)
        .map(|l| OutsideWork {
            lane: l.key.clone(),
            label: l.label.clone(),
            minutes: l.outside_minutes,
        })
        .collect()
}

fn confidence_rank(c: Confidence) -> u8 {
    match c {
        Confidence::Low => 0,
        Confidence::Medium => 1,
        Confidence::High => 2,
    }
}

/// Persist a reconstructed day: lines, quarter shares, and the evidence view.
async fn persist_day(
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    day: &ReconstructedDay,
    cfg: &ReconstructionConfig,
    status: TimesheetStatus,
) -> Result<(), AppError> {
    let now = Utc::now();
    let existing = draft_repo.find_by_user_and_date(user_id, day.date).await?;

    let mut lines: Vec<TimesheetDraftLine> = day
        .allocations
        .iter()
        .map(|a| TimesheetDraftLine {
            id: Uuid::new_v4(),
            gryzzly_project_id: Some(a.gryzzly_project_id.clone()),
            project_name: None,
            hours: a.hours,
            // Lines are DERIVED from the quarters now. A pinned line would be a second
            // source of truth the arbitration could not explain, so there is none.
            is_pinned: false,
            confidence: a.confidence,
            source_refs: a.source_refs.clone(),
        })
        .collect();
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

    let shares: Vec<QuarterShareRow> = day
        .quarters
        .iter()
        .flat_map(|q| {
            q.shares.iter().map(|s| QuarterShareRow {
                id: Uuid::new_v4(),
                quarter_index: q.quarter.index,
                task_id: s.lane.task_id(),
                lane_key: s.lane.as_key(),
                label: s.label.clone(),
                gryzzly_project_id: s.gryzzly_project_id.clone(),
                presence_minutes: s.presence_minutes,
                hours: s.hours,
                is_pinned: s.is_pinned,
            })
        })
        .collect();

    let draft = TimesheetDraft {
        id: existing.as_ref().map(|d| d.id).unwrap_or_else(Uuid::new_v4),
        user_id,
        date: day.date,
        status,
        target_hours: cfg.daily_target_hours,
        total_hours: day.total_hours,
        day_confidence: day.day_confidence,
        // The single-track timeline is gone; `lanes_json` replaces it.
        blocks_json: None,
        unresolved_json: serde_json::to_string(
            &day.unresolved
                .iter()
                .map(|u| {
                    serde_json::json!({
                        "sourceRef": u.source_ref,
                        "label": u.label,
                        "at": u.at.to_string(),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .ok(),
        lanes_json: serde_json::to_string(
            &day.lanes
                .iter()
                .map(|l| {
                    serde_json::json!({
                        "laneKey": l.key.as_key(),
                        "label": l.label,
                        "gryzzlyProjectId": l.gryzzly_project_id,
                        "intervals": l.intervals.iter().map(|(s, e)| [s, e]).collect::<Vec<_>>(),
                        "outsideMinutes": l.outside_minutes,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .ok(),
        lines,
        shares,
        created_at: existing.as_ref().map(|d| d.created_at).unwrap_or(now),
        updated_at: now,
    };
    draft_repo.upsert(&draft).await?;
    Ok(())
}

/// Pin one lane's hours inside one quarter, then rebalance the rest of that quarter.
///
/// The pin is the user's decision and outranks the evidence that suggested it: a later
/// reconstruct preserves it and re-apportions everything else around it.
#[allow(clippy::too_many_arguments)]
pub async fn set_quarter_share(
    worklog_repo: &dyn WorklogRepository,
    meeting_repo: &dyn MeetingRepository,
    task_repo: &dyn TaskRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    mapping_repo: &dyn SignalMappingRepository,
    config_repo: &dyn ConfigRepository,
    activity_repo: &dyn ActivitySlotRepository,
    git: &dyn GitConnector,
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
    quarter_index: u8,
    lane_key: &str,
    hours: f64,
) -> Result<ReconstructedDay, AppError> {
    let cfg = load_reconstruction_config(config_repo, user_id).await?;
    let quarter = quarters(&cfg)
        .into_iter()
        .find(|q| q.index == quarter_index)
        .ok_or_else(|| AppError::Validation(format!("no quarter {quarter_index} in the day")))?;
    if !(0.0..=quarter.hours + 1e-9).contains(&hours) {
        return Err(AppError::Validation(format!(
            "{hours}h does not fit quarter {quarter_index}, which declares {}h",
            quarter.hours
        )));
    }
    if LaneKey::parse(lane_key).is_none() {
        return Err(AppError::Validation(format!("unreadable lane '{lane_key}'")));
    }
    edit_pins(draft_repo, user_id, date, |shares| {
        match shares
            .iter_mut()
            .find(|s| s.quarter_index == quarter_index && s.lane_key == lane_key)
        {
            Some(s) => {
                s.hours = hours;
                s.is_pinned = true;
            }
            // Pinning a lane the quarter had no share for is legitimate: the user knows
            // something the evidence does not.
            None => shares.push(QuarterShareRow {
                id: Uuid::new_v4(),
                quarter_index,
                task_id: LaneKey::parse(lane_key).and_then(|l| l.task_id()),
                lane_key: lane_key.to_string(),
                label: lane_key.to_string(),
                gryzzly_project_id: None,
                presence_minutes: 0,
                hours,
                is_pinned: true,
            }),
        }
    })
    .await?;
    reconstruct_timesheet(
        worklog_repo, meeting_repo, task_repo, catalog_repo, mapping_repo, config_repo,
        activity_repo, git, draft_repo, user_id, date,
    )
    .await
}

/// Release one pinned share back to the evidence.
#[allow(clippy::too_many_arguments)]
pub async fn clear_quarter_share(
    worklog_repo: &dyn WorklogRepository,
    meeting_repo: &dyn MeetingRepository,
    task_repo: &dyn TaskRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    mapping_repo: &dyn SignalMappingRepository,
    config_repo: &dyn ConfigRepository,
    activity_repo: &dyn ActivitySlotRepository,
    git: &dyn GitConnector,
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
    quarter_index: u8,
    lane_key: &str,
) -> Result<ReconstructedDay, AppError> {
    edit_pins(draft_repo, user_id, date, |shares| {
        if let Some(s) = shares
            .iter_mut()
            .find(|s| s.quarter_index == quarter_index && s.lane_key == lane_key)
        {
            s.is_pinned = false;
        }
    })
    .await?;
    reconstruct_timesheet(
        worklog_repo, meeting_repo, task_repo, catalog_repo, mapping_repo, config_repo,
        activity_repo, git, draft_repo, user_id, date,
    )
    .await
}

/// Drop every pin in one quarter, returning it to what the evidence says.
#[allow(clippy::too_many_arguments)]
pub async fn reset_quarter(
    worklog_repo: &dyn WorklogRepository,
    meeting_repo: &dyn MeetingRepository,
    task_repo: &dyn TaskRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    mapping_repo: &dyn SignalMappingRepository,
    config_repo: &dyn ConfigRepository,
    activity_repo: &dyn ActivitySlotRepository,
    git: &dyn GitConnector,
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
    quarter_index: u8,
) -> Result<ReconstructedDay, AppError> {
    edit_pins(draft_repo, user_id, date, |shares| {
        for s in shares.iter_mut().filter(|s| s.quarter_index == quarter_index) {
            s.is_pinned = false;
        }
    })
    .await?;
    reconstruct_timesheet(
        worklog_repo, meeting_repo, task_repo, catalog_repo, mapping_repo, config_repo,
        activity_repo, git, draft_repo, user_id, date,
    )
    .await
}

/// Apply an edit to the persisted pins, refusing on a day that is already finished.
async fn edit_pins(
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
    edit: impl FnOnce(&mut Vec<QuarterShareRow>),
) -> Result<(), AppError> {
    let mut draft = draft_repo
        .find_by_user_and_date(user_id, date)
        .await?
        .ok_or_else(|| AppError::Validation(format!("no timesheet draft for {date}")))?;
    if matches!(
        draft.status,
        TimesheetStatus::Validated | TimesheetStatus::Submitted | TimesheetStatus::DayOff
    ) {
        return Err(AppError::Validation(format!(
            "the day {date} is {} — reopen it before editing its quarters",
            draft.status.as_str()
        )));
    }
    edit(&mut draft.shares);
    draft.updated_at = Utc::now();
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
        unresolved_json: None,
        lanes_json: None,
        lines: vec![],
        shares: vec![],
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
    activity_repo: &dyn ActivitySlotRepository,
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
            activity_repo, git, draft_repo, user_id, *date,
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
        tasks: Vec<Task>,
    }

    impl MemTask {
        fn one(task: Task) -> Self {
            Self { tasks: vec![task] }
        }
    }

    #[async_trait]
    impl TaskRepository for MemTask {
        async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
            Ok(self.tasks.iter().find(|t| t.id == id).cloned())
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
        async fn find_overdue(
            &self,
            _u: UserId,
            _today: chrono::NaiveDate,
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

    /// No hand-run slots. The reconstruction weighs `manual` slots as measured spans;
    /// tests that need one override `slots`.
    #[derive(Default)]
    struct MemActivity {
        slots: Vec<ActivitySlot>,
    }

    #[async_trait]
    impl ActivitySlotRepository for MemActivity {
        async fn find_by_id(&self, _id: ActivitySlotId) -> Result<Option<ActivitySlot>, RepositoryError> {
            Ok(None)
        }
        async fn find_by_user_and_date(
            &self,
            _user_id: UserId,
            _date: NaiveDate,
        ) -> Result<Vec<ActivitySlot>, RepositoryError> {
            Ok(self.slots.clone())
        }
        async fn find_active(&self, _user_id: UserId) -> Result<Option<ActivitySlot>, RepositoryError> {
            Ok(None)
        }
        async fn save(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn update(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn find_by_user_and_date_range(
            &self,
            _user_id: UserId,
            _start_date: NaiveDate,
            _end_date: NaiveDate,
        ) -> Result<Vec<ActivitySlot>, RepositoryError> {
            Ok(vec![])
        }
        async fn delete(&self, _id: ActivitySlotId) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

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
            MemTask::one(make_task_with_project(make_user_id(), Uuid::new_v4(), "unused")),
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

    /// Build a worklog entry at a given LOCAL Paris time on the test date.
    fn entry_at(user_id: UserId, task_id: TaskId, h: u32, m: u32, body: &str) -> WorklogEntry {
        let logged_at = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 6, 8, h - 2, m, 0).unwrap();
        WorklogEntry {
            id: Uuid::new_v4(),
            user_id,
            task_id,
            body: body.to_string(),
            logged_at,
            created_at: logged_at,
            updated_at: logged_at,
            session_id: None,
        }
    }

    fn two_task_setup(user_id: UserId) -> (TaskId, TaskId, MemTask, MemCatalog) {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut task_a = make_task_with_project(user_id, a, "p1");
        task_a.title = "Task A".into();
        let mut task_b = make_task_with_project(user_id, b, "p2");
        task_b.title = "Task B".into();
        task_b.gryzzly_task_id = Some("gt2".into());
        let mut entry_b = make_catalog_entry(user_id, "p2");
        entry_b.gryzzly_task_id = "gt2".into();
        (
            a,
            b,
            MemTask { tasks: vec![task_a, task_b] },
            MemCatalog { entries: vec![make_catalog_entry(user_id, "p1"), entry_b] },
        )
    }

    /// THE regression. Two sessions ran the same afternoon; the second logged only near
    /// the end, which is exactly the shape carry-forward turned into "the first task gets
    /// the whole stretch". Both must declare real hours now.
    #[tokio::test]
    async fn concurrent_tasks_each_declare_hours() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let (a, b, task_repo, catalog_repo) = two_task_setup(user_id);
        let worklog_repo = MemWorklog {
            entries: vec![
                entry_at(user_id, a, 14, 5, "A avance"),
                entry_at(user_id, a, 15, 0, "A continue"),
                entry_at(user_id, a, 16, 30, "A termine"),
                entry_at(user_id, b, 16, 20, "B: tout d'un coup"),
                entry_at(user_id, b, 16, 45, "B: encore"),
            ],
        };
        let draft_repo = MemDraft::default();

        let day = reconstruct_timesheet(
            &worklog_repo,
            &MemMeeting,
            &task_repo,
            &catalog_repo,
            &MemMapping,
            &MemConfig::with(&[("aplan.timezone", "Europe/Paris")]),
            &MemActivity::default(),
            &MemGit,
            &draft_repo,
            user_id,
            date,
        )
        .await
        .expect("reconstruct_timesheet should succeed");

        let hours_of = |label: &str| -> f64 {
            day.quarters
                .iter()
                .flat_map(|q| &q.shares)
                .filter(|s| s.label == label)
                .map(|s| s.hours)
                .sum()
        };
        assert!(hours_of("Task A") > 0.0, "A ran all afternoon");
        assert!(
            hours_of("Task B") >= 0.25,
            "B logged late but ran too — carry-forward gave it nothing, got {}",
            hours_of("Task B")
        );
        assert!(
            day.lanes.len() >= 2,
            "both tasks must appear as concurrent lanes, got {:?}",
            day.lanes.iter().map(|l| &l.label).collect::<Vec<_>>()
        );
    }

    /// A pin is the user's decision. A later reconstruct must preserve it and rebalance
    /// the rest of its quarter around it — otherwise every refresh silently undoes the
    /// arbitration.
    #[tokio::test]
    async fn a_pinned_share_survives_a_reconstruct() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let (a, b, task_repo, catalog_repo) = two_task_setup(user_id);
        let worklog_repo = MemWorklog {
            entries: vec![
                entry_at(user_id, a, 15, 30, "A"),
                entry_at(user_id, b, 16, 30, "B"),
            ],
        };
        let draft_repo = MemDraft::default();
        let config_repo = MemConfig::with(&[("aplan.timezone", "Europe/Paris")]);
        let args = |lane: String, hours: f64| (lane, hours);

        let day = reconstruct_timesheet(
            &worklog_repo, &MemMeeting, &task_repo, &catalog_repo, &MemMapping, &config_repo,
            &MemActivity::default(), &MemGit, &draft_repo, user_id, date,
        )
        .await
        .unwrap();
        let q3 = day.quarters.iter().find(|q| q.quarter.index == 3).expect("Q3 exists");
        let lane = q3.shares.first().expect("Q3 has shares").lane.as_key();
        let (lane, pinned_hours) = args(lane, 1.5);

        let after = set_quarter_share(
            &worklog_repo, &MemMeeting, &task_repo, &catalog_repo, &MemMapping, &config_repo,
            &MemActivity::default(), &MemGit, &draft_repo, user_id, date, 3, &lane, pinned_hours,
        )
        .await
        .expect("pinning should succeed");
        let q3 = after.quarters.iter().find(|q| q.quarter.index == 3).unwrap();
        let pinned = q3.shares.iter().find(|s| s.lane.as_key() == lane).expect("the pinned lane");
        assert!((pinned.hours - 1.5).abs() < 1e-9, "the pin must hold, got {}", pinned.hours);
        assert!(pinned.is_pinned);
        assert!(
            (q3.shares.iter().map(|s| s.hours).sum::<f64>() - q3.declarable_hours).abs() < 1e-9,
            "the rest of the quarter rebalances around the pin"
        );

        // And again, from scratch: the pin is read back off the persisted draft.
        let again = reconstruct_timesheet(
            &worklog_repo, &MemMeeting, &task_repo, &catalog_repo, &MemMapping, &config_repo,
            &MemActivity::default(), &MemGit, &draft_repo, user_id, date,
        )
        .await
        .unwrap();
        let q3 = again.quarters.iter().find(|q| q.quarter.index == 3).unwrap();
        let pinned = q3.shares.iter().find(|s| s.lane.as_key() == lane).expect("the pin survives");
        assert!((pinned.hours - 1.5).abs() < 1e-9, "a refresh must not undo the arbitration");
    }

    /// A `manual` slot is measured time the worklog projection cannot derive, so it
    /// weighs. A `worklog` slot is a projection of the very entries already counted —
    /// weighing it too would double the lane.
    #[tokio::test]
    async fn manual_slots_weigh_and_projected_slots_do_not() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let (a, b, task_repo, catalog_repo) = two_task_setup(user_id);
        let slot = |task_id: TaskId, source: SlotSource| ActivitySlot {
            id: Uuid::new_v4(),
            user_id,
            task_id: Some(task_id),
            start_time: chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 6, 8, 11, 0, 0).unwrap(),
            end_time: Some(chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 6, 8, 13, 0, 0).unwrap()),
            half_day: HalfDay::Afternoon,
            date,
            created_at: Utc::now(),
            session_id: None,
            source,
        };
        let day = reconstruct_timesheet(
            &MemWorklog { entries: vec![entry_at(user_id, b, 16, 30, "B")] },
            &MemMeeting,
            &task_repo,
            &catalog_repo,
            &MemMapping,
            &MemConfig::with(&[("aplan.timezone", "Europe/Paris")]),
            &MemActivity { slots: vec![slot(a, SlotSource::Manual), slot(b, SlotSource::Worklog)] },
            &MemGit,
            &draft_repo_for_slots(),
            user_id,
            date,
        )
        .await
        .unwrap();

        let lane_a = day.lanes.iter().find(|l| l.label == "Task A").expect("the manual slot is a lane");
        assert!(!lane_a.intervals.is_empty(), "a hand-run timer is measured time and must weigh");
        let lane_b = day.lanes.iter().find(|l| l.label == "Task B").expect("B has an entry");
        assert!(
            lane_b.intervals.iter().map(|(s, e)| e - s).sum::<i64>() <= domain::rules::worklog_time::MAX_CONTINUATION_GAP_MINUTES,
            "B's projected slot must not add to its own entry's shadow"
        );
    }

    fn draft_repo_for_slots() -> MemDraft {
        MemDraft::default()
    }

    #[tokio::test]
    async fn reconstruct_persists_a_draft_summing_to_the_four_quarters() {
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
        let task_repo = MemTask::one(make_task_with_project(user_id, task_id, "p1"));
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
            &MemActivity::default(),
            &git,
            &draft_repo,
            user_id,
            date,
        )
        .await
        .expect("reconstruct_timesheet should succeed");

        // The day totals the four quarters — 8h with the default 08-12 / 13-17 windows —
        // NOT `daily_target_hours`. A quarter sums to its own length by construction, so
        // it cannot also sum to a scaled fraction of a different target.
        assert!(
            (day.total_hours - 8.0).abs() < 1e-9,
            "expected total_hours=8.0 (four 2h quarters), got {}",
            day.total_hours
        );
        for q in &day.quarters {
            let sum: f64 = q.shares.iter().map(|s| s.hours).sum();
            assert!(
                (sum - q.declarable_hours).abs() < 1e-9,
                "quarter {} declares {} but its shares sum to {sum}",
                q.quarter.index,
                q.declarable_hours
            );
        }

        // The draft must have been persisted exactly once.
        let saved = draft_repo.saved.lock().unwrap();
        assert_eq!(saved.len(), 1, "expected exactly one upsert to draft repo");
        assert_eq!(saved[0].date, date);
        assert_eq!(saved[0].user_id, user_id);
    }

    /// The unresolved-signal list explains the anonymous bars on the timeline. It used to
    /// live only in the mutation's response, so a reload lost it — it must reach the draft.
    #[tokio::test]
    async fn reconstruct_persists_the_unresolved_signals_on_the_draft() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let logged_at = chrono::DateTime::parse_from_rfc3339("2026-06-08T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let task_id: TaskId = Uuid::new_v4();

        // A task with NO Gryzzly project and no mapping rule → the signal stays unresolved.
        let mut task = make_task_with_project(user_id, task_id, "p1");
        task.gryzzly_project_id = None;
        task.gryzzly_task_id = None;

        let worklog_repo = MemWorklog {
            entries: vec![WorklogEntry {
                id: Uuid::new_v4(),
                user_id,
                task_id,
                body: "refonte du pipeline".to_string(),
                logged_at,
                created_at: logged_at,
                updated_at: logged_at,
                session_id: None,
            }],
        };
        let draft_repo = MemDraft::default();

        let day = reconstruct_timesheet(
            &worklog_repo,
            &MemMeeting,
            &MemTask::one(task),
            &MemCatalog { entries: vec![] },
            &MemMapping,
            &MemConfig::with(&[("aplan.timezone", "UTC")]),
            &MemActivity::default(),
            &MemGit,
            &draft_repo,
            user_id,
            date,
        )
        .await
        .expect("reconstruct_timesheet should succeed");

        assert!(!day.unresolved.is_empty(), "an unmapped signal is unresolved");
        let saved = draft_repo.find_by_user_and_date(user_id, date).await.unwrap().unwrap();
        let json = saved.unresolved_json.expect("unresolved_json must be persisted");
        assert!(json.contains("refonte du pipeline"), "the label explains WHAT was unattributed: {json}");
        assert!(json.contains("\"sourceRef\":\"wl:"), "the sourceRef joins back to the timeline: {json}");
        assert!(json.contains("2026-06-08 09:00:00"), "the local timestamp is kept: {json}");
    }

    /// The timeline shows a project name per bar; the owning task's title is what tells the
    /// user WHAT that time was. It is already loaded to resolve the project, so it must be
    /// carried onto the block and persisted with it — `blocks_json` is the only copy a page
    /// reload gets to read.
    #[tokio::test]
    async fn reconstruct_carries_the_task_title_onto_each_block_and_persists_it() {
        let user_id = make_user_id();
        let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let logged_at = chrono::DateTime::parse_from_rfc3339("2026-06-08T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let task_id: TaskId = Uuid::new_v4();
        let mut task = make_task_with_project(user_id, task_id, "p1");
        task.title = "Refonte du portail client".to_string();

        let worklog_repo = MemWorklog {
            entries: vec![WorklogEntry {
                id: Uuid::new_v4(),
                user_id,
                task_id,
                body: "avancement sur le parseur".to_string(),
                logged_at,
                created_at: logged_at,
                updated_at: logged_at,
                session_id: None,
            }],
        };
        let draft_repo = MemDraft::default();

        let day = reconstruct_timesheet(
            &worklog_repo,
            &MemMeeting,
            &MemTask::one(task),
            &MemCatalog { entries: vec![make_catalog_entry(user_id, "p1")] },
            &MemMapping,
            &MemConfig::with(&[("aplan.timezone", "UTC")]),
            &MemActivity::default(),
            &MemGit,
            &draft_repo,
            user_id,
            date,
        )
        .await
        .expect("reconstruct_timesheet should succeed");

        assert!(
            day.lanes.iter().any(|l| l.label == "Refonte du portail client"),
            "the lane must name the task it came from, not just its project"
        );
        assert!(
            day.quarters.iter().flat_map(|q| &q.shares).any(|s| s.label == "Refonte du portail client"),
            "and so must the share it produces — a project id explains nothing to a reader"
        );
        let saved = draft_repo.find_by_user_and_date(user_id, date).await.unwrap().unwrap();
        let json = saved.lanes_json.expect("lanes_json must be persisted");
        assert!(
            json.contains("Refonte du portail client"),
            "the task title must survive a reload: {json}"
        );
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
                    unresolved_json: None,
                    lanes_json: None,
                    shares: vec![],
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
            &MemTask::one(make_task_with_project(user_id, task_id, "p1")),
            &MemCatalog { entries: vec![make_catalog_entry(user_id, "p1")] },
            &MemMapping,
            &MemConfig::with(&[("aplan.timezone", "UTC")]),
            &MemActivity::default(),
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
                    unresolved_json: None,
                    lanes_json: None,
                    shares: vec![],
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
            &MemTask::one(make_task_with_project(user_id, task_id, "p1")),
            &MemCatalog { entries: vec![make_catalog_entry(user_id, "p1")] },
            &MemMapping,
            &MemConfig::with(&[("aplan.timezone", "UTC")]),
            &MemActivity::default(),
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
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert, &MemActivity::default(),
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
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert, &MemActivity::default(),
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
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert, &MemActivity::default(),
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
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert, &MemActivity::default(),
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
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert, &MemActivity::default(),
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
            unresolved_json: None,
            lanes_json: None,
            shares: vec![],
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
            unresolved_json: None,
            lanes_json: None,
            shares: vec![],
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
