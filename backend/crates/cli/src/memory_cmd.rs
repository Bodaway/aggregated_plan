//! `aplan remember` / `aplan recall` — the write and read surfaces of the
//! semantic memory store.

use crate::cli::MemoryKindArg;
use crate::client::{Client, ClientError};
use crate::lookup::{resolve_task, LookupError};
use crate::output::{print_json, ExitCode};
use crate::queries::{
    brief as brief_query, get_memory, inbox_accept, inbox_merge, inbox_reject, list_projects,
    memory_import, memory_supersede, pending_memories, recall_memories, remember as remember_op,
    Brief as BriefQuery, GetMemory, InboxAccept, InboxMerge, InboxReject, ListProjects,
    MemoryImport, MemorySupersede, PendingMemories, RecallMemories, Remember,
};

/// Map a transport/GraphQL failure onto the exit-code contract.
///
/// GraphQL carries no error code, only a message, so a missing id is recognised
/// by the `AppError::NotFound` prefix the API renders, an ambiguous short
/// reference by the wording of the resolver, and a refused precondition by
/// [`is_precondition_failure`]. Everything else is generic.
fn exit_code_for(error: &ClientError) -> ExitCode {
    match error {
        ClientError::Graphql(message) if message.contains("Not found:") => ExitCode::NotFound,
        ClientError::Graphql(message) if message.contains("Ambiguous memory reference") => {
            ExitCode::Ambiguous
        }
        ClientError::Graphql(message) if is_precondition_failure(message) => {
            ExitCode::PreconditionFailed
        }
        _ => ExitCode::Generic,
    }
}

/// Does this message describe a state the store refuses to leave, rather than a
/// failure to reach it?
///
/// Exit 4 exists for exactly this: an automated caller — the scheduled
/// consolidation is the one this feature has — must tell "this candidate is
/// already active, skip it" from "the network broke, retry the whole run and write
/// no watermark". Both used to exit 1, which made them indistinguishable.
///
/// Matching on the rendered message is the established contract of this surface
/// (see [`exit_code_for`]): async-graphql carries no error code. These substrings
/// are therefore load-bearing — they are what `AppError::Validation`,
/// `DomainError::ValidationError`, `MemoryAlreadyInvalidated` and
/// `MemorySupersessionCycle` render, and the tests pin them verbatim.
fn is_precondition_failure(message: &str) -> bool {
    message.contains("Validation error:")
        || message.contains("is already invalidated")
        || message.contains("would create a cycle in the supersession chain")
}

/// Resolve a `--project` token into a project id: a UUID passes through, anything
/// else is matched case-insensitively against the project names (exact first,
/// then substring). Mirrors the task-lookup contract, including its exit codes.
fn resolve_project(client: &Client, token: &str) -> Result<String, LookupError> {
    let needle = token.trim();
    if needle.is_empty() {
        return Err(LookupError::NotFound(token.to_string()));
    }
    if uuid::Uuid::parse_str(needle).is_ok() {
        return Ok(needle.to_string());
    }

    let projects = client
        .run::<ListProjects>(list_projects::Variables {})?
        .data
        .projects;

    let lowered = needle.to_lowercase();
    let exact: Vec<_> = projects
        .iter()
        .filter(|p| p.name.to_lowercase() == lowered)
        .collect();
    let candidates: Vec<_> = if exact.is_empty() {
        projects
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&lowered))
            .collect()
    } else {
        exact
    };

    match candidates.len() {
        0 => Err(LookupError::NotFound(needle.to_string())),
        1 => Ok(candidates[0].id.clone()),
        n => Err(LookupError::Ambiguous {
            query: needle.to_string(),
            count: n,
            candidates: candidates
                .iter()
                .take(5)
                .map(|p| format!("  - {} {}", p.id, p.name))
                .collect::<Vec<_>>()
                .join("\n"),
        }),
    }
}

/// Lowercase display form of a codegen'd GraphQL enum (`DECISION` → `decision`).
fn enum_label<T: std::fmt::Debug>(value: &T) -> String {
    format!("{value:?}").to_lowercase()
}

