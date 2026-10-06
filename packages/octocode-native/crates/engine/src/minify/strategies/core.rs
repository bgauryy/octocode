use crate::minify::comment_remover::remove_comments;

// ── Brace code ───────────────────────────────────────────────────────────────

/// Brace-delimited code (Rust, Go, Java, C-family, …) where indentation is not
/// syntax: strip comments, indentation, and blank lines like the JS/TS view,
/// keeping one statement line per source line. Literal-spanning lines stay
/// byte-exact.
pub fn minify_brace_code(content: &str, comments: &[&str]) -> String {
    compact_lines(
        &remove_comments(content, comments),
        Some(comments),
        0,
        Indent::Strip,
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Indent {
    Keep,
    Halve,
    Strip,
}

/// Preserve every line intersecting a literal, including its line ending.
/// Outside literals, share one blank/trailing-space pass across strategies.
fn compact_lines(s: &str, comments: Option<&[&str]>, max_blanks: u32, indent: Indent) -> String {
    let rules = comments.map(merge_comment_rules).unwrap_or_default();
    let ranges = crate::minify::comment_remover::literal_ranges(s, &rules);
    let mut result = String::with_capacity(s.len());
    let mut blank_run = 0u32;
    let mut offset = 0;
    let mut range_index = 0;
    let mut last_protected = false;
    for line in s.split_inclusive('\n') {
        let end = offset + line.len();
        while range_index < ranges.len() && ranges[range_index].1 <= offset {
            range_index += 1;
        }
        let current = ranges.get(range_index).copied();
        let protected = current.is_some_and(|(start, stop)| start < end && stop > offset);
        // Leading whitespace is outside the literal unless the line opens inside it.
        let starts_in_literal = current.is_some_and(|(start, _)| start < offset);
        offset = end;
        if protected {
            if indent == Indent::Strip && !starts_in_literal {
                result.push_str(line.trim_start_matches([' ', '\t']));
            } else {
                result.push_str(line);
            }
            blank_run = 0;
            last_protected = true;
            continue;
        }
        last_protected = false;
        let stripped = line.trim_end_matches([' ', '\t', '\r', '\n']);
        if stripped.is_empty() {
            blank_run += 1;
            if blank_run <= max_blanks && !result.is_empty() {
                result.push('\n');
            }
        } else {
            blank_run = 0;
            match indent {
                Indent::Keep => result.push_str(stripped),
                Indent::Halve => {
                    let leading = stripped.len() - stripped.trim_start().len();
                    result.push_str(&" ".repeat(leading / 2));
                    result.push_str(stripped.trim_start());
                }
                Indent::Strip => result.push_str(stripped.trim_start()),
            }
            result.push('\n');
        }
    }
    if last_protected {
        result
    } else {
        result.trim_end_matches('\n').to_owned()
    }
}

/// Combine the `CommentRules` for a set of comment groups into one, so a
/// single literal-range scan covers every quote/regex convention active for
/// this language (e.g. `["hash", "template"]`-style multi-group configs).
pub(super) fn merge_comment_rules(groups: &[&str]) -> crate::minify::comment_remover::CommentRules {
    use crate::minify::comment_remover::{CommentRules, rules_for};
    let mut merged = CommentRules::default();
    for &group in groups {
        if let Some(rules) = rules_for(group) {
            merged.regex = merged.regex || rules.regex;
            merged.powershell_here_strings =
                merged.powershell_here_strings || rules.powershell_here_strings;
            if !rules.quote_delimiters.is_empty() {
                merged.quote_delimiters = rules.quote_delimiters;
            }
        }
    }
    merged
}

// ── Code (whitespace only, preserve indent) ───────────────────────────────────

pub fn minify_code_core(content: &str) -> String {
    compact_lines(content, None, 1, Indent::Keep)
}

// ── General (allow indent compression) ───────────────────────────────────────

pub fn minify_general_core(content: &str) -> String {
    compact_lines(content, None, 2, Indent::Halve)
}
