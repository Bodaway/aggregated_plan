//! `aplan timesheet` — flag-driven reconstruction + review of the day's
//! Gryzzly timesheet. No interactive REPL: the default action reconstructs
//! and renders; `validate`/`set`/`off` are explicit subcommands.

use chrono::{DateTime, Utc};

use crate::client::Client;
use crate::output::{print_json, ExitCode};
use crate::queries::{
    activity_journal, learn_mapping, mark_day_off, reconstruct_timesheet, set_quarter_share,
    signal_mappings, validate_timesheet, ActivityJournal, LearnMapping,
    MarkDayOff, ReconstructTimesheet, SetQuarterShare, SignalMappings,
    ValidateTimesheet,
};

fn today() -> String {
    chrono::Utc::now().date_naive().to_string()
}

/// `aplan timesheet [--date] [--json]` — reconstruct and display the day.
pub fn timesheet(api_url: &str, json: bool, date: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date = date.map(String::from).unwrap_or_else(today);
    let res = client.run::<ReconstructTimesheet>(reconstruct_timesheet::Variables {
        date: date.clone(),
    });
    match res {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            render_day(&r.data.run_timesheet_reconstruction);

            // The overlap gap, computed from the day's raw activity slots — a
            // separate round trip on `ActivityJournal` (task 8's operation),
            // not the Gryzzly reconstruction's `blocks`, which are already a
            // deduplicated per-project view and would not carry the raw
            // double-counted total this line needs.
            //
            // Best-effort, same reasoning as `journal`/`dash`: the day's
            // reconstruction already rendered successfully above, and a
            // failure fetching the supplementary overlap check must not turn
            // that into a hard error — but is noted on stderr rather than
            // swallowed outright, so a quiet day is never indistinguishable
            // from a failed check.
            match client.run::<ActivityJournal>(activity_journal::Variables { date }) {
                Ok(jr) => {
                    // Untagged slots (`taskId: null`) are excluded: they are
                    // time attributed to nobody, so they have no business in
                    // a number about double-booked *attribution* — the same
                    // exclusion `find_overlaps` applies for the same reason
                    // (`domain/rules/overlap.rs`).
                    let tagged = jr
                        .data
                        .activity_journal
                        .iter()
                        .filter(|s| s.task_id.is_some());
                    let raw: i64 = tagged.clone().filter_map(|s| s.duration_minutes).sum();
                    let intervals: Vec<(DateTime<Utc>, DateTime<Utc>)> = tagged
                        .filter_map(|s| {
                            let end = parse_instant(s.end_time.as_deref()?)?;
                            let start = parse_instant(&s.start_time)?;
                            Some((start, end))
                        })
                        .collect();
                    let covered = union_minutes(intervals);
                    let gap = raw - covered;
                    // Even after excluding untagged time, this residual still
                    // counts two overlapping stretches of the *same* task —
                    // which the pair rule deliberately does not (a task may
                    // legitimately have several stretches in a half-day).
                    // That is intentional, not a bug to align with `journal`:
                    // this line answers "how much of today's logged time is
                    // double-booked at all" (you cannot bill 8h20 inside
                    // 7h30 of wall time, regardless of which task it was),
                    // while `journal` answers "which two tasks collided".
                    // The two measures can disagree and both be right.
                    if gap > 0 {
                        println!();
                        println!(
                            "\u{26a0} recouvrement {gap} min sur la journ\u{e9}e \u{2014} brut {} h {:02}, couvert {} h {:02}",
                            raw / 60,
                            raw % 60,
                            covered / 60,
                            covered % 60,
                        );
                    }
                }
                Err(e) => eprintln!("note: overlap check unavailable: {e}"),
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}

/// Parse a server-emitted RFC 3339 instant. The server always emits a valid
/// one; a slot whose timestamp fails to parse is excluded from the raw/
/// covered calculation rather than aborting the whole command over one
/// malformed string.
fn parse_instant(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Minutes spanned by the union of closed intervals — merging
/// overlapping/touching stretches so a minute covered by two slots at once
/// is counted once, not twice. This is `timesheet`'s "elapsed", deliberately
/// not `last_end - first_start`: a day with a gap (lunch, say) would
/// otherwise make `raw - elapsed` negative.
fn union_minutes(mut intervals: Vec<(DateTime<Utc>, DateTime<Utc>)>) -> i64 {
    intervals.sort_by_key(|(start, _)| *start);
    let mut total = 0i64;
    let mut current: Option<(DateTime<Utc>, DateTime<Utc>)> = None;
    for (start, end) in intervals {
        current = match current {
            None => Some((start, end)),
            Some((cur_start, cur_end)) if start <= cur_end => Some((cur_start, cur_end.max(end))),
            Some((cur_start, cur_end)) => {
                total += (cur_end - cur_start).num_minutes();
                Some((start, end))
            }
        };
    }
    if let Some((start, end)) = current {
        total += (end - start).num_minutes();
    }
    total
}

/// Trim a lane label to `max` display characters, on a char boundary.
fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "\u{2026}"
}

fn render_day(d: &reconstruct_timesheet::ReconstructTimesheetRunTimesheetReconstruction) {
    println!(
        "== timesheet {} ==  [{:?}]  {:.2}h / {:.1}h target",
        d.date, d.status, d.total_hours, d.target_hours
    );
    println!("\nhours × project:");
    for l in &d.lines {
        let label = l
            .gryzzly_project_id
            .clone()
            .unwrap_or_else(|| "?? unassigned".into());
        let name = l.project_name.clone().unwrap_or_default();
        let pin = if l.is_pinned { "*" } else { " " };
        println!("  {}{:<8.2}h  {:<24} {}", pin, l.hours, label, name);
    }
    let delta = d.total_hours - d.target_hours;
    let badge = if delta.abs() < 1e-6 {
        "\u{2713} balanced".to_string()
    } else if delta > 0.0 {
        format!("\u{26a0} +{delta:.2}h over")
    } else {
        format!("\u{26a0} {delta:.2}h short")
    };
    println!("  \u{2500}\u{2500} total {:.2}h  ({badge})", d.total_hours);
    if d.unattributed_hours > 1e-9 {
        println!(
            "  !! {:.2}h unattributed \u{2014} assign with `aplan timesheet set --quarter <1-4> <task> <hours>`",
            d.unattributed_hours
        );
    }
    println!("  day confidence: {:?}", d.day_confidence);
    for q in &d.quarters {
        let (sh, sm) = (q.start_min / 60, q.start_min % 60);
        let (eh, em) = (q.end_min / 60, q.end_min % 60);
        println!(
            "\nQ{}  {:02}:{:02}-{:02}:{:02}{}                        confidence: {:?}",
            q.index + 1,
            sh,
            sm,
            eh,
            em,
            if q.ooo_hours > 1e-9 { format!("  ({:.2}h off)", q.ooo_hours) } else { String::new() },
            q.confidence
        );
        if q.shares.is_empty() {
            println!("    (rien de déclaré)");
            continue;
        }
        let span = (q.end_min - q.start_min).max(1);
        for s in &q.shares {
            // The bar is the WEIGHT, not the hours: it is what lets a reader see when a
            // share rests on thin evidence.
            let width = ((s.presence_minutes * 8) / span).clamp(0, 8) as usize;
            let pin = if s.is_pinned { "*" } else { " " };
            println!(
                "  {}{:<26} {:<8} {:>3} min   {:.2}h",
                pin,
                truncate_label(&s.label, 26),
                "\u{2588}".repeat(width),
                s.presence_minutes,
                s.hours
            );
        }
    }
    if !d.outside_workday.is_empty() {
        let total: i64 = d.outside_workday.iter().map(|o| o.minutes).sum();
        let who: Vec<&str> = d.outside_workday.iter().map(|o| o.label.as_str()).take(3).collect();
        println!(
            "\n\u{26a0} {}h {:02} de traces hors plage horaire ({})",
            total / 60,
            total % 60,
            who.join(", ")
        );
    }
    if !d.unresolved.is_empty() {
        println!("\nunresolved signals ({}):", d.unresolved.len());
        for u in &d.unresolved {
            println!("  {} {}", u.at, u.label);
        }
    }
}

/// `aplan timesheet validate`
pub fn timesheet_validate(api_url: &str, json: bool, date: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date = date.map(String::from).unwrap_or_else(today);
    match client.run::<ValidateTimesheet>(validate_timesheet::Variables { date: date.clone() }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            println!("\u{2713} {} validated \u{2014} copy into Gryzzly", date);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}

/// `aplan timesheet set --quarter <1-4> <task> <hours>` — pin one lane inside one quarter.
///
/// The lane is resolved against the day's own quarters: an exact lane key, else a
/// case-insensitive substring of a lane label. Ambiguity is exit 3 with the candidates
/// listed, never a guess — these hours reach a client invoice.
pub fn timesheet_set(
    api_url: &str,
    json: bool,
    date: Option<&str>,
    quarter: u8,
    task: &str,
    hours: f64,
) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date = date.map(String::from).unwrap_or_else(today);
    let quarter_index = (quarter as i64) - 1;

    // Reconstruct first: the lanes are what the user is choosing between, and they are
    // derived from the evidence, not stored on the draft's lines.
    let day = match client.run::<ReconstructTimesheet>(reconstruct_timesheet::Variables {
        date: date.clone(),
    }) {
        Ok(r) => r.data.run_timesheet_reconstruction,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::Generic;
        }
    };

    let needle = task.to_lowercase();
    let mut candidates: Vec<(String, String)> = day
        .quarters
        .iter()
        .filter(|q| q.index == quarter_index)
        .flat_map(|q| q.shares.iter())
        .map(|s| (s.lane_key.clone(), s.label.clone()))
        .collect();
    // A lane present elsewhere in the day is still a legitimate target for this quarter:
    // the user may know they worked on it even where no evidence landed.
    for l in &day.lanes {
        if !candidates.iter().any(|(k, _)| *k == l.lane_key) {
            candidates.push((l.lane_key.clone(), l.label.clone()));
        }
    }
    let exact: Vec<&(String, String)> = candidates.iter().filter(|(k, _)| *k == task).collect();
    let matches: Vec<&(String, String)> = if exact.is_empty() {
        candidates.iter().filter(|(_, label)| label.to_lowercase().contains(&needle)).collect()
    } else {
        exact
    };
    let lane_key = match matches.as_slice() {
        [] => {
            eprintln!("error: no lane matches `{task}` on {date}");
            return ExitCode::NotFound;
        }
        [one] => one.0.clone(),
        many => {
            eprintln!("error: `{task}` is ambiguous on {date}:");
            for (k, label) in many.iter().take(5) {
                eprintln!("  {label}  [{k}]");
            }
            return ExitCode::Ambiguous;
        }
    };

    match client.run::<SetQuarterShare>(set_quarter_share::Variables {
        date: date.clone(),
        quarter_index,
        lane_key: lane_key.clone(),
        hours,
    }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let d = r.data.set_quarter_share;
            println!("\u{270e} Q{quarter} \u{2014} {lane_key} pinned to {hours:.2}h");
            if let Some(q) = d.quarters.iter().find(|q| q.index == quarter_index) {
                for s in &q.shares {
                    let pin = if s.is_pinned { "*" } else { " " };
                    println!("  {}{:<6.2}h  {}", pin, s.hours, s.label);
                }
                println!("  \u{2500}\u{2500} {:.2}h declared", q.declarable_hours);
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}

/// `aplan timesheet off [--am|--pm]`
pub fn timesheet_off(api_url: &str, json: bool, date: Option<&str>, am: bool, pm: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date = date.map(String::from).unwrap_or_else(today);
    let scope = if am {
        mark_day_off::DayOffScopeGql::MORNING
    } else if pm {
        mark_day_off::DayOffScopeGql::AFTERNOON
    } else {
        mark_day_off::DayOffScopeGql::FULL
    };
    if am || pm {
        eprintln!("note: half-day off is not yet honored; marking the full day off");
    }
    match client.run::<MarkDayOff>(mark_day_off::Variables {
        date: date.clone(),
        scope,
    }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            println!("\u{23f8} {} marked off", date);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}

/// `aplan map add --repo <path> [--branch <glob>] --project <gid>` (or
/// `--meeting-subject`/`--meeting-organizer`/`--internal-project` instead of `--repo`)
/// — learn/update a signal→Gryzzly-project mapping rule. Exactly one selector is required.
#[allow(clippy::too_many_arguments)]
pub fn map_add(
    api_url: &str,
    json: bool,
    repo: Option<&str>,
    branch: Option<&str>,
    meeting_subject: Option<&str>,
    meeting_organizer: Option<&str>,
    internal_project: Option<&str>,
    project: &str,
) -> ExitCode {
    let (kind, pattern, branch_pattern) = if let Some(r) = repo {
        if branch.is_some() {
            (
                learn_mapping::MappingKindGql::BRANCH,
                r.to_string(),
                branch.map(String::from),
            )
        } else {
            (learn_mapping::MappingKindGql::REPO_PATH, r.to_string(), None)
        }
    } else if let Some(s) = meeting_subject {
        (
            learn_mapping::MappingKindGql::MEETING_SUBJECT,
            s.to_string(),
            None,
        )
    } else if let Some(o) = meeting_organizer {
        (
            learn_mapping::MappingKindGql::MEETING_ORGANIZER,
            o.to_string(),
            None,
        )
    } else if let Some(p) = internal_project {
        (
            learn_mapping::MappingKindGql::INTERNAL_PROJECT,
            p.to_string(),
            None,
        )
    } else {
        eprintln!(
            "error: provide one of --repo / --meeting-subject / --meeting-organizer / --internal-project"
        );
        return ExitCode::PreconditionFailed;
    };
    let client = Client::new(api_url.to_string());
    let vars = learn_mapping::Variables {
        kind,
        pattern,
        branch_pattern,
        gryzzly_project_id: project.to_string(),
    };
    match client.run::<LearnMapping>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            println!("\u{270e} mapping saved \u{2192} project {project}");
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}

/// `aplan map list` — list enabled mapping rules.
pub fn map_list(api_url: &str, json: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    match client.run::<SignalMappings>(signal_mappings::Variables {}) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            for m in &r.data.signal_mappings {
                let br = m
                    .branch_pattern
                    .clone()
                    .map(|b| format!("@{b}"))
                    .unwrap_or_default();
                let name = m.gryzzly_project_name.clone().unwrap_or_default();
                println!(
                    "  [{:?}] {}{} \u{2192} {} {}",
                    m.kind, m.pattern, br, m.gryzzly_project_id, name
                );
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}

#[cfg(test)]
mod overlap_gap_tests {
    use super::*;
    use chrono::TimeZone;

    fn t(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 9, h, m, 0).unwrap()
    }

    #[test]
    fn a_valid_instant_parses() {
        assert_eq!(parse_instant("2026-03-09T09:00:00Z"), Some(t(9, 0)));
    }

    #[test]
    fn a_malformed_instant_is_excluded_rather_than_panicking() {
        assert_eq!(parse_instant("not-a-time"), None);
    }

    /// The defect the brief names explicitly: with a real gap (a lunch break)
    /// between two stretches, the union is the *sum* of their lengths, not
    /// the outer span `last_end - first_start` (which would be 8h here, not
    /// 7h, and would make `raw - elapsed` negative for a day with no
    /// overlap at all).
    #[test]
    fn a_days_gap_does_not_inflate_the_union_into_the_outer_span() {
        let morning = (t(9, 0), t(12, 0)); // 3h
        let afternoon = (t(13, 0), t(17, 0)); // 4h
        let covered = union_minutes(vec![morning, afternoon]);
        assert_eq!(covered, 7 * 60, "must be the sum of the two stretches");
        assert_ne!(
            covered,
            (t(17, 0) - t(9, 0)).num_minutes(),
            "must not equal the naive last_end - first_start (8h)"
        );
    }

    /// The property the whole function exists for: two slots that overlap by
    /// an hour must have that hour counted once, not twice.
    #[test]
    fn overlapping_intervals_count_the_shared_time_once() {
        let a = (t(9, 0), t(11, 0)); // 2h
        let b = (t(10, 0), t(12, 0)); // 2h, overlapping a by 1h
        let covered = union_minutes(vec![a, b]);
        assert_eq!(covered, 3 * 60, "9:00-12:00, not the naive sum of 4h");
    }

    #[test]
    fn touching_intervals_cover_without_a_phantom_gap() {
        let a = (t(9, 0), t(10, 0));
        let b = (t(10, 0), t(11, 0));
        assert_eq!(union_minutes(vec![a, b]), 2 * 60);
    }

    #[test]
    fn a_fully_nested_interval_adds_nothing_extra() {
        let outer = (t(9, 0), t(12, 0)); // 3h
        let inner = (t(10, 0), t(10, 30)); // 30m, inside outer
        assert_eq!(union_minutes(vec![outer, inner]), 3 * 60);
    }

    #[test]
    fn union_of_nothing_is_zero() {
        assert_eq!(union_minutes(vec![]), 0);
    }
}