/// ISO date part of an ISO-8601 timestamp; the whole string if it is shorter.
fn date_part(timestamp: &str) -> String {
    timestamp.chars().take(10).collect()
}

/// `aplan remember <title> [--kind K] [--why TEXT] [--project P] [--to PERSON]
/// [--task T] [--source-ref REF] [--confirm]`
#[allow(clippy::too_many_arguments)]
pub fn remember(
    api_url: &str,
    json: bool,
    title: &str,
    kind: &MemoryKindArg,
    why: Option<&str>,
    project: Option<&str>,
    to: &[String],
    task: Option<&str>,
    source_ref: Option<&str>,
    confirm: bool,
) -> ExitCode {
    let client = Client::new(api_url.to_string());

    let project_id = match project {
        None => None,
        Some(token) => match resolve_project(&client, token) {
            Ok(id) => Some(id),
            Err(e) => {
                eprintln!("error: {e}");
                return e.exit_code();
            }
        },
    };

    let task_id = match task {
        None => None,
        Some(token) => match resolve_task(&client, Some(token)) {
            Ok(t) => Some(t.id),
            Err(e) => {
                eprintln!("error: {e}");
                return e.exit_code();
            }
        },
    };

    let vars = remember_op::Variables {
        input: remember_op::RememberInputGql {
            kind: kind.as_graphql(),
            title: title.to_string(),
            body: why.map(String::from),
            occurred_at: None,
            source: Some(remember_op::MemorySourceGql::CLAUDE_SESSION),
            source_ref: source_ref.map(String::from),
            confirmed: Some(confirm),
            project_id,
            task_id,
            stakeholders: if to.is_empty() {
                None
            } else {
                Some(to.to_vec())
            },
        },
    };

    match client.run::<Remember>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let m = &r.data.remember;
            println!(
                "\u{270e} remembered [{}] {}",
                enum_label(&m.kind),
                m.title
            );
            println!("  {} \u{00b7} {}", m.id, enum_label(&m.status));
            if !confirm {
                println!("  to validate: `aplan inbox`");
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            // Same mapping as every other memory verb: a title the domain refuses
            // is a precondition failure (4), not an unexplained one (1).
            exit_code_for(&e)
        }
    }
}

/// `aplan recall <id>` — expand one memory. `<id>` is a full UUID or the short
/// reference the brief renders (`m:7c1`); an ambiguous prefix exits 3 rather than
/// expanding a memory the reader did not mean.
pub fn recall_one(api_url: &str, json: bool, id: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let result = match client.run::<GetMemory>(get_memory::Variables { id: id.to_string() }) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return exit_code_for(&e);
        }
    };

    let Some(m) = result.data.memory.clone() else {
        eprintln!("error: no memory matches `{id}`");
        return ExitCode::NotFound;
    };

    if json {
        if let Err(e) = print_json(&result.raw) {
            eprintln!("error writing output: {e}");
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }

    println!("[{}] {}", enum_label(&m.kind), m.title);
    println!("  id       : {}", m.id);
    println!("  occurred : {}", date_part(&m.occurred_at));
    println!("  status   : {}", enum_label(&m.status));
    if let Some(body) = &m.body {
        println!("  why      : {body}");
    }
    if !m.stakeholders.is_empty() {
        println!("  towards  : {}", m.stakeholders.join(", "));
    }
    if let Some(pid) = &m.project_id {
        println!("  project  : {pid}");
    }
    if let Some(tid) = &m.task_id {
        println!("  task     : {tid}");
    }
    if let Some(invalidated) = &m.invalidated_at {
        let by = m.superseded_by.clone().unwrap_or_else(|| "\u{2014}".into());
        println!(
            "  \u{26a0} no longer true since {} \u{2192} superseded by {}",
            date_part(invalidated),
            by
        );
    }
    ExitCode::Success
}

