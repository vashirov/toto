//! Terminal color theme built from the `[style]` config values.
//!
//! Honors the `NO_COLOR` environment variable: when it is set to a non-empty
//! value, all color and attribute codes are suppressed except the reset
//! sequence, and the selected row falls back to reverse video.

use crate::config::StyleConfig;

pub const RESET: &str = "\x1b[0m";

/// ANSI codes for one rendering style of the picker.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub tag: &'static str,
    pub comment: &'static str,
    pub snippet: &'static str,
    /// Detail prose in the preview pane.
    pub detail: &'static str,
    /// Prompt and selected-row text.
    pub accent: &'static str,
    /// Matched characters in the query.
    pub matches: &'static str,
    pub bold: &'static str,
    pub dim: &'static str,
    /// Background of the selected row.
    pub selected_bg: &'static str,
    pub reset: &'static str,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            tag: "\x1b[36m",     // cyan
            comment: "\x1b[34m", // blue
            snippet: "\x1b[2m",  // dim
            detail: "\x1b[38;5;245m",
            accent: "\x1b[32m",  // green
            matches: "\x1b[33m", // yellow
            bold: "\x1b[1m",
            dim: "\x1b[2m",
            selected_bg: "\x1b[48;5;236m",
            reset: RESET,
        }
    }
}

pub fn no_color() -> bool {
    std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty())
}

/// Map a color name to its ANSI code. None for unknown names.
fn color_code(name: &str) -> Option<&'static str> {
    match name {
        "" | "default" | "none" => Some(""),
        "black" => Some("\x1b[30m"),
        "red" => Some("\x1b[31m"),
        "green" => Some("\x1b[32m"),
        "yellow" => Some("\x1b[33m"),
        "blue" => Some("\x1b[34m"),
        "magenta" => Some("\x1b[35m"),
        "cyan" => Some("\x1b[36m"),
        "white" => Some("\x1b[37m"),
        "gray" | "grey" => Some("\x1b[38;5;245m"),
        "bright-black" => Some("\x1b[90m"),
        "bright-red" => Some("\x1b[91m"),
        "bright-green" => Some("\x1b[92m"),
        "bright-yellow" => Some("\x1b[93m"),
        "bright-blue" => Some("\x1b[94m"),
        "bright-magenta" => Some("\x1b[95m"),
        "bright-cyan" => Some("\x1b[96m"),
        "bright-white" => Some("\x1b[97m"),
        "dim" => Some("\x1b[2m"),
        _ => None,
    }
}

fn color(name: &str, field: &str, fallback: &'static str) -> &'static str {
    match color_code(name) {
        Some(code) => code,
        None => {
            eprintln!("Warning: unknown color '{name}' for [style].{field}, using the default");
            fallback
        }
    }
}

impl Theme {
    /// Build a theme from the loaded config.
    pub fn from_style(style: &StyleConfig) -> Self {
        Self::build(style, no_color())
    }

    fn build(style: &StyleConfig, no_color: bool) -> Self {
        let base = Self::default();
        if no_color {
            return Self {
                tag: "",
                comment: "",
                snippet: "",
                detail: "",
                accent: "",
                matches: "",
                bold: "",
                dim: "",
                selected_bg: "\x1b[7m",
                reset: RESET,
            };
        }
        Self {
            tag: color(&style.tag_color, "tag_color", base.tag),
            comment: color(&style.comment_color, "comment_color", base.comment),
            snippet: color(&style.snippet_color, "snippet_color", base.snippet),
            ..base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(tag: &str, comment: &str, snippet: &str) -> StyleConfig {
        StyleConfig {
            tag_color: tag.into(),
            comment_color: comment.into(),
            snippet_color: snippet.into(),
        }
    }

    #[test]
    fn defaults_keep_builtin_codes() {
        let t = Theme::build(&StyleConfig::default(), false);
        assert_eq!(t.tag, "\x1b[36m");
        assert_eq!(t.comment, "\x1b[34m");
        assert_eq!(t.snippet, "\x1b[2m");
        assert_eq!(t.selected_bg, "\x1b[48;5;236m");
    }

    #[test]
    fn named_colors_resolve() {
        let s = style("green", "bright-red", "white");
        let t = Theme::build(&s, false);
        assert_eq!(t.tag, "\x1b[32m");
        assert_eq!(t.comment, "\x1b[91m");
        assert_eq!(t.snippet, "\x1b[37m");
    }

    #[test]
    fn none_drops_color() {
        let s = style("none", "default", "none");
        let t = Theme::build(&s, false);
        assert_eq!(t.tag, "");
        assert_eq!(t.comment, "");
    }

    #[test]
    fn no_color_disables_all_codes() {
        let t = Theme::build(&StyleConfig::default(), true);
        assert_eq!(t.tag, "");
        assert_eq!(t.bold, "");
        assert_eq!(t.selected_bg, "\x1b[7m"); // reverse video keeps selection visible
        assert_eq!(t.reset, RESET);
    }

    #[test]
    fn unknown_color_falls_back_without_warning_code() {
        let s = style("teal", "blue", "dim");
        let t = Theme::build(&s, false);
        assert_eq!(t.tag, "\x1b[36m"); // default
        assert_eq!(t.comment, "\x1b[34m");
    }
}
