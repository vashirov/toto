//! Unified fuzzy picker TUI.
//!
//! Supports two display modes:
//! - **Plain mode**: single-column list (pipe)
//! - **Tricks mode**: multi-column list with preview pane
//!
//! Supports two height modes:
//! - **Fullscreen**: alternate screen buffer
//! - **Inline**: reserves N rows at bottom of terminal
//!
//! Writes to `/dev/tty` so it works when stdout is captured.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use crate::matcher::{self, MatchResult, Scheme};
use unicode_width::UnicodeWidthStr;

// -- Background matcher thread messages --

enum MatchRequest {
    Query(String),
    Quit,
}

struct MatchResponse {
    results: Vec<MatchResult>,
    query: String,
}

// -- ANSI escape codes --

const ALT_ON: &str = "\x1b[?1049h";
const ALT_OFF: &str = "\x1b[?1049l";
const SHOW_CUR: &str = "\x1b[?25h";
const HIDE_CUR: &str = "\x1b[?25l";
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const C_CYAN: &str = "\x1b[36m";
const C_BLUE: &str = "\x1b[34m";
const C_GREEN: &str = "\x1b[32m";
const BG_SEL: &str = "\x1b[48;5;236m";
const C_GRAY: &str = "\x1b[38;5;245m";
const C_MATCH: &str = "\x1b[33m"; // yellow for matched chars

// -- TTY handle --

struct Tty {
    file: File,
    original: libc::termios,
}

impl Tty {
    fn open() -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        let fd = file.as_raw_fd();

        let mut original: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut original) } != 0 {
            return Err(io::Error::last_os_error());
        }

        let mut raw = original;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
        raw.c_cc[libc::VMIN] = 1; // block until at least 1 byte (poll handles timeout)
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(Self { file, original })
    }

    fn size(&self) -> (usize, usize) {
        let fd = self.file.as_raw_fd();
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) } == 0 {
            (ws.ws_col as usize, ws.ws_row as usize)
        } else {
            (80, 24)
        }
    }

    /// Poll for input readiness with timeout.
    fn poll(&self, timeout: Duration) -> bool {
        let fd = self.file.as_raw_fd();
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        unsafe { libc::poll(&mut pfd, 1, ms) > 0 }
    }
}

impl Drop for Tty {
    fn drop(&mut self) {
        let fd = self.file.as_raw_fd();
        unsafe {
            libc::tcsetattr(fd, libc::TCSANOW, &self.original);
        }
    }
}

// -- Public types --

/// Source of items for the picker.
pub enum ItemSource {
    /// All items known upfront.
    Static(Vec<String>),
}

/// Display configuration for the picker.
pub struct PickerConfig {
    pub prompt: String,
    pub scheme: Scheme,
    pub height: Height,
    /// Pre-fill the search query.
    pub initial_query: Option<String>,
    /// Enable Ctrl-E to return Edit result. When false, Ctrl-E is end-of-line.
    pub edit_enabled: bool,
    /// Tricks mode: multi-column display with preview pane.
    pub tricks: Option<TricksDisplay>,
}

/// Height specification for the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Height {
    Full,
    Percent(u16),
    Rows(u16),
}

impl Height {
    pub fn resolve(&self, term_rows: usize) -> usize {
        match self {
            Height::Full => term_rows,
            Height::Percent(p) => {
                let rows = term_rows * (*p as usize) / 100;
                rows.max(3)
            }
            Height::Rows(r) => (*r as usize).min(term_rows).max(3),
        }
    }

    pub fn is_full(&self) -> bool {
        matches!(self, Height::Full)
    }
}

pub fn parse_height(s: &str) -> Height {
    let s = s.trim();
    if s.eq_ignore_ascii_case("full") || s.is_empty() {
        return Height::Full;
    }
    if let Some(pct) = s.strip_suffix('%') {
        if let Ok(n) = pct.trim().parse::<u16>() {
            return Height::Percent(n.min(100).max(1));
        }
    }
    if let Ok(n) = s.parse::<u16>() {
        return Height::Rows(n.max(3));
    }
    Height::Full
}

