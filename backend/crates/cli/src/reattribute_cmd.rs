//! `aplan reattribute` — move logged time from one task to another.
//!
//! The gap this closes: `aplan log` writes to the task it is given, and nothing
//! could correct that choice afterwards. A day recorded against the wrong task
//! reaches the timesheet and then the client invoice, so a wrong attribution is a
//! defect, not a cosmetic detail.
//!
//! ```text
//! aplan reattribute --from <task> --to <task> --date 2026-08-03            # preview
//! aplan reattribute --from <task> --to <task> --date 2026-08-03 --confirm  # apply
//! aplan reattribute --from <task> --to <task> --since D --until D
//! aplan reattribute --from <task> --to <task> --entry 7c1 --entry 9ab
//! ```
//!
//! **Preview by default.** The tokens this verb takes are fuzzy — a title fragment
//! resolves to a task the caller never named if they mistyped it — and what it
//! rewrites is billing history. So the default run resolves everything, prints both
//! task titles and the before/after hours, and writes nothing; `--confirm` then
//! applies exactly what was shown. `aplan remember --confirm` already means "yes,
//! write it" on this surface, so the flag reads the same way here.

use crate::client::{Client, ClientError};
use crate::lookup::{resolve_task, TaskRef};
use crate::output::{hm, print_json, ExitCode};
use crate::queries::{reattribute_worklog, ReattributeWorklog};

/// Map a transport/GraphQL failure onto the exit-code contract.
///
/// Same technique as `memory_cmd::exit_code_for`, and for the same reason:
/// async-graphql carries no error code, so the rendered message is the contract. A
/// missing entry or task is exit 2, an ambiguous reference exit 3, and a refusal the
/// store will not leave — same source and destination, an entry that belongs
/// elsewhere, an empty selection, a page too large — exit 4. These substrings are
/// what `AppError::NotFound`, `AppError::Ambiguous` and `AppError::Validation`
/// render.
fn exit_code_for(error: &ClientError) -> ExitCode {
    match error {
        ClientError::Graphql(message) if message.contains("Not found:") => ExitCode::NotFound,
        ClientError::Graphql(message) if message.contains("Ambiguous") => ExitCode::Ambiguous,
        ClientError::Graphql(message) if message.contains("Validation error:") => {
            ExitCode::PreconditionFailed
        }
        _ => ExitCode::Generic,
    }
}

/// A task label short enough for a two-column report, preferring the Jira key.
fn label(task: &TaskRef) -> String {
    let title: String = if task.title.chars().count() > 42 {
        format!("{}…", task.title.chars().take(41).collect::<String>())
    } else {
        task.title.clone()
    };
    format!("{title} ({})", short(&task.id))
}

/// The id prefix this surface prints, wide enough to be unique in practice and
/// short enough to retype.
fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Plural-aware entry count.
fn entries(count: usize) -> String {
    format!("{count} entr{}", if count == 1 { "y" } else { "ies" })
}

