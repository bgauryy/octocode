//! String-aware comment removal: the comment-removal strategy for supported
//! language comment groups.

use super::strategies::utf8_seq_len;

mod constructs;
mod docstrings;
mod rules;
mod templates;

use constructs::{
    find_block_comment, find_csharp_verbatim, find_line_comment, find_powershell_here_string,
    find_regex_literal, find_rust_raw_string,
};
pub use docstrings::strip_python_docstrings;
#[cfg(test)]
use rules::COMMENT_GROUPS;
use rules::effective_delimiters;
pub use rules::{CommentRules, rules_for};

// ── Inner state machine ───────────────────────────────────────────────────────

enum QuoteState {
    Outside,
    /// Tracking escape-aware single-char string (delimiter byte)
    Single {
        delim: u8,
        escaped: bool,
    },
    /// Tracking multi-char close delimiter (e.g. `"""`, `'''`)
    Multi {
        end: &'static str,
    },
}

/// The bytes at which `strip_string_aware_comments` and `literal_ranges`
/// can recognise a construct while outside a string: the first byte of each
/// raw-string, verbatim-string, here-string, regex, quote-delimiter and (with
/// `comments`) comment form. Every other byte is ordinary text, so a scan
/// copies or skips the run up to the next marked byte in one step. Marked
/// bytes are ASCII or sequence leads, never continuation bytes, so a run
/// always ends on a char boundary.
fn construct_starts(rules: &CommentRules, comments: bool) -> [bool; 256] {
    let mut starts = [false; 256];
    // Rust raw strings (`r"`, `br"`) and C# verbatim strings (`@"`, `$@"`)
    // are tried whatever the rules.
    for byte in *b"rb@$" {
        starts[usize::from(byte)] = true;
    }
    starts[usize::from(b'/')] = rules.regex;
    for delimiter in effective_delimiters(rules) {
        starts[usize::from(delimiter.as_bytes()[0])] = true;
    }
    if comments {
        for token in rules
            .block
            .iter()
            .map(|rule| rule.start)
            .chain(rules.line.iter().map(|rule| rule.token))
        {
            starts[usize::from(token.as_bytes()[0])] = true;
        }
    }
    starts
}

/// The length of the run of ordinary text at `pos`: bytes before the next
/// marked one (see [`construct_starts`]), or the rest of `bytes`.
fn ordinary_run(bytes: &[u8], pos: usize, starts: &[bool; 256]) -> usize {
    bytes[pos..]
        .iter()
        .position(|byte| starts[usize::from(*byte)])
        .unwrap_or(bytes.len() - pos)
}

pub fn strip_string_aware_comments(content: &str, rules: &CommentRules) -> String {
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(content.len());
    let mut pos = 0usize;
    let mut qstate = QuoteState::Outside;

    // String-start delimiters sorted longest-first (so `"""` wins over `"`).
    // This is loop-invariant, so build it once rather than per Outside-state char.
    let mut sorted_delims: Vec<&'static str> = effective_delimiters(rules).to_vec();
    sorted_delims.sort_by_key(|d| std::cmp::Reverse(d.len()));
    let starts = construct_starts(rules, true);

    while pos < len {
        match &mut qstate {
            // ── inside multi-char string ──────────────────────────────────
            QuoteState::Multi { end } => {
                let end_str: &'static str = end; // copy the &'static str out
                if content[pos..].starts_with(end_str) {
                    result.push_str(end_str);
                    pos += end_str.len();
                    qstate = QuoteState::Outside;
                } else {
                    if let Some(c) = content[pos..].chars().next() {
                        result.push(c);
                    }
                    pos += utf8_seq_len(bytes[pos]);
                }
            }
            // ── inside single-char string ─────────────────────────────────
            QuoteState::Single { delim, escaped } => {
                let ch = bytes[pos];
                let ch_len = utf8_seq_len(bytes[pos]);
                result.push_str(&content[pos..pos + ch_len]);
                if *escaped {
                    *escaped = false;
                } else if ch == b'\\' {
                    *escaped = true;
                } else if ch == *delim {
                    qstate = QuoteState::Outside;
                }
                pos += ch_len;
            }
            // ── outside strings ───────────────────────────────────────────
            QuoteState::Outside => {
                // 0. A run of text no construct starts in.
                let run = ordinary_run(bytes, pos, &starts);
                if run > 0 {
                    result.push_str(&content[pos..pos + run]);
                    pos += run;
                    continue;
                }

                // 1. PowerShell here-string
                if rules.powershell_here_strings
                    && let Some(end_pos) = find_powershell_here_string(content, pos)
                {
                    result.push_str(&content[pos..end_pos]);
                    pos = end_pos;
                    continue;
                }

                // 2. Rust raw string  r##"..."##
                if let Some(end_pos) = find_rust_raw_string(content, pos) {
                    result.push_str(&content[pos..end_pos]);
                    pos = end_pos;
                    continue;
                }

                // 3. C# verbatim string  @"..."
                if let Some(end_pos) = find_csharp_verbatim(content, pos) {
                    result.push_str(&content[pos..end_pos]);
                    pos = end_pos;
                    continue;
                }

                // 4. JS/TS regex literal
                if rules.regex
                    && let Some(end_pos) = find_regex_literal(content, pos)
                {
                    result.push_str(&content[pos..end_pos]);
                    pos = end_pos;
                    continue;
                }

                // 5. String-start delimiter (longest match first)
                if rules.regex && bytes[pos] == b'`' {
                    let end = templates::end(content, pos);
                    result.push_str(&content[pos..end]);
                    pos = end;
                    continue;
                }
                if let Some(&delim) = sorted_delims
                    .iter()
                    .find(|&&d| content[pos..].starts_with(d))
                {
                    result.push_str(delim);
                    pos += delim.len();
                    if delim.len() == 1 {
                        qstate = QuoteState::Single {
                            delim: delim.as_bytes()[0],
                            escaped: false,
                        };
                    } else {
                        qstate = QuoteState::Multi { end: delim };
                    }
                    continue;
                }

                // 6. Block comment
                if let Some((end_pos, preserve_newlines)) =
                    find_block_comment(content, pos, rules.block)
                {
                    if preserve_newlines {
                        for ch in content[pos..end_pos].chars() {
                            if ch == '\n' || ch == '\r' {
                                result.push(ch);
                            }
                        }
                    }
                    pos = end_pos;
                    continue;
                }

                // 7. Line comment
                if let Some(()) = find_line_comment(content, pos, rules.line) {
                    let skip_to = content[pos..].find('\n').map(|i| pos + i).unwrap_or(len);
                    pos = skip_to;
                    continue;
                }

                // 8. Ordinary character
                let ch_len = utf8_seq_len(bytes[pos]);
                result.push_str(&content[pos..pos + ch_len]);
                pos += ch_len;
            }
        }
    }
    result
}

