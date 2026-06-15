/// Ripgrep `--json` NDJSON output parser.
///
/// Replaces the TypeScript `parseRipgrepJson` (utils/parsers/ripgrep.ts) which
/// ran `JSON.parse` + Zod `safeParse` per line and a `[...value]` UTF-16 spread
/// per match snippet. This module does a single streaming pass with `serde_json`,
/// grouping match/context lines by file and assembling context windows in one go.
use std::collections::{hash_map::Entry, HashMap};

use serde::Deserialize;

use crate::types::{
    RipgrepFile, RipgrepMatch, RipgrepParseOptions, RipgrepParseResult, RipgrepStats,
};

// ── ripgrep --json wire types ─────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct RgTextPath {
    text: String,
}

#[derive(Deserialize, Debug)]
struct RgTextLines {
    text: String,
}

#[derive(Deserialize, Debug)]
struct RgSubmatch {
    start: usize,
}

#[derive(Deserialize, Debug)]
struct RgMatchData {
    path: RgTextPath,
    lines: RgTextLines,
    line_number: u32,
    #[allow(dead_code)]
    absolute_offset: u64,
    submatches: Vec<RgSubmatch>,
}

#[derive(Deserialize, Debug)]
struct RgContextData {
    path: RgTextPath,
    lines: RgTextLines,
    line_number: u32,
    #[allow(dead_code)]
    absolute_offset: u64,
}

#[derive(Deserialize, Debug)]
struct RgElapsed {
    human: String,
}

#[derive(Deserialize, Debug)]
struct RgStatsData {
    matches: u32,
    matched_lines: u32,
    searches_with_match: u32,
    searches: u32,
    bytes_searched: i64,
    elapsed: RgElapsed,
}

#[derive(Deserialize, Debug)]
struct RgSummaryData {
    stats: RgStatsData,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "data", rename_all = "lowercase")]
enum RgMessage {
    Match(RgMatchData),
    Context(RgContextData),
    Summary(RgSummaryData),
    Begin(()),
    End(()),
}

// ── intermediate state ────────────────────────────────────────────────────────

struct RawMatch {
    line_text: String,
    line_number: u32,
    column: u32,
}

struct FileEntry {
    raw_matches: Vec<RawMatch>,
    /// line_number → context text
    contexts: HashMap<u32, String>,
}

impl FileEntry {
    fn new() -> Self {
        Self {
            raw_matches: Vec::new(),
            contexts: HashMap::new(),
        }
    }
}

// ── core parsing logic ────────────────────────────────────────────────────────

/// Strips a single trailing `\r\n` or `\n` from a line, matching ripgrep's
/// included newline in the `lines.text` field.
fn strip_trailing_newline(s: &str) -> &str {
    s.strip_suffix("\r\n")
        .or_else(|| s.strip_suffix('\n'))
        .unwrap_or(s)
}

/// Truncates a string to at most `max_chars` Unicode scalar values, appending
/// `...` when truncated. Avoids `[...value]` spread allocation from JS.
fn truncate_unicode(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if max_chars <= 3 {
        let mut chars = s.chars();
        if chars.by_ref().take(max_chars).count() < max_chars || chars.next().is_none() {
            return s.to_owned();
        }
        return ".".repeat(max_chars);
    }

    let head_chars = max_chars - 3;
    let mut head_byte_end = 0;
    for (char_idx, (byte_idx, _ch)) in s.char_indices().enumerate() {
        if char_idx == head_chars {
            head_byte_end = byte_idx;
        }
        if char_idx == max_chars {
            return format!("{}...", &s[..head_byte_end]);
        }
    }
    s.to_owned()
}

fn entry_for_path<'a>(
    file_map: &'a mut HashMap<String, FileEntry>,
    file_order: &mut Vec<String>,
    path: String,
) -> &'a mut FileEntry {
    match file_map.entry(path) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            file_order.push(entry.key().clone());
            entry.insert(FileEntry::new())
        }
    }
}

fn push_joined_line(out: &mut String, line: &str) {
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(line);
}