/// `aplan reattribute --from T --to T [--date D | --since D --until D | --entry ID…] [--confirm]`
#[allow(clippy::too_many_arguments)]
pub fn reattribute(
    api_url: &str,
    json: bool,
    from: &str,
    to: &str,
    date: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    entry_refs: &[String],
    confirm: bool,
) -> ExitCode {
    let client = Client::new(api_url.to_string());

    // `--date D` is the one-day shorthand; the API defaults `until` to `since`.
    let (since, until) = match date {
        Some(day) => (Some(day.to_string()), None),
        None => (since.map(String::from), until.map(String::from)),
    };

    if since.is_none() && entry_refs.is_empty() {
        eprintln!(
            "error: nothing selected\nhint: pass --date <YYYY-MM-DD> for a day, \
             --since/--until for a range, or --entry <id> for single entries"
        );
        return ExitCode::PreconditionFailed;
    }

    let source = match resolve_task(&client, Some(from)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: --from {e}");
            return e.exit_code();
        }
    };
    let destination = match resolve_task(&client, Some(to)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: --to {e}");
            return e.exit_code();
        }
    };

    let vars = reattribute_worklog::Variables {
        from_task: source.id.clone(),
        to_task: destination.id.clone(),
        entry_refs: if entry_refs.is_empty() {
            None
        } else {
            Some(entry_refs.to_vec())
        },
        since,
        until,
        confirm: Some(confirm),
    };

    let result = match client.run::<ReattributeWorklog>(vars) {
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

    let out = &result.data.reattribute_worklog_entries;
    let selected = out.selected_entries.len();
    let days = out.affected_dates.join(", ");

    if out.applied {
        println!(
            "\u{2713} moved {} to {}",
            entries(out.moved_entries as usize),
            label(&destination)
        );
        if (out.moved_entries as usize) < selected {
            println!(
                "  \u{26a0} {} were selected: the rest left {} before the move",
                entries(selected),
                short(&source.id)
            );
        }
    } else {
        println!("\u{25c7} dry run \u{2014} nothing was written");
        println!("  would move {} from {}", entries(selected), label(&source));
        println!("                    to {}", label(&destination));
    }

    println!("  days: {days}");
    println!(
        "  slots: {} dropped \u{2192} {} rebuilt",
        out.slots_discarded, out.slots_rebuilt
    );
    println!("  hours on those days:");
    println!(
        "    {:<46} {} \u{2192} {}",
        label(&source),
        hm(out.source.hours_before),
        hm(out.source.hours_after)
    );
    println!(
        "    {:<46} {} \u{2192} {}",
        label(&destination),
        hm(out.destination.hours_before),
        hm(out.destination.hours_after)
    );

    let total_before = out.source.hours_before + out.destination.hours_before;
    let total_after = out.source.hours_after + out.destination.hours_after;
    println!(
        "    {:<46} {} \u{2192} {}",
        "total",
        hm(total_before),
        hm(total_after)
    );
    // A whole half-day changing hands conserves the total. Two things legitimately
    // change it — a partial move re-spanning both sides, and a half-day whose slots the
    // entries do not account for being rebuilt from what the entries now say. Both are
    // worth a line: the operator is the one who decides whether that is what they meant.
    if hm(total_before) != hm(total_after) {
        println!(
            "  \u{26a0} the total on those days changes \u{2014} either a partial move \
             re-spans both half-days, or a half-day held slots the worklog does not \
             account for (several flushes, or a flush predating the 45-minute gap \
             rule) and is being rebuilt from the entries"
        );
    }

    if out.applied {
        println!(
            "  the timesheet draft for {days} predates this correction \u{2014} \
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
    fn hours_read_as_a_clock() {
        assert_eq!(hm(0.0), "0h00");
        assert_eq!(hm(4.5), "4h30");
        assert_eq!(hm(2.5), "2h30");
        assert_eq!(hm(1.0 / 60.0), "0h01");
        assert_eq!(hm(8.0), "8h00");
    }

    /// 4h35 is the real mis-attribution this verb was written for; it must not print
    /// as 4h34 or 4h36.
    #[test]
    fn the_real_days_total_rounds_to_the_minute() {
        assert_eq!(hm(4.0 + 35.0 / 60.0), "4h35");
    }

    #[test]
    fn a_negative_duration_never_prints_as_one() {
        assert_eq!(hm(-1.0), "0h00");
    }

    #[test]
    fn an_id_is_shortened_to_a_typable_prefix() {
        assert_eq!(short("b6a62457-3a64-43f5-9a96-833c95667cc6"), "b6a62457");
        assert_eq!(short("abc"), "abc");
    }

    #[test]
    fn a_long_title_is_truncated_but_the_id_survives() {
        let task = TaskRef {
            id: "b6a62457-3a64-43f5-9a96-833c95667cc6".into(),
            title: "Saft: basculer le temps projet vers le bon compte analytique".into(),
            source_id: None,
        };
        let rendered = label(&task);
        assert!(rendered.contains("(b6a62457)"), "{rendered}");
        assert!(rendered.contains('\u{2026}'), "{rendered}");
    }

    #[test]
    fn entry_counts_are_plural_aware() {
        assert_eq!(entries(1), "1 entry");
        assert_eq!(entries(37), "37 entries");
        assert_eq!(entries(0), "0 entries");
    }

    #[test]
    fn the_exit_code_contract_distinguishes_the_failure_modes() {
        let cases = [
            ("Not found: worklog entry `7c1`", ExitCode::NotFound),
            (
                "Ambiguous worklog entry reference `7c`: 2 matches",
                ExitCode::Ambiguous,
            ),
            (
                "Validation error: source and destination are the same task: nothing would move",
                ExitCode::PreconditionFailed,
            ),
            (
                "Validation error: no worklog entry matches the selection: nothing to move",
                ExitCode::PreconditionFailed,
            ),
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