/// `aplan recall --q "…" [--history] [--project P] [--limit N]`
pub fn recall_search(
    api_url: &str,
    json: bool,
    query: &str,
    history: bool,
    project: Option<&str>,
    limit: i64,
) -> ExitCode {
    let client = Client::new(api_url.to_string());

    let project_id = match project {
        None => None,
        Some(token) => match resolve_project(&client, token) {
            Ok(id) => Some(id),
            Err(e) => {
                eprintln!("error: {e}");
                return e.exit_code();
            }
        },
    };

    let vars = recall_memories::Variables {
        q: query.to_string(),
        project_id,
        include_history: history,
        limit,
    };

    match client.run::<RecallMemories>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let hits = &r.data.recall;
            if hits.is_empty() {
                println!("no memory matches `{query}`");
                return ExitCode::Success;
            }
            println!("{} memor{}", hits.len(), if hits.len() == 1 { "y" } else { "ies" });
            for hit in hits {
                let m = &hit.memory;
                let stale = if m.invalidated_at.is_some() { " \u{26a0}" } else { "" };
                println!(
                    "  [{}] {} ({}, {:.2}){}",
                    enum_label(&m.kind),
                    m.title,
                    date_part(&m.occurred_at),
                    hit.score,
                    stale
                );
                println!("      {}", m.id);
            }
            println!("detail: `aplan recall <id>`");
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            // A query with nothing searchable in it (`""`, `*`, punctuation only) is
            // a refused precondition, not a transport failure.
            exit_code_for(&e)
        }
    }
}

/// `aplan inbox` — the pending validation queue.
pub fn inbox_list(api_url: &str, json: bool, limit: i64) -> ExitCode {
    let client = Client::new(api_url.to_string());
    match client.run::<PendingMemories>(pending_memories::Variables { limit }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let rows = &r.data.pending_memories;
            if rows.is_empty() {
                println!("nothing to triage");
                return ExitCode::Success;
            }
            println!("{} to triage", rows.len());
            for m in rows {
                println!(
                    "  [{}] {} ({})",
                    enum_label(&m.kind),
                    m.title,
                    date_part(&m.occurred_at)
                );
                println!("      {}", m.id);
            }
            println!(
                "accept: `aplan inbox accept <id>` \u{00b7} reject: `aplan inbox reject <id>`"
            );
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_code_for(&e)
        }
    }
}

/// `aplan inbox accept <id> [--kind K] [--force]`
pub fn inbox_accept(
    api_url: &str,
    json: bool,
    id: &str,
    kind: Option<&MemoryKindArg>,
    force: bool,
) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let vars = inbox_accept::Variables {
        id: id.to_string(),
        kind: kind.map(|k| k.as_graphql_accept()),
        force,
    };
    match client.run::<InboxAccept>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                // A blocked accept is still a precondition failure in JSON mode:
                // the payload says so, and so does the exit code.
                return if r.data.accept_memory.accepted.is_some() {
                    ExitCode::Success
                } else {
                    ExitCode::PreconditionFailed
                };
            }
            match &r.data.accept_memory.accepted {
                Some(m) => {
                    println!("\u{2713} accepted [{}] {}", enum_label(&m.kind), m.title);
                    ExitCode::Success
                }
                None => {
                    // Never a silent add: the caller has to choose.
                    eprintln!("near-duplicate of an active memory, nothing was added:");
                    for dup in &r.data.accept_memory.near_duplicates {
                        eprintln!(
                            "  [{}] {} ({})",
                            enum_label(&dup.kind),
                            dup.title,
                            date_part(&dup.occurred_at)
                        );
                        eprintln!("      {}", dup.id);
                    }
                    eprintln!(
                        "same fact, better wording?  `aplan inbox merge {id} --into <id>`\n\
                         the fact changed?           `aplan inbox supersede {id} --replaces <id>`\n\
                         genuinely new?              `aplan inbox accept {id} --force`"
                    );
                    ExitCode::PreconditionFailed
                }
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_code_for(&e)
        }
    }
}

/// `aplan inbox reject <id>`
pub fn inbox_reject(api_url: &str, json: bool, id: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    match client.run::<InboxReject>(inbox_reject::Variables { id: id.to_string() }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            println!("\u{2717} rejected: {}", r.data.reject_memory.title);
            println!("  kept as a tombstone, it will not be proposed again");
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_code_for(&e)
        }
    }
}

