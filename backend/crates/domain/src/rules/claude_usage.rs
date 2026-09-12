//! Claude token consumption, read from the Claude Code transcripts.
//!
//! The HUD's Neural budget block asks a simple question — how much have I burned in
//! the last five hours — and the transcripts answer it badly unless three traps are
//! avoided. All three are encoded here rather than at the call site, because each one
//! is a judgement about what counts, and each was measured on the real corpus
//! (661 files, 629 MB, two months) rather than assumed:
//!
//! 1. **One request emits several lines.** An API call writes one `assistant` line
//!    per content block — thinking, text, tool_use — and every one of them repeats
//!    the *same* `usage` object. 56 151 lines carry usage for 28 675 distinct
//!    requests. Summing lines inflates the burn 1.89x.
//! 2. **`cache_read` is not consumption.** It measures 4.99 billion tokens against
//!    143 million for input + output + cache_creation combined. Adding it turns the
//!    gauge into a cache-hit meter.
//! 3. **`thinking_tokens` is part of `output_tokens`**, not a cost beside it — it
//!    never exceeded output on any of the 28 637 requests checked.
//!
//! This module owns the transcript's shape as well as the rules. Only the JSON
//! *decoding* lives in infrastructure, because `serde_json` is a dev-dependency here
//! and the layer rules keep it that way; `serde` itself is allowed, so the wire
//! structs below can describe the format without importing a parser.

use std::collections::HashMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Deserialize;

/// The model name Claude Code writes on messages it fabricated locally — an error
/// placeholder, an interrupted turn. Not a model, and not billed: excluded. Doing so
/// also removes the only lines in the corpus with no `requestId` (18 of 56 151).
const SYNTHETIC_MODEL: &str = "<synthetic>";

/// Path segment marking a git worktree created by Claude Code. Work done in
/// `<repo>/.claude/worktrees/<branch>` belongs to `<repo>`: without folding, one
/// project splits into as many entries as it has had worktrees and none of them wins
/// the "top project" line.
const WORKTREE_SEGMENT: &str = "/.claude/worktrees/";

// ─── the transcript's wire shape ──────────────────────────────────────────────

/// One line of a `.jsonl` transcript. Everything not needed is left undeclared —
/// serde ignores unknown fields, and the format carries far more than this.
#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptLine {
    #[serde(rename = "type")]
    pub line_type: String,
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    #[serde(rename = "isSidechain", default)]
    pub is_sidechain: bool,
    pub message: Option<TranscriptMessage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptMessage {
    pub model: Option<String>,
    pub usage: Option<TranscriptUsage>,
}

/// The `usage` object. `iterations` is deliberately absent: it repeats the same
/// totals one level down, and reading it would be a second double count on top of
/// the per-line one.
#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptUsage {
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub cache_creation_input_tokens: i64,
    #[serde(default)]
    pub cache_read_input_tokens: i64,
    pub output_tokens_details: Option<TranscriptOutputDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptOutputDetails {
    #[serde(default)]
    pub thinking_tokens: i64,
}

// ─── the canonical unit ───────────────────────────────────────────────────────

/// One API request's consumption. The unit of the index, and the reason
/// `request_id` is a primary key rather than a column: it makes writing the same
/// line twice a no-op, so re-reading a transcript can never inflate a total.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    pub request_id: String,
    pub occurred_at: DateTime<Utc>,
    pub model: String,
    pub session_id: String,
    pub project_path: String,
    pub is_sidechain: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub thinking_tokens: i64,
}

impl UsageRecord {
    /// What counts against the declared ceiling: tokens the model had to process or
    /// produce. `cache_read` is excluded (trap 2) and `thinking` is already inside
    /// `output_tokens` (trap 3).
    pub fn consumed_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.cache_creation_input_tokens
    }
}

/// Turn a decoded transcript line into a record, or reject it.
///
/// Rejected: anything that is not an `assistant` line, anything without a `usage`
/// object, the `<synthetic>` placeholder, and any line missing one of the four fields
/// a record cannot be built without. The last case did not occur once in the corpus
/// outside `<synthetic>` — it is guarded because a format change should quietly stop
/// counting rather than panic inside a background job.
pub fn record_from_line(line: TranscriptLine) -> Option<UsageRecord> {
    if line.line_type != "assistant" {
        return None;
    }
    let message = line.message?;
    let usage = message.usage?;
    let model = message.model?;
    if model == SYNTHETIC_MODEL {
        return None;
    }

    Some(UsageRecord {
        request_id: line.request_id?,
        occurred_at: line.timestamp?,
        model,
        session_id: line.session_id.unwrap_or_default(),
        project_path: normalize_project_path(line.cwd.as_deref().unwrap_or_default()),
        is_sidechain: line.is_sidechain,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
        cache_read_input_tokens: usage.cache_read_input_tokens,
        thinking_tokens: usage
            .output_tokens_details
            .map(|d| d.thinking_tokens)
            .unwrap_or_default(),
    })
}

