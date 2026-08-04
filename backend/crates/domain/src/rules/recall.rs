//! Recall rules: pure, I/O-free. Two responsibilities.
//!
//! 1. [`build_match_query`] turns raw user input into a safe FTS5 `MATCH`
//!    expression. Raw input must NEVER reach `MATCH`: `-`, `:` and `*` are FTS5
//!    query syntax, so the everyday vocabulary (`AP-1234`, `Cartier : certificat`)
//!    makes the search *error out*, not merely miss.
//! 2. [`score`] / [`rank`] combine the retrieval signals into one number:
//!    relevance, entity match, recency decay and kind weight.

use chrono::{DateTime, Utc};

use crate::errors::DomainError;
use crate::types::memory::{Memory, MemoryKind};
use crate::types::common::{ProjectId, TaskId};

/// Alphabetic groups at least this long get prefix-expanded (`"engagement"*`).
/// Shorter groups are left alone: prefix-expanding `sur` would match half the corpus.
pub const PREFIX_EXPANSION_MIN_LEN: usize = 4;

/// Alphabetic groups at least this long also get a de-pluralized OR branch, so
/// that a plural query reaches a singular document. Below this length, dropping
/// the final letter mangles too many real words.
pub const DEPLURALIZATION_MIN_LEN: usize = 5;

/// Age at which the recency signal has halved.
pub const RECENCY_HALF_LIFE_DAYS: f64 = 90.0;

// ─── Query building ──────────────────────────────────────────────────────────

/// Build a safe FTS5 `MATCH` expression from raw user input.
///
/// Splits on WHITESPACE only, and emits each group as one quoted phrase with its
/// internal punctuation intact. A quoted FTS5 string is a literal phrase: no
/// character inside it is read as an operator, and the tokenizer still splits it
/// into positioned tokens. So `AP-1234` becomes the phrase `"AP-1234"`, which
/// requires `AP` to be immediately followed by `1234` — splitting it into
/// `"AP" "1234"` would be an unpositioned AND, matching a memory that mentions
/// the two twenty words apart.
///
/// Two expansions apply to purely-alphabetic groups, because the tokenizer does
/// no lemmatization:
/// - prefix (`"engagement"*`) from [`PREFIX_EXPANSION_MIN_LEN`] characters, so a
///   singular query reaches a plural document;
/// - de-pluralization from [`DEPLURALIZATION_MIN_LEN`] characters when the group
///   ends in `s` or `x`, emitting `("engagements"* OR "engagement"*)`. `*` can
///   only lengthen the typed word, so without this branch a plural query would
///   never reach a singular document. The parentheses are load-bearing: FTS5
///   binds AND tighter than OR, so an unparenthesized branch would swallow the
///   neighbouring groups.
///
/// Groups are joined by an EXPLICIT `AND`. FTS5's implicit AND (a bare space) is
/// only defined *between phrases*: as soon as one group is a parenthesized OR,
/// `"wave"* ("engagements"* OR …)` raises `fts5: syntax error near "("`.
///
/// Returns `ValidationError` when no group holds an alphanumeric character
/// (`""`, `"` alone, `*`, pure punctuation): those forms raise
/// `unknown special query` or `fts5: syntax error` when passed through raw, and
/// there is nothing left to search for.
pub fn build_match_query(user_input: &str) -> Result<String, DomainError> {
    let groups: Vec<&str> = user_input
        .split_whitespace()
        .filter(|group| group.chars().any(char::is_alphanumeric))
        .collect();

    if groups.is_empty() {
        return Err(DomainError::ValidationError(format!(
            "search query has no searchable term: {user_input:?}"
        )));
    }

    Ok(groups
        .into_iter()
        .map(build_group)
        .collect::<Vec<_>>()
        .join(" AND "))
}

