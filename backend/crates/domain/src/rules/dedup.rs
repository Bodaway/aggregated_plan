use std::collections::HashMap;

/// Similarity score result from comparing two tasks.
pub struct SimilarityScore {
    pub title_score: f64,
    pub assignee_match: bool,
    pub project_match: bool,
    pub overall: f64,
}

/// Confidence threshold above which two tasks are considered potential duplicates.
pub const DEDUP_CONFIDENCE_THRESHOLD: f64 = 0.7;

/// Below this, two tokens are different words rather than one word mistyped.
const TOKEN_MATCH_THRESHOLD: f64 = 0.85;

/// Confidence each matching attribute (assignee, project) adds on top of the title.
const ATTRIBUTE_BONUS: f64 = 0.1;

/// R08: Check if a Jira ticket key appears in text.
pub fn find_jira_key_in_text(jira_key: &str, text: &str) -> bool {
    text.contains(jira_key)
}

/// Normalized Levenshtein distance: 1.0 = identical, 0.0 = completely different.
pub fn normalized_levenshtein(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    // Characters, not bytes: the distance is counted in characters, and an accented
    // title measured in bytes would get an inflated score for free.
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let distance = levenshtein_distance(a, b);
    1.0 - (distance as f64 / max_len as f64)
}

/// Split a title into comparable words: case-folded, accent-stripped, punctuation
/// dropped. Digits are kept as their own tokens — `wave 0` and `wave 1` must not
/// collapse into the same title.
fn tokenize(title: &str) -> Vec<String> {
    title
        .to_lowercase()
        .chars()
        .map(fold_accent)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Map a lowercase accented Latin letter onto its unaccented counterpart.
fn fold_accent(c: char) -> char {
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

/// How much each title word says about *which* task it is, across the titles being
/// compared.
///
/// Titles in one backlog share a lot of scaffolding — a client name, a project
/// prefix, a component path — and two tasks differing only in the word naming their
/// phase are not duplicates, however much boilerplate they have in common. Weighting
/// each word by how rarely it occurs stops that scaffolding from carrying a pair
/// over the threshold. The smoothing keeps the weights near-uniform while there are
/// too few titles to tell common words from rare ones.
pub struct TitleCorpus {
    title_count: usize,
    occurrences: HashMap<String, usize>,
}

impl TitleCorpus {
    /// Count, for every word, how many of these titles it appears in.
    pub fn from_titles<'a, I: IntoIterator<Item = &'a str>>(titles: I) -> Self {
        let mut title_count = 0usize;
        let mut occurrences: HashMap<String, usize> = HashMap::new();
        for title in titles {
            title_count += 1;
            let mut counted: Vec<String> = Vec::new();
            for token in tokenize(title) {
                if counted.contains(&token) {
                    continue;
                }
                *occurrences.entry(token.clone()).or_insert(0) += 1;
                counted.push(token);
            }
        }
        Self {
            title_count,
            occurrences,
        }
    }

    /// A corpus that distinguishes nothing: every word weighs exactly the same.
    pub fn uniform() -> Self {
        Self {
            title_count: 0,
            occurrences: HashMap::new(),
        }
    }

    fn weight(&self, token: &str) -> f64 {
        let occurrences = self.occurrences.get(token).copied().unwrap_or(0);
        ((self.title_count as f64 + 1.0) / (occurrences as f64 + 1.0)).ln() + 1.0
    }
}

/// Similarity of two task titles in `[0, 1]`: a Dice coefficient over tokens paired
/// one-to-one and weighted by how distinctive each one is.
///
/// Two records of the same work are re-ordered, re-punctuated and re-qualified far
/// more often than they are mistyped, so words are compared as units — `Azure
/// Assessment` and `Assessment Azure` are the same title, which a whole-string edit
/// distance cannot see. Edit distance survives one level down, where it belongs:
/// pairing two tokens absorbs a typo inside a word.
pub fn title_similarity(a: &str, b: &str, corpus: &TitleCorpus) -> f64 {
    let tokens_a = tokenize(a);
    let tokens_b = tokenize(b);
    if tokens_a.is_empty() && tokens_b.is_empty() {
        return 1.0;
    }
    if tokens_a.is_empty() || tokens_b.is_empty() {
        return 0.0;
    }

    let mut paired = vec![false; tokens_b.len()];
    let mut shared = 0.0;
    for token in &tokens_a {
        if let Some(index) = closest_token(token, &tokens_b, &paired) {
            paired[index] = true;
            // Averaged, so a fuzzy pairing weighs the same read from either side.
            shared += (corpus.weight(token) + corpus.weight(&tokens_b[index])) / 2.0;
        }
    }

    let total: f64 = tokens_a
        .iter()
        .chain(tokens_b.iter())
        .map(|token| corpus.weight(token))
        .sum();
    if total <= 0.0 {
        return 0.0;
    }

    2.0 * shared / total
}

/// Index of the closest still-unpaired candidate, if one is close enough to be the
/// same word. An exact hit wins immediately; otherwise the best candidate above
/// `TOKEN_MATCH_THRESHOLD` is taken.
fn closest_token(token: &str, candidates: &[String], paired: &[bool]) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (index, candidate) in candidates.iter().enumerate() {
        if paired[index] {
            continue;
        }
        if candidate == token {
            return Some(index);
        }
        let score = normalized_levenshtein(token, candidate);
        if score >= TOKEN_MATCH_THRESHOLD && best.is_none_or(|(_, previous)| score > previous) {
            best = Some((index, score));
        }
    }
    best.map(|(index, _)| index)
}

// DP table indexing reads cleaner than enumerate-based iteration for textbook Levenshtein
#[allow(clippy::needless_range_loop)]
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();
    let mut matrix = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m {
        matrix[i][0] = i;
    }
    for j in 0..=n {
        matrix[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }
    matrix[m][n]
}

/// R09: Calculate similarity between two tasks.
///
/// The title carries the decision on its own: two tasks titled the same way score
/// 1.0 whether or not they have an assignee or a project. Most tasks in a personal
/// cockpit have neither, and a scheme that reserved a share of the score for those
/// attributes would put the threshold out of their reach entirely.
///
/// A matching assignee or project closes `ATTRIBUTE_BONUS` of the gap the title
/// left open, so attributes can confirm a title that is already close but can never
/// carry two unrelated titles over the threshold.
pub fn calculate_similarity(
    title_a: &str,
    title_b: &str,
    assignee_a: Option<&str>,
    assignee_b: Option<&str>,
    project_a: Option<&str>,
    project_b: Option<&str>,
    corpus: &TitleCorpus,
) -> SimilarityScore {
    let title_score = title_similarity(title_a, title_b, corpus);
    let assignee_match = match (assignee_a, assignee_b) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => false,
    };
    let project_match = match (project_a, project_b) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => false,
    };
    let bonus = if assignee_match { ATTRIBUTE_BONUS } else { 0.0 }
        + if project_match { ATTRIBUTE_BONUS } else { 0.0 };
    let overall = title_score + (1.0 - title_score) * bonus;

    SimilarityScore {
        title_score,
        assignee_match,
        project_match,
        overall,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── find_jira_key_in_text ───

    #[test]
    fn jira_key_found_in_text() {
        assert!(find_jira_key_in_text("PROJ-123", "Working on PROJ-123 today"));
    }

    #[test]
    fn jira_key_not_found_in_text() {
        assert!(!find_jira_key_in_text("PROJ-123", "Working on PROJ-456 today"));
    }

    #[test]
    fn jira_key_exact_match() {
        assert!(find_jira_key_in_text("ABC-1", "ABC-1"));
    }

    // ─── normalized_levenshtein ───

    #[test]
    fn levenshtein_identical_strings() {
        assert!((normalized_levenshtein("hello", "hello") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn levenshtein_completely_different() {
        let score = normalized_levenshtein("abc", "xyz");
        assert!(score < 0.1);
    }

    #[test]
    fn levenshtein_both_empty() {
        assert!((normalized_levenshtein("", "") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn levenshtein_one_empty() {
        assert!((normalized_levenshtein("hello", "") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn levenshtein_similar_strings() {
        let score = normalized_levenshtein("kitten", "sitting");
        // Distance = 3, max_len = 7, so score = 1 - 3/7 ≈ 0.571
        assert!(score > 0.5);
        assert!(score < 0.6);
    }

    #[test]
    fn levenshtein_denominator_counts_chars_not_bytes() {
        // "été" is 3 chars but 5 bytes; a byte denominator would inflate this to 0.6.
        let score = normalized_levenshtein("été", "ete");
        assert!((score - (1.0 - 2.0 / 3.0)).abs() < 1e-9, "got {score}");
    }

    // ─── tokenize ───

    #[test]
    fn tokenize_folds_case_accents_and_punctuation() {
        assert_eq!(
            tokenize("Réalisation de la SOLUTION"),
            vec!["realisation", "de", "la", "solution"]
        );
    }

    #[test]
    fn tokenize_drops_separators_and_keeps_digits() {
        assert_eq!(
            tokenize("Pernod Ricard — Azure (Wave 0)"),
            vec!["pernod", "ricard", "azure", "wave", "0"]
        );
    }

    // ─── title_similarity ───

    #[test]
    fn title_similarity_ignores_word_order() {
        let score = title_similarity("Azure Assessment", "Assessment Azure", &TitleCorpus::uniform());
        assert!((score - 1.0).abs() < f64::EPSILON, "got {score}");
    }

    #[test]
    fn title_similarity_ignores_punctuation_style() {
        let score = title_similarity("Pernod Ricard — Wave 0", "Pernod Ricard : Wave 0", &TitleCorpus::uniform());
        assert!((score - 1.0).abs() < f64::EPSILON, "got {score}");
    }

    #[test]
    fn title_similarity_tolerates_typo_inside_a_word() {
        // "assessement" vs "assessment" is one insertion — still the same word.
        let score = title_similarity("Azure assessement report", "Azure assessment report", &TitleCorpus::uniform());
        assert!(score > 0.95, "got {score}");
    }

    #[test]
    fn title_similarity_penalises_unmatched_qualifiers() {
        let score = title_similarity("Deploy", "Deploy the whole platform to production", &TitleCorpus::uniform());
        assert!(score < 0.5, "got {score}");
    }

    // ─── R09 regression cases from the real cockpit ───

    #[test]
    fn similarity_flags_identical_titles_without_assignee_or_project() {
        // Personal tasks carry neither assignee nor project; identical titles must
        // still clear the threshold on their own.
        let score = calculate_similarity(
            "Test uppercase kind",
            "Test uppercase kind",
            None,
            None,
            None,
            None,
            &TitleCorpus::uniform(),
        );
        assert!(
            score.overall >= DEDUP_CONFIDENCE_THRESHOLD,
            "got {}",
            score.overall
        );
    }

    #[test]
    fn similarity_flags_reordered_variant_of_same_task() {
        let score = calculate_similarity(
            "Pernod Ricard — Azure Assessment Report (Wave 0)",
            "Pernod Ricard : Assessment Azure (Tech Foundation Wave 0)",
            None,
            None,
            None,
            None,
            &TitleCorpus::uniform(),
        );
        assert!(
            score.overall >= DEDUP_CONFIDENCE_THRESHOLD,
            "got {}",
            score.overall
        );
    }

    #[test]
    fn title_similarity_discounts_words_the_whole_backlog_shares() {
        // Same component, two different phases of it. Nothing but the last word tells
        // them apart, and that word is exactly the one the backlog does not repeat.
        let backlog = [
            "eActions/ Anonymisation & pinning services - Cadrage technique",
            "eActions/ Anonymisation & pinning services - Développement",
            "eActions/ Anonymisation & pinning services - Recette",
            "eActions/ Anonymisation & pinning services - Livraison",
            "eActions/ Sync APU post-prod/ Traitement 4000 cartes",
            "Pernod Ricard — Azure Assessment Report (Wave 0)",
        ];
        let corpus = TitleCorpus::from_titles(backlog.iter().copied());

        let weighted = title_similarity(backlog[0], backlog[1], &corpus);
        let unweighted = title_similarity(backlog[0], backlog[1], &TitleCorpus::uniform());
        assert!(
            weighted < unweighted,
            "shared scaffolding should count for less: {weighted} vs {unweighted}"
        );
        assert!(
            weighted < DEDUP_CONFIDENCE_THRESHOLD,
            "got {weighted}"
        );
    }

    #[test]
    fn title_similarity_keeps_real_duplicates_above_the_bar_in_a_corpus() {
        let backlog = [
            "eActions/ Anonymisation & pinning services - Cadrage technique",
            "eActions/ Anonymisation & pinning services - Développement",
            "Pernod Ricard — Azure Assessment Report (Wave 0)",
            "Pernod Ricard : Assessment Azure (Tech Foundation Wave 0)",
        ];
        let corpus = TitleCorpus::from_titles(backlog.iter().copied());
        let score = title_similarity(backlog[2], backlog[3], &corpus);
        assert!(score >= DEDUP_CONFIDENCE_THRESHOLD, "got {score}");
    }

    #[test]
    fn uniform_corpus_weighs_every_word_the_same() {
        let corpus = TitleCorpus::uniform();
        assert!((corpus.weight("eactions") - 1.0).abs() < f64::EPSILON);
        assert!((corpus.weight("assessment") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_rejects_distinct_phases_of_one_project() {
        // Same assignee and same project, but two different pieces of work: the
        // attribute bonuses must not be able to carry an unrelated title over the bar.
        let score = calculate_similarity(
            "Déployer la solution",
            "Réalisation de la solution",
            Some("alice"),
            Some("alice"),
            Some("proj"),
            Some("proj"),
            &TitleCorpus::uniform(),
        );
        assert!(score.assignee_match);
        assert!(score.project_match);
        assert!(
            score.overall < DEDUP_CONFIDENCE_THRESHOLD,
            "got {}",
            score.overall
        );
    }

    // ─── calculate_similarity ───

    #[test]
    fn similarity_identical_all_matching() {
        let score = calculate_similarity(
            "Fix login bug",
            "Fix login bug",
            Some("alice"),
            Some("Alice"),
            Some("ProjectX"),
            Some("projectx"),
            &TitleCorpus::uniform(),
        );
        assert!((score.title_score - 1.0).abs() < f64::EPSILON);
        assert!(score.assignee_match);
        assert!(score.project_match);
        assert!((score.overall - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_identical_title_no_assignee_no_project() {
        let score = calculate_similarity(
            "Fix login bug",
            "Fix login bug",
            None,
            None,
            None,
            None,
            &TitleCorpus::uniform(),
        );
        assert!((score.title_score - 1.0).abs() < f64::EPSILON);
        assert!(!score.assignee_match);
        assert!(!score.project_match);
        // The title alone is conclusive; missing attributes must not cap the score.
        assert!((score.overall - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_different_everything() {
        let score = calculate_similarity(
            "Fix login bug",
            "Deploy to prod",
            Some("alice"),
            Some("bob"),
            Some("ProjectX"),
            Some("ProjectY"),
            &TitleCorpus::uniform(),
        );
        assert!(score.title_score < 0.5);
        assert!(!score.assignee_match);
        assert!(!score.project_match);
        assert!(score.overall < 0.3);
    }

    #[test]
    fn similarity_matching_assignee_and_project_only() {
        let score = calculate_similarity(
            "aaa",
            "zzz",
            Some("alice"),
            Some("alice"),
            Some("proj"),
            Some("proj"),
            &TitleCorpus::uniform(),
        );
        assert!(score.assignee_match);
        assert!(score.project_match);
        // Nothing in common in the titles: the attribute bonuses lift the score off
        // zero but come nowhere near the threshold.
        assert!((score.title_score - 0.0).abs() < f64::EPSILON);
        assert!((score.overall - 0.2).abs() < 1e-9, "got {}", score.overall);
        assert!(score.overall < DEDUP_CONFIDENCE_THRESHOLD);
    }

    // ─── threshold constant ───

    #[test]
    fn dedup_threshold_is_reasonable() {
        assert!((DEDUP_CONFIDENCE_THRESHOLD - 0.7).abs() < f64::EPSILON);
    }
}
