/// Similarity score result from comparing two tasks.
pub struct SimilarityScore {
    pub title_score: f64,
    pub assignee_match: bool,
    pub project_match: bool,
    pub overall: f64,
}

/// Confidence threshold above which two tasks are considered potential duplicates.
pub const DEDUP_CONFIDENCE_THRESHOLD: f64 = 0.7;

/// R08: Check if a Jira ticket key appears in text.
pub fn find_jira_key_in_text(jira_key: &str, text: &str) -> bool {
    text.contains(jira_key)
}

/// Normalized Levenshtein distance: 1.0 = identical, 0.0 = completely different.
pub fn normalized_levenshtein(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    let distance = levenshtein_distance(a, b);
    1.0 - (distance as f64 / max_len as f64)
}

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
/// Weights: title 60%, assignee 20%, project 20%.
pub fn calculate_similarity(
    title_a: &str,
    title_b: &str,
    assignee_a: Option<&str>,
    assignee_b: Option<&str>,
    project_a: Option<&str>,
    project_b: Option<&str>,
) -> SimilarityScore {
    let title_score = normalized_levenshtein(title_a, title_b);
    let assignee_match = match (assignee_a, assignee_b) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => false,
    };
    let project_match = match (project_a, project_b) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => false,
    };
    let overall = title_score * 0.6
        + if assignee_match { 0.2 } else { 0.0 }
        + if project_match { 0.2 } else { 0.0 };

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
        );
        assert!((score.title_score - 1.0).abs() < f64::EPSILON);
        assert!(score.assignee_match);
        assert!(score.project_match);
        assert!((score.overall - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_identical_title_no_assignee_no_project() {
        let score = calculate_similarity("Fix login bug", "Fix login bug", None, None, None, None);
        assert!((score.title_score - 1.0).abs() < f64::EPSILON);
        assert!(!score.assignee_match);
        assert!(!score.project_match);
        assert!((score.overall - 0.6).abs() < f64::EPSILON);
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
        );
        assert!(!score.assignee_match || !score.project_match || score.title_score < 0.1 || {
            // title_score for "aaa" vs "zzz" is 0.0
            // overall = 0.0 * 0.6 + 0.2 + 0.2 = 0.4
            true
        });
        assert!(score.assignee_match);
        assert!(score.project_match);
        assert!((score.overall - 0.4).abs() < f64::EPSILON);
    }

    // ─── threshold constant ───

    #[test]
    fn dedup_threshold_is_reasonable() {
        assert!(DEDUP_CONFIDENCE_THRESHOLD > 0.0);
        assert!(DEDUP_CONFIDENCE_THRESHOLD < 1.0);
        assert!((DEDUP_CONFIDENCE_THRESHOLD - 0.7).abs() < f64::EPSILON);
    }
}