/// Same construction as [`build_match_query`], but joining the groups with `OR`
/// instead of `AND`: match ANY group rather than all of them.
///
/// Used by near-duplicate detection, which searches with a whole title. An `AND`
/// there would require every word of the title to be present, and so would miss
/// exactly the rewordings it exists to catch. The precision comes back from the
/// similarity threshold in `memory_lifecycle::near_duplicates`.
pub fn build_match_query_any(user_input: &str) -> Result<String, DomainError> {
    let groups: Vec<&str> = user_input
        .split_whitespace()
        .filter(|group| group.chars().any(char::is_alphanumeric))
        .collect();

    if groups.is_empty() {
        return Err(DomainError::ValidationError(format!(
            "search query has no searchable term: {user_input:?}"
        )));
    }

    Ok(groups
        .into_iter()
        .map(build_group)
        .collect::<Vec<_>>()
        .join(" OR "))
}

/// Turn one whitespace-delimited group into a quoted FTS5 phrase, with the
/// prefix and de-pluralization branches where they apply.
fn build_group(group: &str) -> String {
    let quoted = quote(group);
    let length = group.chars().count();

    // Numbers (`1234`) and mixed groups (`AP-1234`, `AP1234`) stay exact: a
    // prefix on an identifier matches unrelated identifiers.
    if !group.chars().all(char::is_alphabetic) {
        return quoted;
    }
    if length >= DEPLURALIZATION_MIN_LEN && ends_in_plural_mark(group) {
        let singular: String = group.chars().take(length - 1).collect();
        return format!("({quoted}* OR {}*)", quote(&singular));
    }
    if length >= PREFIX_EXPANSION_MIN_LEN {
        return format!("{quoted}*");
    }
    quoted
}

/// Wrap in double quotes, doubling any embedded quote — FTS5's own escape. Since
/// groups keep their punctuation, a typed `"` would otherwise close the phrase
/// early and let the rest of the group reach the parser as syntax.
fn quote(group: &str) -> String {
    format!("\"{}\"", group.replace('"', "\"\""))
}

/// French plurals end in `s` (`engagements`) or `x` (`travaux`).
fn ends_in_plural_mark(group: &str) -> bool {
    matches!(
        group.chars().next_back().map(|c| c.to_ascii_lowercase()),
        Some('s') | Some('x')
    )
}

// ─── Scoring ─────────────────────────────────────────────────────────────────

/// Relative weight of each signal in the final sum. Tunable without touching
/// the formula; no Reciprocal Rank Fusion in v1 since there is only one ranked
/// list (BM25) to fuse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecallWeights {
    pub relevance: f64,
    pub entity: f64,
    pub recency: f64,
    pub kind: f64,
}

impl Default for RecallWeights {
    fn default() -> Self {
        Self {
            relevance: 1.0,
            entity: 0.6,
            recency: 0.4,
            kind: 0.3,
        }
    }
}

/// Which entities are in focus for the current query. Empty context = no bonus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecallContext {
    pub project_id: Option<ProjectId>,
    pub task_id: Option<TaskId>,
    pub stakeholders: Vec<String>,
}

/// A memory with the score it obtained, best first once ranked.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredMemory {
    pub memory: Memory,
    pub score: f64,
}

/// Normalized relevance from a raw FTS5 `bm25()` value.
///
/// `bm25()` returns NEGATIVE values and MORE NEGATIVE means a BETTER match
/// (measured on the embedded SQLite: `-0.000001`). Relevance is therefore the
/// negation. A positive `bm25` cannot express a match, so it clamps to zero —
/// which is also what makes a sign mistake visible instead of silently
/// reversing the ranking. The magnitude has no fixed scale, so it is saturated
/// into `[0, 1)` before being summed with the other bounded signals.
pub fn relevance_from_bm25(bm25: f64) -> f64 {
    let magnitude = (-bm25).max(0.0);
    magnitude / (1.0 + magnitude)
}

