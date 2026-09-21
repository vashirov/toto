use std::collections::HashSet;
use std::io::BufRead;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{anyhow, Context, Result};
use regex::Regex;

use crate::item::{Item, SuggestionOpts, VariableMap};

// --- Static regexes (no lazy_static, using std LazyLock) ---

static VAR_LINE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\$\s*([^:]+):(.*)").unwrap());

/// Single regex matching all markdown inline formatting.
/// Captures the inner content in named group `inner`.
/// Order: bold-italic (***) before bold (**) before italic (*), same for underscores.
static MD_INLINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:\*{3}(.+?)\*{3}|\*{2}(.+?)\*{2}|\*(.+?)\*|_{3}(.+?)_{3}|_{2}(.+?)_{2}|_(.+?)_|~~(.+?)~~|`([^`]+)`)"
    ).unwrap()
});

/// Strip markdown inline formatting from text in a single pass.
fn strip_markdown_inline(text: &str) -> String {
    MD_INLINE
        .replace_all(text, |caps: &regex::Captures| {
            // Return the first non-empty capture group
            (1..=8)
                .find_map(|i| caps.get(i).map(|m| m.as_str()))
                .unwrap_or("")
                .to_string()
        })
        .to_string()
}

use crate::item::LINE_SEPARATOR;

/// Strip the first character (prefix like `@`) and trim whitespace.
fn without_prefix(line: &str) -> String {
    line.get(1..).unwrap_or("").trim().to_string()
}

/// Parse the `---` options portion of a `$ var: cmd --- --opts` line.
fn parse_opts(text: &str) -> Result<SuggestionOpts> {
    let mut opts = SuggestionOpts::default();

    let parts = shellwords::split(text)
        .map_err(|_| anyhow!("Missing closing quote in variable options"))?;

    let mut it = parts.into_iter().peekable();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--prevent-extra" => opts.prevent_extra = true,
            "--expand" => opts.expand = true,
            "--column" => {
                opts.column = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--column requires a value"))?
                        .parse::<u8>()
                        .context("--column value must be u8")?,
                );
            }
            "--delimiter" => {
                opts.delimiter = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--delimiter requires a value"))?
                        .to_string(),
                );
            }
            "--query" => {
                opts.query = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--query requires a value"))?
                        .to_string(),
                );
            }
            "--filter" => {
                opts.filter = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--filter requires a value"))?
                        .to_string(),
                );
            }
            "--preview" => {
                opts.preview = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--preview requires a value"))?
                        .to_string(),
                );
            }
            "--preview-window" => {
                opts.preview_window = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--preview-window requires a value"))?
                        .to_string(),
                );
            }
            "--header" => {
                opts.header = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--header requires a value"))?
                        .to_string(),
                );
            }
            "--headers" | "--header-lines" => {
                opts.header_lines = it
                    .next()
                    .ok_or_else(|| anyhow!("--header-lines requires a value"))?
                    .parse::<u8>()
                    .context("--header-lines value must be u8")?;
            }
            "--map" => {
                opts.map = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--map requires a value"))?
                        .to_string(),
                );
            }
            _ => {} // ignore unknown flags
        }
    }

    Ok(opts)
}

/// Parse a `$ variable: command --- options` line.
fn parse_variable_line(line: &str) -> Result<(&str, &str, Option<SuggestionOpts>)> {
    let caps = VAR_LINE_REGEX
        .captures(line)
        .ok_or_else(|| anyhow!("Invalid variable line: `{}`", line))?;

    let variable = caps.get(1).unwrap().as_str().trim();
    let rest = caps.get(2).unwrap().as_str();

    let mut parts = rest.splitn(2, "---");
    let command = parts.next().unwrap();
    let opts = parts.next().map(parse_opts).transpose()?;

    Ok((variable, command, opts))
}

/// Result of parsing a `.trick.md` file.
pub struct ParseResult {
    pub items: Vec<Item>,
    pub variables: VariableMap,
}

/// Parse a `.trick.md` file and return all items + variables.
pub fn parse_file(path: &Path) -> Result<ParseResult> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open trickbook `{}`", path.display()))?;
    let reader = std::io::BufReader::new(file);
    let file_str = path.to_string_lossy().to_string();

    let lines = reader.lines().enumerate().map(|(i, r)| {
        r.with_context(|| format!("Failed to read line {} in `{}`", i + 1, path.display()))
    });

    parse_lines(lines, &file_str)
}

/// Parse lines from any source (file, string, etc).
pub fn parse_lines<I>(lines: I, source: &str) -> Result<ParseResult>
where
    I: Iterator<Item = Result<String>>,
{
    let mut items: Vec<Item> = Vec::new();
    let mut variables = VariableMap::default();
    let mut seen: HashSet<u64> = HashSet::new();

    let mut current = Item::new();
    current.file = Some(source.to_string());
    let mut variable_cmd = String::new();
    let mut inside_snippet = false;
    let mut in_front_matter = false;
    let mut front_matter_lines: Vec<String> = Vec::new();
    let mut pending_prose: Vec<String> = Vec::new();

    for (line_nr, line_result) in lines.enumerate() {
        let line = line_result?;

        // --- YAML front-matter (only at line 0) ---
        if line_nr == 0 && line.trim() == "---" {
            in_front_matter = true;
            continue;
        }
        if in_front_matter {
            if line.trim() == "---" {
                in_front_matter = false;
                apply_front_matter(&front_matter_lines, &mut current, &mut variables);
            } else {
                front_matter_lines.push(line.clone());
            }
            continue;
        }

        // --- Inside code block ---
        if inside_snippet {
            // Closing fence
            if line.starts_with("```") {
                inside_snippet = false;
                flush_item(&mut current, &mut items, &mut seen);
                current.snippet.clear();
                pending_prose.clear();
                continue;
            }

            // Variable inside code block
            if !variable_cmd.is_empty() || (line.starts_with('$') && line.contains(':')) {
                if !current.snippet.is_empty() && variable_cmd.is_empty() {
                    flush_item(&mut current, &mut items, &mut seen);
                    current.snippet.clear();
                }

                variable_cmd.push_str(line.trim_end_matches('\\'));
                if !line.ends_with('\\') {
                    parse_and_insert_variable(
                        &variable_cmd,
                        &current.tags,
                        &mut variables,
                        line_nr,
                        source,
                    )?;
                    variable_cmd.clear();
                }
                continue;
            }

            // Empty line inside code block
            if line.is_empty() {
                if !current.snippet.is_empty() {
                    current.snippet.push_str(LINE_SEPARATOR);
                }
                continue;
            }

            // Regular snippet line
            if !current.snippet.is_empty() {
                current.snippet.push_str(LINE_SEPARATOR);
            }
            current.snippet.push_str(&line);
            continue;
        }

        // --- Outside code block ---

        // Opening code fence
        if line.starts_with("```") {
            inside_snippet = true;
            let lang = line.trim_start_matches('`').trim();
            if !lang.is_empty() {
                current.lang = Some(lang.to_string());
            }
            if !pending_prose.is_empty() {
                let prose_text = strip_markdown_inline(&pending_prose.join("\n"));
                if current.comment.is_empty() {
                    // No ## heading -- use prose as comment
                    current.comment = prose_text;
                } else {
                    // ## heading already set comment -- prose becomes detail
                    current.detail = Some(prose_text);
                }
            }
            pending_prose.clear();
            continue;
        }

        // Blank line
        if line.is_empty() {
            continue;
        }

        // Explicit % tag (navi compat)
        if line.starts_with('%') {
            flush_item(&mut current, &mut items, &mut seen);
            current.snippet.clear();
            current.detail = None;
            current.tags = without_prefix(&line);
            pending_prose.clear();
            continue;
        }

        // @ dependency
        if line.starts_with('@') {
            let dep = without_prefix(&line);
            variables.insert_dependency(&current.tags, &dep);
            continue;
        }

        // ; metacomment
        if line.starts_with(';') {
            continue;
        }

        // $ variable outside code block
        if !variable_cmd.is_empty() || (line.starts_with('$') && line.contains(':')) {
            flush_item(&mut current, &mut items, &mut seen);
            current.snippet.clear();

            variable_cmd.push_str(line.trim_end_matches('\\'));
            if !line.ends_with('\\') {
                parse_and_insert_variable(
                    &variable_cmd,
                    &current.tags,
                    &mut variables,
                    line_nr,
                    source,
                )?;
                variable_cmd.clear();
            }
            continue;
        }

        // # Heading
        if line.starts_with('#') {
            let trimmed = line.trim_start_matches('#');
            let hashes = line.len() - trimmed.len();
            let heading_text = trimmed.trim();

            if hashes == 1 {
                // H1 -> tags
                flush_item(&mut current, &mut items, &mut seen);
                current.snippet.clear();
                current.detail = None;
                current.tags = strip_markdown_inline(heading_text);
            } else {
                // H2+ -> comment/description
                flush_item(&mut current, &mut items, &mut seen);
                current.snippet.clear();
                current.detail = None;
                current.comment = strip_markdown_inline(heading_text);
                current.line = Some(line_nr + 1); // 1-indexed
            }
            pending_prose.clear();
            continue;
        }

        // Prose line -- strip list markers, accumulate as pending description/detail
        let prose = line.trim();
        let prose = prose
            .strip_prefix("- ")
            .or_else(|| prose.strip_prefix("* "))
            .unwrap_or(prose);
        pending_prose.push(prose.to_string());
    }

    // Flush any remaining item
    flush_item(&mut current, &mut items, &mut seen);

    Ok(ParseResult { items, variables })
}