/// Extra display info for tricks mode.
pub struct TricksDisplay {
    pub entries: Vec<TrickEntry>,
    pub preview_height: usize,
}

/// A trick entry with display columns and metadata.
#[derive(Clone)]
#[allow(dead_code)] // Fields read via entries vec in main.rs
pub struct TrickEntry {
    pub tags: String,
    pub comment: String,
    pub snippet_preview: String,
    pub snippet_full: String,
    pub detail: Option<String>,
    pub usage_count: u64,
    /// Source file for Ctrl-E editing.
    pub file: Option<String>,
    pub source_line: Option<usize>,
}

/// What the picker returned.
pub enum PickResult {
    /// User selected an item. Carries (index, item_text).
    Selected(usize, String),
    Cancelled,
    /// User pressed edit key. Carries (index, item_text, current_query).
    Edit {
        index: usize,
        query: String,
    },
}

// -- Truncation/wrapping helpers --

fn truncate_pad(text: &str, width: usize) -> String {
    let w = UnicodeWidthStr::width(text);
    if w <= width {
        format!("{text}{}", " ".repeat(width - w))
    } else {
        let mut out = String::new();
        let mut used = 0;
        for ch in text.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + cw >= width {
                break;
            }
            out.push(ch);
            used += cw;
        }
        format!("{out}.{}", " ".repeat(width.saturating_sub(used + 1)))
    }
}

/// Truncate text to `width` visual columns, highlighting matched char positions.
/// `positions` contains char (not byte) indices of matched characters.
/// `base_style` is the ANSI style to return to after each highlight.
fn truncate_highlight(text: &str, width: usize, positions: &[u32], base_style: &str) -> String {
    use std::collections::HashSet;
    use std::fmt::Write as FmtWrite;

    let match_set: HashSet<u32> = positions.iter().copied().collect();
    let mut out = String::new();
    let mut used = 0;
    let mut in_highlight = false;

    for (ci, ch) in text.chars().enumerate() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw >= width && used > 0 {
            // Would exceed - truncate with dot
            if in_highlight {
                let _ = write!(out, "{RESET}{base_style}");
            }
            out.push('.');
            let pad = width.saturating_sub(used + 1);
            for _ in 0..pad {
                out.push(' ');
            }
            return out;
        }
        let is_match = match_set.contains(&(ci as u32));
        if is_match && !in_highlight {
            let _ = write!(out, "{BOLD}{C_MATCH}");
            in_highlight = true;
        } else if !is_match && in_highlight {
            let _ = write!(out, "{RESET}{base_style}");
            in_highlight = false;
        }
        out.push(ch);
        used += cw;
    }
    if in_highlight {
        let _ = write!(out, "{RESET}{base_style}");
    }
    // Pad remaining
    for _ in 0..width.saturating_sub(used) {
        out.push(' ');
    }
    out
}

/// Wrap text into lines, ANSI-escape-aware.
fn wrap_lines(text: &str, width: usize) -> Vec<String> {
    let mut result = Vec::new();
    for line in text.lines() {
        if width == 0 {
            result.push(line.to_string());
            continue;
        }
        let mut current = String::new();
        let mut used = 0;
        let mut in_esc = false;

        for ch in line.chars() {
            if ch == '\x1b' {
                in_esc = true;
                current.push(ch);
                continue;
            }
            if in_esc {
                current.push(ch);
                if ch.is_ascii_alphabetic() {
                    in_esc = false;
                }
                continue;
            }

            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + cw > width && used > 0 {
                result.push(current);
                current = String::new();
                used = 0;
            }
            current.push(ch);
            used += cw;
        }
        result.push(current);
    }
    result
}

const SEPARATOR_CHAR: char = '\u{2500}';

// -- Cached draw state (reused across frames) --

