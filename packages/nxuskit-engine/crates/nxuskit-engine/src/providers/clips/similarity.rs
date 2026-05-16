//! String similarity utilities for did-you-mean suggestions.
//!
//! This module provides functions to find similar strings using
//! the Jaro-Winkler similarity algorithm.

use strsim::jaro_winkler;

/// Default similarity threshold for suggestions (0.0-1.0).
pub const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.7;

/// Maximum number of suggestions to return.
pub const DEFAULT_MAX_SUGGESTIONS: usize = 3;

/// Find strings similar to the input from a list of candidates.
///
/// Uses Jaro-Winkler similarity with configurable threshold.
/// Returns up to `max_suggestions` results sorted by similarity (highest first).
///
/// # Arguments
///
/// * `input` - The string to find matches for
/// * `candidates` - List of candidate strings to search
/// * `threshold` - Minimum similarity score (0.0-1.0)
/// * `max_suggestions` - Maximum number of suggestions to return
///
/// # Example
///
/// ```
/// use nxuskit_engine::providers::clips::similarity::find_similar_strings;
///
/// let candidates = vec!["patient", "symptom", "diagnosis", "treatment"];
/// let suggestions = find_similar_strings("patiant", &candidates, 0.7, 3);
/// assert_eq!(suggestions, vec!["patient"]);
/// ```
pub fn find_similar_strings(
    input: &str,
    candidates: &[impl AsRef<str>],
    threshold: f64,
    max_suggestions: usize,
) -> Vec<String> {
    let mut scored: Vec<(f64, String)> = candidates
        .iter()
        .filter_map(|candidate| {
            let candidate = candidate.as_ref();
            let score = jaro_winkler(input, candidate);
            if score >= threshold {
                Some((score, candidate.to_string()))
            } else {
                None
            }
        })
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Take top results
    scored
        .into_iter()
        .take(max_suggestions)
        .map(|(_, s)| s)
        .collect()
}

/// Find similar strings with default threshold and max suggestions.
///
/// Uses a threshold of 0.7 and returns up to 3 suggestions.
///
/// # Example
///
/// ```
/// use nxuskit_engine::providers::clips::similarity::find_similar;
///
/// let candidates = vec!["error", "warning", "info"];
/// let suggestions = find_similar("eror", &candidates);
/// assert_eq!(suggestions, vec!["error"]);
/// ```
pub fn find_similar(input: &str, candidates: &[impl AsRef<str>]) -> Vec<String> {
    find_similar_strings(
        input,
        candidates,
        DEFAULT_SIMILARITY_THRESHOLD,
        DEFAULT_MAX_SUGGESTIONS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_similar_exact_match() {
        let candidates = vec!["patient", "symptom", "diagnosis"];
        let suggestions = find_similar("patient", &candidates);
        assert_eq!(suggestions, vec!["patient"]);
    }

    #[test]
    fn test_find_similar_typo() {
        let candidates = vec!["patient", "symptom", "diagnosis", "treatment"];
        let suggestions = find_similar("patiant", &candidates);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0], "patient");
    }

    #[test]
    fn test_find_similar_no_match() {
        let candidates = vec!["patient", "symptom", "diagnosis"];
        let suggestions = find_similar("xyz123", &candidates);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_find_similar_multiple_matches() {
        let candidates = vec!["test", "tests", "testing", "taste", "text"];
        let suggestions = find_similar_strings("test", &candidates, 0.8, 5);
        // Should include "test" (exact), "tests" (high similarity), possibly "testing"
        assert!(suggestions.contains(&"test".to_string()));
        assert!(suggestions.len() <= 5);
    }

    #[test]
    fn test_find_similar_respects_max() {
        let candidates = vec!["aa", "ab", "ac", "ad", "ae"];
        let suggestions = find_similar_strings("a", &candidates, 0.5, 2);
        assert!(suggestions.len() <= 2);
    }

    #[test]
    fn test_find_similar_case_sensitive() {
        let candidates = vec!["Patient", "PATIENT", "patient"];
        let suggestions = find_similar("patient", &candidates);
        // Jaro-Winkler is case-sensitive, so exact match should score highest
        assert!(suggestions.contains(&"patient".to_string()));
    }

    #[test]
    fn test_find_similar_empty_candidates() {
        let candidates: Vec<&str> = vec![];
        let suggestions = find_similar("test", &candidates);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_find_similar_empty_input() {
        let candidates = vec!["patient", "symptom"];
        let suggestions = find_similar("", &candidates);
        // Empty string has low similarity with all candidates
        assert!(suggestions.is_empty());
    }
}
