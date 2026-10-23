//! Recognizers for the comment, string and regex constructs the scans skip.

use super::rules::{BlockRule, LineRule};
use crate::minify::strategies::utf8_seq_len;

pub(super) fn find_block_comment(
    content: &str,
    pos: usize,
    rules: &[BlockRule],
) -> Option<(usize, bool)> {
    for rule in rules {
        if !content[pos..].starts_with(rule.start) {
            continue;
        }
        let after = pos + rule.start.len();
        let end_pos = if rule.nested {
            find_nested_block_end(content, after, rule.start, rule.end)
        } else {
            content[after..]
                .find(rule.end)
                .map(|i| after + i + rule.end.len())
                .unwrap_or(content.len())
        };
        return Some((end_pos, true));
    }
    None
}

pub(super) fn find_nested_block_end(content: &str, start: usize, open: &str, close: &str) -> usize {
    let mut depth = 1usize;
    let mut pos = start;
    while pos < content.len() {
        if content[pos..].starts_with(open) {
            depth += 1;
            pos += open.len();
        } else if content[pos..].starts_with(close) {
            depth -= 1;
            pos += close.len();
            if depth == 0 {
                return pos;
            }
        } else {
            pos += utf8_seq_len(content.as_bytes()[pos]);
        }
    }
    content.len()
}

pub(super) fn has_line_boundary(content: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let prev = content.as_bytes()[pos - 1];
    matches!(prev, b' ' | b'\t' | b'\n' | b'\r')
}

pub(super) fn find_line_comment(content: &str, pos: usize, rules: &[LineRule]) -> Option<()> {
    for rule in rules {
        if !content[pos..].starts_with(rule.token) {
            continue;
        }
        if rule.preserve_shebang && content[pos..].starts_with("#!") {
            continue;
        }
        if rule.require_boundary && !has_line_boundary(content, pos) {
            continue;
        }
        return Some(());
    }
    None
}

pub(super) fn find_powershell_here_string(content: &str, pos: usize) -> Option<usize> {
    let quote = if content[pos..].starts_with("@\"") {
        '"'
    } else if content[pos..].starts_with("@'") {
        '\''
    } else {
        return None;
    };
    let after = pos + 2;
    let b = content.as_bytes().get(after)?;
    if *b != b'\n' && *b != b'\r' {
        return None;
    }
    let end_marker = format!("\n{}'", quote);
    content[after..]
        .find(&end_marker)
        .map(|i| after + i + end_marker.len())
}

pub(super) fn find_rust_raw_string(content: &str, pos: usize) -> Option<usize> {
    let rest = &content[pos..];
    if !rest.starts_with("r#")
        && !rest.starts_with("r\"")
        && !rest.starts_with("br#")
        && !rest.starts_with("br\"")
    {
        return None;
    }
    // Scan hashes
    let mut hi = if rest.starts_with('b') { 2 } else { 1 };
    while rest.as_bytes().get(hi) == Some(&b'#') {
        hi += 1;
    }
    if rest.as_bytes().get(hi) != Some(&b'"') {
        return None;
    }
    let hash_count = hi - if rest.starts_with('b') { 2 } else { 1 };
    let body_start = pos + hi + 1;
    let end_marker = format!("\"{}", "#".repeat(hash_count));
    content[body_start..]
        .find(&end_marker)
        .map(|i| body_start + i + end_marker.len())
}

pub(super) fn find_csharp_verbatim(content: &str, pos: usize) -> Option<usize> {
    let body_start = if content[pos..].starts_with("@\"") {
        pos + 2
    } else if content[pos..].starts_with("$@\"") || content[pos..].starts_with("@$\"") {
        pos + 3
    } else {
        return None;
    };
    let bytes = content.as_bytes();
    let mut i = body_start;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            if bytes.get(i + 1) == Some(&b'"') {
                i += 2; // escaped quote
            } else {
                return Some(i + 1);
            }
        } else {
            i += utf8_seq_len(bytes[i]);
        }
    }
    Some(content.len())
}

pub(super) fn find_regex_literal(content: &str, pos: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    if bytes.get(pos) != Some(&b'/') {
        return None;
    }
    let next = bytes.get(pos + 1)?;
    if *next == b'/' || *next == b'*' {
        return None;
    }
    // Check preceding token to see if `/` could be regex
    let prev_pos = {
        let mut p = pos.saturating_sub(1);
        while p > 0 && matches!(content.as_bytes()[p], b' ' | b'\t') {
            p -= 1;
        }
        p
    };
    if pos > 0 {
        let prev = bytes[prev_pos];
        let regex_ok = matches!(
            prev,
            b'(' | b'['
                | b'{'
                | b'='
                | b','
                | b':'
                | b';'
                | b'!'
                | b'&'
                | b'|'
                | b'?'
                | b'+'
                | b'-'
                | b'*'
                | b'~'
                | b'^'
                | b'<'
                | b'>'
        );
        let keyword_before = {
            let before = &content[..=prev_pos];
            before.ends_with("return")
                || before.ends_with("throw")
                || before.ends_with("typeof")
                || before.ends_with("case")
                || before.ends_with("yield")
                || before.ends_with("await")
        };
        if !regex_ok && !keyword_before {
            return None;
        }
    }
    // Scan regex body
    let mut i = pos + 1;
    let mut escaped = false;
    let mut in_class = false;
    while i < bytes.len() {
        let c = bytes[i];
        if escaped {
            // Step over the whole escaped character: one byte would land
            // inside a multibyte char and skip past the closing `/`.
            escaped = false;
            i += utf8_seq_len(bytes[i]);
            continue;
        }
        if c == b'\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if c == b'[' {
            in_class = true;
            i += 1;
            continue;
        }
        if c == b']' {
            in_class = false;
            i += 1;
            continue;
        }
        if c == b'/' && !in_class {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            return Some(i);
        }
        if c == b'\n' || c == b'\r' {
            return None;
        }
        i += utf8_seq_len(bytes[i]);
    }
    None
}
