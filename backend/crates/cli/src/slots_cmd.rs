//! `aplan slots repair` — give back their task to the slots that lost it.
//!
//! The damage: a write that used `INSERT OR REPLACE INTO tasks` deletes before it
//! inserts, which fired `activity_slots.task_id`'s `ON DELETE SET NULL`. The task row
//! came back identical while the slots pointing at it came out unattributed — the
//! "(no task)" hours of `aplan journal`, real time belonging to nobody.
//!
//! ```text
//! aplan slots repair --from 2026-08-04 --to 2026-08-10            # preview
//! aplan slots repair --from 2026-08-04 --to 2026-08-10 --confirm  # apply
//! ```
//!
//! **Preview by default**, like `aplan reattribute` and for the same reason: this
//! rewrites billing-relevant history. The preview prints, per date, how many orphans
//! would be dropped and what they were worth against how many slots would be written,
//! and per task the before/after hours — enough that confirming is informed rather
//! than a reflex. `--confirm` then applies exactly what was shown.
//!
//! A slot the projection does not own is never touched, whatever the range says: an
//! unattributed `manual` slot is a hand-run timer from before migration `014`, no
//! worklog entry can reproduce it, and dropping one would destroy time nothing can
//! rebuild.

use crate::client::{Client, ClientError};
use crate::output::{hm, print_json, ExitCode};
use crate::queries::{repair_orphaned_slots, RepairOrphanedSlots};

/// Map a transport/GraphQL failure onto the exit-code contract.
///
/// Same technique as `reattribute_cmd::exit_code_for`: async-graphql carries no error
/// code, so the rendered message is the contract. A refusal the store will not leave —
/// a range that ends before it starts, a range holding more worklog entries than one
/// page — is exit 4, so an automated caller can tell "narrow the range" from "the
/// backend is down".
fn exit_code_for(error: &ClientError) -> ExitCode {
    match error {
        ClientError::Graphql(message) if message.contains("Validation error:") => {
            ExitCode::PreconditionFailed
        }
        ClientError::Graphql(message) if message.contains("Not found:") => ExitCode::NotFound,
        _ => ExitCode::Generic,
    }
}

/// Plural-aware orphan count.
fn orphans(count: i64) -> String {
    format!("{count} orphan{}", if count == 1 { "" } else { "s" })
}

/// Plural-aware slot count.
fn slots(count: i64) -> String {
    format!("{count} slot{}", if count == 1 { "" } else { "s" })
}

/// The id prefix this surface prints, wide enough to be unique in practice and short
/// enough to retype.
fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

/// A task label for a two-column report. The title is what makes the confirmation
/// informed: these tasks were discovered by the repair, not named by the caller, so a
/// bare id would ask the operator to approve a rewrite of hours they cannot identify.
fn label(title: Option<&str>, id: &str) -> String {
    let title = title.unwrap_or("(task deleted)");
    let title: String = if title.chars().count() > 42 {
        format!("{}\u{2026}", title.chars().take(41).collect::<String>())
    } else {
        title.to_string()
    };
    format!("{title} ({})", short(id))
}

/// A local date, or a refusal naming what was wrong with it.
///
/// Checked here rather than left to the server's scalar coercion: a mistyped date is a
/// precondition failure (exit 4), and coercion would report it as a generic GraphQL
/// error (exit 1) — the code an automated caller retries on. The *ordering* of the two
/// dates is deliberately NOT checked here: that rule belongs to the use case, and two
/// implementations of it could disagree.
fn parse_date(flag: &str, value: &str) -> Result<String, String> {
    match chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        Ok(date) => Ok(date.to_string()),
        Err(_) => Err(format!("{flag} {value} is not a date (expected YYYY-MM-DD)")),
    }
}

