//! Cross-entity text matching.
//!
//! `memories_fts` folds diacritics (`unicode61 remove_diacritics 2`), a SQLite
//! `LIKE` does not. Searching memories one way and tasks another would give a
//! query that behaves differently depending on which entity it happens to hit,
//! so the folding lives here — one rule, four entities, no I/O.

/// Lowercase and strip the diacritics FTS5 strips, so the same query reaches
/// memories and tasks alike. Deliberately narrow: the Latin-1 range covers every
/// accent this store actually holds (French, plus the odd German umlaut).
pub fn normalize(text: &str) -> String {
    text.chars()
        .flat_map(|c| c.to_lowercase())
        .map(fold_diacritic)
        .collect()
}

fn fold_diacritic(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
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
}
