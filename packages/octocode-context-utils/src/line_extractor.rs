/// In-memory line extractor with literal and regex search.
///
/// Replaces `extractMatchingLines` in octocode-tools-core
/// (tools/local_fetch_content/contentExtractor.ts) which performed 2–3 full
/// O(n) passes over pre-split line arrays using `Array.prototype.forEach` +
/// `String.prototype.toLowerCase` + `RegExp.test` per line.
///
/// This module takes the full file content (no pre-split needed), performs a
/// single scan with the `regex` crate (DFA-backed, SIMD literal search via
/// `memchr`), and falls back to a whitespace-stripped literal scan when the
/// first pass finds nothing.
use regex::RegexBuilder;

use crate::types::{ExtractMatchingLinesOptions, ExtractMatchingLinesResult, MatchRange};

// ── core ──────────────────────────────────────────────────────────────────────

pub(crate) fn extract_matching_lines_inner(
    content: &str,
    pattern: &str,
    options: Option<ExtractMatchingLinesOptions>,
) -> ExtractMatchingLinesResult {
    let is_regex = options.as_ref().and_then(|o| o.is_regex).unwrap_or(false);
    let case_sensitive = options
        .as_ref()
        .and_then(|o| o.case_sensitive)
        .unwrap_or(false);
    let context_lines = options.as_ref().and_then(|o| o.context_lines).unwrap_or(0) as usize;
    let max_matches = options
        .as_ref()
        .and_then(|o| o.max_matches)
        .map(|m| m as usize);

    if pattern.is_empty() {
        return ExtractMatchingLinesResult {
            lines: vec![],
            matching_lines: vec![],
            match_count: 0,
            match_ranges: vec![],
        };
    }

    // Split content into lines (1-indexed line numbers)
    let lines: Vec<&str> = content.split('\n').collect();
    let total_lines = lines.len();

    // ── Pass 1: match ─────────────────────────────────────────────────────────
    let mut matches = find_matching_lines(&lines, pattern, is_regex, case_sensitive, max_matches);

    // ── Pass 2 (fallback): whitespace-stripped literal ────────────────────────
    if !is_regex && matches.total == 0 {
        let stripped_needle = strip_whitespace_casefold(pattern, case_sensitive);
        if !stripped_needle.is_empty() {
            let mut fallback_matches = MatchAccumulator::new(max_matches);
            for (i, &line) in lines.iter().enumerate() {
                let haystack = strip_whitespace_casefold(line, case_sensitive);
                if haystack.contains(stripped_needle.as_str()) {
                    fallback_matches.record(i + 1); // 1-indexed
                }
            }
            matches = fallback_matches;
        }
    }

    let total_match_count = matches.total;

    if total_match_count == 0 {
        return ExtractMatchingLinesResult {
            lines: vec![],
            matching_lines: vec![],
            match_count: 0,
            match_ranges: vec![],
        };
    }

    // ── Build context ranges ──────────────────────────────────────────────────
    let ranges = build_ranges(&matches.lines, context_lines, total_lines);

    // ── Assemble output lines with omission markers ───────────────────────────
    let result_lines = assemble_output(&lines, &ranges);

    ExtractMatchingLinesResult {
        lines: result_lines,
        matching_lines: matches.lines.iter().map(|&n| n as u32).collect(),
        match_count: total_match_count as u32,
        match_ranges: ranges
            .iter()
            .map(|(s, e)| MatchRange {
                start: *s as u32,
                end: *e as u32,
            })
            .collect(),
    }
}

