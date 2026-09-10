//! `aplan recurrence list|cancel|skip` — the maintenance surface for recurring-task
//! templates. `sweep` is deliberately not here: it drives the stale-occurrence use
//! case (a later task), while these three verbs only ever touch what a human names.
//!
//! ```text
//! aplan recurrence list                  # every active template
//! aplan recurrence cancel <template>      # deactivate a series, sweep its instances
//! aplan recurrence skip <task>             # cancel a single occurrence, series lives on
//! ```

use crate::client::{Client, ClientError};
use crate::lookup::{resolve_task, LookupError};
use crate::output::{print_json, ExitCode};
use crate::queries::{
    cancel_recurrence, recurrence_templates, skip_occurrence, CancelRecurrence,
    RecurrenceTemplates, SkipOccurrence,
};

/// Map a transport/GraphQL failure onto the exit-code contract.
///
/// Same technique as `slots_cmd::exit_code_for`: async-graphql carries no error
/// code, so the rendered message is the contract. `cancelRecurrence`/`skipOccurrence`
/// report a missing or foreign template/task as `AppError::NotFound` ("Not found:
/// ...") and a task with no recurrence to skip as `AppError::Validation`
/// ("Validation error: ..."); everything else is generic.
fn exit_code_for(error: &ClientError) -> ExitCode {
    match error {
        ClientError::Graphql(message) if message.contains("Not found:") => ExitCode::NotFound,
        ClientError::Graphql(message) if message.contains("Validation error:") => {
            ExitCode::PreconditionFailed
        }
        _ => ExitCode::Generic,
    }
}

/// The id prefix this surface prints, wide enough to be unique in practice and
/// short enough to retype. Mirrors `slots_cmd::short` / `reattribute_cmd::short`.
fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Resolve a user-supplied `(id, title)` needle against every candidate's id
/// prefix — case-insensitively, since a retyped short id is easy to miscase.
///
/// Pure and independent of the GraphQL transport so it can be tested directly;
/// `resolve_template` below is the only caller and supplies the live list.
fn select_by_id_prefix<'a>(
    candidates: impl Iterator<Item = (&'a str, &'a str)>,
    needle: &str,
) -> Result<String, LookupError> {
    let lowered = needle.to_lowercase();
    let matches: Vec<(&str, &str)> = candidates
        .filter(|(id, _)| id.to_lowercase().starts_with(&lowered))
        .collect();
    match matches.len() {
        0 => Err(LookupError::NotFound(needle.to_string())),
        1 => Ok(matches[0].0.to_string()),
        n => Err(LookupError::Ambiguous {
            query: needle.to_string(),
            count: n,
            candidates: matches
                .iter()
                .take(5)
                .map(|(id, title)| format!("  - {} {}", short(id), title))
                .collect::<Vec<_>>()
                .join("\n"),
        }),
    }
}

/// Resolve a `cancel` template argument (a full id or an unambiguous prefix)
/// into the full template id `cancelRecurrence` requires. The mutation itself
/// only accepts a well-formed id — there is no prefix resolution server side —
/// so this fetches the active-template list and matches client-side, exactly
/// like `memory_cmd::resolve_project` does for `--project`.
fn resolve_template(client: &Client, token: &str) -> Result<String, LookupError> {
    let needle = token.trim();
    if needle.is_empty() {
        return Err(LookupError::NotFound(token.to_string()));
    }
    let templates = client
        .run::<RecurrenceTemplates>(recurrence_templates::Variables {})?
        .data
        .recurrence_templates;
    select_by_id_prefix(
        templates.iter().map(|t| (t.id.as_str(), t.title.as_str())),
        needle,
    )
}

/// `resolve_template`'s failure worded for a template rather than a task: the
/// shared `LookupError::NotFound` message says "no task matches", which is
/// wrong here. Mirrors `memory_cmd::describe_project_error`.
fn describe_template_error(error: &LookupError) -> String {
    match error {
        LookupError::NotFound(token) => format!("no recurrence template matches `{token}`"),
        other => other.to_string(),
    }
}

