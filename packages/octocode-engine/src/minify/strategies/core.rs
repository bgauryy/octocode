use crate::minify::comment_remover::remove_comments;

// ── Conservative ─────────────────────────────────────────────────────────────

/// Strip comments then collapse ≥3 blank lines to 2, trim trailing whitespace.
/// Preserves indentation (agents need structural context).
pub fn minify_conservative(content: &str, comments: Option<&[&str]>) -> String {
    let s = if let Some(groups) = comments {
        remove_comments(content, groups)
    } else {
        content.to_owned()
    };
    compact_lines(&s, comments, 2, false)
}

/// Preserve every line intersecting a literal, including its line ending.
/// Outside literals, share one blank/trailing-space pass across strategies.
fn compact_lines(
    s: &str,
    comments: Option<&[&str]>,
    max_blanks: u32,
    halve_indent: bool,
) -> String {
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
        let protected = ranges
            .get(range_index)
            .is_some_and(|&(start, stop)| start < end && stop > offset);
        offset = end;
        if protected {
            result.push_str(line);
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
            if halve_indent {
                let leading = stripped.len() - stripped.trim_start().len();
                result.push_str(&" ".repeat(leading / 2));
                result.push_str(stripped.trim_start());
            } else {
                result.push_str(stripped);
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

// ── Aggressive ───────────────────────────────────────────────────────────────

pub fn minify_aggressive(content: &str, comments: Option<&[&str]>) -> String {
    let s = if let Some(groups) = comments {
        remove_comments(content, groups)
    } else {
        content.to_owned()
    };
    // Merge the comment groups' quote/regex rules so the whitespace and
    // punctuation passes below can skip string/regex literal spans instead
    // of mutating their contents (see comment_remover::literal_ranges).
    let merged_rules = comments.map(merge_comment_rules);
    let s = super::collapse_whitespace(&s, merged_rules.as_ref());
    let s = super::re_tighten_punct(&s, merged_rules.as_ref());
    s.trim().to_owned()
}

/// Combine the `CommentRules` for a set of comment groups into one, so a
/// single literal-range scan covers every quote/regex convention active for
/// this language (e.g. `["hash", "template"]`-style multi-group configs).
pub(super) fn merge_comment_rules(groups: &[&str]) -> crate::minify::comment_remover::CommentRules {
    use crate::minify::comment_remover::{rules_for, CommentRules};
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
    compact_lines(content, None, 1, false)
}

// ── General (allow indent compression) ───────────────────────────────────────

pub fn minify_general_core(content: &str) -> String {
    compact_lines(content, None, 2, true)
}
