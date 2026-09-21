use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

/// Recursively find all `.trick.md` files under the given paths.
///
/// Uses the `ignore` crate (same engine as ripgrep) for parallel walking.
/// Does not skip hidden files here -- trick files may live in dotdirs.
pub fn find_trick_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for base in paths {
        if !base.exists() {
            continue;
        }
        for result in WalkBuilder::new(base)
            .hidden(false)
            .git_ignore(false)
            .follow_links(true)
            .sort_by_file_path(|a, b| a.cmp(b))
            .build()
        {
            match result {
                Ok(entry) => {
                    if entry.file_type().is_some_and(|ft| ft.is_file()) {
                        let path = entry.path();
                        if path.to_str().is_some_and(|s| s.ends_with(".trick.md")) {
                            files.push(path.to_path_buf());
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: {e}");
                }
            }
        }
    }
    files
}

/// Default data directory: `$XDG_DATA_HOME/toto`.
pub fn default_data_dir() -> Option<PathBuf> {
    etcetera::base_strategy::Xdg::new().ok().map(|xdg| {
        use etcetera::BaseStrategy;
        let mut p = xdg.data_dir();
        p.push("toto");
        p
    })
}

/// Default trick directory: `$XDG_DATA_HOME/toto/tricks`.
pub fn default_tricks_dir() -> Option<PathBuf> {
    default_data_dir().map(|mut p| {
        p.push("tricks");
        p
    })
}

/// Default config file: `$XDG_CONFIG_HOME/toto/config.toml`.
pub fn default_config_path() -> Option<PathBuf> {
    etcetera::base_strategy::Xdg::new().ok().map(|xdg| {
        use etcetera::BaseStrategy;
        let mut p = xdg.config_dir();
        p.push("toto");
        p.push("config.toml");
        p
    })
}

/// Expand `~` at the start of a path to the user's home directory.
pub fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Ok(home) = etcetera::home_dir() {
            return home.join(stripped);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dirs_exist() {
        // Just ensure these don't panic.
        let _ = default_tricks_dir();
        let _ = default_config_path();
    }

    #[test]
    fn expand_tilde_no_tilde() {
        let p = Path::new("/foo/bar");
        assert_eq!(expand_tilde(p), PathBuf::from("/foo/bar"));
    }
}
