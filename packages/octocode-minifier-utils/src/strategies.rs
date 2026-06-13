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
            if blank_run <= 2 { result.push('\n'); }
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
    // Collapse whitespace sequences to single space, tighten punctuation
    let s = collapse_whitespace(&s);
    let s = re_tighten_punct(&s);
    s.trim().to_owned()
}

fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !ws { result.push(' '); ws = true; }
        } else {
            ws = false;
            result.push(ch);
        }
    }
    result
}

/// Byte length of the UTF-8 sequence starting with `b`. Callers only invoke
/// this on sequence-lead bytes (valid `&str` + always advancing by full
/// sequences), so continuation bytes never reach it.
#[inline]
pub(crate) fn utf8_seq_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Copy the full UTF-8 sequence at byte `i` of `s` into `result`; returns the
/// index of the next sequence. Keeps byte-level scanners multibyte-safe: all
/// branch decisions compare ASCII bytes (which never occur inside a multibyte
/// sequence), and passthrough copies whole sequences instead of `u8 as char`.
#[inline]
fn copy_seq(s: &str, i: usize, result: &mut String) -> usize {
    let len = utf8_seq_len(s.as_bytes()[i]).min(s.len() - i);
    result.push_str(&s[i..i + len]);
    i + len
}

fn re_tighten_punct(s: &str) -> String {
    // Remove spaces around {}:;, and ><
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b' '
            && matches!(bytes.get(i + 1).copied(), Some(b'{' | b'}' | b':' | b';' | b',' | b'<' | b'>'))
        {
            i += 1; continue;
        }
        if matches!(b, b'{' | b'}' | b':' | b';' | b',') && bytes.get(i + 1) == Some(&b' ') {
            result.push(b as char);
            i += 2; continue;
        }
        if b == b'>' && bytes.get(i + 1) == Some(&b' ') && bytes.get(i + 2) == Some(&b'<') {
            result.push('>');
            i += 2; continue;
        }
        i = copy_seq(s, i, &mut result);
    }
    result
}

// ── JSON ─────────────────────────────────────────────────────────────────────

pub fn minify_json_core_inner(content: &str) -> (String, bool) {
    // Try direct parse first
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        return (serde_json::to_string(&v).unwrap_or_else(|_| content.trim().to_owned()), false);
    }
    // JSONC / JSON5: strip comments + trailing commas then parse
    let cleaned = strip_json_noise(content);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&cleaned) {
        (serde_json::to_string(&v).unwrap_or_else(|_| content.trim().to_owned()), false)
    } else {
        (content.trim().to_owned(), false)
    }
}

pub fn minify_json_readable_inner(content: &str) -> (String, bool) {
    if serde_json::from_str::<serde_json::Value>(content).is_ok() {
        return (content.to_owned(), false); // already clean JSON — return as-is
    }
    let cleaned = strip_json_noise(content);
    let cleaned = cleaned
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    // collapse ≥3 blank lines
    let mut result = String::with_capacity(cleaned.len());
    let mut blanks = 0u32;
    for line in cleaned.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks <= 2 { result.push('\n'); }
        } else {
            blanks = 0;
            result.push_str(line);
            result.push('\n');
        }
    }
    (result.trim().to_owned(), false)
}

fn strip_json_noise(s: &str) -> String {
    let after_comments = strip_json_comments(s);
    strip_trailing_commas(&after_comments)
}

fn strip_json_comments(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut result = String::with_capacity(content.len());
    let mut i = 0;
    let mut in_str = false;
    let mut escaped = false;
    while i < bytes.len() {
        let ch = bytes[i];
        if in_str {
            if escaped { escaped = false; }
            else if ch == b'\\' { escaped = true; }
            else if ch == b'"' { in_str = false; }
            i = copy_seq(content, i, &mut result);
            continue;
        }
        if ch == b'"' { in_str = true; result.push('"'); i += 1; continue; }
        if ch == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
            continue;
        }
        if ch == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') { i += 1; }
            if i + 1 < bytes.len() { i += 2; }
            continue;
        }
        i = copy_seq(content, i, &mut result);
    }
    result
}

fn strip_trailing_commas(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut result = String::with_capacity(content.len());
    let mut i = 0;
    let mut in_str = false;
    let mut escaped = false;
    while i < bytes.len() {
        let ch = bytes[i];
        if in_str {
            if escaped { escaped = false; }
            else if ch == b'\\' { escaped = true; }
            else if ch == b'"' || ch == b'\'' { in_str = false; }
            i = copy_seq(content, i, &mut result);
            continue;
        }
        if ch == b'"' || ch == b'\'' { in_str = true; result.push(ch as char); i += 1; continue; }
        if ch == b',' {
            let mut look = i + 1;
            while look < bytes.len() && matches!(bytes[look], b' '|b'\t'|b'\n'|b'\r') { look += 1; }
            if look < bytes.len() && matches!(bytes[look], b'}'|b']') { i += 1; continue; }
        }
        i = copy_seq(content, i, &mut result);
    }
    result
}