pub(crate) fn parse_ripgrep_json_inner(
    stdout: &str,
    options: Option<RipgrepParseOptions>,
) -> RipgrepParseResult {
    let context_lines = options.as_ref().and_then(|o| o.context_lines).unwrap_or(0);
    let max_snippet = options
        .as_ref()
        .and_then(|o| o.max_snippet_chars)
        .unwrap_or(500) as usize;

    let mut file_map: HashMap<String, FileEntry> = HashMap::new();
    let mut stats = RipgrepStats::default();
    // Track insertion order for deterministic output
    let mut file_order: Vec<String> = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.starts_with('{') {
            continue;
        }

        let msg: RgMessage = match serde_json::from_str(trimmed) {
            Ok(m) => m,
            Err(_) => continue,
        };

        match msg {
            RgMessage::Match(m) => {
                let path = m.path.text;
                let line_text = strip_trailing_newline(&m.lines.text).to_owned();
                let column = m.submatches.first().map(|s| s.start as u32).unwrap_or(0);

                let entry = entry_for_path(&mut file_map, &mut file_order, path);
                entry.raw_matches.push(RawMatch {
                    line_text,
                    line_number: m.line_number,
                    column,
                });
            }
            RgMessage::Context(c) => {
                let path = c.path.text;
                let line_text = strip_trailing_newline(&c.lines.text).to_owned();
                let entry = entry_for_path(&mut file_map, &mut file_order, path);
                entry.contexts.insert(c.line_number, line_text);
            }
            RgMessage::Summary(s) => {
                stats = RipgrepStats {
                    match_count: Some(s.stats.matches),
                    matched_lines: Some(s.stats.matched_lines),
                    files_matched: Some(s.stats.searches_with_match),
                    files_searched: Some(s.stats.searches),
                    bytes_searched: Some(s.stats.bytes_searched),
                    search_time: Some(s.stats.elapsed.human),
                };
            }
            RgMessage::Begin(_) | RgMessage::End(_) => {}
        }
    }

    let files: Vec<RipgrepFile> = file_order
        .into_iter()
        .filter_map(|path| {
            let entry = file_map.remove(&path)?;
            let matches: Vec<RipgrepMatch> = entry
                .raw_matches
                .iter()
                .map(|m| {
                    let mut joined = String::new();
                    for i in (1..=context_lines).rev() {
                        if let Some(ctx) = entry.contexts.get(&m.line_number.saturating_sub(i)) {
                            push_joined_line(&mut joined, ctx);
                        }
                    }
                    push_joined_line(&mut joined, &m.line_text);
                    for i in 1..=context_lines {
                        if let Some(ctx) = entry.contexts.get(&(m.line_number + i)) {
                            push_joined_line(&mut joined, ctx);
                        }
                    }

                    let value = truncate_unicode(&joined, max_snippet);

                    RipgrepMatch {
                        line: m.line_number,
                        column: m.column,
                        value,
                    }
                })
                .collect();

            let match_count = matches.len() as u32;
            Some(RipgrepFile {
                path,
                match_count,
                matches,
            })
        })
        .collect();

    RipgrepParseResult { files, stats }
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_match_line(path: &str, line_text: &str, line_number: u32, col: usize) -> String {
        serde_json::json!({
            "type": "match",
            "data": {
                "path": { "text": path },
                "lines": { "text": line_text },
                "line_number": line_number,
                "absolute_offset": 0,
                "submatches": [{ "match": { "text": "x" }, "start": col, "end": col + 1 }]
            }
        })
        .to_string()
    }

    fn make_context_line(path: &str, line_text: &str, line_number: u32) -> String {
        serde_json::json!({
            "type": "context",
            "data": {
                "path": { "text": path },
                "lines": { "text": line_text },
                "line_number": line_number,
                "absolute_offset": 0
            }
        })
        .to_string()
    }

    fn make_summary(matches: u32, matched_lines: u32) -> String {
        serde_json::json!({
            "type": "summary",
            "data": {
                "elapsed": { "human": "0.001s", "nanos": 1000000, "secs": 0 },
                "stats": {
                    "bytes_printed": 100,
                    "bytes_searched": 500,
                    "elapsed": { "human": "0.001s", "nanos": 1000000, "secs": 0 },
                    "matched_lines": matched_lines,
                    "matches": matches,
                    "searches": 1,
                    "searches_with_match": 1
                }
            }
        })
        .to_string()
    }

    #[test]
    fn empty_stdout_returns_empty_result() {
        let r = parse_ripgrep_json_inner("", None);
        assert!(r.files.is_empty());
        assert!(r.stats.match_count.is_none());
    }

    #[test]
    fn parses_single_match() {
        let stdout = make_match_line("src/foo.ts", "  const x = 1;\n", 10, 8);
        let r = parse_ripgrep_json_inner(&stdout, None);
        assert_eq!(r.files.len(), 1);
        let f = &r.files[0];
        assert_eq!(f.path, "src/foo.ts");
        assert_eq!(f.match_count, 1);
        assert_eq!(f.matches[0].line, 10);
        assert_eq!(f.matches[0].column, 8);
        assert_eq!(f.matches[0].value, "  const x = 1;");
    }

    #[test]
    fn strips_trailing_newline_from_match_line() {
        let stdout = make_match_line("f.ts", "line\n", 1, 0);
        let r = parse_ripgrep_json_inner(&stdout, None);
        assert_eq!(r.files[0].matches[0].value, "line");
    }

    #[test]
    fn parses_summary_stats() {
        let stdout = [make_match_line("f.ts", "x\n", 1, 0), make_summary(2, 2)].join("\n");
        let r = parse_ripgrep_json_inner(&stdout, None);
        assert_eq!(r.stats.match_count, Some(2));
        assert_eq!(r.stats.search_time, Some("0.001s".to_owned()));
    }

    #[test]
    fn assembles_context_window() {
        let stdout = [
            make_context_line("f.ts", "before\n", 9),
            make_match_line("f.ts", "match\n", 10, 0),
            make_context_line("f.ts", "after\n", 11),
        ]
        .join("\n");
        let r = parse_ripgrep_json_inner(
            &stdout,
            Some(RipgrepParseOptions {
                context_lines: Some(1),
                max_snippet_chars: None,
            }),
        );
        let val = &r.files[0].matches[0].value;
        assert!(val.contains("before"));
        assert!(val.contains("match"));
        assert!(val.contains("after"));
    }

    #[test]
    fn truncates_long_snippets() {
        let long = "a".repeat(600);
        let stdout = make_match_line("f.ts", &format!("{long}\n"), 1, 0);
        let r = parse_ripgrep_json_inner(
            &stdout,
            Some(RipgrepParseOptions {
                context_lines: None,
                max_snippet_chars: Some(10),
            }),
        );
        let val = &r.files[0].matches[0].value;
        assert!(val.ends_with("..."));
        // actual char count including "..." is 10
        assert!(val.chars().count() <= 10);
    }

    #[test]
    fn groups_multiple_matches_per_file() {
        let stdout = [
            make_match_line("f.ts", "line1\n", 1, 0),
            make_match_line("f.ts", "line2\n", 5, 0),
        ]
        .join("\n");
        let r = parse_ripgrep_json_inner(&stdout, None);
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].match_count, 2);
    }

    #[test]
    fn ignores_non_json_lines() {
        let stdout = "not json\n\n  \n".to_owned() + &make_match_line("f.ts", "x\n", 1, 0);
        let r = parse_ripgrep_json_inner(&stdout, None);
        assert_eq!(r.files.len(), 1);
    }

    #[test]
    fn preserves_unicode_content() {
        let stdout = make_match_line("f.ts", "café → naïve\n", 1, 0);
        let r = parse_ripgrep_json_inner(&stdout, None);
        assert_eq!(r.files[0].matches[0].value, "café → naïve");
    }

    #[test]
    fn truncate_unicode_counts_chars_not_bytes() {
        // "café" is 4 chars but 5 bytes (é = 2 bytes)
        let s = "café world";
        let r = truncate_unicode(s, 4);
        // should truncate at "c" boundary before limit and add "..."
        // limit 4 → head at max_chars-3=1 chars + "..."
        assert!(r.ends_with("..."));
    }

    #[test]
    fn truncate_unicode_zero_limit_returns_empty() {
        assert_eq!(truncate_unicode("hello", 0), "");
    }

    #[test]
    fn truncate_unicode_tiny_limits_never_exceed_limit() {
        assert_eq!(truncate_unicode("hello", 1), ".");
        assert_eq!(truncate_unicode("hello", 2), "..");
        assert_eq!(truncate_unicode("hello", 3), "...");
    }

    #[test]
    fn truncate_unicode_tiny_limits_preserve_short_input() {
        assert_eq!(truncate_unicode("é", 1), "é");
        assert_eq!(truncate_unicode("é", 2), "é");
    }
}