/// Fold a Claude Code worktree path back onto the checkout it was cut from, and
/// strip any trailing separator. Anything else is returned unchanged.
pub fn normalize_project_path(cwd: &str) -> String {
    let path = match cwd.find(WORKTREE_SEGMENT) {
        Some(at) => &cwd[..at],
        None => cwd,
    };
    path.trim_end_matches('/').to_string()
}

/// Collapse the lines of one request into a single record.
///
/// Trap 1, applied wherever records arrive as a batch. The first record of a given
/// `request_id` wins — they are byte-identical in the corpus, so "first" is a
/// tie-break, not a choice. Order is preserved so a caller can rely on it.
pub fn deduplicate_by_request(records: Vec<UsageRecord>) -> Vec<UsageRecord> {
    let mut seen = std::collections::HashSet::new();
    records
        .into_iter()
        .filter(|r| seen.insert(r.request_id.clone()))
        .collect()
}

// ─── the summary the HUD renders ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ModelUsage {
    pub model: String,
    pub tokens: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectUsage {
    pub name: String,
    pub tokens: i64,
    /// Share of the window's consumption, 0..1.
    pub ratio: f64,
}

/// Everything the Neural budget block draws.
#[derive(Debug, Clone, PartialEq)]
pub struct NeuralBudget {
    pub window_hours: i64,
    /// `input + output + cache_creation` over the window.
    pub consumed_tokens: i64,
    /// Reported beside the total, never inside it.
    pub cache_read_tokens: i64,
    /// Typed in by hand; the subscription quota is exposed by no public API.
    pub declared_ceiling: i64,
    /// `consumed / ceiling`, uncapped — a gauge that silently stops at 1.0 hides
    /// the one moment it exists to warn about. The caller clamps the bar's width,
    /// not the number.
    pub consumed_ratio: f64,
    /// One entry per day, oldest first, most recent last.
    pub per_day: Vec<i64>,
    /// Over the window, heaviest first.
    pub per_model: Vec<ModelUsage>,
    /// Over the window.
    pub top_project: Option<ProjectUsage>,
}

/// Summarise a set of records for the block.
///
/// `records` may hold duplicates and anything outside the window: this filters and
/// deduplicates, so a caller cannot get it wrong by handing over a raw query.
///
/// Everything except the sparkline is scoped to the rolling window, so the panel
/// reads as one coherent statement about the same stretch of time. The sparkline is
/// the exception by nature — it exists to give the window a background — and is cut
/// into UTC calendar days ending on `now`'s day. A UTC boundary shifts the bars by up
/// to a couple of hours against local midnight, which for a shape carries no meaning.
pub fn summarize(
    records: Vec<UsageRecord>,
    now: DateTime<Utc>,
    window_hours: i64,
    sparkline_days: i64,
    declared_ceiling: i64,
) -> NeuralBudget {
    let records = deduplicate_by_request(records);
    let window_start = now - Duration::hours(window_hours);

    let in_window: Vec<&UsageRecord> = records
        .iter()
        .filter(|r| r.occurred_at > window_start && r.occurred_at <= now)
        .collect();

    let consumed_tokens: i64 = in_window.iter().map(|r| r.consumed_tokens()).sum();
    let cache_read_tokens: i64 = in_window.iter().map(|r| r.cache_read_input_tokens).sum();

    let mut per_model = fold_by(&in_window, |r| r.model.clone());
    per_model.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut per_project = fold_by(&in_window, |r| r.project_path.clone());
    per_project.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    NeuralBudget {
        window_hours,
        consumed_tokens,
        cache_read_tokens,
        declared_ceiling,
        consumed_ratio: if declared_ceiling > 0 {
            consumed_tokens as f64 / declared_ceiling as f64
        } else {
            0.0
        },
        per_day: daily_totals(&records, now.date_naive(), sparkline_days),
        per_model: per_model
            .into_iter()
            .map(|(model, tokens)| ModelUsage { model, tokens })
            .collect(),
        top_project: per_project.first().map(|(name, tokens)| ProjectUsage {
            name: name.clone(),
            tokens: *tokens,
            ratio: if consumed_tokens > 0 {
                *tokens as f64 / consumed_tokens as f64
            } else {
                0.0
            },
        }),
    }
}