/// If current item is valid and not a duplicate, push it to the list.
///
/// Deduplication uses a 64-bit hash of (tags, comment, snippet). This trades a
/// theoretical collision risk (~1 in 2^64) for zero heap allocation per lookup.
/// For typical trickbook sizes (< 10K items), this is a good trade-off.
fn flush_item(current: &mut Item, items: &mut Vec<Item>, seen: &mut HashSet<u64>) {
    if current.is_valid() {
        let h = current.hash_key();
        if seen.insert(h) {
            current.finalize_snippet();
            items.push(current.clone());
        }
    }
}

/// Parse a variable line and insert into the VariableMap.
fn parse_and_insert_variable(
    line: &str,
    tags: &str,
    variables: &mut VariableMap,
    line_nr: usize,
    source: &str,
) -> Result<()> {
    let (variable, command, opts) = parse_variable_line(line).with_context(|| {
        format!(
            "Failed to parse variable at line {} in `{}`",
            line_nr + 1,
            source
        )
    })?;
    variables.insert_suggestion(tags, variable, (command.to_string(), opts));
    Ok(())
}

/// Parse simple YAML front-matter key-value pairs.
fn apply_front_matter(lines: &[String], item: &mut Item, variables: &mut VariableMap) {
    for line in lines {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("tags:") {
            let value = value.trim().trim_start_matches('[').trim_end_matches(']');
            item.tags = value
                .split(',')
                .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                .collect::<Vec<_>>()
                .join(", ");
        } else if let Some(value) = line.strip_prefix("depend:") {
            for dep in value.split(',').map(|s| s.trim()) {
                if !dep.is_empty() {
                    variables.insert_dependency(&item.tags, dep);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse a string as a .trick.md file.
    fn parse(input: &str) -> ParseResult {
        let lines = input
            .lines()
            .map(|s| Ok::<String, anyhow::Error>(s.to_string()));
        parse_lines(lines, "test.trick.md").unwrap()
    }

    // --- strip_markdown_inline ---

    #[test]
    fn strip_bold() {
        assert_eq!(strip_markdown_inline("**bold**"), "bold");
    }

    #[test]
    fn strip_italic() {
        assert_eq!(strip_markdown_inline("*italic*"), "italic");
    }

    #[test]
    fn strip_mixed() {
        assert_eq!(
            strip_markdown_inline("Run **this** and *that*"),
            "Run this and that"
        );
    }

    #[test]
    fn strip_none() {
        assert_eq!(strip_markdown_inline("plain text"), "plain text");
    }

    // --- Parser: headings ---

    #[test]
    fn h1_becomes_tags() {
        let r = parse("# ssh\n\n## login\n\n```\nssh <host>\n```\n");
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].tags, "ssh");
        assert_eq!(r.items[0].comment, "login");
        assert_eq!(r.items[0].snippet, "ssh <host>");
    }

    #[test]
    fn multiple_h1_sections() {
        let r = parse(
            "# net\n\n## ping\n\n```\nping <h>\n```\n\n# files\n\n## ls\n\n```\nls -la\n```\n",
        );
        assert_eq!(r.items.len(), 2);
        assert_eq!(r.items[0].tags, "net");
        assert_eq!(r.items[1].tags, "files");
    }

    #[test]
    fn multiple_snippets_under_h1() {
        let r = parse(
            "# podman\n\n## ps\n\n```\npodman ps\n```\n\n## stop\n\n```\npodman stop <id>\n```\n",
        );
        assert_eq!(r.items.len(), 2);
        assert_eq!(r.items[0].tags, "podman");
        assert_eq!(r.items[1].tags, "podman");
        assert_eq!(r.items[0].comment, "ps");
        assert_eq!(r.items[1].comment, "stop");
    }

    // --- Parser: prose ---

    #[test]
    fn prose_skipped_outside_code_blocks() {
        let r = parse("# tools\n\nSome intro prose.\n\n## dig\n\n```\ndig <d>\n```\n");
        assert_eq!(r.items.len(), 1);
        assert!(!r.items[0].snippet.contains("intro"));
    }

    #[test]
    fn prose_becomes_comment_when_no_heading() {
        let r = parse("# net\n\nSends ICMP packets\n\n```\nping <h>\n```\n");
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].comment, "Sends ICMP packets");
    }

    #[test]
    fn h2_overrides_prose() {
        let r = parse("# net\n\n## Ping host\n\nExtra prose.\n\n```\nping <h>\n```\n");
        assert_eq!(r.items[0].comment, "Ping host");
        assert_eq!(r.items[0].detail.as_deref(), Some("Extra prose."));
    }

    #[test]
    fn list_item_as_comment() {
        let r = parse("# tools\n\n- Test connectivity\n\n```\nping <h>\n```\n");
        assert_eq!(r.items[0].comment, "Test connectivity");
    }

    #[test]
    fn detail_multiline_prose() {
        let r = parse(
            "# tools\n\n## Run scan\n\nThis does a full scan.\nIt may take a while.\n\n```\nnmap -p- <host>\n```\n",
        );
        assert_eq!(r.items[0].comment, "Run scan");
        assert_eq!(
            r.items[0].detail.as_deref(),
            Some("This does a full scan.\nIt may take a while.")
        );
    }

    #[test]
    fn no_detail_when_no_prose() {
        let r = parse("# t\n\n## cmd\n\n```\necho hi\n```\n");
        assert!(r.items[0].detail.is_none());
    }

    #[test]
    fn prose_without_heading_becomes_comment_not_detail() {
        let r = parse("# net\n\nSends ICMP packets\n\n```\nping <h>\n```\n");
        assert_eq!(r.items[0].comment, "Sends ICMP packets");
        assert!(r.items[0].detail.is_none());
    }

    // --- Parser: bold/italic in headings ---

    #[test]
    fn formatting_stripped_from_headings() {
        let r = parse("# **network** tools\n\n## Show *connected* devices\n\n```\narp -a\n```\n");
        assert_eq!(r.items[0].tags, "network tools");
        assert_eq!(r.items[0].comment, "Show connected devices");
    }

    // --- Parser: front-matter ---

    #[test]
    fn front_matter_tags() {
        let r = parse("---\ntags: [git, code]\n---\n\n## checkout\n\n```\ngit checkout <b>\n```\n");
        assert_eq!(r.items[0].tags, "git, code");
    }

    #[test]
    fn front_matter_plain_tags() {
        let r = parse("---\ntags: podman, containers\n---\n\n## ps\n\n```\npodman ps\n```\n");
        assert_eq!(r.items[0].tags, "podman, containers");
    }

    #[test]
    fn front_matter_quoted_tags() {
        let r = parse("---\ntags: [\"my tag\", 'other']\n---\n\n## test\n\n```\necho hi\n```\n");
        assert_eq!(r.items[0].tags, "my tag, other");
    }

    // --- Parser: variables ---

    #[test]
    fn variables_outside_code_block() {
        let r = parse("# ssh\n\n## login\n\n```\nssh <user>@<host>\n```\n\n$ user: echo root\n$ host: echo example.com\n");
        assert!(r.variables.get_suggestion("ssh", "user").is_some());
        assert!(r.variables.get_suggestion("ssh", "host").is_some());
    }

    #[test]
    fn variables_inside_code_block() {
        let r = parse(
            "# tools\n\n## vars\n\n```\n$ domain: echo google.com\n$ host: echo 8.8.8.8\n```\n",
        );
        assert!(r.variables.get_suggestion("tools", "domain").is_some());
        assert!(r.variables.get_suggestion("tools", "host").is_some());
    }

    #[test]
    fn multiline_variable() {
        let r =
            parse("# t\n\n## cmd\n\n```\necho <mv>\n```\n\n$ mv: echo xoo \\\n   | tr 'x' 'f'\n");
        assert!(r.variables.get_suggestion("t", "mv").is_some());
    }

    #[test]
    fn variable_with_opts() {
        let r = parse("# t\n\n## cmd\n\n```\necho <x>\n```\n\n$ x: echo '1 2' | tr ' ' '\\n' --- --column 1 --prevent-extra\n");
        let s = r.variables.get_suggestion("t", "x").unwrap();
        let opts = s.1.as_ref().unwrap();
        assert_eq!(opts.column, Some(1));
        assert!(opts.prevent_extra);
    }

    // --- Parser: code blocks ---

    #[test]
    fn multiline_snippet() {
        let r = parse("# t\n\n## multi\n\n```bash\necho 1\necho 2\necho 3\n```\n");
        assert_eq!(r.items.len(), 1);
        assert!(r.items[0].snippet.contains("echo 1"));
        assert!(r.items[0].snippet.contains("echo 3"));
    }

    #[test]
    fn empty_code_block_skipped() {
        let r = parse("# t\n\n## empty\n\n```\n```\n\n## real\n\n```\necho hi\n```\n");
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].comment, "real");
    }

    // --- Parser: compat ---

    #[test]
    fn explicit_percent_tag() {
        let r = parse("% custom, tags\n\n## cmd\n\n```\necho hi\n```\n");
        assert_eq!(r.items[0].tags, "custom, tags");
    }

    #[test]
    fn at_dependency() {
        let r = parse("# child\n\n@ parent\n\n## use var\n\n```\necho <x>\n```\n");
        assert_eq!(r.items[0].tags, "child");
    }

    #[test]
    fn semicolon_metacomment_ignored() {
        let r = parse("# t\n\n; this is a comment\n\n## cmd\n\n```\necho hi\n```\n");
        assert_eq!(r.items.len(), 1);
    }

    #[test]
    fn mid_file_dashes_not_front_matter() {
        let r = parse("# t\n\n---\n\n## cmd\n\n```\necho hi\n```\n");
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].tags, "t");
    }

    // --- Parser: deduplication ---

    #[test]
    fn duplicate_items_deduplicated() {
        let r = parse("# t\n\n## dup\n\n```\necho hi\n```\n\n## dup\n\n```\necho hi\n```\n");
        assert_eq!(r.items.len(), 1);
    }
}
