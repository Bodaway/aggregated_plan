//! Cross-entity text matching.
//!
//! `memories_fts` folds diacritics (`unicode61 remove_diacritics 2`), a SQLite
//! `LIKE` does not. Searching memories one way and tasks another would give a
//! query that behaves differently depending on which entity it happens to hit,
//! so the folding lives here — one rule, four entities, no I/O.

use chrono::NaiveDate;

/// How many hits a group shows before it starts hiding them. The caller is an
/// agent: a command that prints 642 tasks is a command nobody calls twice.
pub const SEARCH_MAX_PER_GROUP: usize = 5;

/// One result, reduced to what a caller needs to decide whether to drill in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub occurred_on: NaiveDate,
}

/// One entity's results, plus the count they were cut down from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchGroup {
    pub hits: Vec<SearchHit>,
    /// How many matched, before the cap.
    pub total: usize,
}

impl SearchGroup {
    pub fn hidden(&self) -> usize {
        self.total.saturating_sub(self.hits.len())
    }
}

/// Cap a group, remembering what it dropped.
pub fn group_from(hits: Vec<SearchHit>, cap: usize) -> SearchGroup {
    let total = hits.len();
    SearchGroup {
        hits: hits.into_iter().take(cap).collect(),
        total,
    }
}

/// Lowercase and strip the diacritics FTS5 strips, so the same query reaches
/// memories and tasks alike.
///
/// Checked live against real FTS5 (`unicode61 remove_diacritics 2`, sqlite3
/// 3.53.4) — ground truth is `fts5vocab`, which shows the stored token
/// spellings; a `MATCH` query returning a row does *not* prove a character
/// was folded, it can just as easily mean both sides matched unfolded.
///
/// At parity: the acute, caron, ogonek and ring accents (Latin-1 French/
/// German, plus the Polish/Czech/Slovak/Hungarian marks in
/// [`fold_diacritic`]) fold in both engines. The **stroke and ligature
/// letters — `ł`, `ø`, `đ`, `æ`, `œ`, `ß` — fold in *neither*.** They must be
/// typed as-is on both sides; do not add them to [`fold_diacritic`]. Any
/// other character is a real, unproven gap: it would match a query on
/// memories (FTS5-backed) but not on tasks or worklog (this function).
/// Extend [`fold_diacritic`] only after confirming the new letter folds in
/// FTS5 via `fts5vocab`, not via a `MATCH` row-return.
///
/// Two passes are needed, not one: lowercasing alone does not remove every
/// diacritic. Turkish `İ` (U+0130) lowercases to `i` + COMBINING DOT ABOVE
/// (U+0307) — Unicode's own special-casing, not a bug in this code — and
/// already-decomposed input (NFD) carries its accents as separate combining
/// marks the same way. Both are stripped generically by the combining-mark
/// filter below, rather than one arm per decomposed letter.
pub fn normalize(text: &str) -> String {
    text.chars()
        .flat_map(|c| c.to_lowercase())
        .filter(|c| !is_combining_diacritic(*c))
        .map(fold_diacritic)
        .collect()
}

/// Combining diacritical marks (U+0300–U+036F): accents left dangling by
/// Unicode case folding (see [`normalize`]), or present in NFD input.
fn is_combining_diacritic(c: char) -> bool {
    ('\u{0300}'..='\u{036F}').contains(&c)
}

/// Precomposed diacritics folded to their bare letter: Latin-1 (French,
/// German) plus the acute/caron/ogonek/ring-accented Latin Extended-A
/// letters that occur in Polish, Czech, Slovak and Hungarian names. Deliberately
/// excludes the stroke and ligature letters (`ł`, `ø`, `đ`, `æ`, `œ`, `ß`) —
/// see [`normalize`] for why those fold in neither engine.
fn fold_diacritic(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ě' | 'ę' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ő' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        'ś' | 'š' => 's',
        'ť' => 't',
        'ď' => 'd',
        'ř' => 'r',
        other => other,
    }
}

/// The query, cut into normalized terms. Whitespace only — no operators, no
/// quoting: this is not FTS5's query language and must not pretend to be.
pub fn parse_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(normalize)
        .filter(|t| !t.is_empty())
        .collect()
}

/// Every term must appear. An empty term list matches nothing — returning
/// everything would turn a typo into a full table dump.
pub fn matches(haystack: &str, terms: &[String]) -> bool {
    if terms.is_empty() {
        return false;
    }
    let hay = normalize(haystack);
    terms.iter().all(|t| hay.contains(t.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_folds_case_and_diacritics() {
        assert_eq!(normalize("Réunion ÉLECTRIQUE"), "reunion electrique");
        assert_eq!(normalize("mémoire"), normalize("MEMOIRE"));
    }

    #[test]
    fn parse_terms_splits_on_whitespace_and_drops_empties() {
        assert_eq!(parse_terms("  WAF   eActions "), vec!["waf", "eactions"]);
        assert!(parse_terms("   ").is_empty());
    }

    #[test]
    fn matches_requires_every_term() {
        let terms = parse_terms("waf eactions");
        assert!(matches("Les 403 sur l'API eActions viennent du WAF Front Door", &terms));
        assert!(!matches("Le WAF de TotalEnergies", &terms));
    }

    #[test]
    fn matches_ignores_accents_on_both_sides() {
        assert!(matches("fenêtre de maintenance", &parse_terms("fenetre")));
        assert!(matches("fenetre de maintenance", &parse_terms("fenêtre")));
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(!matches("n'importe quoi", &parse_terms("")));
    }

    #[test]
    fn normalize_folds_central_and_eastern_european_diacritics() {
        // Confirmed via fts5vocab (ground truth for stored token spellings,
        // unlike a MATCH row-return): the ó and ź fold under FTS5, so they
        // must fold here too — but ł is a stroke letter, folded by neither
        // engine, and must stay ł on both sides.
        assert_eq!(normalize("łódź"), "łodz");
        assert_eq!(normalize("Košice"), "kosice");
    }

    #[test]
    fn matches_folds_central_and_eastern_european_diacritics_too() {
        assert!(matches("žluťoučký kůň", &parse_terms("zlutoucky kun")));
    }

    #[test]
    fn normalize_strips_the_combining_mark_turkish_lowercasing_leaves_behind() {
        // Rust's to_lowercase() on 'İ' (U+0130) yields 'i' + COMBINING DOT
        // ABOVE (U+0307), not plain 'i'. FTS5 folds both forms to the same
        // string; this must too, or "İstanbul" would fail to match "istanbul".
        assert_eq!(normalize("İstanbul"), normalize("istanbul"));
    }

    fn hit(title: &str, day: u32) -> SearchHit {
        SearchHit {
            id: format!("id-{day}"),
            title: title.to_string(),
            occurred_on: NaiveDate::from_ymd_opt(2026, 8, day).expect("valid date"),
        }
    }

    #[test]
    fn a_group_caps_and_says_what_it_hid() {
        let group = group_from(vec![hit("un", 1), hit("deux", 2), hit("trois", 3)], 2);

        assert_eq!(group.hits.len(), 2);
        assert_eq!(group.total, 3);
        assert_eq!(group.hidden(), 1, "la troncature n'est jamais silencieuse");
    }

    #[test]
    fn a_group_under_its_cap_hides_nothing() {
        let group = group_from(vec![hit("un", 1)], SEARCH_MAX_PER_GROUP);

        assert_eq!(group.hidden(), 0);
    }
}
