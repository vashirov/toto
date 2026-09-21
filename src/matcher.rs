//! Fuzzy matching with nucleo-matcher + rayon parallelism.
//!
//! Smart-case: case-insensitive unless query contains uppercase.
//! Supports default scoring scheme.
//! Uses rayon for parallel scoring across all CPU cores.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use rayon::prelude::*;

/// Scoring scheme for fuzzy matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Default scoring -- whitespace and delimiter boundaries.
    Default,
}

/// A scored match result.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Index into the original items list.
    pub index: usize,
    /// Fuzzy match score (higher = better).
    pub score: u32,
    /// Byte positions of matched characters in the haystack (sorted, deduped).
    pub positions: Vec<u32>,
}

/// Pre-compiled pattern for reuse across multiple score calls with the same query.
/// Avoids re-parsing the pattern when only items change (e.g. streaming).
pub struct CachedPattern {
    pattern: Pattern,
    config: Config,
    empty: bool,
}

impl CachedPattern {
    pub fn new(query: &str, scheme: Scheme) -> Self {
        if query.is_empty() {
            return Self {
                pattern: Pattern::parse("", CaseMatching::Ignore, Normalization::Smart),
                config: Config::DEFAULT,
                empty: true,
            };
        }
        let case = if query.chars().any(|c| c.is_uppercase()) {
            CaseMatching::Respect
        } else {
            CaseMatching::Ignore
        };
        let config = match scheme {
            Scheme::Default => Config::DEFAULT,
        };
        let pattern = Pattern::parse(query, case, Normalization::Smart);
        Self {
            pattern,
            config,
            empty: false,
        }
    }

    pub fn score(&self, items: &[String]) -> Vec<MatchResult> {
        if self.empty {
            return items
                .iter()
                .enumerate()
                .map(|(i, _)| MatchResult {
                    index: i,
                    score: 0,
                    positions: Vec::new(),
                })
                .collect();
        }
        score_items(&self.pattern, &self.config, items)
    }
}

/// Score items against a pre-compiled pattern.
fn score_items(pattern: &Pattern, config: &Config, items: &[String]) -> Vec<MatchResult> {
    const PAR_THRESHOLD: usize = 5_000;

    let mut results: Vec<MatchResult> = if items.len() >= PAR_THRESHOLD {
        items
            .par_iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let mut m = Matcher::new(config.clone());
                let mut buf = Vec::new();
                let mut indices = Vec::new();
                let haystack = Utf32Str::new(item, &mut buf);
                pattern
                    .indices(haystack, &mut m, &mut indices)
                    .map(|score| {
                        indices.sort_unstable();
                        indices.dedup();
                        MatchResult {
                            index: i,
                            score,
                            positions: indices,
                        }
                    })
            })
            .collect()
    } else {
        let mut m = Matcher::new(config.clone());
        let mut buf = Vec::new();
        let mut indices = Vec::new();
        items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                indices.clear();
                let haystack = Utf32Str::new(item, &mut buf);
                pattern
                    .indices(haystack, &mut m, &mut indices)
                    .map(|score| {
                        let mut pos = indices.clone();
                        pos.sort_unstable();
                        pos.dedup();
                        MatchResult {
                            index: i,
                            score,
                            positions: pos,
                        }
                    })
            })
            .collect()
    };

    results.sort_unstable_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
    results
}

/// Fuzzy-match `query` against `items`, return scored results sorted best-first.
///
/// Smart-case: case-insensitive unless query contains uppercase.
/// Uses rayon parallel iteration for large item sets.
pub fn fuzzy_match(query: &str, items: &[String], scheme: Scheme) -> Vec<MatchResult> {
    CachedPattern::new(query, scheme).score(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_all() {
        let items: Vec<String> = vec!["foo".into(), "bar".into(), "baz".into()];
        let results = fuzzy_match("", &items, Scheme::Default);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn basic_fuzzy() {
        let items: Vec<String> = vec![
            "src/main.rs".into(),
            "src/matcher.rs".into(),
            "README.md".into(),
        ];
        let results = fuzzy_match("match", &items, Scheme::Default);
        assert!(!results.is_empty());
        assert_eq!(results[0].index, 1);
    }

    #[test]
    fn smart_case_lower() {
        let items: Vec<String> = vec!["FooBar".into(), "foobar".into()];
        let results = fuzzy_match("foo", &items, Scheme::Default);
        assert_eq!(results.len(), 2); // both match, case-insensitive
    }

    #[test]
    fn smart_case_upper() {
        let items: Vec<String> = vec!["FooBar".into(), "foobar".into()];
        let results = fuzzy_match("Foo", &items, Scheme::Default);
        assert_eq!(results.len(), 1); // only FooBar matches
        assert_eq!(results[0].index, 0);
    }

    #[test]
    fn no_match() {
        let items: Vec<String> = vec!["foo".into()];
        let results = fuzzy_match("zzz", &items, Scheme::Default);
        assert!(results.is_empty());
    }

    #[test]
    fn positions_populated() {
        let items: Vec<String> = vec!["src/matcher.rs".into()];
        let results = fuzzy_match("match", &items, Scheme::Default);
        assert_eq!(results.len(), 1);
        assert!(!results[0].positions.is_empty());
        // "match" should match chars at positions 4,5,6,7,8 in "src/matcher.rs"
        assert_eq!(results[0].positions, vec![4, 5, 6, 7, 8]);
    }

    #[test]
    fn positions_empty_for_empty_query() {
        let items: Vec<String> = vec!["foo".into()];
        let results = fuzzy_match("", &items, Scheme::Default);
        assert_eq!(results.len(), 1);
        assert!(results[0].positions.is_empty());
    }
}
