//! Variable interpolation and command execution.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use anyhow::{anyhow, Context, Result};
use regex::Regex;

use crate::item::{SuggestionOpts, VariableMap};
use crate::matcher::Scheme;
use crate::picker::{self, Height, ItemSource, PickResult, PickerConfig};

/// Regex to find `<variable_name>` placeholders in snippets.
static VAR_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<(\w[\w\d\-_]*)>").unwrap());

/// Interpolate all `<var>` placeholders in a snippet by prompting the user.
pub fn interpolate(
    snippet: &str,
    tags: &str,
    variables: &VariableMap,
    shell: &str,
) -> Result<Option<String>> {
    let mut result = snippet.to_string();

    // Find all unique variables in order of appearance
    let var_names: Vec<String> = VAR_REGEX
        .find_iter(snippet)
        .map(|m| m.as_str()[1..m.as_str().len() - 1].to_string())
        .collect::<Vec<_>>();

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    let unique_vars: Vec<String> = var_names
        .into_iter()
        .filter(|v| seen.insert(v.clone()))
        .collect();

    for var_name in &unique_vars {
        let bracketed = format!("<{var_name}>");

        let value = if let Some(suggestion) = variables.get_suggestion(tags, var_name) {
            // Run suggestion command, let user pick
            let (cmd, opts) = suggestion;
            prompt_variable(var_name, cmd, opts.as_ref(), shell)?
        } else {
            // No suggestion -- free-form input from user
            prompt_freeform(var_name)?
        };

        let value = match value {
            Some(v) => v,
            None => return Ok(None), // User cancelled
        };

        // Replace all occurrences of this variable
        result = result.replace(&bracketed, &value);
    }

    Ok(Some(result))
}

/// Run a suggestion command, collect output lines, let user pick with the picker.
fn prompt_variable(
    var_name: &str,
    command: &str,
    opts: Option<&SuggestionOpts>,
    shell: &str,
) -> Result<Option<String>> {
    let cmd = command.trim();
    if cmd.is_empty() {
        return prompt_freeform(var_name);
    }

    // Buffer suggestions before opening the picker. Opening it while a command
    // is still producing items lets input arrive before matches are available.
    let output = Command::new(shell)
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("Failed to run suggestion command for <{var_name}>"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|line| !line.is_empty()).collect();

    if lines.is_empty() {
        return prompt_freeform(var_name);
    }

    if let Some(opts) = opts {
        if let Some(ref filter_text) = opts.filter {
            for line in &lines {
                if line.contains(filter_text.as_str()) {
                    return Ok(Some(extract_column(line, opts)));
                }
            }
        }
    }

    let items: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
    pick_from_items(var_name, items, opts)
}

/// Show picker with pre-collected items and apply column extraction.
fn pick_from_items(
    var_name: &str,
    items: Vec<String>,
    opts: Option<&SuggestionOpts>,
) -> Result<Option<String>> {
    let picker_config = PickerConfig {
        prompt: var_name.to_string(),
        scheme: Scheme::Default,
        height: Height::Full,
        initial_query: opts.and_then(|o| o.query.clone()),
        edit_enabled: false,
        tricks: None,
    };

    match picker::run(ItemSource::Static(items), &picker_config)
        .map_err(|e| anyhow!("Picker failed for <{var_name}>: {e}"))?
    {
        PickResult::Selected(_, item) => {
            Ok(Some(opts.map(|o| extract_column(&item, o)).unwrap_or(item)))
        }
        PickResult::Cancelled | PickResult::Edit { .. } => Ok(None),
    }
}

/// Extract a specific column from a line based on SuggestionOpts.
fn extract_column(line: &str, opts: &SuggestionOpts) -> String {
    if let Some(col) = opts.column {
        let col = col as usize;
        if col == 0 {
            return line.to_string();
        }
        let parts: Vec<&str> = if let Some(ref delim) = opts.delimiter {
            line.split(delim.as_str()).collect()
        } else {
            line.split_whitespace().collect()
        };
        parts.get(col - 1).unwrap_or(&line).trim().to_string()
    } else {
        line.to_string()
    }
}

/// Prompt for free-form text input (no suggestions).
/// Uses /dev/tty for both prompt display and input so it works
/// when stdout is captured.
fn prompt_freeform(var_name: &str) -> Result<Option<String>> {
    use std::io::BufRead;

    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .context("Failed to open /dev/tty for variable prompt")?;

    write!(tty, "{var_name}: ")?;
    tty.flush()?;

    let mut input = String::new();
    let mut reader = std::io::BufReader::new(&tty);
    reader.read_line(&mut input)?;
    let input = input.trim().to_string();

    // Empty input is valid -- returns empty string, not cancellation.
    // Only EOF (read_line returns 0 bytes) would indicate cancellation,
    // but that's unlikely from /dev/tty.
    Ok(Some(input))
}

/// Execute a command via shell.
pub fn execute(command: &str, shell: &str) -> Result<()> {
    let expanded = crate::item::snippet_to_display(command);
    let status = Command::new(shell)
        .arg("-c")
        .arg(&expanded)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute: {expanded}"))?;

    if !status.success() {
        if let Some(code) = status.code() {
            std::process::exit(code);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_to_display_converts() {
        let s = format!("echo 1{}echo 2", crate::item::LINE_SEPARATOR);
        assert_eq!(crate::item::snippet_to_display(&s), "echo 1\necho 2");
    }

    #[test]
    fn extract_column_default() {
        let opts = SuggestionOpts::default();
        assert_eq!(extract_column("hello world", &opts), "hello world");
    }

    #[test]
    fn extract_column_whitespace() {
        let opts = SuggestionOpts {
            column: Some(2),
            ..Default::default()
        };
        assert_eq!(extract_column("abc  def  ghi", &opts), "def");
    }

    #[test]
    fn extract_column_delimiter() {
        let opts = SuggestionOpts {
            column: Some(2),
            delimiter: Some(";".to_string()),
            ..Default::default()
        };
        assert_eq!(extract_column("abc;def;ghi", &opts), "def");
    }

    #[test]
    fn var_regex_finds_variables() {
        let matches: Vec<&str> = VAR_REGEX
            .find_iter("ssh <user>@<host> -p <port>")
            .map(|m| &m.as_str()[1..m.as_str().len() - 1])
            .collect();
        assert_eq!(matches, vec!["user", "host", "port"]);
    }

    #[test]
    fn var_regex_no_match() {
        let matches: Vec<&str> = VAR_REGEX
            .find_iter("echo hello world")
            .map(|m| m.as_str())
            .collect();
        assert!(matches.is_empty());
    }
}