/// `aplan inbox merge <id> --into <id>` — same fact, better wording. One row survives.
pub fn inbox_merge(api_url: &str, json: bool, id: &str, into: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let vars = inbox_merge::Variables {
        id: id.to_string(),
        into: into.to_string(),
    };
    match client.run::<InboxMerge>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let merged = &r.data.merge_memory;
            println!("\u{2713} merged into {}", merged.survivor.id);
            println!("  {}", merged.survivor.title);
            println!("  the candidate row is gone (a merge keeps no history)");
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_code_for(&e)
        }
    }
}

/// `aplan inbox supersede <id> --replaces <old>` and `aplan memory supersede <old> --by <new>`.
/// Both write `invalidatedAt` on the old row; both rows survive.
pub fn supersede(api_url: &str, json: bool, old: &str, by: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let vars = memory_supersede::Variables {
        old: old.to_string(),
        by: by.to_string(),
    };
    match client.run::<MemorySupersede>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let outcome = &r.data.supersede_memory;
            println!("\u{2713} superseded");
            println!(
                "  no longer true: {} ({})",
                outcome.invalidated.title,
                outcome
                    .invalidated
                    .invalidated_at
                    .as_deref()
                    .map(date_part)
                    .unwrap_or_default()
            );
            println!("  now true      : {}", outcome.successor.title);
            println!("  both rows survive; `aplan recall --q \"…\" --history` shows the old one");
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_code_for(&e)
        }
    }
}

/// `aplan brief [--morning] [--project P] [--date YYYY-MM-DD]`
///
/// The rendering is produced by `domain::rules::brief` and arrives already capped
/// at 40 lines: printing it here rather than re-formatting keeps one renderer, so
/// the ceiling cannot be bypassed by the CLI.
pub fn brief(
    api_url: &str,
    json: bool,
    morning: bool,
    project: Option<&str>,
    date: Option<&str>,
) -> ExitCode {
    let client = Client::new(api_url.to_string());

    let project_id = match project {
        None => None,
        Some(token) => match resolve_project(&client, token) {
            Ok(id) => Some(id),
            Err(e) => {
                eprintln!("error: {e}");
                return e.exit_code();
            }
        },
    };

    let vars = brief_query::Variables {
        variant: if morning {
            brief_query::BriefVariantGql::MORNING
        } else {
            brief_query::BriefVariantGql::SESSION
        },
        project_id,
        date: date.map(String::from),
    };

    match client.run::<BriefQuery>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            for line in &r.data.brief.lines {
                println!("{line}");
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_code_for(&e)
        }
    }
}

