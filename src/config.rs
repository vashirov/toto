//! TOML configuration file support.

use std::path::PathBuf;

use serde::Deserialize;

use crate::files;

/// Top-level config structure matching `config.toml`.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub paths: PathsConfig,
    pub style: StyleConfig,
    pub shell: ShellConfig,
    pub history: HistoryConfig,
    pub picker: PickerConfig,
    pub keys: KeysConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    pub enabled: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PickerConfig {
    /// Number of rows for the preview pane. Default: 7.
    pub preview_height: usize,
    /// Default window height: "full" (alternate screen), "40%", or "20" (rows). Default: "full".
    pub height: String,
    /// Override height for tricks mode.
    pub tricks_height: Option<String>,
}

impl Default for PickerConfig {
    fn default() -> Self {
        Self {
            preview_height: 7,
            height: "full".into(),
            tricks_height: None,
        }
    }
}

impl PickerConfig {
    /// Resolve height for a given mode, falling back to the default `height`.
    pub fn height_for(&self, mode: &str) -> &str {
        let override_val = match mode {
            "tricks" => self.tricks_height.as_deref(),
            _ => None,
        };
        override_val.unwrap_or(&self.height)
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    pub tricks: Vec<String>,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self { tricks: Vec::new() }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct StyleConfig {
    pub tag_color: String,
    pub comment_color: String,
    pub snippet_color: String,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            tag_color: "cyan".into(),
            comment_color: "blue".into(),
            snippet_color: "white".into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    pub command: String,
}

impl Default for ShellConfig {
    fn default() -> Self {
        let command = std::env::var("SHELL").unwrap_or_else(|_| "sh".into());
        Self { command }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct KeysConfig {
    /// Shell keybinding for tricks picker. Default: "\\C-t" (Ctrl-T).
    pub tricks: String,
}

impl Default for KeysConfig {
    fn default() -> Self {
        Self {
            tricks: "\\C-t".into(),
        }
    }
}

/// Load config from TOML file (if it exists).
pub fn load_config() -> Config {
    // Check TOTO_CONFIG env var first
    let config_path = std::env::var("TOTO_CONFIG")
        .ok()
        .map(PathBuf::from)
        .or_else(files::default_config_path);

    if let Some(path) = config_path {
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                match toml::from_str::<Config>(&contents) {
                    Ok(config) => return config,
                    Err(e) => {
                        eprintln!("Warning: failed to parse {}: {e}", path.display());
                    }
                }
            }
        }
    }

    Config::default()
}

/// Resolve trick paths from CLI flag, env var, config, or default.
pub fn resolve_trick_paths(cli_path: Option<&str>, config: &Config) -> Vec<PathBuf> {
    // 1. CLI flag wins
    if let Some(p) = cli_path {
        return p
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|s| files::expand_tilde(std::path::Path::new(s)))
            .collect();
    }

    // 2. TOTO_PATH env var
    if let Ok(env_path) = std::env::var("TOTO_PATH") {
        return env_path
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|s| files::expand_tilde(std::path::Path::new(s)))
            .collect();
    }

    // 3. Config file paths
    if !config.paths.tricks.is_empty() {
        return config
            .paths
            .tricks
            .iter()
            .map(|s| files::expand_tilde(std::path::Path::new(s)))
            .collect();
    }

    // 4. Default directory
    if let Some(default) = files::default_tricks_dir() {
        if default.exists() {
            return vec![default];
        }
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let config: Config = toml::from_str("").unwrap();
        // Shell defaults to $SHELL env var or "sh"
        assert!(!config.shell.command.is_empty());
        assert!(config.paths.tricks.is_empty());
    }

    #[test]
    fn full_config_parses() {
        let toml_str = r#"
[paths]
tricks = ["~/tricks", "/opt/tricks"]

[style]
tag_color = "green"
comment_color = "yellow"

[shell]
command = "zsh"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.paths.tricks.len(), 2);
        assert_eq!(config.style.tag_color, "green");
        assert_eq!(config.shell.command, "zsh");
    }

    #[test]
    fn partial_config_fills_defaults() {
        let config: Config = toml::from_str("[shell]\ncommand = \"fish\"\n").unwrap();
        assert_eq!(config.shell.command, "fish");
        assert_eq!(config.style.tag_color, "cyan"); // default
    }

    #[test]
    fn picker_config() {
        let config: Config = toml::from_str("[picker]\npreview_height = 10\n").unwrap();
        assert_eq!(config.picker.preview_height, 10);
    }

    #[test]
    fn picker_default() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.picker.preview_height, 7);
    }

    #[test]
    fn picker_height_for_fallback() {
        let config: Config = toml::from_str("[picker]\nheight = \"40%\"\n").unwrap();
        assert_eq!(config.picker.height_for("tricks"), "40%");
        assert_eq!(config.picker.height_for("pipe"), "40%");
    }

    #[test]
    fn picker_height_for_override() {
        let toml_str = r#"
[picker]
height = "full"
tricks_height = "full"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.picker.height_for("tricks"), "full");
        // unknown mode falls back to default
        assert_eq!(config.picker.height_for("pipe"), "full");
    }

    #[test]
    fn picker_height_for_partial_override() {
        let toml_str = "[picker]\ntricks_height = \"30%\"\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.picker.height_for("tricks"), "30%");
    }

    #[test]
    fn keys_config() {
        let config: Config = toml::from_str("[keys]\ntricks = \"\\\\C-f\"\n").unwrap();
        assert_eq!(config.keys.tricks, "\\C-f");
    }

    #[test]
    fn keys_default() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.keys.tricks, "\\C-t");
    }
}