struct DrawState {
    /// Pre-allocated output buffer to avoid per-frame allocations.
    buf: String,
    /// Cached column widths for tricks mode: (tag_w, comment_w, snippet_w).
    col_widths: Option<(usize, usize, usize)>,
    /// Number of matches when col_widths was last computed.
    col_widths_match_count: usize,
    /// Terminal columns when col_widths was last computed.
    col_widths_cols: usize,
}

impl DrawState {
    fn new() -> Self {
        Self {
            buf: String::with_capacity(8192),
            col_widths: None,
            col_widths_match_count: 0,
            col_widths_cols: 0,
        }
    }
}

// -- Core picker --

pub fn run(source: ItemSource, config: &PickerConfig) -> io::Result<PickResult> {
    let mut tty = Tty::open()?;
    let (_cols, term_rows) = tty.size();
    let use_full = config.height.is_full();
    let draw_rows = config.height.resolve(term_rows);

    // Enter screen mode
    let origin_row: usize = if use_full {
        write!(tty.file, "{ALT_ON}{HIDE_CUR}")?;
        tty.file.flush()?;
        0
    } else {
        // Inline: reserve space
        let reserve = draw_rows;
        for _ in 0..reserve {
            write!(tty.file, "\n")?;
        }
        let origin = term_rows.saturating_sub(reserve);
        write!(tty.file, "\x1b[{};1H{HIDE_CUR}", origin + 1)?;
        tty.file.flush()?;
        origin
    };

    let result = run_inner(&mut tty, source, config, draw_rows, origin_row);

    // Leave screen mode
    if use_full {
        write!(tty.file, "{ALT_OFF}{SHOW_CUR}")?;
    } else {
        // Clear inline area
        for i in 0..draw_rows {
            write!(tty.file, "\x1b[{};1H\x1b[2K", origin_row + i + 1)?;
        }
        write!(tty.file, "\x1b[{};1H{SHOW_CUR}", origin_row + 1)?;
    }
    tty.file.flush()?;

    result
}

/// Non-interactive best-match for --best-match mode.
pub fn best_match(items: &[String], query: &str, scheme: Scheme) -> Option<usize> {
    let results = matcher::fuzzy_match(query, items, scheme);
    results.first().map(|r| r.index)
}