/// `aplan memory import <dir>` — one-shot, idempotent, read-only on the directory.
pub fn memory_import(api_url: &str, json: bool, dir: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let vars = memory_import::Variables {
        directory: dir.to_string(),
    };
    match client.run::<MemoryImport>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let result = &r.data.import_memories;
            println!(
                "imported {} \u{00b7} skipped {}",
                result.imported_count, result.skipped_count
            );
            for m in &result.imported {
                println!("  + [{}] {}", enum_label(&m.kind), m.title);
            }
            for s in &result.skipped {
                println!("  - {} ({})", s.file_name, s.reason);
            }
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
    fn date_part_keeps_the_iso_day() {
        assert_eq!(date_part("2026-06-12T14:00:00+00:00"), "2026-06-12");
        assert_eq!(date_part("2026-06"), "2026-06");
        assert_eq!(date_part(""), "");
    }

    /// The exit-code contract, seen from the CLI. GraphQL carries no error code —
    /// only a message — so these two wordings ARE the contract between the API and
    /// the skill: they are what `AppError::NotFound` and `AppError::Ambiguous`
    /// render for a memory reference that resolved to nothing, or to several
    /// memories. If either drifts, `aplan inbox accept 7c1` starts exiting 1 and
    /// the caller can no longer tell "no such memory" from "which one did you
    /// mean".
    #[test]
    fn a_reference_that_fails_to_resolve_maps_onto_the_exit_code_contract() {
        assert_eq!(
            exit_code_for(&ClientError::Graphql(
                "Not found: memory `9ab9`".to_string()
            )),
            ExitCode::NotFound
        );
        assert_eq!(
            exit_code_for(&ClientError::Graphql(
                "Ambiguous memory reference `ab01`: 2 matches; please add more characters\n  \
                 - ab010000-0000-0000-0000-000000000001 candidat\n  \
                 - ab010000-0000-0000-0000-000000000002 fait actif"
                    .to_string()
            )),
            ExitCode::Ambiguous
        );
        assert_eq!(
            exit_code_for(&ClientError::Unreachable {
                url: "http://127.0.0.1:3001/graphql".to_string()
            }),
            ExitCode::Generic
        );
    }

    /// Exit 4, not 1, for a precondition the store refuses.
    ///
    /// The scheduled consolidation job is exactly the automated caller that must
    /// tell "this candidate is already active" from "the network broke": the first
    /// is a normal outcome to skip, the second means the whole run must be retried
    /// with no marker written. Collapsing both onto 1 makes that impossible.
    ///
    /// These are the messages `AppError` renders for the three domain refusals a
    /// caller can hit on the memory verbs, verbatim.
    #[test]
    fn a_refused_precondition_exits_four_not_one() {
        for message in [
            // `inbox accept` / `reject` on a row that is no longer pending —
            // domain::rules::memory_lifecycle::require_pending.
            "Domain error: Validation error: memory 9ab9ff00-0000-0000-0000-000000000001 \
             is active and cannot be accepted; only a pending candidate can",
            "Domain error: Validation error: memory 9ab9ff00-0000-0000-0000-000000000001 \
             is rejected and cannot be rejected; only a pending candidate can",
            // `inbox merge --into` a target that holds no truth.
            "Domain error: Validation error: memory 9ab9ff00-0000-0000-0000-000000000002 \
             is pending and cannot receive a merge; only an active memory can",
            // Re-superseding a row that already has a successor.
            "Domain error: Memory 9ab9ff00-0000-0000-0000-000000000003 is already \
             invalidated; supersede the head of its chain instead",
            // Closing a supersession loop.
            "Domain error: Superseding memory 9ab9ff00-0000-0000-0000-000000000003 by \
             9ab9ff00-0000-0000-0000-000000000004 would create a cycle in the \
             supersession chain",
            // `AppError::Validation`, raised outside the domain.
            "Validation error: nothing searchable in the query",
        ] {
            assert_eq!(
                exit_code_for(&ClientError::Graphql(message.to_string())),
                ExitCode::PreconditionFailed,
                "{message}"
            );
        }
    }

    /// A transport failure must NOT be dressed up as a precondition: the job has to
    /// know the run was never really attempted.
    #[test]
    fn a_broken_connection_is_still_a_generic_failure() {
        assert_eq!(
            exit_code_for(&ClientError::Unreachable {
                url: "http://127.0.0.1:3001/graphql".to_string()
            }),
            ExitCode::Generic
        );
        assert_eq!(
            exit_code_for(&ClientError::HttpStatus {
                status: 500,
                body: "boom".to_string()
            }),
            ExitCode::Generic
        );
        assert_eq!(
            exit_code_for(&ClientError::Graphql(
                "Repository error: Database error: disk I/O error".to_string()
            )),
            ExitCode::Generic
        );
    }

    /// The two lookup outcomes keep their own codes: a precondition check must not
    /// swallow "no such memory" (2) or "which one did you mean" (3).
    #[test]
    fn the_lookup_codes_still_win_over_the_precondition_code() {
        assert_eq!(
            exit_code_for(&ClientError::Graphql(
                "Not found: memory `9ab9`".to_string()
            )),
            ExitCode::NotFound
        );
        assert_eq!(
            exit_code_for(&ClientError::Graphql(
                "Ambiguous memory reference `ab01`: 2 matches; please add more characters"
                    .to_string()
            )),
            ExitCode::Ambiguous
        );
    }

    /// The codegen'd enums are SCREAMING_CASE; the display form is lowercase.
    #[test]
    fn enum_label_lowercases_the_graphql_variant() {
        assert_eq!(
            enum_label(&remember_op::MemoryKindGql::DECISION),
            "decision"
        );
        assert_eq!(
            enum_label(&remember_op::MemorySourceGql::CLAUDE_SESSION),
            "claude_session"
        );
    }
}