// ── Code (whitespace only, preserve indent) ───────────────────────────────────

pub fn minify_code_core(content: &str) -> String {
    // mirrors TS: strip trailing whitespace per line, then
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
            if consecutive_blanks <= 1 { result.push('\n'); }
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
            if blank_run <= 2 { result.push('\n'); }
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

// ── Markdown ─────────────────────────────────────────────────────────────────

pub fn minify_markdown_core(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n");
    let source: Vec<&str> = normalized.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(source.len());
    let mut fence: Option<FenceState> = None;
    let mut in_html_comment = false;
    let mut in_generated_toc = false;
    let src = &source;
    let first_content = src.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
    let mut in_frontmatter = first_content == 0
        && src.first().map(|l| l.trim_end() == "---").unwrap_or(false);

    let mut i = 0;
    while i < src.len() {
        let original = src[i];

        // Inside code fence
        if let Some(ref f) = fence {
            append_md(&mut out, original, true);
            if is_fence_close(original, f) { fence = None; }
            i += 1; continue;
        }
        // Fence start
        if let Some(f) = fence_start(original) {
            fence = Some(f);
            append_md(&mut out, original.trim_end(), true);
            i += 1; continue;
        }
        // Indented code
        if original.starts_with("    ") || original.starts_with('\t') {
            append_md(&mut out, original, true);
            i += 1; continue;
        }
        // Frontmatter
        if in_frontmatter {
            append_md(&mut out, original.trim_end(), false);
            if i > 0 && (original.trim_end() == "---" || original.trim_end() == "...") {
                in_frontmatter = false;
            }
            i += 1; continue;
        }
        // Generated TOC
        if in_generated_toc {
            if is_toc_end(original) { in_generated_toc = false; append_md(&mut out, "", false); }
            i += 1; continue;
        }
        if is_toc_start(original) {
            in_generated_toc = !is_toc_end(original);
            append_md(&mut out, "", false);
            i += 1; continue;
        }
        // Strip HTML comments
        let (stripped, still_in_comment) = strip_md_html_comment(original, in_html_comment);
        in_html_comment = still_in_comment;
        let line = &stripped;
        // Pseudo-comment or badge line
        if is_pseudo_comment(line) || is_badge_line(line) {
            append_md(&mut out, "", false);
            i += 1; continue;
        }
        // Setext heading conversion
        if let Some(level) = setext_level(line) {
            if convert_setext(&mut out, level) { i += 1; continue; }
        }
        // Thematic break
        if is_thematic_break(line) {
            append_md(&mut out, "---", false);
            i += 1; continue;
        }
        // Table row
        let is_table = is_delimiter_row(line)
            || src.get(i.saturating_sub(1)).is_some_and(|l| is_delimiter_row(l))
            || src.get(i + 1).is_some_and(|l| is_delimiter_row(l));
        let compacted = if is_table {
            compact_table_row(line.trim_end())
        } else {
            compact_md_line(line)
        };
        append_md(&mut out, &compacted, false);
        i += 1;
    }
    let joined = out.join("\n");
    joined.trim().to_owned()
}

// ── Markdown helpers ──────────────────────────────────────────────────────────

struct FenceState { marker: char, length: usize }

fn fence_start(line: &str) -> Option<FenceState> {
    let leading = line.len() - line.trim_start().len();
    if leading > 3 { return None; }
    let rest = line.trim_start();
    let marker = rest.chars().next()?;
    if marker != '`' && marker != '~' { return None; }
    let length = rest.chars().take_while(|&c| c == marker).count();
    if length >= 3 { Some(FenceState { marker, length }) } else { None }
}

fn is_fence_close(line: &str, f: &FenceState) -> bool {
    let rest = line.trim_start();
    let count = rest.chars().take_while(|&c| c == f.marker).count();
    if count < f.length { return false; }
    rest[count..].trim().is_empty()
}

fn is_thematic_break(line: &str) -> bool {
    let compact: String = line.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 3 { return false; }
    let Some(m) = compact.chars().next() else { return false; };
    (m == '-' || m == '_' || m == '*') && compact.chars().all(|c| c == m)
}

fn setext_level(line: &str) -> Option<u8> {
    let t = line.trim();
    if t.chars().all(|c| c == '=') && !t.is_empty() { Some(1) }
    else if t.chars().all(|c| c == '-') && !t.is_empty() { Some(2) }
    else { None }
}

fn convert_setext(out: &mut Vec<String>, level: u8) -> bool {
    let prefix = if level == 1 { "# " } else { "## " };
    let mut heading_lines: Vec<String> = Vec::new();
    while let Some(l) = out.last() {
        if l.trim().is_empty() { break; }
        if let Some(line) = out.pop() { heading_lines.push(line); }
    }
    if heading_lines.is_empty() { return false; }
    heading_lines.reverse();
    let text = heading_lines.iter().map(|l| l.trim().to_owned()).collect::<Vec<_>>().join(" ");
    let candidate = format!("{}{}", prefix, text);
    append_md(out, &candidate, false);
    true
}

fn is_toc_start(line: &str) -> bool {
    let toc = regex_like_toc(line);
    toc.0
}
fn is_toc_end(line: &str) -> bool { regex_like_toc(line).1 }
fn regex_like_toc(line: &str) -> (bool, bool) {
    let lower = line.to_lowercase();
    let is_end   = lower.contains("<!-- end") || lower.contains("<!-- /toc") || lower.contains("tocstop");
    let is_start = !is_end && (lower.contains("<!-- toc") || lower.contains("<!-- table of contents")
        || lower.contains("<!-- doctoc") || lower.contains("<!-- markdown-toc"));
    (is_start, is_end)
}

fn strip_md_html_comment(line: &str, mut in_comment: bool) -> (String, bool) {
    let mut output = String::new();
    let mut cur = 0usize;
    let bytes = line.as_bytes();
    while cur < bytes.len() {
        if in_comment {
            if let Some(pos) = &line[cur..].find("-->") {
                cur += pos + 3;
                in_comment = false;
            } else {
                return (output, true);
            }
        } else {
            if let Some(pos) = line[cur..].find("<!--") {
                output.push_str(&line[cur..cur + pos]);
                let rest = &line[cur + pos..];
                if let Some(end) = rest.find("-->") {
                    cur += pos + end + 3;
                } else {
                    return (output, true);
                }
            } else {
                output.push_str(&line[cur..]);
                break;
            }
        }
    }
    (output, in_comment)
}

fn is_pseudo_comment(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("[//]: #")
}

fn is_badge_line(line: &str) -> bool {
    let t = line.trim();
    let badge_domains = ["img.shields.io","badge.fury.io","badgen.net","codecov.io","coveralls.io","circleci.com","travis-ci"];
    // Find all image urls
    let images: Vec<_> = t.match_indices("![").collect();
    if images.is_empty() { return false; }
    // Check if every image URL is a badge
    let mut has_badge = false;
    for (_, _) in &images {
        let all_badges = badge_domains.iter().any(|d| t.contains(d));
        if all_badges { has_badge = true; }
    }
    has_badge && {
        // Check nothing is left after removing badge images
        let mut cleaned = t.to_owned();
        // remove [![...](...)(...)] and ![![...](...)]
        while let Some(start) = cleaned.find("![") {
            if let Some(end) = cleaned[start..].find(')') {
                cleaned.replace_range(start..start + end + 1, "");
            } else { break; }
        }
        cleaned.trim().is_empty()
    }
}

fn is_delimiter_row(line: &str) -> bool {
    let parts: Vec<&str> = line.trim().split('|').filter(|p| !p.is_empty()).collect();
    parts.len() >= 2 && parts.iter().all(|p| {
        let t = p.trim();
        t.starts_with(':') || t.ends_with(':') || t.trim_matches('-').trim_matches(':').is_empty()
    })
}

fn compact_table_row(line: &str) -> String {
    line.split('|').map(|p| p.trim()).collect::<Vec<_>>().join("|")
}

fn compact_md_line(line: &str) -> String {
    // Compact heading, list markers, blockquote, trim trailing whitespace
    let s = line.trim_end();
    // Setext-like lines handled above; just tighten multiple spaces inside text
    let mut result = s.to_owned();
    // Compact ATX heading
    if result.starts_with('#') {
        // allow up to 6 #
        let hash_count = result.chars().take_while(|&c| c == '#').count().min(6);
        let text = result[hash_count..].trim().trim_end_matches('#').trim();
        if !text.is_empty() {
            result = format!("{} {}", "#".repeat(hash_count), text);
        }
    }
    result
}

fn append_md(out: &mut Vec<String>, line: &str, preserve_blank: bool) {
    if line.trim().is_empty() {
        if preserve_blank || out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
    } else {
        out.push(line.to_owned());
    }
}

// ── CSS ─────────────────────────────────────────────────────────────────────────

/// Regex fallback — always available, fast.
pub fn minify_css_core(content: &str) -> String {
    let s = remove_comments(content, &["c-style"]);
    let s = collapse_whitespace(&s);
    let s = re_tighten_punct(&s);
    s.trim().to_owned()
}

/// High-quality CSS minification via lightningcss (100× better than regex).
/// Falls back to `minify_css_core` on parse error.
pub fn minify_css_quality(content: &str) -> String {
    use lightningcss::stylesheet::{StyleSheet, ParserOptions, PrinterOptions};
    match StyleSheet::parse(content, ParserOptions::default()) {
        Ok(ss) => ss.to_css(PrinterOptions { minify: true, ..Default::default() })
                    .map(|out| out.code.to_string())
                    .unwrap_or_else(|_| minify_css_core(content)),
        Err(_) => minify_css_core(content),
    }
}

// ── HTML ─────────────────────────────────────────────────────────────────────────

/// Regex fallback — always available.
pub fn minify_html_core(content: &str) -> String {
    let s = remove_comments(content, &["html"]);
    let s = collapse_whitespace(&s);
    let s = s.replace("> <", "><");
    s.trim().to_owned()
}

/// High-quality HTML minification via minify-html crate.
/// Falls back to `minify_html_core` on error.
pub fn minify_html_quality(content: &str) -> String {
    use minify_html::{Cfg, minify};
    let cfg = Cfg { minify_css: true, minify_js: false, ..Cfg::default() };
    let out = minify(content.as_bytes(), &cfg);
    String::from_utf8(out).unwrap_or_else(|_| minify_html_core(content))
}

// ── OXC JS/TS AST minifier ─────────────────────────────────────────────────────────────────────

/// OXC-backed JS/TS minification.
///
/// `mangle = true`: rename locals (maximum compression, for full minify path).
/// `mangle = false`: preserve names (agent-readable, for content-view path).
///
/// Returns `None` on parse error so callers can fall back gracefully.
pub fn minify_js_oxc(content: &str, file_path: &str, mangle: bool) -> Option<String> {
    use oxc_allocator::Allocator;
    use oxc_codegen::{Codegen, CodegenOptions, CommentOptions};
    use oxc_minifier::{CompressOptions, MangleOptions, Minifier, MinifierOptions};
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    let allocator = Allocator::default();
    let ext = crate::file_extension::get_extension_internal(file_path, true, "js");
    let source_type = match ext.as_str() {
        "ts"  => SourceType::ts(),
        "tsx" => SourceType::tsx(),
        "jsx" => SourceType::jsx(),
        "mjs" => SourceType::mjs(),
        _     => SourceType::default(), // js / cjs
    };

    let parser_ret = Parser::new(&allocator, content, source_type).parse();
    // If plain-JS parse failed (e.g. Flow-annotated file with `import type`), retry as TS.
    let parser_ret = if !parser_ret.errors.is_empty()
        && matches!(ext.as_str(), "js" | "cjs")
    {
        Parser::new(&allocator, content, SourceType::ts()).parse()
    } else {
        parser_ret
    };
    if !parser_ret.errors.is_empty() { return None; }

    let mut program = parser_ret.program;

    // P1: Strip TypeScript-only top-level statements before code generation.
    // Removes: `import type`, `interface`, `type alias`, TS-declare statements.
    //
    // Note: Statement::is_typescript_syntax() returns false for ImportDeclaration
    // even when import_kind == Type (OXC considers it valid syntax for other purposes).
    // We therefore write an explicit match.
    use oxc_ast::ast::{ImportOrExportKind, Statement as Stmt};
    program.body.retain(|stmt| match stmt {
        // `import type { Foo } from '...'` — no runtime value
        Stmt::ImportDeclaration(decl) => decl.import_kind != ImportOrExportKind::Type,
        // All other TS-only nodes (interface, type alias, declare, etc.)
        _ => !stmt.is_typescript_syntax(),
    });

    // Use safest() for compression + mangle for variable renaming.
    // safest() = keep all code, join vars, comma sequences — no dead-code removal
    // (avoids OXC 0.95 bug where CompressOptions::default() can produce empty output).
    Minifier::new(MinifierOptions {
        mangle:   if mangle { Some(MangleOptions::default()) } else { None },
        compress: Some(CompressOptions::safest()),
    }).minify(&allocator, &mut program);

    let codegen_opts = CodegenOptions {
        minify:   true,
        // Strip ALL comment classes — the "standard" view contract removes
        // known language comments (normal + jsdoc default to true in oxc).
        comments: CommentOptions {
            normal: false,
            jsdoc: false,
            annotation: false,
            ..CommentOptions::default()
        },
        ..CodegenOptions::default()
    };
    let result = Codegen::new().with_options(codegen_opts).build(&program).code;
    // Guard: never return empty output — that signals OXC mangle produced broken code.
    // Also fall back gracefully if OXC grew the content (shouldn't happen but be safe).
    if result.is_empty() || result.len() >= content.len() { return None; }
    Some(result)
}

pub fn minify_javascript_core(content: &str) -> String {
    let s = remove_comments(content, &["c-style"]);
    let s = collapse_whitespace(&s);
    let s = re_tighten_punct_js(&s);
    // Split back to lines, drop empty
    s.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn re_tighten_punct_js(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b' '
            && matches!(bytes.get(i + 1).copied(), Some(b'{' | b'}' | b'(' | b')' | b';' | b',' | b':'))
        {
            i += 1; continue;
        }
        if matches!(b, b'{' | b'}' | b'(' | b')' | b';' | b',')
            && bytes.get(i + 1) == Some(&b' ') {
                result.push(b as char);
                i += 2; continue;
            }
        i = copy_seq(s, i, &mut result);
    }
    result
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── JSON ──────────────────────────────────────────────────────────────────
    #[test]
    fn json_core_compacts_valid_json() {
        let (out, failed) = minify_json_core_inner("{\"a\": 1,  \"b\": 2 }");
        assert_eq!(out, r#"{"a":1,"b":2}"#);
        assert!(!failed);
    }

    #[test]
    fn json_core_strips_jsonc_comments_and_trailing_commas() {
        let src = "{ // comment\n  \"key\": \"value\", // trailing comma\n}";
        let (out, failed) = minify_json_core_inner(src);
        assert!(!failed);
        assert!(out.contains("key"));
        assert!(!out.contains("comment"));
    }

    #[test]
    fn json_core_preserves_non_ascii_through_jsonc_strip() {
        let (out, failed) = minify_json_core_inner("{\n  // comment\n  \"k\": \"café\",\n}");
        assert!(!failed);
        assert!(out.contains("café"), "JSONC strip corrupted UTF-8: '{out}'");
        assert!(!out.contains('Ã'), "Latin-1 mojibake detected: '{out}'");
    }

    // ── conservative ──────────────────────────────────────────────────────────
    #[test]
    fn conservative_strips_c_style_comments() {
        let out = minify_conservative("int x; // comment\nint y;", Some(&["c-style"]));
        assert!(!out.contains("comment"));
        assert!(out.contains("int x"));
    }

    #[test]
    fn conservative_strips_hash_comments_preserving_code() {
        let out = minify_conservative("x = 1 # comment\n# full line\ny = 2", Some(&["hash"]));
        assert!(!out.contains("comment"));
        assert!(!out.contains("full line"));
        assert!(out.contains("x = 1") && out.contains("y = 2"));
    }

    // ── code core ─────────────────────────────────────────────────────────────
    #[test]
    fn code_core_collapses_blank_runs_to_one() {
        // Mirrors the TS contract exactly: "a\n\n\n\nb" → "a\n\nb"
        assert_eq!(minify_code_core("a\n\n\n\nb"), "a\n\nb");
    }

    // ── aggressive: UTF-8 safety through collapse + punct tightening ──────────
    #[test]
    fn aggressive_preserves_non_ascii() {
        let out = minify_aggressive("local s = \"café → naïve\" { x = 1 }", None);
        assert!(out.contains("café → naïve"), "aggressive path corrupted UTF-8: '{out}'");
        assert!(!out.contains('Ã'), "Latin-1 mojibake detected: '{out}'");
    }

    // ── javascript core ───────────────────────────────────────────────────────
    #[test]
    fn javascript_core_preserves_non_ascii_and_strips_comments() {
        let out = minify_javascript_core("const s = \"café\"; // strip me");
        assert!(out.contains("café"), "punct tightening corrupted UTF-8: '{out}'");
        assert!(!out.contains("strip me"));
    }

    // ── markdown ──────────────────────────────────────────────────────────────
    #[test]
    fn markdown_normalizes_crlf_without_carriage_returns() {
        // leak() removal itself is enforced by clippy disallowed-methods.
        let out = minify_markdown_core("# Title\r\n\r\nSome text  \r\n");
        assert!(out.contains("# Title"));
        assert!(out.contains("Some text"));
        assert!(!out.contains('\r'));
    }
}