fn run_inner(
    tty: &mut Tty,
    source: ItemSource,
    config: &PickerConfig,
    draw_rows: usize,
    origin_row: usize,
) -> io::Result<PickResult> {
    let mut query: Vec<char> = config
        .initial_query
        .as_deref()
        .unwrap_or("")
        .chars()
        .collect();
    let mut qpos: usize = query.len();
    let mut sel: usize = 0;
    let mut scroll: usize = 0;

    // Matcher and TUI share the static item list without cloning it per query.
    let items: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));

    match source {
        ItemSource::Static(static_items) => {
            *items.write().unwrap() = static_items;
        }
    }

    // -- Background matcher thread --
    let (query_tx, query_rx) = mpsc::channel::<MatchRequest>();
    let (result_tx, result_rx) = mpsc::channel::<MatchResponse>();
    let matcher_items = Arc::clone(&items);
    let scheme = config.scheme;

    let matcher_handle = thread::spawn(move || {
        let mut cached_query = String::new();
        let mut cached_pattern: Option<matcher::CachedPattern> = None;

        loop {
            let req = match query_rx.recv() {
                Ok(r) => r,
                Err(_) => return,
            };

            match req {
                MatchRequest::Quit => return,
                MatchRequest::Query(q) => {
                    // Debounce: drain pending requests, use latest query
                    let mut latest = q;
                    while let Ok(newer) = query_rx.try_recv() {
                        match newer {
                            MatchRequest::Quit => return,
                            MatchRequest::Query(q) => {
                                latest = q;
                            }
                        }
                    }

                    // Reuse cached pattern if query hasn't changed (ItemsChanged case)
                    if latest != cached_query || cached_pattern.is_none() {
                        cached_pattern = Some(matcher::CachedPattern::new(&latest, scheme));
                        cached_query = latest.clone();
                    }

                    // Read lock -- no clone. Matcher scores against shared slice.
                    let guard = matcher_items.read().unwrap();
                    let results = cached_pattern.as_ref().unwrap().score(&guard);
                    drop(guard);
                    let _ = result_tx.send(MatchResponse {
                        results,
                        query: latest,
                    });
                }
            }
        }
    });

    // Initial match
    let initial_q: String = query.iter().collect();
    let _ = query_tx.send(MatchRequest::Query(initial_q.clone()));
    let initial = result_rx.recv().unwrap_or(MatchResponse {
        results: Vec::new(),
        query: String::new(),
    });
    let mut matches = initial.results;
    let mut matched_query = initial.query;

    let mut draw_state = DrawState::new();
    let result = loop {
        // Pick up latest match results (non-blocking)
        while let Ok(resp) = result_rx.try_recv() {
            matches = resp.results;
            matched_query = resp.query;
        }

        // Clamp
        if matches.is_empty() {
            sel = 0;
        } else if sel >= matches.len() {
            sel = matches.len() - 1;
        }
        if sel < scroll {
            scroll = sel;
        }

        let (cols, _) = tty.size();
        let query_str: String = query.iter().collect();
        let items_guard = items.read().unwrap();

        // Draw
        draw(
            &mut tty.file,
            &items_guard,
            &matches,
            &query_str,
            qpos,
            sel,
            &mut scroll,
            &config.prompt,
            true,
            cols,
            draw_rows,
            origin_row,
            config.tricks.as_ref(),
            &mut draw_state,
        )?;

        drop(items_guard); // release lock before blocking on input

        // Poll with timeout
        let timeout = if matched_query != query_str {
            Duration::from_millis(16)
        } else {
            Duration::from_secs(60)
        };

        if !tty.poll(timeout) {
            continue;
        }

        // Read input
        let mut buf = [0u8; 32];
        let n = tty.file.read(&mut buf)?;
        if n == 0 {
            continue;
        }

        // If we got a lone ESC byte, check if more bytes follow (escape sequence).
        // Arrow keys send ESC [ A/B/C/D -- if we only got ESC, poll briefly for rest.
        // Use 50ms timeout -- generous enough for slow terminals/SSH.
        let n = if n == 1 && buf[0] == 0x1b {
            if tty.poll(Duration::from_millis(50)) {
                let extra = tty.file.read(&mut buf[1..])?;
                1 + extra
            } else {
                1 // bare Escape -- no more bytes coming
            }
        } else {
            n
        };

        let mut query_changed = false;

        match &buf[..n] {
            // Enter
            b"\r" | b"\n" => {
                if let Some(m) = matches.get(sel) {
                    let item_text = items
                        .read()
                        .unwrap()
                        .get(m.index)
                        .cloned()
                        .unwrap_or_default();
                    break PickResult::Selected(m.index, item_text);
                }
            }
            // Bare Escape (no following bytes after 10ms)
            b"\x1b" => break PickResult::Cancelled,
            // Ctrl-C
            b"\x03" => break PickResult::Cancelled,
            // Ctrl-E: edit (when enabled) or end-of-line
            b"\x05" => {
                if config.edit_enabled {
                    if let Some(m) = matches.get(sel) {
                        break PickResult::Edit {
                            index: m.index,
                            query: query_str,
                        };
                    }
                } else {
                    qpos = query.len();
                }
            }
            // Ctrl-U: clear
            b"\x15" => {
                query.clear();
                qpos = 0;
                query_changed = true;
            }
            // Ctrl-A: start
            b"\x01" => qpos = 0,
            // Ctrl-W: delete word
            b"\x17" => {
                if qpos > 0 {
                    let left: String = query[..qpos].iter().collect();
                    let trimmed = left.trim_end();
                    let new_end = trimmed.rfind(' ').map(|p| p + 1).unwrap_or(0);
                    query.drain(new_end..qpos);
                    qpos = new_end;
                    query_changed = true;
                }
            }
            // Ctrl-K: delete to end
            b"\x0b" => {
                query.truncate(qpos);
                query_changed = true;
            }
            // Backspace
            b"\x7f" | b"\x08" => {
                if qpos > 0 {
                    qpos -= 1;
                    query.remove(qpos);
                    query_changed = true;
                }
            }
            // Up / Ctrl-P (ESC[A normal mode, ESC OA application mode)
            [0x1b, b'[', b'A'] | [0x1b, b'O', b'A'] | b"\x10" => {
                sel = sel.saturating_sub(1);
            }
            // Down / Ctrl-N
            [0x1b, b'[', b'B'] | [0x1b, b'O', b'B'] | b"\x0e" => {
                if !matches.is_empty() && sel < matches.len() - 1 {
                    sel += 1;
                }
            }
            // Left / Ctrl-B
            [0x1b, b'[', b'D'] | [0x1b, b'O', b'D'] | b"\x02" => {
                qpos = qpos.saturating_sub(1);
            }
            // Right / Ctrl-F
            [0x1b, b'[', b'C'] | [0x1b, b'O', b'C'] | b"\x06" => {
                if qpos < query.len() {
                    qpos += 1;
                }
            }
            // Home
            [0x1b, b'[', b'H'] | [0x1b, b'O', b'H'] | [0x1b, b'[', b'1', b'~'] => qpos = 0,
            // End
            [0x1b, b'[', b'F'] | [0x1b, b'O', b'F'] | [0x1b, b'[', b'4', b'~'] => {
                qpos = query.len()
            }
            // Delete
            [0x1b, b'[', b'3', b'~'] => {
                if qpos < query.len() {
                    query.remove(qpos);
                    query_changed = true;
                }
            }
            // Page Up
            [0x1b, b'[', b'5', b'~'] => {
                let page = draw_rows.saturating_sub(2);
                sel = sel.saturating_sub(page);
            }
            // Page Down
            [0x1b, b'[', b'6', b'~'] => {
                if !matches.is_empty() {
                    let page = draw_rows.saturating_sub(2);
                    sel = (sel + page).min(matches.len() - 1);
                }
            }
            // Printable
            _ => {
                if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                    if s.chars().all(|c| !c.is_control()) {
                        for ch in s.chars() {
                            query.insert(qpos, ch);
                            qpos += 1;
                        }
                        query_changed = true;
                    }
                }
            }
        }

        if query_changed {
            let q: String = query.iter().collect();
            let _ = query_tx.send(MatchRequest::Query(q));
            sel = 0;
            scroll = 0;
        }
    };

    // Shut down matcher thread
    let _ = query_tx.send(MatchRequest::Quit);
    let _ = matcher_handle.join();

    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn draw(
    out: &mut impl Write,
    items: &[String],
    matches: &[MatchResult],
    query: &str,
    qpos: usize,
    sel: usize,
    scroll: &mut usize,
    prompt: &str,
    walk_done: bool,
    cols: usize,
    draw_rows: usize,
    origin_row: usize,
    tricks: Option<&TricksDisplay>,
    state: &mut DrawState,
) -> io::Result<()> {
    use std::fmt::Write as FmtWrite;

    // Calculate layout
    let preview_h = tricks.map(|t| t.preview_height).unwrap_or(0);
    // prompt(1) + list + separator(if preview) + preview + status(1)
    let overhead = 1 + if preview_h > 0 { 1 + preview_h + 1 } else { 1 };
    let list_h = draw_rows.saturating_sub(overhead);

    // Scroll clamping
    if sel < *scroll {
        *scroll = sel;
    }
    if sel >= *scroll + list_h {
        *scroll = sel - list_h + 1;
    }

    // Reuse pre-allocated buffer
    state.buf.clear();

    // Move to origin
    let _ = write!(state.buf, "\x1b[{};1H", origin_row + 1);

    // Prompt
    let status_char = if walk_done { ' ' } else { '*' };
    let _ = write!(
        state.buf,
        "{BOLD}{C_GREEN}{prompt}{status_char}{RESET} {}/{}>  {query}",
        matches.len(),
        items.len()
    );
    // Truncate prompt to terminal width (count visible chars only)
    state.buf.push_str("\x1b[K\r\n");

    // List
    let end = ((*scroll) + list_h).min(matches.len());

    // Compute column widths -- cache and reuse when match count + cols unchanged
    let col_widths = tricks.map(|td| {
        if state.col_widths.is_some()
            && state.col_widths_match_count == matches.len()
            && state.col_widths_cols == cols
        {
            state.col_widths.unwrap()
        } else {
            let max_tag = matches
                .iter()
                .filter_map(|m| td.entries.get(m.index))
                .map(|e| UnicodeWidthStr::width(e.tags.as_str()))
                .max()
                .unwrap_or(4);
            let tag_w = (max_tag + 2).min(cols / 3).max(4);
            let remaining = cols.saturating_sub(tag_w + 4);
            let comment_w = (remaining * 55 / 100).max(10);
            let snippet_w = remaining.saturating_sub(comment_w + 2);
            let widths = (tag_w, comment_w, snippet_w);
            state.col_widths = Some(widths);
            state.col_widths_match_count = matches.len();
            state.col_widths_cols = cols;
            widths
        }
    });

    for i in *scroll..end {
        let m = &matches[i];
        let is_sel = i == sel;

        if let Some(ref td) = tricks {
            // Tricks mode: multi-column
            let (tag_w, comment_w, snippet_w) = col_widths.unwrap();
            if let Some(entry) = td.entries.get(m.index) {
                draw_trick_row_buf(&mut state.buf, entry, is_sel, tag_w, comment_w, snippet_w);
            } else {
                state.buf.push_str("\x1b[K\r\n");
            }
        } else {
            // Plain mode: single item with match highlighting
            let text = if m.index < items.len() {
                &items[m.index]
            } else {
                ""
            };
            let avail = cols.saturating_sub(3);
            if m.positions.is_empty() {
                let display = truncate_pad(text, avail);
                if is_sel {
                    let _ = write!(
                        state.buf,
                        "{BG_SEL}{BOLD}{C_GREEN}> {display}\x1b[K{RESET}\r\n"
                    );
                } else {
                    let _ = write!(state.buf, "  {display}\x1b[K{RESET}\r\n");
                }
            } else if is_sel {
                let base = format!("{BG_SEL}{BOLD}{C_GREEN}");
                let display = truncate_highlight(text, avail, &m.positions, &base);
                let _ = write!(
                    state.buf,
                    "{BG_SEL}{BOLD}{C_GREEN}> {display}\x1b[K{RESET}\r\n"
                );
            } else {
                let display = truncate_highlight(text, avail, &m.positions, RESET);
                let _ = write!(state.buf, "  {display}\x1b[K{RESET}\r\n");
            }
        }
    }

    // Blank remaining list rows
    for _ in (end - *scroll)..list_h {
        state.buf.push_str("\x1b[K\r\n");
    }

    // Preview pane (tricks mode only)
    if let Some(ref td) = tricks {
        if td.preview_height > 0 {
            // Separator
            let _ = write!(state.buf, "{DIM}");
            for _ in 0..cols {
                state.buf.push(SEPARATOR_CHAR);
            }
            let _ = write!(state.buf, "{RESET}\r\n");

            let mut preview_rows = 0;
            if let Some(m) = matches.get(sel) {
                if let Some(entry) = td.entries.get(m.index) {
                    // Comment
                    let _ = write!(
                        state.buf,
                        "{BOLD}{C_BLUE}{}{RESET}\x1b[K\r\n",
                        truncate_pad(&entry.comment, cols)
                    );
                    preview_rows += 1;

                    // Detail (gray)
                    if let Some(ref detail) = entry.detail {
                        for line in wrap_lines(detail, cols) {
                            if preview_rows >= td.preview_height {
                                break;
                            }
                            let _ = write!(state.buf, "{C_GRAY}{line}{RESET}\x1b[K\r\n");
                            preview_rows += 1;
                        }
                    }

                    // Empty line before snippet
                    if preview_rows < td.preview_height {
                        state.buf.push_str("\x1b[K\r\n");
                        preview_rows += 1;
                    }

                    // Snippet
                    for snippet_line in entry.snippet_full.lines() {
                        for wl in wrap_lines(snippet_line, cols) {
                            if preview_rows >= td.preview_height {
                                break;
                            }
                            let _ = write!(state.buf, "{wl}{RESET}\x1b[K\r\n");
                            preview_rows += 1;
                        }
                    }
                }
            }

            // Blank remaining preview rows
            for _ in preview_rows..td.preview_height {
                state.buf.push_str("\x1b[K\r\n");
            }
        }
    }

    // Cursor position on prompt
    let prompt_prefix_len =
        prompt.len() + 2 + format!("{}/{}", matches.len(), items.len()).len() + 3;
    let prefix: String = query.chars().take(qpos).collect();
    let cursor_col = prompt_prefix_len + UnicodeWidthStr::width(prefix.as_str()) + 1;
    let _ = write!(
        state.buf,
        "\x1b[{};{}H{SHOW_CUR}",
        origin_row + 1,
        cursor_col
    );

    // Single write syscall for entire frame
    out.write_all(state.buf.as_bytes())?;
    out.flush()
}