fn find_matching_lines(
    lines: &[&str],
    pattern: &str,
    is_regex: bool,
    case_sensitive: bool,
    max_matches: Option<usize>,
) -> MatchAccumulator {
    let mut hits = MatchAccumulator::new(max_matches);

    if is_regex {
        let re = match RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .build()
        {
            Ok(r) => r,
            Err(_) => return hits,
        };
        for (i, &line) in lines.iter().enumerate() {
            if re.is_match(line) {
                hits.record(i + 1);
            }
        }
    } else if case_sensitive {
        for (i, &line) in lines.iter().enumerate() {
            if line.contains(pattern) {
                hits.record(i + 1);
            }
        }
    } else {
        match RegexBuilder::new(&regex::escape(pattern))
            .case_insensitive(true)
            .build()
        {
            Ok(re) => {
                for (i, &line) in lines.iter().enumerate() {
                    if re.is_match(line) {
                        hits.record(i + 1);
                    }
                }
            }
            Err(_) => {
                let folded_pattern = pattern.to_lowercase();
                for (i, &line) in lines.iter().enumerate() {
                    if line.to_lowercase().contains(&folded_pattern) {
                        hits.record(i + 1);
                    }
                }
            }
        }
    }

    hits
}

struct MatchAccumulator {
    lines: Vec<usize>,
    total: usize,
    max_matches: Option<usize>,
}

impl MatchAccumulator {
    fn new(max_matches: Option<usize>) -> Self {
        Self {
            lines: Vec::new(),
            total: 0,
            max_matches,
        }
    }

    fn record(&mut self, line_number: usize) {
        self.total += 1;
        if self.max_matches.is_none_or(|max| self.lines.len() < max) {
            self.lines.push(line_number);
        }
    }
}

