use crate::comment_remover::remove_comments;

// ── Conservative ─────────────────────────────────────────────────────────────

/// Strip comments then collapse ≥3 blank lines to 2, trim trailing whitespace.
/// Preserves indentation (agents need structural context).
pub fn minify_conservative(content: &str, comments: Option<&[&str]>) -> String {
    let mut s = if let Some(groups) = comments {
        remove_comments(content, groups)
    } else {
        content.to_owned()
    };
    s = s.replace("\r\n", "\n");
    collapse_blanks_preserve_indent(&s)
}

fn collapse_blanks_preserve_indent(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut blank_run = 0u32;
    for line in s.split('\n') {
        let stripped = line.trim_end_matches([' ', '\t']);
        if stripped.is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                result.push('\n');
            }
        } else {
            blank_run = 0;
            result.push_str(stripped);
            result.push('\n');
        }
    }
    result.trim_end_matches('\n').to_owned()
}

// ── Aggressive ───────────────────────────────────────────────────────────────

pub fn minify_aggressive(content: &str, comments: Option<&[&str]>) -> String {
    let s = if let Some(groups) = comments {
        remove_comments(content, groups)
    } else {
        content.to_owned()
    };
    let s = super::collapse_whitespace(&s);
    let s = super::re_tighten_punct(&s);
    s.trim().to_owned()
}

// ── Code (whitespace only, preserve indent) ───────────────────────────────────

pub fn minify_code_core(content: &str) -> String {
    // Strip trailing whitespace per line, then
    // replace 3+ consecutive newlines (\n\s*\n\s*\n+) with \n\n
    let s = content.replace("\r\n", "\n");
    let lines: Vec<&str> = s.split('\n').collect();
    let mut result = String::with_capacity(s.len());
    let mut consecutive_blanks = 0u32;
    for line in &lines {
        let stripped = line.trim_end_matches([' ', '\t']);
        if stripped.is_empty() {
            consecutive_blanks += 1;
            // allow max 1 blank line (= 2 consecutive \n in output)
            if consecutive_blanks <= 1 {
                result.push('\n');
            }
        } else {
            consecutive_blanks = 0;
            result.push_str(stripped);
            result.push('\n');
        }
    }
    result
        .trim_start_matches('\n')
        .trim_end_matches('\n')
        .to_owned()
}

// ── General (allow indent compression) ───────────────────────────────────────

pub fn minify_general_core(content: &str) -> String {
    let s = content.replace("\r\n", "\n");
    let mut result = String::with_capacity(s.len());
    let mut blank_run = 0u32;
    for line in s.split('\n') {
        let stripped = line.trim_end_matches([' ', '\t']);
        if stripped.is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                result.push('\n');
            }
        } else {
            blank_run = 0;
            // Halve leading whitespace
            let leading = stripped.len() - stripped.trim_start().len();
            let half = leading / 2;
            result.push_str(&" ".repeat(half));
            result.push_str(stripped.trim_start());
            result.push('\n');
        }
    }
    result.trim().to_owned()
}