/// Entity-linking bonus in `[0, 1]`: the memory's project / task / stakeholders
/// against the current context. This is the cheap replacement for a graph — the
/// entities are already relational in aplan.
pub fn entity_bonus(memory: &Memory, ctx: &RecallContext) -> f64 {
    const PROJECT_BONUS: f64 = 0.5;
    const TASK_BONUS: f64 = 0.3;
    const STAKEHOLDER_BONUS: f64 = 0.2;

    let mut bonus = 0.0;

    if let (Some(a), Some(b)) = (memory.project_id, ctx.project_id) {
        if a == b {
            bonus += PROJECT_BONUS;
        }
    }
    if let (Some(a), Some(b)) = (memory.task_id, ctx.task_id) {
        if a == b {
            bonus += TASK_BONUS;
        }
    }
    let stakeholder_match = memory.stakeholders.iter().any(|person| {
        ctx.stakeholders
            .iter()
            .any(|wanted| wanted.eq_ignore_ascii_case(person))
    });
    if stakeholder_match {
        bonus += STAKEHOLDER_BONUS;
    }

    bonus.min(1.0)
}

/// Recency signal in `(0, 1]`, halving every [`RECENCY_HALF_LIFE_DAYS`] on
/// `occurred_at` (when the thing was decided, not when aplan learned it).
/// A memory dated in the future scores as if it were dated today.
pub fn recency_decay(occurred_at: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    let age_days = (now - occurred_at).num_seconds() as f64 / 86_400.0;
    if age_days <= 0.0 {
        return 1.0;
    }
    0.5_f64.powf(age_days / RECENCY_HALF_LIFE_DAYS)
}

/// Kind weight in `[0, 1]`: decisions and commitments outrank facts and
/// preferences on a "what had we decided?" question.
pub fn kind_weight(kind: MemoryKind) -> f64 {
    match kind {
        MemoryKind::Decision => 1.0,
        MemoryKind::Commitment => 1.0,
        MemoryKind::Fact => 0.6,
        MemoryKind::Preference => 0.5,
    }
}

/// Weighted sum of the four normalized signals. `bm25` is the RAW value returned
/// by FTS5 — negative, more negative meaning a better match.
pub fn score(
    memory: &Memory,
    bm25: f64,
    ctx: &RecallContext,
    now: DateTime<Utc>,
    weights: &RecallWeights,
) -> f64 {
    weights.relevance * relevance_from_bm25(bm25)
        + weights.entity * entity_bonus(memory, ctx)
        + weights.recency * recency_decay(memory.occurred_at, now)
        + weights.kind * kind_weight(memory.kind)
}

/// Score every candidate and order them best-first. `candidates` pairs a memory
/// with its RAW `bm25()` value. Ties keep the input order (stable sort).
pub fn rank(
    candidates: Vec<(Memory, f64)>,
    ctx: &RecallContext,
    now: DateTime<Utc>,
    weights: &RecallWeights,
) -> Vec<ScoredMemory> {
    let mut scored: Vec<ScoredMemory> = candidates
        .into_iter()
        .map(|(memory, bm25)| {
            let score = score(&memory, bm25, ctx, now, weights);
            ScoredMemory { memory, score }
        })
        .collect();
    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    scored
}

#[cfg(test)]
mod build_match_query_tests {
    use super::*;

    /// The whole group stays in ONE phrase, so adjacency is required. Splitting
    /// it into `"AP" "1234"` would be an unpositioned AND. Raw, unquoted, it
    /// raises `no such column: 1234`.
    #[test]
    fn jira_key_becomes_a_single_quoted_phrase() {
        assert_eq!(build_match_query("AP-1234").unwrap(), "\"AP-1234\"");
    }

    #[test]
    fn a_jira_key_gets_no_or_branch_and_no_prefix() {
        let query = build_match_query("AP-1234").unwrap();
        assert!(!query.contains(" OR "), "identifiers must stay exact: {query}");
        assert!(!query.contains('*'), "identifiers must stay exact: {query}");
    }

    /// Raw `Cartier: certificat` raises `no such column: Cartier`. Whitespace
    /// splitting yields two phrases; the lone `:` group carries no alphanumeric
    /// and is dropped.
    #[test]
    fn client_colon_subject_becomes_two_quoted_phrases() {
        assert_eq!(
            build_match_query("Cartier : certificat").unwrap(),
            "\"Cartier\"* AND \"certificat\"*"
        );
        assert_eq!(
            build_match_query("Cartier: certificat").unwrap(),
            "\"Cartier:\" AND \"certificat\"*",
            "the colon rides along inside the phrase when it is not spaced out"
        );
    }

