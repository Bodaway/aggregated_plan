//! `aplan timesheet` — flag-driven reconstruction + review of the day's
//! Gryzzly timesheet. No interactive REPL: the default action reconstructs
//! and renders; `validate`/`set`/`off` are explicit subcommands.

use crate::client::Client;
use crate::output::{print_json, ExitCode};
use crate::queries::{
    learn_mapping, mark_day_off, reconstruct_timesheet, save_timesheet_draft, signal_mappings,
    timesheet_draft, validate_timesheet, LearnMapping, MarkDayOff, ReconstructTimesheet,
    SaveTimesheetDraft, SignalMappings, TimesheetDraft, ValidateTimesheet,
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
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
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
            "  !! {:.2}h unattributed \u{2014} assign with `aplan timesheet set <project> <hours>`",
            d.unattributed_hours
        );
    }
    println!("  day confidence: {:?}", d.day_confidence);
    if !d.blocks.is_empty() {
        println!("\ntimeline:");
        let mut blocks: Vec<_> = d.blocks.iter().collect();
        blocks.sort_by(|a, b| a.start_time.cmp(&b.start_time));
        for b in blocks {
            let glyph = match b.kind {
                reconstruct_timesheet::BlockKindGql::MEETING => "\u{2593} meet",
                reconstruct_timesheet::BlockKindGql::OUT_OF_OFFICE => "\u{2591} off ",
                _ => "\u{00b7} work",
            };
            let proj = b.gryzzly_project_id.clone().unwrap_or_else(|| "-".into());
            println!(
                "  {}\u{2013}{}  {}  {:.2}h  {}",
                b.start_time, b.end_time, glyph, b.hours, proj
            );
        }
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

/// `aplan timesheet set <project> <hours>` — pin one project to an exact number of hours.
/// Loads the current lines from the PERSISTED draft (preserving prior pins), sets/pins the
/// target project, carries the other lines forward, and saves.
///
/// IMPORTANT (bug avoided): do NOT load lines by calling `runTimesheetReconstruction` — for a
/// non-validated day that upserts a FRESH draft and wipes any previously saved pins, so two
/// consecutive `set` commands would lose the first pin. Read `timesheetDraft(date)` instead
/// (it preserves `isPinned`); only reconstruct when no draft exists yet.
pub fn timesheet_set(
    api_url: &str,
    json: bool,
    date: Option<&str>,
    project: &str,
    hours: f64,
) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date = date.map(String::from).unwrap_or_else(today);
    // Prefer the persisted draft (keeps prior pins); reconstruct once only if it's null.
    let mut lines: Vec<save_timesheet_draft::TimesheetLineInput> =
        match client.run::<TimesheetDraft>(timesheet_draft::Variables { date: date.clone() }) {
            Ok(r) => match r.data.timesheet_draft {
                Some(d) => d
                    .lines
                    .iter()
                    .map(|l| save_timesheet_draft::TimesheetLineInput {
                        gryzzly_project_id: l.gryzzly_project_id.clone(),
                        hours: l.hours,
                        is_pinned: l.is_pinned,
                    })
                    .collect(),
                None => match client.run::<ReconstructTimesheet>(reconstruct_timesheet::Variables {
                    date: date.clone(),
                }) {
                    Ok(rr) => rr
                        .data
                        .run_timesheet_reconstruction
                        .lines
                        .iter()
                        .map(|l| save_timesheet_draft::TimesheetLineInput {
                            gryzzly_project_id: l.gryzzly_project_id.clone(),
                            hours: l.hours,
                            is_pinned: l.is_pinned,
                        })
                        .collect(),
                    Err(e) => {
                        eprintln!("error: {e}");
                        return ExitCode::Generic;
                    }
                },
            },
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::Generic;
            }
        };
    match lines
        .iter_mut()
        .find(|l| l.gryzzly_project_id.as_deref() == Some(project))
    {
        Some(l) => {
            l.hours = hours;
            l.is_pinned = true;
        }
        None => lines.push(save_timesheet_draft::TimesheetLineInput {
            gryzzly_project_id: Some(project.to_string()),
            hours,
            is_pinned: true,
        }),
    }
    match client.run::<SaveTimesheetDraft>(save_timesheet_draft::Variables {
        date: date.clone(),
        lines,
    }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            println!("\u{270e} pinned {project} = {hours:.2}h; other lines rebalanced to target");
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