/// `aplan recurrence list`
pub fn list(api_url: &str, json: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    match client.run::<RecurrenceTemplates>(recurrence_templates::Variables {}) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }

            let templates = &r.data.recurrence_templates;
            if templates.is_empty() {
                println!("no recurrence template");
                return ExitCode::Success;
            }

            println!(
                "{} recurrence template{}",
                templates.len(),
                if templates.len() == 1 { "" } else { "s" }
            );
            for t in templates {
                let bullet = if t.active { "\u{25cf}" } else { "\u{25cb}" };
                let span = match &t.ends_on {
                    Some(ends_on) => format!("{} \u{2192} {}", t.starts_on, ends_on),
                    None => format!("{} \u{2192} (open-ended)", t.starts_on),
                };
                let generated = t
                    .last_generated_through
                    .as_deref()
                    .unwrap_or("never generated");
                println!(
                    "{bullet} {}  {}  {}  generated through {}",
                    short(&t.id),
                    t.title,
                    span,
                    generated
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

/// `aplan recurrence cancel <template>`
pub fn cancel(api_url: &str, json: bool, template: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());

    let id = match resolve_template(&client, template) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("error: {}", describe_template_error(&e));
            return e.exit_code();
        }
    };

    match client.run::<CancelRecurrence>(cancel_recurrence::Variables { id }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }

            let out = &r.data.cancel_recurrence;
            println!(
                "\u{2713} recurrence cancelled \u{2014} {} instance(s) deleted, {} kept because they carry logged time",
                out.deleted, out.cancelled
            );
            if out.cancelled > 0 {
                // Kept, not incomplete: the distinction this verb exists for. A cancel
                // that only printed one total would hide the only signal that billing
                // evidence was spared, and read as an unfinished cleanup.
                println!(
                    "  kept instances are marked cancelled, not destroyed \u{2014} their logged time still reaches the invoice"
                );
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_code_for(&e)
        }
    }
}

/// `aplan recurrence skip <task>`
pub fn skip(api_url: &str, json: bool, task: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());

    let target = match resolve_task(&client, Some(task)) {
        Ok(target) => target,
        Err(e) => {
            eprintln!("error: {e}");
            return e.exit_code();
        }
    };

    match client.run::<SkipOccurrence>(skip_occurrence::Variables {
        task_id: target.id.clone(),
    }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }

            let out = &r.data.skip_occurrence;
            println!(
                "\u{2713} occurrence skipped \u{2014} {} is now {:?}",
                out.title, out.status
            );
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_code_for(&e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_shortened_to_a_typable_prefix() {
        assert_eq!(short("b6a62457-3a64-43f5-9a96-833c95667cc6"), "b6a62457");
    }

    #[test]
    fn a_full_id_resolves_to_itself() {
        let candidates = [("b6a62457-3a64-43f5-9a96-833c95667cc6", "Test recurring enum")];
        assert_eq!(
            select_by_id_prefix(candidates.into_iter(), "b6a62457-3a64-43f5-9a96-833c95667cc6")
                .expect("resolves"),
            "b6a62457-3a64-43f5-9a96-833c95667cc6"
        );
    }

    #[test]
    fn an_unambiguous_prefix_resolves_case_insensitively() {
        let candidates = [
            ("b6a62457-3a64-43f5-9a96-833c95667cc6", "Test recurring enum"),
            ("9c1f2a33-1111-2222-3333-444455556666", "Test uppercase kind"),
        ];
        assert_eq!(
            select_by_id_prefix(candidates.into_iter(), "B6A62457").expect("resolves"),
            "b6a62457-3a64-43f5-9a96-833c95667cc6"
        );
    }

    #[test]
    fn no_match_is_not_found() {
        let candidates = [("b6a62457-3a64-43f5-9a96-833c95667cc6", "Test recurring enum")];
        let err = select_by_id_prefix(candidates.into_iter(), "zzz").expect_err("no match");
        assert!(matches!(err, LookupError::NotFound(token) if token == "zzz"));
    }

    #[test]
    fn a_shared_prefix_is_ambiguous_and_lists_candidates() {
        let candidates = [
            ("aaaaaaaa-0000-0000-0000-000000000001", "Test recurring enum"),
            ("aaaaaaab-0000-0000-0000-000000000002", "Test uppercase kind"),
        ];
        match select_by_id_prefix(candidates.into_iter(), "aaaaaaa").expect_err("ambiguous") {
            LookupError::Ambiguous { count, candidates, .. } => {
                assert_eq!(count, 2);
                assert!(candidates.contains("Test recurring enum"), "{candidates}");
                assert!(candidates.contains("Test uppercase kind"), "{candidates}");
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn describing_a_not_found_template_says_template_not_task() {
        let err = LookupError::NotFound("zzz".to_string());
        assert_eq!(
            describe_template_error(&err),
            "no recurrence template matches `zzz`"
        );
    }

    #[test]
    fn the_exit_code_contract_distinguishes_the_failure_modes() {
        let cases = [
            ("Not found: RecurrenceTemplate zzz", ExitCode::NotFound),
            (
                "Validation error: skip_occurrence requires a recurring task instance",
                ExitCode::PreconditionFailed,
            ),
            ("something else entirely", ExitCode::Generic),
        ];
        for (message, expected) in cases {
            assert_eq!(exit_code_for(&ClientError::Graphql(message.into())), expected, "{message}");
        }
    }
}