    #[test]
    fn wave_zero_expands_the_word_but_not_the_number() {
        assert_eq!(build_match_query("wave 0").unwrap(), "\"wave\"* AND \"0\"");
    }

    /// `*` only lengthens the typed word, so the plural needs an explicit
    /// de-pluralized branch to reach a singular document.
    #[test]
    fn a_plural_group_gets_a_depluralized_or_branch() {
        assert_eq!(
            build_match_query("engagements").unwrap(),
            "(\"engagements\"* OR \"engagement\"*)"
        );
        assert_eq!(
            build_match_query("travaux").unwrap(),
            "(\"travaux\"* OR \"travau\"*)",
            "French -x plurals get the same treatment"
        );
    }

    #[test]
    fn a_singular_group_keeps_prefix_expansion_only() {
        assert_eq!(build_match_query("engagement").unwrap(), "\"engagement\"*");
        assert!(!build_match_query("engagement").unwrap().contains(" OR "));
    }

    #[test]
    fn a_four_letter_group_ending_in_s_gets_no_or_branch() {
        // Below DEPLURALIZATION_MIN_LEN: dropping the `s` of `bras` mangles it.
        assert_eq!(build_match_query("bras").unwrap(), "\"bras\"*");
        assert_eq!(build_match_query("avis").unwrap(), "\"avis\"*");
    }

    #[test]
    fn the_or_branch_is_parenthesized_so_it_cannot_swallow_its_neighbours() {
        // FTS5 binds AND tighter than OR: without the parentheses this would
        // parse as `(wave AND engagements*) OR engagement*`.
        assert_eq!(
            build_match_query("wave engagements").unwrap(),
            "\"wave\"* AND (\"engagements\"* OR \"engagement\"*)"
        );
    }

    /// Groups are joined by an explicit `AND`, never a bare space: FTS5's
    /// implicit AND is only defined between phrases, so a space in front of a
    /// parenthesized OR raises `fts5: syntax error near "("`.
    #[test]
    fn groups_are_joined_by_an_explicit_and() {
        for input in ["wave engagements", "Cartier certificat", "AP-1234 revue"] {
            let query = build_match_query(input).unwrap();
            assert!(
                query.contains(" AND "),
                "{input:?} must use an explicit AND, got {query}"
            );
            assert!(
                !query.contains("\" \""),
                "{input:?} must not rely on the implicit AND, got {query}"
            );
        }
    }