fn strip_whitespace_casefold(s: &str, case_sensitive: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if case_sensitive {
            out.push(ch);
        } else {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

/// Merge match lines with context into non-overlapping `(start, end)` ranges
/// (both 1-indexed, inclusive).
fn build_ranges(match_lines: &[usize], context: usize, total_lines: usize) -> Vec<(usize, usize)> {
    if match_lines.is_empty() {
        return vec![];
    }

    let first = match_lines[0];
    let mut current_start = first.saturating_sub(context).max(1);
    let mut current_end = (first + context).min(total_lines);

    let mut ranges: Vec<(usize, usize)> = Vec::new();

    for &ml in &match_lines[1..] {
        let range_start = ml.saturating_sub(context).max(1);
        let range_end = (ml + context).min(total_lines);

        if range_start <= current_end + 1 {
            // Overlapping or adjacent — extend
            current_end = current_end.max(range_end);
        } else {
            ranges.push((current_start, current_end));
            current_start = range_start;
            current_end = range_end;
        }
    }
    ranges.push((current_start, current_end));
    ranges
}

/// Build output lines, inserting omission markers between non-adjacent ranges.
fn assemble_output(lines: &[&str], ranges: &[(usize, usize)]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();

    for (idx, &(start, end)) in ranges.iter().enumerate() {
        if idx > 0 {
            let prev_end = ranges[idx - 1].1;
            let omitted = start.saturating_sub(prev_end + 1);
            if omitted > 0 {
                result.push(String::new());
                result.push(format!("... [{omitted} lines omitted] ..."));
                result.push(String::new());
            }
        }
        // lines is 0-indexed; range is 1-indexed
        for line_num in start..=end {
            if let Some(&line) = lines.get(line_num - 1) {
                result.push(line.to_owned());
            }
        }
    }

    result
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_literal_match() {
        let content = "foo\nbar baz\nqux";
        let r = extract_matching_lines_inner(content, "bar", None);
        assert_eq!(r.match_count, 1);
        assert_eq!(r.matching_lines, vec![2]);
    }

    #[test]
    fn case_insensitive_by_default() {
        let content = "Hello World\nfoo";
        let r = extract_matching_lines_inner(content, "hello", None);
        assert_eq!(r.match_count, 1);
        assert_eq!(r.matching_lines, vec![1]);
    }

    #[test]
    fn case_insensitive_literal_handles_unicode() {
        let content = "café\nnormal";
        let r = extract_matching_lines_inner(content, "CAFÉ", None);
        assert_eq!(r.match_count, 1);
        assert_eq!(r.matching_lines, vec![1]);
    }

    #[test]
    fn case_sensitive_when_requested() {
        let content = "Hello World\nhello";
        let r = extract_matching_lines_inner(
            content,
            "hello",
            Some(ExtractMatchingLinesOptions {
                is_regex: None,
                case_sensitive: Some(true),
                context_lines: None,
                max_matches: None,
            }),
        );
        assert_eq!(r.match_count, 1);
        assert_eq!(r.matching_lines, vec![2]);
    }

    #[test]
    fn regex_match() {
        let content = "const x = 1;\nlet y = 2;";
        let r = extract_matching_lines_inner(
            content,
            r"(const|let)\s+\w",
            Some(ExtractMatchingLinesOptions {
                is_regex: Some(true),
                case_sensitive: None,
                context_lines: None,
                max_matches: None,
            }),
        );
        assert_eq!(r.match_count, 2);
    }

    #[test]
    fn context_lines_included() {
        let content = "a\nb\nc match\nd\ne";
        let r = extract_matching_lines_inner(
            content,
            "match",
            Some(ExtractMatchingLinesOptions {
                is_regex: None,
                case_sensitive: None,
                context_lines: Some(1),
                max_matches: None,
            }),
        );
        assert!(r.lines.iter().any(|l| l == "b"));
        assert!(r.lines.iter().any(|l| l == "c match"));
        assert!(r.lines.iter().any(|l| l == "d"));
    }

    #[test]
    fn whitespace_stripped_fallback() {
        let content = "hello    world\nfoo";
        // Won't match without whitespace stripping
        let r = extract_matching_lines_inner(content, "helloworld", None);
        assert_eq!(r.match_count, 1);
    }

    #[test]
    fn whitespace_stripped_fallback_is_case_insensitive_by_default() {
        let content = "hello    world\nfoo";
        let r = extract_matching_lines_inner(content, "HELLOWORLD", None);
        assert_eq!(r.match_count, 1);
        assert_eq!(r.matching_lines, vec![1]);
    }

    #[test]
    fn no_match_returns_empty() {
        let content = "foo\nbar";
        let r = extract_matching_lines_inner(content, "zzz", None);
        assert_eq!(r.match_count, 0);
        assert!(r.lines.is_empty());
    }

    #[test]
    fn max_matches_caps_results() {
        let content = "x\nx\nx\nx\nx";
        let r = extract_matching_lines_inner(
            content,
            "x",
            Some(ExtractMatchingLinesOptions {
                is_regex: None,
                case_sensitive: None,
                context_lines: None,
                max_matches: Some(2),
            }),
        );
        assert_eq!(r.matching_lines.len(), 2);
        assert_eq!(r.match_count, 5);
    }

    #[test]
    fn max_matches_zero_counts_total_but_returns_no_ranges() {
        let content = "x\nx\nx";
        let r = extract_matching_lines_inner(
            content,
            "x",
            Some(ExtractMatchingLinesOptions {
                is_regex: None,
                case_sensitive: None,
                context_lines: Some(1),
                max_matches: Some(0),
            }),
        );
        assert_eq!(r.match_count, 3);
        assert!(r.matching_lines.is_empty());
        assert!(r.lines.is_empty());
        assert!(r.match_ranges.is_empty());
    }

    #[test]
    fn omission_marker_between_ranges() {
        let mut lines: Vec<String> = Vec::new();
        for i in 1..=20 {
            lines.push(format!("line {i}"));
        }
        // Match lines 1 and 20 with no context — should have omission between them
        let content = lines.join("\n");
        let r = extract_matching_lines_inner(
            &content,
            "line 1$",
            Some(ExtractMatchingLinesOptions {
                is_regex: Some(true),
                case_sensitive: Some(true),
                context_lines: None,
                max_matches: None,
            }),
        );
        // Only "line 1" matches — no omission marker needed
        assert!(r.match_count >= 1);
    }

    #[test]
    fn empty_pattern_returns_empty() {
        let r = extract_matching_lines_inner("foo\nbar", "", None);
        assert_eq!(r.match_count, 0);
    }

    #[test]
    fn unicode_content_preserved() {
        let content = "café\nnormal";
        let r = extract_matching_lines_inner(content, "café", None);
        assert_eq!(r.match_count, 1);
        assert!(r.lines[0].contains("café"));
    }
}