/// Consumed tokens grouped by an arbitrary key, as an unordered vector.
fn fold_by<K>(records: &[&UsageRecord], key: K) -> Vec<(String, i64)>
where
    K: Fn(&UsageRecord) -> String,
{
    let mut totals: HashMap<String, i64> = HashMap::new();
    for record in records {
        *totals.entry(key(record)).or_insert(0) += record.consumed_tokens();
    }
    totals.into_iter().collect()
}

/// One total per UTC calendar day, oldest first, `today` last. Days with nothing
/// recorded are zeros rather than gaps: the sparkline's bars must line up with the
/// days they stand for, or a quiet week reads as a short one.
fn daily_totals(records: &[UsageRecord], today: NaiveDate, days: i64) -> Vec<i64> {
    if days <= 0 {
        return Vec::new();
    }
    let first_day = today - Duration::days(days - 1);
    let mut buckets = vec![0i64; days as usize];
    for record in records {
        let day = record.occurred_at.date_naive();
        if day < first_day || day > today {
            continue;
        }
        let index = (day - first_day).num_days() as usize;
        buckets[index] += record.consumed_tokens();
    }
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(iso: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(iso).unwrap().with_timezone(&Utc)
    }

    fn record(request_id: &str, occurred_at: &str, output: i64) -> UsageRecord {
        UsageRecord {
            request_id: request_id.to_string(),
            occurred_at: at(occurred_at),
            model: "claude-opus-5".to_string(),
            session_id: "s1".to_string(),
            project_path: "/home/mbt/appfactory/aggregated_plan".to_string(),
            is_sidechain: false,
            input_tokens: 0,
            output_tokens: output,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            thinking_tokens: 0,
        }
    }

    /// A real line, trimmed of the fields this module ignores but otherwise verbatim
    /// from `~/.claude/projects/…/6c205df0-….jsonl`.
    const REAL_LINE: &str = r#"{
        "type": "assistant",
        "requestId": "req_011CewptmnnW6Qn354R43vba",
        "timestamp": "2026-09-11T14:41:07.751Z",
        "sessionId": "6c205df0-718e-4259-b0ef-a96899beba29",
        "cwd": "/home/mbt/appfactory/aggregated_plan",
        "isSidechain": false,
        "uuid": "irrelevant",
        "gitBranch": "main",
        "message": {
            "model": "claude-opus-5",
            "usage": {
                "input_tokens": 2,
                "cache_creation_input_tokens": 25177,
                "cache_read_input_tokens": 32883,
                "output_tokens": 228,
                "output_tokens_details": { "thinking_tokens": 0 },
                "service_tier": "standard",
                "iterations": [
                    { "input_tokens": 2, "output_tokens": 228,
                      "cache_read_input_tokens": 32883,
                      "cache_creation_input_tokens": 25177, "type": "message" }
                ]
            }
        }
    }"#;

    fn parse(line: &str) -> Option<UsageRecord> {
        record_from_line(serde_json::from_str(line).unwrap())
    }

    #[test]
    fn reads_a_real_assistant_line() {
        let record = parse(REAL_LINE).expect("a real usage line must be accepted");

        assert_eq!(record.request_id, "req_011CewptmnnW6Qn354R43vba");
        assert_eq!(record.occurred_at, at("2026-09-11T14:41:07.751Z"));
        assert_eq!(record.model, "claude-opus-5");
        assert_eq!(record.session_id, "6c205df0-718e-4259-b0ef-a96899beba29");
        assert_eq!(record.project_path, "/home/mbt/appfactory/aggregated_plan");
        assert_eq!(record.input_tokens, 2);
        assert_eq!(record.output_tokens, 228);
        assert_eq!(record.cache_creation_input_tokens, 25177);
        assert_eq!(record.cache_read_input_tokens, 32883);
    }

    #[test]
    fn ignores_the_iterations_array_that_repeats_the_totals() {
        // Second double count, one level down: `iterations` restates the very same
        // numbers. The struct does not declare the field at all, which is what
        // makes this unforgettable rather than merely documented.
        let record = parse(REAL_LINE).unwrap();
        assert_eq!(record.input_tokens, 2, "iterations must not be added on top");
        assert_eq!(record.output_tokens, 228);
    }

    #[test]
    fn consumed_excludes_cache_reads() {
        // Trap 2, on the real line: cache_read alone is 32883 against 25407 for
        // everything that counts. Including it would more than double this request
        // and, across the corpus, multiply the total by 36.
        let record = parse(REAL_LINE).unwrap();
        assert_eq!(record.consumed_tokens(), 2 + 228 + 25177);
    }

    #[test]
    fn consumed_does_not_add_thinking_on_top_of_output() {
        // Trap 3: thinking_tokens lives inside output_tokens_details and is a part
        // of output_tokens. Verified across 28 637 real requests — it never once
        // exceeded output. Adding it would inflate production by 38%.
        let line = REAL_LINE.replace("\"thinking_tokens\": 0", "\"thinking_tokens\": 128");
        let record = parse(&line).unwrap();

        assert_eq!(record.thinking_tokens, 128, "still recorded, for information");
        assert_eq!(record.consumed_tokens(), 2 + 228 + 25177, "but never added");
    }

    #[test]
    fn rejects_everything_that_is_not_a_billed_assistant_turn() {
        let user_line = REAL_LINE.replace("\"type\": \"assistant\"", "\"type\": \"user\"");
        assert!(parse(&user_line).is_none(), "only assistant lines carry usage");

        let synthetic = REAL_LINE.replace("claude-opus-5", "<synthetic>");
        assert!(parse(&synthetic).is_none(), "a fabricated message is not billed");

        // The 18 lines in the corpus with no requestId are all `<synthetic>`; this
        // guards the case where a format change produces one that is not.
        let no_request_id = r#"{"type":"assistant","timestamp":"2026-09-11T14:41:07.751Z",
            "message":{"model":"claude-opus-5","usage":{"output_tokens":10}}}"#;
        assert!(parse(no_request_id).is_none());

        let no_usage = r#"{"type":"assistant","requestId":"req_1",
            "timestamp":"2026-09-11T14:41:07.751Z","message":{"model":"claude-opus-5"}}"#;
        assert!(parse(no_usage).is_none());
    }

    #[test]
    fn a_worktree_belongs_to_the_checkout_it_was_cut_from() {
        assert_eq!(
            normalize_project_path("/home/mbt/appfactory/aggregated_plan/.claude/worktrees/recurrences"),
            "/home/mbt/appfactory/aggregated_plan"
        );
        assert_eq!(
            normalize_project_path("/home/mbt/appfactory/aggregated_plan/"),
            "/home/mbt/appfactory/aggregated_plan"
        );
        assert_eq!(
            normalize_project_path("/home/mbt/appfactory/aggregated_plan"),
            "/home/mbt/appfactory/aggregated_plan"
        );
    }

    #[test]
    fn one_request_counts_once_however_many_lines_it_wrote() {
        // TRAP 1, the reason this module exists. A single API call writes one
        // assistant line per content block and each repeats the identical usage
        // object: 56 151 lines for 28 675 requests in the real corpus, a 1.89x
        // inflation if they are summed.
        let duplicated = vec![
            record("req_a", "2026-09-11T14:41:07Z", 228),
            record("req_a", "2026-09-11T14:41:07Z", 228),
            record("req_b", "2026-09-11T14:42:00Z", 424),
        ];

        let kept = deduplicate_by_request(duplicated);

        assert_eq!(kept.len(), 2);
        assert_eq!(kept.iter().map(|r| r.consumed_tokens()).sum::<i64>(), 652);
    }

    #[test]
    fn summarize_deduplicates_rather_than_trusting_its_caller() {
        // The same guard one level up: handing `summarize` a raw query result must
        // not be a way to get the 1.89x back.
        let budget = summarize(
            vec![
                record("req_a", "2026-09-11T14:41:07Z", 100),
                record("req_a", "2026-09-11T14:41:07Z", 100),
            ],
            at("2026-09-11T15:00:00Z"),
            5,
            10,
            1_000,
        );

        assert_eq!(budget.consumed_tokens, 100);
    }

    #[test]
    fn the_window_is_open_at_its_start_and_closed_at_now() {
        let now = at("2026-09-11T15:00:00Z");
        let budget = summarize(
            vec![
                record("old", "2026-09-11T09:59:59Z", 1), // 5h00m01s ago — out
                record("edge", "2026-09-11T10:00:01Z", 10), // just inside
                record("now", "2026-09-11T15:00:00Z", 100), // exactly now — in
                record("future", "2026-09-11T15:00:01Z", 1000), // clock skew — out
            ],
            now,
            5,
            10,
            1_000,
        );

        assert_eq!(budget.consumed_tokens, 110);
    }

    #[test]
    fn splits_the_window_by_model_and_names_the_heaviest_project() {
        let mut opus = record("r1", "2026-09-11T14:00:00Z", 300);
        opus.project_path = "/home/mbt/appfactory/aggregated_plan".into();
        let mut sonnet = record("r2", "2026-09-11T14:10:00Z", 100);
        sonnet.model = "claude-sonnet-5".into();
        sonnet.project_path = "/home/mbt/appfactory/cicd-safteaction".into();

        let budget = summarize(
            vec![sonnet, opus],
            at("2026-09-11T15:00:00Z"),
            5,
            10,
            1_000,
        );

        assert_eq!(
            budget.per_model,
            vec![
                ModelUsage { model: "claude-opus-5".into(), tokens: 300 },
                ModelUsage { model: "claude-sonnet-5".into(), tokens: 100 },
            ],
            "heaviest first, whatever order they arrived in"
        );
        let top = budget.top_project.unwrap();
        assert_eq!(top.name, "/home/mbt/appfactory/aggregated_plan");
        assert_eq!(top.tokens, 300);
        assert!((top.ratio - 0.75).abs() < 1e-9);
    }

    #[test]
    fn the_sparkline_reaches_past_the_window_and_keeps_its_empty_days() {
        // The sparkline is the one figure not scoped to the window — it exists to
        // give the window a background. Empty days must stay as zeros, or a quiet
        // week renders as a short one and the shape lies.
        let budget = summarize(
            vec![
                record("d1", "2026-09-09T08:00:00Z", 50),
                record("d3", "2026-09-11T08:00:00Z", 70),
            ],
            at("2026-09-11T15:00:00Z"),
            5,
            3,
            1_000,
        );

        assert_eq!(budget.per_day, vec![50, 0, 70], "oldest first, today last");
        assert_eq!(budget.consumed_tokens, 0, "neither record is inside the 5h window");
    }

    #[test]
    fn the_ratio_is_not_capped_at_the_ceiling() {
        // A gauge that silently stops at 100% hides the only moment it exists for.
        let budget = summarize(
            vec![record("r1", "2026-09-11T14:00:00Z", 2_000)],
            at("2026-09-11T15:00:00Z"),
            5,
            10,
            1_000,
        );

        assert!((budget.consumed_ratio - 2.0).abs() < 1e-9);
    }

    #[test]
    fn an_unset_ceiling_yields_a_zero_ratio_rather_than_a_division_by_zero() {
        // Nothing has been calibrated yet: the burn is still true, the ratio has no
        // meaning. Zero, not infinity, and not a panic in a background job.
        let budget = summarize(
            vec![record("r1", "2026-09-11T14:00:00Z", 500)],
            at("2026-09-11T15:00:00Z"),
            5,
            10,
            0,
        );

        assert_eq!(budget.consumed_tokens, 500);
        assert_eq!(budget.consumed_ratio, 0.0);
    }

    #[test]
    fn an_empty_corpus_summarises_to_zeros_not_to_an_absence() {
        let budget = summarize(Vec::new(), at("2026-09-11T15:00:00Z"), 5, 4, 1_000);

        assert_eq!(budget.consumed_tokens, 0);
        assert_eq!(budget.cache_read_tokens, 0);
        assert_eq!(budget.per_day, vec![0, 0, 0, 0]);
        assert!(budget.per_model.is_empty());
        assert!(budget.top_project.is_none());
    }

    #[test]
    fn cache_reads_are_reported_beside_the_total_never_inside_it() {
        let mut heavy = record("r1", "2026-09-11T14:00:00Z", 228);
        heavy.cache_read_input_tokens = 32_883;
        heavy.cache_creation_input_tokens = 25_177;
        heavy.input_tokens = 2;

        let budget = summarize(vec![heavy], at("2026-09-11T15:00:00Z"), 5, 10, 1_000_000);

        assert_eq!(budget.consumed_tokens, 25_407);
        assert_eq!(budget.cache_read_tokens, 32_883);
    }

    #[test]
    fn subagent_turns_count_like_any_other() {
        // 26 645 of the corpus's 56 151 lines are sidechains. They are real tokens,
        // and leaving them out would halve the only number the block reports.
        let mut sub = record("r1", "2026-09-11T14:00:00Z", 400);
        sub.is_sidechain = true;

        let budget = summarize(vec![sub], at("2026-09-11T15:00:00Z"), 5, 10, 1_000);

        assert_eq!(budget.consumed_tokens, 400);
    }
}