    #[test]
    fn star_alone_is_rejected_instead_of_raising_unknown_special_query() {
        assert!(matches!(
            build_match_query("*").unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[test]
    fn not_is_quoted_instead_of_raising_a_syntax_error() {
        // Raw `NOT` raises `fts5: syntax error near "NOT"`; quoted it is a literal.
        assert_eq!(build_match_query("NOT").unwrap(), "\"NOT\"");
    }

    #[test]
    fn or_and_and_are_quoted_literals_too() {
        assert_eq!(build_match_query("OR").unwrap(), "\"OR\"");
        assert_eq!(build_match_query("AND").unwrap(), "\"AND\"");
        assert_eq!(build_match_query("NEAR").unwrap(), "\"NEAR\"*");
    }

    #[test]
    fn empty_input_is_rejected() {
        assert!(matches!(
            build_match_query("").unwrap_err(),
            DomainError::ValidationError(_)
        ));
        assert!(matches!(
            build_match_query("   \t\n ").unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[test]
    fn a_lone_double_quote_is_rejected() {
        assert!(matches!(
            build_match_query("\"").unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[test]
    fn punctuation_only_is_rejected() {
        assert!(matches!(
            build_match_query(" :-,.!?()\"' ").unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    /// Groups keep their punctuation now, so a typed `"` would close the phrase
    /// early. It must be doubled — FTS5's own escape.
    #[test]
    fn an_embedded_quote_is_escaped_by_doubling() {
        assert_eq!(build_match_query("a\"b").unwrap(), "\"a\"\"b\"");
        assert_eq!(
            build_match_query("say \"hello\" now").unwrap(),
            "\"say\" AND \"\"\"hello\"\"\" AND \"now\"",
            "the quoted word is no longer purely alphabetic, so no prefix either"
        );
    }

    #[test]
    fn short_groups_are_not_prefix_expanded() {
        assert_eq!(build_match_query("AP").unwrap(), "\"AP\"");
        assert_eq!(build_match_query("sur").unwrap(), "\"sur\"");
        assert_eq!(build_match_query("wave").unwrap(), "\"wave\"*");
    }

    #[test]
    fn mixed_alphanumeric_groups_stay_exact() {
        assert_eq!(build_match_query("AP1234").unwrap(), "\"AP1234\"");
        assert_eq!(build_match_query("1234").unwrap(), "\"1234\"");
    }

    #[test]
    fn accented_words_are_kept_whole() {
        // `unicode61 remove_diacritics 2` folds the accents at index time, so the
        // accented form must reach MATCH as one phrase.
        assert_eq!(build_match_query("limitée").unwrap(), "\"limitée\"*");
    }

    /// An elision stays inside its group: `d'archi` is one phrase, and the
    /// tokenizer splits it into the adjacent tokens `d` + `archi`.
    #[test]
    fn an_elision_stays_in_one_phrase() {
        assert_eq!(
            build_match_query("décision d'archi").unwrap(),
            "\"décision\"* AND \"d'archi\""
        );
    }

    #[test]
    fn the_any_variant_joins_with_or_and_keeps_the_same_group_form() {
        assert_eq!(
            build_match_query_any("Cartier certificat").unwrap(),
            "\"Cartier\"* OR \"certificat\"*"
        );
        assert_eq!(build_match_query_any("AP-1234").unwrap(), "\"AP-1234\"");
        assert_eq!(
            build_match_query_any("wave engagements").unwrap(),
            "\"wave\"* OR (\"engagements\"* OR \"engagement\"*)",
            "the de-pluralized branch stays parenthesized"
        );
    }

    #[test]
    fn the_any_variant_rejects_the_same_degenerate_input() {
        for raw in ["", "*", "\"", " :-, "] {
            assert!(
                matches!(
                    build_match_query_any(raw).unwrap_err(),
                    DomainError::ValidationError(_)
                ),
                "for input {raw:?}"
            );
        }
    }

    /// Split a built query into its unquoted and quoted segments, honouring the
    /// `""` escape. Anything unquoted has reached the FTS5 parser as syntax.
    fn segments(query: &str) -> (Vec<String>, Vec<String>) {
        let mut outside = Vec::new();
        let mut inside = Vec::new();
        let mut buf = String::new();
        let mut in_quotes = false;
        let mut chars = query.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '"' {
                buf.push(c);
                continue;
            }
            if in_quotes {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    buf.push('"');
                    continue;
                }
                inside.push(std::mem::take(&mut buf));
                in_quotes = false;
            } else {
                outside.push(std::mem::take(&mut buf));
                in_quotes = true;
            }
        }
        assert!(!in_quotes, "unterminated quote in {query:?}");
        outside.push(buf);
        (outside, inside)
    }

    /// Nothing but the operators this module emits (`*`, `(`, `)`, `AND`, `OR`)
    /// may appear outside quotes, whatever the user typed.
    #[test]
    fn no_hostile_input_escapes_its_quotes() {
        let hostile = [
            "AP-1234",
            "Cartier : certificat",
            "NOT wave OR *",
            "a\"b\"c",
            "\" OR x:y \"",
            "^start",
            "col:value",
            "(paren) [bracket] {brace}",
            "NEAR(a b)",
            "e -f",
            "100%",
            "engagements travaux",
        ];
        for input in hostile {
            for build in [build_match_query, build_match_query_any] {
                let query =
                    build(input).unwrap_or_else(|_| panic!("{input:?} should build a query"));
                let (outside, inside) = segments(&query);
                for segment in &outside {
                    let words = segment
                        .split([' ', '*', '(', ')'])
                        .filter(|word| !word.is_empty());
                    for word in words {
                        assert!(
                            word == "AND" || word == "OR",
                            "{word:?} of {input:?} reached the parser unquoted (query: {query})"
                        );
                    }
                }
                for phrase in &inside {
                    assert!(
                        phrase.chars().any(char::is_alphanumeric),
                        "empty phrase {phrase:?} in {input:?} (query: {query})"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod scoring_tests {
    use super::*;
    use crate::types::memory::{MemorySource, MemoryStatus, NewMemory};
    use uuid::Uuid;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-03T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn days_ago(n: i64) -> DateTime<Utc> {
        now() - chrono::Duration::days(n)
    }

    fn memory(kind: MemoryKind, occurred_at: DateTime<Utc>) -> Memory {
        Memory::new(
            Uuid::new_v4(),
            NewMemory {
                kind,
                title: "Wave 0 limited to the Microsoft AI scope".into(),
                body: None,
                occurred_at: Some(occurred_at),
                source: MemorySource::ClaudeSession,
                source_ref: None,
                status: MemoryStatus::Active,
                proposed_supersedes: None,
                project_id: None,
                task_id: None,
                stakeholders: vec![],
            },
            now(),
        )
        .expect("valid fixture")
    }

    #[test]
    fn relevance_grows_as_bm25_gets_more_negative() {
        assert!(relevance_from_bm25(-5.0) > relevance_from_bm25(-0.5));
        assert!(relevance_from_bm25(-0.5) > relevance_from_bm25(-0.000_001));
        assert!(relevance_from_bm25(-0.000_001) > 0.0);
    }

    #[test]
    fn relevance_is_bounded_and_clamps_non_negative_bm25() {
        assert_eq!(relevance_from_bm25(0.0), 0.0);
        assert_eq!(relevance_from_bm25(5.0), 0.0, "a positive bm25 is not a match");
        assert!(relevance_from_bm25(-1e12) < 1.0);
    }

    /// The trap: `bm25()` is NEGATIVE and more negative = better. This test fails
    /// if relevance is ever written as `+bm25`.
    #[test]
    fn better_bm25_match_ranks_first() {
        let strong = memory(MemoryKind::Decision, days_ago(10));
        let weak = memory(MemoryKind::Decision, days_ago(10));
        let strong_id = strong.id;
        let weak_id = weak.id;

        // `weak` is deliberately first in the input so a tie (or a flipped sign)
        // would leave it in front.
        let ranked = rank(
            vec![(weak, -0.5), (strong, -5.0)],
            &RecallContext::default(),
            now(),
            &RecallWeights::default(),
        );

        assert_eq!(
            ranked[0].memory.id, strong_id,
            "bm25 -5.0 is a BETTER match than -0.5; relevance must be -bm25()"
        );
        assert_eq!(ranked[1].memory.id, weak_id);
        assert!(ranked[0].score > ranked[1].score);
    }

    #[test]
    fn score_increases_with_match_quality_all_else_equal() {
        let m = memory(MemoryKind::Fact, days_ago(30));
        let ctx = RecallContext::default();
        let w = RecallWeights::default();
        assert!(score(&m, -5.0, &ctx, now(), &w) > score(&m, -0.5, &ctx, now(), &w));
    }

    #[test]
    fn entity_bonus_rewards_project_task_and_stakeholder_matches() {
        let project = Uuid::new_v4();
        let task = Uuid::new_v4();
        let mut m = memory(MemoryKind::Commitment, days_ago(1));
        m.project_id = Some(project);
        m.task_id = Some(task);
        m.stakeholders = vec!["Pierre".into()];

        assert_eq!(entity_bonus(&m, &RecallContext::default()), 0.0);

        let project_only = RecallContext {
            project_id: Some(project),
            ..RecallContext::default()
        };
        assert!(entity_bonus(&m, &project_only) > 0.0);

        let all = RecallContext {
            project_id: Some(project),
            task_id: Some(task),
            stakeholders: vec!["pierre".into()],
        };
        assert!(
            entity_bonus(&m, &all) > entity_bonus(&m, &project_only),
            "more entity matches must score higher"
        );
        assert!(entity_bonus(&m, &all) <= 1.0, "the bonus stays normalized");
    }

    #[test]
    fn entity_bonus_ignores_a_different_project() {
        let mut m = memory(MemoryKind::Decision, days_ago(1));
        m.project_id = Some(Uuid::new_v4());
        let other = RecallContext {
            project_id: Some(Uuid::new_v4()),
            ..RecallContext::default()
        };
        assert_eq!(entity_bonus(&m, &other), 0.0);
    }

    #[test]
    fn recency_halves_at_the_half_life() {
        assert!((recency_decay(now(), now()) - 1.0).abs() < 1e-9);
        let half = recency_decay(days_ago(RECENCY_HALF_LIFE_DAYS as i64), now());
        assert!((half - 0.5).abs() < 1e-6, "expected ~0.5, got {half}");
        assert!(recency_decay(days_ago(365), now()) < half);
    }

    #[test]
    fn a_future_occurred_at_is_treated_as_today() {
        assert_eq!(recency_decay(now() + chrono::Duration::days(7), now()), 1.0);
    }

    #[test]
    fn recent_memory_outranks_an_old_one_all_else_equal() {
        let recent = memory(MemoryKind::Decision, days_ago(2));
        let old = memory(MemoryKind::Decision, days_ago(400));
        let recent_id = recent.id;
        let ranked = rank(
            vec![(old, -1.0), (recent, -1.0)],
            &RecallContext::default(),
            now(),
            &RecallWeights::default(),
        );
        assert_eq!(ranked[0].memory.id, recent_id);
    }

    #[test]
    fn decisions_and_commitments_outweigh_facts_and_preferences() {
        assert!(kind_weight(MemoryKind::Decision) > kind_weight(MemoryKind::Fact));
        assert!(kind_weight(MemoryKind::Commitment) > kind_weight(MemoryKind::Fact));
        assert!(kind_weight(MemoryKind::Fact) > kind_weight(MemoryKind::Preference));
        assert_eq!(
            kind_weight(MemoryKind::Decision),
            kind_weight(MemoryKind::Commitment)
        );
    }

    #[test]
    fn a_decision_outranks_a_fact_at_equal_relevance_and_age() {
        let fact = memory(MemoryKind::Fact, days_ago(5));
        let decision = memory(MemoryKind::Decision, days_ago(5));
        let decision_id = decision.id;
        let ranked = rank(
            vec![(fact, -1.0), (decision, -1.0)],
            &RecallContext::default(),
            now(),
            &RecallWeights::default(),
        );
        assert_eq!(ranked[0].memory.id, decision_id);
    }

    #[test]
    fn rank_is_ordered_descending_and_stable_on_ties() {
        let a = memory(MemoryKind::Fact, days_ago(5));
        let b = memory(MemoryKind::Fact, days_ago(5));
        let (a_id, b_id) = (a.id, b.id);
        let ranked = rank(
            vec![(a, -1.0), (b, -1.0)],
            &RecallContext::default(),
            now(),
            &RecallWeights::default(),
        );
        assert_eq!(ranked[0].memory.id, a_id, "ties keep the input order");
        assert_eq!(ranked[1].memory.id, b_id);
        assert!(ranked[0].score >= ranked[1].score);
    }

    #[test]
    fn rank_of_nothing_is_nothing() {
        assert!(rank(
            vec![],
            &RecallContext::default(),
            now(),
            &RecallWeights::default()
        )
        .is_empty());
    }

    #[test]
    fn zero_weights_flatten_every_signal() {
        let m = memory(MemoryKind::Decision, days_ago(1));
        let flat = RecallWeights {
            relevance: 0.0,
            entity: 0.0,
            recency: 0.0,
            kind: 0.0,
        };
        assert_eq!(score(&m, -5.0, &RecallContext::default(), now(), &flat), 0.0);
    }
}
