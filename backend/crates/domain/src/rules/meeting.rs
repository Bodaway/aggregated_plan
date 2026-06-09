/// Returns true if `title` contains any of `patterns` (case-insensitive).
/// Empty or whitespace-only patterns are ignored (they never match).
pub fn is_excluded(title: &str, patterns: &[String]) -> bool {
    let title_lc = title.to_lowercase();
    patterns
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .any(|p| title_lc.contains(&p.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pats(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn matches_case_insensitive_substring() {
        assert!(is_excluded("Pause Midi — équipe", &pats(&["pause midi"])));
    }
    #[test]
    fn no_match_returns_false() {
        assert!(!is_excluded("Sprint review", &pats(&["pause midi", "standup"])));
    }
    #[test]
    fn blank_patterns_are_ignored() {
        assert!(!is_excluded("Anything", &pats(&["", "   "])));
    }
    #[test]
    fn empty_list_excludes_nothing() {
        assert!(!is_excluded("Anything", &[]));
    }
    #[test]
    fn matches_any_of_several() {
        assert!(is_excluded("Daily standup", &pats(&["pause midi", "standup"])));
    }
}
