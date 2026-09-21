use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Multi-line snippet separator (same as navi for compatibility).
pub const LINE_SEPARATOR: &str = " \x15 ";

/// Replace LINE_SEPARATOR with real newlines.
pub fn snippet_to_display(s: &str) -> String {
    s.replace(LINE_SEPARATOR, "\n")
}

/// A parsed trick item: one snippet with its metadata.
#[derive(Debug, Clone)]
pub struct Item {
    pub tags: String,
    pub comment: String,
    pub snippet: String,
    /// Snippet with LINE_SEPARATOR replaced by newlines (pre-computed).
    pub snippet_display: String,
    /// Prose text between the heading and the code block (explanatory note).
    pub detail: Option<String>,
    /// Language hint from code fence (e.g. "sh", "bash", "python").
    pub lang: Option<String>,
    /// Source file path.
    pub file: Option<String>,
    /// Line number of the heading (1-indexed) in the source file.
    pub line: Option<usize>,
}

impl Item {
    pub fn new() -> Self {
        Self {
            tags: String::new(),
            comment: String::new(),
            snippet: String::new(),
            snippet_display: String::new(),
            detail: None,
            lang: None,
            file: None,
            line: None,
        }
    }

    /// Recompute `snippet_display` from `snippet`. Call after modifying snippet.
    pub fn finalize_snippet(&mut self) {
        self.snippet_display = snippet_to_display(&self.snippet);
    }

    /// Compute a hash for deduplication (based on tags + comment + snippet).
    pub fn hash_key(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.tags.trim().hash(&mut h);
        self.comment.trim().hash(&mut h);
        self.snippet.trim().hash(&mut h);
        h.finish()
    }

    /// True if this item has enough data to be displayed.
    pub fn is_valid(&self) -> bool {
        !self.comment.is_empty() && !self.snippet.trim().is_empty()
    }
}

/// Options controlling how a variable suggestion is displayed/selected.
#[derive(Debug, Clone, Default)]
pub struct SuggestionOpts {
    pub column: Option<u8>,
    pub delimiter: Option<String>,
    pub prevent_extra: bool,
    pub query: Option<String>,
    pub filter: Option<String>,
    pub preview: Option<String>,
    pub preview_window: Option<String>,
    pub header: Option<String>,
    pub header_lines: u8,
    pub map: Option<String>,
    pub expand: bool,
}

/// A variable suggestion: the command to run + display options.
pub type Suggestion = (String, Option<SuggestionOpts>);

/// Maps (tag_hash, variable_name) -> Suggestion.
///
/// Also tracks tag dependencies so variables can be inherited.
#[derive(Debug, Clone, Default)]
pub struct VariableMap {
    variables: HashMap<u64, HashMap<String, Suggestion>>,
    dependencies: HashMap<u64, Vec<u64>>,
}

impl VariableMap {
    /// Record that `tags` depends on `dep_tags` (for variable lookup fallback).
    pub fn insert_dependency(&mut self, tags: &str, dep_tags: &str) {
        let k = hash_str(tags);
        self.dependencies
            .entry(k)
            .or_default()
            .push(hash_str(dep_tags));
    }

    /// Store a variable suggestion under the given tags.
    pub fn insert_suggestion(&mut self, tags: &str, variable: &str, value: Suggestion) {
        self.variables
            .entry(hash_str(tags))
            .or_default()
            .insert(variable.to_string(), value);
    }

    /// Merge another VariableMap into this one.
    pub fn merge(&mut self, other: VariableMap) {
        for (k, vars) in other.variables {
            self.variables.entry(k).or_default().extend(vars);
        }
        for (k, deps) in other.dependencies {
            self.dependencies.entry(k).or_default().extend(deps);
        }
    }

    /// Look up a suggestion by tags + variable name, falling back to dependencies.
    pub fn get_suggestion(&self, tags: &str, variable: &str) -> Option<&Suggestion> {
        let k = hash_str(tags);

        // Direct lookup.
        if let Some(vars) = self.variables.get(&k) {
            if let Some(s) = vars.get(variable) {
                return Some(s);
            }
        }

        // Fallback to dependency tags.
        if let Some(deps) = self.dependencies.get(&k) {
            for dep_key in deps {
                if let Some(vars) = self.variables.get(dep_key) {
                    if let Some(s) = vars.get(variable) {
                        return Some(s);
                    }
                }
            }
        }

        None
    }
}

fn hash_str(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.trim().hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_valid() {
        let mut item = Item::new();
        assert!(!item.is_valid());
        item.comment = "test".into();
        assert!(!item.is_valid());
        item.snippet = "echo hi".into();
        assert!(item.is_valid());
    }

    #[test]
    fn item_hash_stable() {
        let mut a = Item::new();
        a.tags = "git".into();
        a.comment = "checkout".into();
        a.snippet = "git checkout <b>".into();

        let mut b = a.clone();
        b.lang = Some("sh".into()); // lang doesn't affect hash
        assert_eq!(a.hash_key(), b.hash_key());
    }

    #[test]
    fn variable_map_direct_lookup() {
        let mut vm = VariableMap::default();
        vm.insert_suggestion("git", "branch", ("git branch".into(), None));
        assert!(vm.get_suggestion("git", "branch").is_some());
        assert!(vm.get_suggestion("git", "remote").is_none());
        assert!(vm.get_suggestion("podman", "branch").is_none());
    }

    #[test]
    fn variable_map_dependency_fallback() {
        let mut vm = VariableMap::default();
        vm.insert_suggestion("common", "host", ("echo localhost".into(), None));
        vm.insert_dependency("network", "common");

        // Direct lookup on "common" works.
        assert!(vm.get_suggestion("common", "host").is_some());
        // Fallback from "network" to "common" works.
        assert!(vm.get_suggestion("network", "host").is_some());
        // No fallback for unrelated tags.
        assert!(vm.get_suggestion("podman", "host").is_none());
    }
}
