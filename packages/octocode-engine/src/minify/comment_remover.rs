/// String-aware comment removal.
/// Comment-removal strategy for supported language comment groups.

// ── Rule types ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct BlockRule {
    pub start: &'static str,
    pub end: &'static str,
    pub nested: bool,
}

#[derive(Clone)]
pub struct LineRule {
    pub token: &'static str,
    pub require_boundary: bool, // false → always strip (e.g. SQL --)
    pub preserve_shebang: bool,
}

#[derive(Clone, Default)]
pub struct CommentRules {
    pub block: Vec<BlockRule>,
    pub line: Vec<LineRule>,
    pub regex: bool,
    pub powershell_here_strings: bool,
    pub quote_delimiters: &'static [&'static str],
}

/// Return the CommentRules for the named CommentPatternGroup.
pub fn rules_for(group: &str) -> Option<CommentRules> {
    match group {
        "c-style" => Some(CommentRules {
            block: vec![BlockRule {
                start: "/*",
                end: "*/",
                nested: false,
            }],
            line: vec![LineRule {
                token: "//",
                require_boundary: true,
                preserve_shebang: false,
            }],
            regex: true,
            ..Default::default()
        }),
        "hash" => Some(CommentRules {
            line: vec![LineRule {
                token: "#",
                require_boundary: true,
                preserve_shebang: true,
            }],
            ..Default::default()
        }),
        "html" => Some(CommentRules {
            block: vec![BlockRule {
                start: "<!--",
                end: "-->",
                nested: false,
            }],
            ..Default::default()
        }),
        "sql" => Some(CommentRules {
            block: vec![BlockRule {
                start: "/*",
                end: "*/",
                nested: false,
            }],
            line: vec![LineRule {
                token: "--",
                require_boundary: false,
                preserve_shebang: false,
            }],
            ..Default::default()
        }),
        "lua" => Some(CommentRules {
            block: vec![BlockRule {
                start: "--[[",
                end: "]]",
                nested: false,
            }],
            line: vec![LineRule {
                token: "--",
                require_boundary: true,
                preserve_shebang: false,
            }],
            ..Default::default()
        }),
        "haskell" => Some(CommentRules {
            block: vec![BlockRule {
                start: "{-",
                end: "-}",
                nested: false,
            }],
            line: vec![LineRule {
                token: "--",
                require_boundary: true,
                preserve_shebang: false,
            }],
            ..Default::default()
        }),
        "semicolon" => Some(CommentRules {
            line: vec![LineRule {
                token: ";",
                require_boundary: true,
                preserve_shebang: false,
            }],
            ..Default::default()
        }),
        // Clojure/ClojureScript: `'` and `` ` `` are quote/syntax-quote prefixes,
        // never string delimiters (strings are `"..."` only) — unlike the
        // `DEFAULT_QUOTE_DELIMITERS` fallback, which would treat a stray `'foo`
        // as an unterminated string and swallow everything after it, including
        // real string literals, so their whitespace never gets protected from
        // the aggressive strategy's collapse/tighten passes.
        "clojure" => Some(CommentRules {
            line: vec![LineRule {
                token: ";",
                require_boundary: true,
                preserve_shebang: false,
            }],
            quote_delimiters: &["\"\"\"", "\""],
            ..Default::default()
        }),
        "wasm-text" => Some(CommentRules {
            block: vec![BlockRule {
                start: "(;",
                end: ";)",
                nested: false,
            }],
            line: vec![LineRule {
                token: ";;",
                require_boundary: true,
                preserve_shebang: false,
            }],
            ..Default::default()
        }),
        "percent" => Some(CommentRules {
            line: vec![LineRule {
                token: "%",
                require_boundary: true,
                preserve_shebang: false,
            }],
            ..Default::default()
        }),
        "haml" => Some(CommentRules {
            line: vec![LineRule {
                token: "-#",
                require_boundary: true,
                preserve_shebang: false,
            }],
            ..Default::default()
        }),
        "slim" => Some(CommentRules {
            line: vec![LineRule {
                token: "/",
                require_boundary: true,
                preserve_shebang: false,
            }],
            ..Default::default()
        }),
        "powershell" => Some(CommentRules {
            block: vec![BlockRule {
                start: "<#",
                end: "#>",
                nested: false,
            }],
            line: vec![LineRule {
                token: "#",
                require_boundary: true,
                preserve_shebang: true,
            }],
            powershell_here_strings: true,
            ..Default::default()
        }),
        "bang" => Some(CommentRules {
            line: vec![LineRule {
                token: "!",
                require_boundary: true,
                preserve_shebang: false,
            }],
            ..Default::default()
        }),
        "apostrophe" => Some(CommentRules {
            line: vec![LineRule {
                token: "'",
                require_boundary: true,
                preserve_shebang: false,
            }],
            quote_delimiters: &["\"\"\"", "\"", "`"],
            ..Default::default()
        }),
        "double-dash" => Some(CommentRules {
            line: vec![LineRule {
                token: "--",
                require_boundary: true,
                preserve_shebang: false,
            }],
            ..Default::default()
        }),
        "fsharp-block" => Some(CommentRules {
            block: vec![BlockRule {
                start: "(*",
                end: "*)",
                nested: true,
            }],
            ..Default::default()
        }),
        "pascal" => Some(CommentRules {
            block: vec![
                BlockRule {
                    start: "(*",
                    end: "*)",
                    nested: true,
                },
                BlockRule {
                    start: "{",
                    end: "}",
                    nested: false,
                },
            ],
            line: vec![LineRule {
                token: "//",
                require_boundary: true,
                preserve_shebang: false,
            }],
            quote_delimiters: &["'", "\""],
            ..Default::default()
        }),
        "template" => Some(CommentRules {
            block: vec![
                BlockRule {
                    start: "{{!--",
                    end: "--}}",
                    nested: false,
                },
                BlockRule {
                    start: "{{!",
                    end: "}}",
                    nested: false,
                },
                BlockRule {
                    start: "<%#",
                    end: "%>",
                    nested: false,
                },
                BlockRule {
                    start: "{#",
                    end: "#}",
                    nested: false,
                },
            ],
            ..Default::default()
        }),
        _ => None,
    }
}

