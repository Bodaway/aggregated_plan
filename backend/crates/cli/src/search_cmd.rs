//! `aplan search` — cross-entity text search across tasks, worklog entries,
//! meetings and memories the user holds.
//!
//! Matching, ordering and the per-group cap all happen server-side
//! (`domain`/`application`): this module only transports the query and
//! renders what comes back, grouped exactly as the server sent it — memories
//! by relevance, the rest by recency — never re-sorted or interleaved.

use crate::client::Client;
use crate::output::{print_json, ExitCode};
use crate::queries::{search as search_query, Search as SearchQuery};

type Hit = search_query::SearchHitFields;

/// Matches `domain::rules::brief::BRIEF_MAX_LINE_CHARS` (R55): `search`'s
/// audience is the same agent with a token budget measured in hundreds, and
/// the longest worklog body in the store runs to 887 characters — printed
/// unclamped, one group could cost roughly 4 KB for a single hit. Duplicated
/// rather than imported: this binary depends on nothing from `domain`, only
/// on GraphQL over HTTP.
const TITLE_MAX_CHARS: usize = 140;

/// Clamp a rendered title to [`TITLE_MAX_CHARS`], marking the cut — the same
/// technique `domain::rules::brief::clamp_line` uses for the brief. `search`
/// already caps how many hits are shown; this caps how wide each one is.
fn clamp_title(title: &str) -> String {
    if title.chars().count() <= TITLE_MAX_CHARS {
        return title.to_string();
    }
    let kept: String = title.chars().take(TITLE_MAX_CHARS - 1).collect();
    format!("{kept}…")
}

/// One group as rendered: its French label, how many matched before the cap,
/// and the (already capped) hits to print.
struct Group<'a> {
    label: &'static str,
    total: i64,
    hits: &'a [Hit],
}

impl Group<'_> {
    /// A group with no hits is omitted entirely, not printed as an empty
    /// heading. Truncation is announced in the count itself — `(12, 5
    /// affichés)` — the same house style `aplan brief` uses, so the header
    /// costs no extra line either way.
    fn print(&self) {
        if self.hits.is_empty() {
            return;
        }
        let shown = self.hits.len() as i64;
        if shown == self.total {
            println!("{} ({})", self.label, self.total);
        } else {
            println!("{} ({}, {} affichés)", self.label, self.total, shown);
        }
        for hit in self.hits {
            println!("  {} ({})", clamp_title(&hit.title), hit.occurred_on);
            println!("      {}", hit.id);
        }
    }
}

/// `aplan search --q <TERMS> [--limit N]`
pub fn search(api_url: &str, json: bool, q: &str, limit: i64) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let vars = search_query::Variables {
        q: q.to_string(),
        limit,
    };

    match client.run::<SearchQuery>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }

            let s = &r.data.search;
            let groups = [
                Group {
                    label: "Tâches",
                    total: s.task_total,
                    hits: &s.tasks,
                },
                Group {
                    label: "Worklog",
                    total: s.worklog_total,
                    hits: &s.worklog,
                },
                Group {
                    label: "Réunions",
                    total: s.meeting_total,
                    hits: &s.meetings,
                },
                Group {
                    label: "Mémoires",
                    total: s.memory_total,
                    hits: &s.memories,
                },
            ];

            if groups.iter().all(|g| g.hits.is_empty()) {
                println!("no match for \"{q}\"");
                return ExitCode::Success;
            }

            for group in &groups {
                group.print();
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}