/// Write a trick row into a String buffer (avoids per-row write syscalls).
fn draw_trick_row_buf(
    buf: &mut String,
    entry: &TrickEntry,
    is_sel: bool,
    tag_w: usize,
    comment_w: usize,
    snippet_w: usize,
) {
    use std::fmt::Write as FmtWrite;
    let tag_s = truncate_pad(&entry.tags, tag_w);
    let com_s = truncate_pad(&entry.comment, comment_w);
    let snip_s = truncate_pad(&entry.snippet_preview, snippet_w);

    if is_sel {
        let _ = write!(
            buf,
            "{BG_SEL}{BOLD}{C_CYAN}{tag_s}{RESET}{BG_SEL} {BOLD}{C_BLUE}{com_s}{RESET}{BG_SEL} {BOLD}{snip_s}\x1b[K{RESET}\r\n"
        );
    } else {
        let _ = write!(
            buf,
            "{C_CYAN}{tag_s}{RESET} {C_BLUE}{com_s}{RESET} {DIM}{snip_s}{RESET}\x1b[K\r\n"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_pad_short() {
        assert_eq!(truncate_pad("hi", 5), "hi   ");
    }

    #[test]
    fn truncate_pad_exact() {
        assert_eq!(truncate_pad("hello", 5), "hello");
    }

    #[test]
    fn truncate_pad_long() {
        let r = truncate_pad("hello world", 6);
        assert_eq!(r.len(), 6);
    }

    #[test]
    fn height_full() {
        assert_eq!(parse_height("full"), Height::Full);
        assert_eq!(parse_height(""), Height::Full);
    }

    #[test]
    fn height_percent() {
        assert_eq!(parse_height("40%"), Height::Percent(40));
    }

    #[test]
    fn height_rows() {
        assert_eq!(parse_height("20"), Height::Rows(20));
    }

    #[test]
    fn height_resolve() {
        assert_eq!(Height::Full.resolve(50), 50);
        assert_eq!(Height::Percent(40).resolve(50), 20);
        assert_eq!(Height::Rows(20).resolve(50), 20);
        assert_eq!(Height::Rows(100).resolve(50), 50);
    }
}