const DEFAULT_QUOTE_DELIMITERS: &[&str] = &["\"\"\"", "'''", "\"", "'", "`"];

fn effective_delimiters(rules: &CommentRules) -> &[&'static str] {
    if rules.quote_delimiters.is_empty() {
        DEFAULT_QUOTE_DELIMITERS
    } else {
        rules.quote_delimiters
    }
}

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
                    pos += next_char_len(bytes, pos);
                }
            }
            // ── inside single-char string ─────────────────────────────────
            QuoteState::Single { delim, escaped } => {
                let ch = bytes[pos];
                let ch_len = next_char_len(bytes, pos);
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
                // 1. PowerShell here-string
                if rules.powershell_here_strings {
                    if let Some(end_pos) = find_powershell_here_string(content, pos) {
                        result.push_str(&content[pos..end_pos]);
                        pos = end_pos;
                        continue;
                    }
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
                if rules.regex {
                    if let Some(end_pos) = find_regex_literal(content, pos) {
                        result.push_str(&content[pos..end_pos]);
                        pos = end_pos;
                        continue;
                    }
                }

                // 5. String-start delimiter (longest match first)
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
                    find_block_comment(content, pos, &rules.block)
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
                if let Some(()) = find_line_comment(content, pos, &rules.line) {
                    let skip_to = content[pos..].find('\n').map(|i| pos + i).unwrap_or(len);
                    pos = skip_to;
                    continue;
                }

                // 8. Ordinary character
                let ch_len = next_char_len(bytes, pos);
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

    while pos < len {
        match &mut qstate {
            QuoteState::Multi { end } => {
                let end_str: &'static str = end;
                if content[pos..].starts_with(end_str) {
                    pos += end_str.len();
                    ranges.push((cur_start, pos));
                    qstate = QuoteState::Outside;
                } else {
                    pos += next_char_len(bytes, pos);
                }
            }
            QuoteState::Single { delim, escaped } => {
                let ch = bytes[pos];
                let ch_len = next_char_len(bytes, pos);
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
                if rules.powershell_here_strings {
                    if let Some(end_pos) = find_powershell_here_string(content, pos) {
                        ranges.push((pos, end_pos));
                        pos = end_pos;
                        continue;
                    }
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
                if rules.regex {
                    if let Some(end_pos) = find_regex_literal(content, pos) {
                        ranges.push((pos, end_pos));
                        pos = end_pos;
                        continue;
                    }
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
                pos += next_char_len(bytes, pos);
            }
        }
    }
    ranges
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn next_char_len(bytes: &[u8], pos: usize) -> usize {
    let b = bytes[pos];
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

fn find_block_comment(content: &str, pos: usize, rules: &[BlockRule]) -> Option<(usize, bool)> {
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

fn find_nested_block_end(content: &str, start: usize, open: &str, close: &str) -> usize {
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
            pos += next_char_len(content.as_bytes(), pos);
        }
    }
    content.len()
}

fn has_line_boundary(content: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let prev = content.as_bytes()[pos - 1];
    matches!(prev, b' ' | b'\t' | b'\n' | b'\r')
}

fn find_line_comment(content: &str, pos: usize, rules: &[LineRule]) -> Option<()> {
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

fn find_powershell_here_string(content: &str, pos: usize) -> Option<usize> {
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

fn find_rust_raw_string(content: &str, pos: usize) -> Option<usize> {
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

fn find_csharp_verbatim(content: &str, pos: usize) -> Option<usize> {
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
            i += next_char_len(bytes, i);
        }
    }
    Some(content.len())
}

fn find_regex_literal(content: &str, pos: usize) -> Option<usize> {
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
            escaped = false;
            i += 1;
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
        i += next_char_len(bytes, i);
    }
    None
}

// ── Python docstrings ─────────────────────────────────────────────────────────

pub fn strip_python_docstrings(content: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let blank: &str = "";
    let mut i = 0;

    let prev_code_line = |idx: usize, lines: &[&str]| -> &'static str {
        for j in (0..idx).rev() {
            let t = lines[j].trim();
            if !t.is_empty() && !t.starts_with('#') {
                return "NON_EMPTY"; // just signals "something is there"
            }
        }
        ""
    };

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let delim = if trimmed.starts_with("\"\"\"") {
                "\"\"\""
            } else {
                "'''"
            };
            let prev = prev_code_line(i, &lines);
            let is_doc = prev.is_empty() || {
                // Check if previous code line ends with ':'
                let mut code_line = "";
                for j in (0..i).rev() {
                    let t = lines[j].trim();
                    if !t.is_empty() && !t.starts_with('#') {
                        code_line = t;
                        break;
                    }
                }
                code_line.ends_with(':') || code_line.is_empty()
            };

            if is_doc {
                let after_open = trimmed.get(3..).unwrap_or("");
                out.push(blank); // preserve line count
                i += 1;
                if !after_open.contains(delim) {
                    while i < lines.len() {
                        out.push(blank);
                        let nl = lines[i];
                        i += 1;
                        if nl.contains(delim) {
                            break;
                        }
                    }
                }
                continue;
            }
        }
        out.push(line);
        i += 1;
    }
    out.join("\n")
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
            result = strip_string_aware_comments(&result, &rules);
        }
        // Unknown group → skip silently (matches TS behaviour)
    }
    result
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_style_line_and_block_comments_removed() {
        let src = "int x = 1; // comment\nint y = 2; /* block */ int z;";
        let out = remove_comments(src, &["c-style"]);
        assert!(!out.contains("comment"));
        assert!(!out.contains("block"));
        assert!(out.contains("int x"));
        assert!(out.contains("int z"));
    }

    #[test]
    fn hash_comments_removed_code_preserved() {
        let src = "x = 1 # inline\n# full line\ny = 2";
        let out = remove_comments(src, &["hash"]);
        assert!(!out.contains("inline"));
        assert!(out.contains("x = 1"));
        assert!(out.contains("y = 2"));
    }

    #[test]
    fn comment_markers_inside_strings_preserved() {
        let src = "const url = \"http://x\"; // real comment";
        let out = remove_comments(src, &["c-style"]);
        assert!(
            out.contains("http://x"),
            "string content must survive: '{out}'"
        );
        assert!(!out.contains("real comment"));
    }

    #[test]
    fn unknown_comment_group_returns_content_unchanged() {
        assert_eq!(remove_comments("hello", &["nonexistent-type"]), "hello");
    }

    #[test]
    fn literal_ranges_finds_quoted_strings() {
        let rules = rules_for("c-style").unwrap();
        let content = r#"a = "hello world"; b = 1;"#;
        let ranges = literal_ranges(content, &rules);
        assert_eq!(ranges.len(), 1);
        let (start, end) = ranges[0];
        assert_eq!(&content[start..end], "\"hello world\"");
    }

    #[test]
    fn literal_ranges_finds_js_regex_literal() {
        let rules = rules_for("c-style").unwrap();
        let content = "const re = /a\\/b*c/g;";
        let ranges = literal_ranges(content, &rules);
        assert_eq!(ranges.len(), 1);
        let (start, end) = ranges[0];
        assert_eq!(&content[start..end], "/a\\/b*c/g");
    }

    #[test]
    fn literal_ranges_ignores_content_with_no_strings() {
        let rules = rules_for("hash").unwrap();
        assert!(literal_ranges("x = 1\ny = 2", &rules).is_empty());
    }
}
