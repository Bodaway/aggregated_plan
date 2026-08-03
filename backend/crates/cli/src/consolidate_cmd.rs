//! `aplan consolidate` — the deterministic half of the 17:30 consolidation.
//!
//! The consolidation itself is a **scheduled Claude Code session**: the backend has
//! no model, no API key and no prompt (§6.2 of the design). These three verbs are
//! what that session drives, and `--json` on each of them is what makes it
//! drivable:
//!
//! ```text
//! aplan consolidate pending --json      # read the entries nobody consolidated
//! …propose memories through `aplan remember` / `aplan inbox`…
//! aplan consolidate mark <id>… --json   # LAST: stamp the watermark
//! aplan consolidate record-run --json   # so the brief can see the job is alive
//! ```
//!
//! The order is not cosmetic. Marking before the memories are persisted trades a
//! recoverable failure (a duplicate candidate, which the rejection tombstones stop
//! coming back) for an unrecoverable one (an entry marked and never turned into
//! anything).

use crate::client::Client;
use crate::output::{print_json, ExitCode};
use crate::queries::{
    mark_consolidated, record_consolidation_run, unconsolidated_entries, MarkConsolidated,
    RecordConsolidationRun, UnconsolidatedEntries,
};

/// ISO date-and-minute part of a timestamp, for the human rendering.
fn stamp(timestamp: &str) -> String {
    timestamp
        .chars()
        .take(16)
        .map(|c| if c == 'T' { ' ' } else { c })
        .collect()
}

/// `aplan consolidate pending [--limit N]`
///
/// Read-only, and deliberately so: this is also the reachability probe a scheduled
/// run must pass before touching anything. If the API is down, this call fails with
/// exit 1 and **no marker has been written**, so the next run picks the whole
/// backlog up (§6.2).
pub fn pending(api_url: &str, json: bool, limit: i64) -> ExitCode {
    let client = Client::new(api_url.to_string());
    match client.run::<UnconsolidatedEntries>(unconsolidated_entries::Variables { limit }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let entries = &r.data.unconsolidated_worklog_entries;
            if entries.is_empty() {
                println!("nothing to consolidate");
                return ExitCode::Success;
            }
            println!(
                "{} entr{} awaiting consolidation (oldest first)",
                entries.len(),
                if entries.len() == 1 { "y" } else { "ies" }
            );
            for entry in entries {
                let task = entry
                    .task
                    .as_ref()
                    .map(|t| t.title.as_str())
                    .unwrap_or("(task gone)");
                println!("  {} \u{00b7} {}", stamp(&entry.logged_at), task);
                println!("      {}", entry.body);
                println!("      {}", entry.id);
            }
            println!(
                "when the memories are written: `aplan consolidate mark <id>\u{2026}` \
                 then `aplan consolidate record-run`"
            );
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}

/// `aplan consolidate mark <id>...`
///
/// Idempotent: an id already consolidated moves no row and is not an error, so a
/// run that crashed after stamping can safely retry. That is why `marked` can be
/// lower than `requested`, and why both are printed.
pub fn mark(api_url: &str, json: bool, ids: &[String]) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let vars = mark_consolidated::Variables { ids: ids.to_vec() };
    match client.run::<MarkConsolidated>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let result = &r.data.mark_worklog_entries_consolidated;
            println!(
                "\u{2713} marked {}/{} consolidated at {}",
                result.marked,
                result.requested,
                stamp(&result.consolidated_at)
            );
            if result.marked < result.requested {
                println!("  the rest were already marked (or belong to someone else)");
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}

/// `aplan consolidate record-run`
///
/// Writes `memory.consolidation.last_run` in `configuration` — the key
/// `aplan brief` reads. Without this call the brief reports "jamais exécutée"
/// forever, which is precisely the silent breakage the watermark exists to expose.
pub fn record_run(api_url: &str, json: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    match client.run::<RecordConsolidationRun>(record_consolidation_run::Variables {}) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let run = &r.data.record_consolidation_run;
            println!(
                "\u{2713} consolidation run recorded at {}",
                stamp(&run.ran_at)
            );
            println!("  {} \u{00b7} shown by `aplan brief`", run.key);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_keeps_the_day_and_the_minute() {
        assert_eq!(stamp("2026-08-03T17:30:00+00:00"), "2026-08-03 17:30");
        assert_eq!(stamp("2026-08-03"), "2026-08-03");
        assert_eq!(stamp(""), "");
    }
}