/// Byte ranges of string/regex/raw-string literals in `content`, using the
/// same recognition rules as `strip_string_aware_comments` (PowerShell
/// here-strings, Rust raw strings, C# verbatim strings, JS/TS regex literals,
/// quoted strings) — minus comment detection, since callers run this on
/// content that has already had comments stripped.
///
/// Ranges are non-overlapping, sorted by start position, and byte-index
/// aligned to `content`'s char boundaries (mirrors the same scan used for
/// comment stripping, so it inherits the same UTF-8 safety). Callers that
/// need to skip transformation inside literals — e.g. the whitespace/
/// punctuation passes below — walk `content` with a cursor into this list
/// instead of re-implementing quote tracking.
pub fn literal_ranges(content: &str, rules: &CommentRules) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;
    let mut qstate = QuoteState::Outside;
    let mut ranges = Vec::new();
    let mut cur_start = 0usize;

    let mut sorted_delims: Vec<&'static str> = effective_delimiters(rules).to_vec();
    sorted_delims.sort_by_key(|d| std::cmp::Reverse(d.len()));
    let starts = construct_starts(rules, false);

    while pos < len {
        match &mut qstate {
            QuoteState::Multi { end } => {
                let end_str: &'static str = end;
                if content[pos..].starts_with(end_str) {
                    pos += end_str.len();
                    ranges.push((cur_start, pos));
                    qstate = QuoteState::Outside;
                } else {
                    pos += utf8_seq_len(bytes[pos]);
                }
            }
            QuoteState::Single { delim, escaped } => {
                let ch = bytes[pos];
                let ch_len = utf8_seq_len(bytes[pos]);
                if *escaped {
                    *escaped = false;
                    pos += ch_len;
                } else if ch == b'\\' {
                    *escaped = true;
                    pos += ch_len;
                } else if ch == *delim {
                    pos += ch_len;
                    ranges.push((cur_start, pos));
                    qstate = QuoteState::Outside;
                } else {
                    pos += ch_len;
                }
            }
            QuoteState::Outside => {
                let run = ordinary_run(bytes, pos, &starts);
                if run > 0 {
                    pos += run;
                    continue;
                }
                if rules.powershell_here_strings
                    && let Some(end_pos) = find_powershell_here_string(content, pos)
                {
                    ranges.push((pos, end_pos));
                    pos = end_pos;
                    continue;
                }
                if let Some(end_pos) = find_rust_raw_string(content, pos) {
                    ranges.push((pos, end_pos));
                    pos = end_pos;
                    continue;
                }
                if let Some(end_pos) = find_csharp_verbatim(content, pos) {
                    ranges.push((pos, end_pos));
                    pos = end_pos;
                    continue;
                }
                if rules.regex
                    && let Some(end_pos) = find_regex_literal(content, pos)
                {
                    ranges.push((pos, end_pos));
                    pos = end_pos;
                    continue;
                }
                if rules.regex && bytes[pos] == b'`' {
                    let end = templates::end(content, pos);
                    ranges.push((pos, end));
                    pos = end;
                    continue;
                }
                if let Some(&delim) = sorted_delims
                    .iter()
                    .find(|&&d| content[pos..].starts_with(d))
                {
                    cur_start = pos;
                    pos += delim.len();
                    qstate = if delim.len() == 1 {
                        QuoteState::Single {
                            delim: delim.as_bytes()[0],
                            escaped: false,
                        }
                    } else {
                        QuoteState::Multi { end: delim }
                    };
                    continue;
                }
                pos += utf8_seq_len(bytes[pos]);
            }
        }
    }
    // A partial source read can end inside a literal. Its remaining bytes
    // are still literal content, even without the closing delimiter.
    if !matches!(qstate, QuoteState::Outside) {
        ranges.push((cur_start, len));
    }
    ranges
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Remove comments from `content` using the named comment-group types.
pub fn remove_comments(content: &str, groups: &[&str]) -> String {
    let mut result = content.to_owned();
    for &group in groups {
        if group == "python-docstring" {
            result = strip_python_docstrings(&result);
            continue;
        }
        if let Some(rules) = rules_for(group) {
            result = strip_string_aware_comments(&result, rules);
        }
        // An unknown group strips nothing.
    }
    result
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests;