/// `aplan slots repair --from <DATE> --to <DATE> [--confirm]`
pub fn repair(api_url: &str, json: bool, from: &str, to: &str, confirm: bool) -> ExitCode {
    let (from, to) = match (parse_date("--from", from), parse_date("--to", to)) {
        (Ok(from), Ok(to)) => (from, to),
        (Err(message), _) | (_, Err(message)) => {
            eprintln!("error: {message}");
            return ExitCode::PreconditionFailed;
        }
    };

    let client = Client::new(api_url.to_string());
    let vars = repair_orphaned_slots::Variables {
        from: from.clone(),
        to: to.clone(),
        confirm: Some(confirm),
    };

    let result = match client.run::<RepairOrphanedSlots>(vars) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return exit_code_for(&e);
        }
    };

    if json {
        if let Err(e) = print_json(&result.raw) {
            eprintln!("error writing output: {e}");
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }

    let out = &result.data.repair_orphaned_slots;

    if out.dates.is_empty() {
        println!(
            "\u{2713} nothing to repair between {} and {} \u{2014} no orphaned worklog slot",
            out.from, out.to
        );
        return ExitCode::Success;
    }

    if out.applied {
        println!(
            "\u{2713} repaired {} \u{2192} {}: {} dropped, {} written back",
            out.from,
            out.to,
            orphans(out.orphans_dropped),
            slots(out.slots_written)
        );
    } else {
        println!("\u{25c7} dry run \u{2014} nothing was written");
        println!(
            "  would repair {} \u{2192} {}: {} ({}) dropped, {} written back",
            out.from,
            out.to,
            orphans(out.orphans_dropped),
            hm(out.orphan_hours),
            slots(out.slots_written)
        );
    }

    for date in &out.dates {
        println!(
            "  {}  {} ({}) \u{2192} {} written{}",
            date.date,
            orphans(date.orphans_dropped),
            hm(date.orphan_hours),
            slots(date.slots_written),
            if date.slots_discarded > 0 {
                format!(", {} of the tasks' own replaced", date.slots_discarded)
            } else {
                String::new()
            }
        );
    }

    if !out.tasks.is_empty() {
        println!("  hours in the rebuilt half-days:");
        for task in &out.tasks {
            println!(
                "    {:<46} {} \u{2192} {}",
                label(task.task.as_ref().map(|t| t.title.as_str()), &task.task_id),
                hm(task.hours_before),
                hm(task.hours_after)
            );
        }
        let before: f64 = out.tasks.iter().map(|t| t.hours_before).sum();
        let after: f64 = out.tasks.iter().map(|t| t.hours_after).sum();
        println!("    {:<46} {} \u{2192} {}", "total", hm(before), hm(after));
        println!(
            "    {:<46} {}",
            "of which came back from the orphans",
            hm(out.orphan_hours)
        );
    }

    // A date whose orphans have no worklog entry left to rebuild from: that time is
    // discarded, not moved. Worth its own line — it is the only outcome of this verb
    // that loses hours, and the operator is the one who decides whether to accept it.
    for date in out.dates.iter().filter(|d| d.slots_written == 0) {
        println!(
            "  \u{26a0} {}: {} ({}) with no worklog entry left to rebuild from \u{2014} \
             that time is discarded, not re-attributed",
            date.date,
            orphans(date.orphans_dropped),
            hm(date.orphan_hours)
        );
    }

    if out.applied {
        println!(
            "  the timesheet drafts of those days predate this repair \u{2014} \
             re-run `aplan timesheet --date <day>`"
        );
    } else {
        println!("  to apply: add --confirm");
    }

    ExitCode::Success
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_plural_aware() {
        assert_eq!(orphans(1), "1 orphan");
        assert_eq!(orphans(16), "16 orphans");
        assert_eq!(orphans(0), "0 orphans");
        assert_eq!(slots(1), "1 slot");
        assert_eq!(slots(9), "9 slots");
    }

    #[test]
    fn an_id_is_shortened_to_a_typable_prefix() {
        assert_eq!(short("b6a62457-3a64-43f5-9a96-833c95667cc6"), "b6a62457");
    }

    #[test]
    fn a_long_title_is_truncated_but_the_id_survives() {
        let rendered = label(
            Some("Design : couche mémoire agent, lot 3 — reconstruction du temps"),
            "b6a62457-3a64-43f5-9a96-833c95667cc6",
        );
        assert!(rendered.contains("(b6a62457)"), "{rendered}");
        assert!(rendered.contains('\u{2026}'), "{rendered}");
    }

    /// A task deleted between the repair and the report still gets a line: its hours
    /// moved, and hiding the line would make the totals unexplainable.
    #[test]
    fn a_missing_task_is_named_as_deleted_rather_than_dropped_from_the_report() {
        let rendered = label(None, "b6a62457-3a64-43f5-9a96-833c95667cc6");
        assert!(rendered.contains("(task deleted)"), "{rendered}");
        assert!(rendered.contains("(b6a62457)"), "{rendered}");
    }

    #[test]
    fn a_date_is_accepted_only_in_the_iso_form() {
        assert_eq!(parse_date("--from", "2026-08-04"), Ok("2026-08-04".into()));
        assert!(parse_date("--from", "04/08/2026").is_err());
        assert!(parse_date("--to", "yesterday").is_err());
        assert!(parse_date("--to", "2026-13-01").is_err());
    }

    #[test]
    fn the_exit_code_contract_distinguishes_the_failure_modes() {
        let cases = [
            (
                "Validation error: the range ends before it starts (2026-08-10 → 2026-08-04)",
                ExitCode::PreconditionFailed,
            ),
            (
                "Validation error: the repaired range holds at least 1000 worklog entries, \
                 which is the page cap: narrow the range and correct it in several passes",
                ExitCode::PreconditionFailed,
            ),
            ("Not found: user", ExitCode::NotFound),
            ("something else entirely", ExitCode::Generic),
        ];
        for (message, expected) in cases {
            assert_eq!(
                exit_code_for(&ClientError::Graphql(message.into())),
                expected,
                "{message}"
            );
        }
    }

    /// A network failure must stay exit 1: a caller that retries on 4 would retry a
    /// refusal forever, and one that gives up on 1 would give up on an outage.
    #[test]
    fn an_unreachable_api_is_generic_not_a_precondition() {
        assert_eq!(
            exit_code_for(&ClientError::Unreachable {
                url: "http://127.0.0.1:3001/graphql".into()
            }),
            ExitCode::Generic
        );
    }
}
