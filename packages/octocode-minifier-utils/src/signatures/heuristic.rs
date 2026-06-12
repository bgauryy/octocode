//! Per-language heuristic signature extractors.
//! Used as fallback when tree-sitter is not available for a language,
//! and as the primary extractor for many non-C-family languages.
//!
//! All strategies are ported directly from the TS `extractSignatures.ts`
//! line-pattern / heuristic implementations.

use regex::Regex;
use std::sync::OnceLock;

// ── shared utility ────────────────────────────────────────────────────────────

fn brace_delta(line: &str) -> i32 {
    let code = line.split("//").next().unwrap_or(line);
    code.chars().filter(|&c| c == '{').count() as i32
        - code.chars().filter(|&c| c == '}').count() as i32
}

fn round_delta(line: &str) -> i32 {
    line.chars().filter(|&c| c == '(').count() as i32
        - line.chars().filter(|&c| c == ')').count() as i32
}

/// Keep lines matching any of `patterns`, extending across unbalanced parens.
fn extract_line_pattern(
    content: &str,
    patterns: &[Regex],
    comment_strip: impl Fn(&str) -> bool,
) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if !trimmed.is_empty()
            && !comment_strip(trimmed)
            && patterns.iter().any(|p| p.is_match(line))
        {
            kept.push((i + 1, line.trim_end().to_string()));
            // Extend across multi-line parameter list
            let mut depth = round_delta(line);
            while depth > 0 && i + 1 < lines.len() {
                i += 1;
                let cont = lines[i];
                kept.push((i + 1, cont.trim_end().to_string()));
                depth += round_delta(cont);
            }
        }
        i += 1;
    }

    if kept.is_empty() { None } else { Some(kept) }
}

fn c_comment(t: &str) -> bool { t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') }
fn hash_comment(t: &str) -> bool { t.starts_with('#') && !t.starts_with("#!") }

// ── Kotlin / Java / C# ───────────────────────────────────────────────────────

fn java_cs_patterns() -> &'static Vec<Regex> {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| vec![
        Regex::new(r"^\s*(public|private|protected|static|abstract|final|override|sealed|internal)\s+").unwrap(),
        Regex::new(r"^\s*(class|interface|enum|record|object)\s+\w+").unwrap(),
        Regex::new(r"^\s*(import|using|package|namespace)\s+").unwrap(),
    ])
}

pub fn extract_kotlin_java_cs(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, java_cs_patterns(), c_comment)
}

// ── Scala ─────────────────────────────────────────────────────────────────────

fn scala_patterns() -> &'static Vec<Regex> {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| vec![
        Regex::new(r"^\s*(package|import)\s+").unwrap(),
        Regex::new(r"^\s*(sealed\s+|abstract\s+|final\s+|case\s+)*(class|object|trait|enum)\s+\w+").unwrap(),
        Regex::new(r"^\s*(override\s+|private\s+|protected\s+|implicit\s+|given\s+)*(def|val|var|type)\s+\w+").unwrap(),
    ])
}

pub fn extract_scala(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, scala_patterns(), c_comment)
}

// ── Ruby ─────────────────────────────────────────────────────────────────────

fn ruby_patterns() -> &'static Vec<Regex> {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| vec![
        Regex::new(r"^\s*(require|require_relative|include|extend|module_function|alias)\b").unwrap(),
        Regex::new(r"^\s*attr_(reader|writer|accessor)\b").unwrap(),
        Regex::new(r"^\s*(def|class|module)\s+\S").unwrap(),
    ])
}

pub fn extract_ruby(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, ruby_patterns(), hash_comment)
}

// ── PHP ───────────────────────────────────────────────────────────────────────

fn php_patterns() -> &'static Vec<Regex> {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| vec![
        Regex::new(r"^\s*(use|namespace)\s+[\w\\]").unwrap(),
        Regex::new(r"^\s*(abstract\s+|final\s+)*(class|interface|trait|enum)\s+\w+").unwrap(),
        Regex::new(r"^\s*((public|private|protected|static|abstract|final)\s+)*function\s+&?\w+\s*\(").unwrap(),
        Regex::new(r"^\s*((public|private|protected)\s+)?const\s+\w+").unwrap(),
    ])
}

pub fn extract_php(content: &str) -> Option<Vec<(usize, String)>> {
    // PHP uses both // and # comments
    extract_line_pattern(content, php_patterns(), |t| {
        t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') || t.starts_with('#')
    })
}

// ── Swift ─────────────────────────────────────────────────────────────────────

fn swift_patterns() -> &'static Vec<Regex> {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| vec![
        Regex::new(r"^\s*import\s+\w").unwrap(),
        Regex::new(r"^\s*@\w+(\([^)]*\))?\s*$").unwrap(),
        Regex::new(r"^\s*((public|private|fileprivate|internal|open|final|static|override|required|convenience|indirect|mutating|class)\s+)*(func|init|class|struct|protocol|enum|extension|subscript|typealias)\b").unwrap(),
        Regex::new(r"^\s*((public|private|fileprivate|internal|open|static|final)\s+)+(var|let)\s+\w").unwrap(),
    ])
}

pub fn extract_swift(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, swift_patterns(), c_comment)
}

// ── CSS / SCSS / LESS ─────────────────────────────────────────────────────────

pub fn extract_css_signatures(content: &str) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut depth   = 0i32;
    let mut in_comment = false;

    for (i, &line) in lines.iter().enumerate() {
        let t = line.trim();

        if in_comment { if t.contains("*/") { in_comment = false; } continue; }
        if t.starts_with("/*") && !t.contains("*/") { in_comment = true; continue; }
        if t.is_empty() { continue; }

        let delta = brace_delta(line);

        if delta > 0 {
            // Pull in preceding comma-continuation selector lines
            let back_idx = kept.len();
            let _ = back_idx; // hint already added inline below
            kept.push((i + 1, line.trim_end().to_string()));
        } else if (t.starts_with('@') && t.ends_with(';'))
               || (depth == 0 && t.starts_with('$') && t.contains(':'))
               || (depth == 0 && t.contains('{') && t.contains('}'))
        {
            kept.push((i + 1, line.trim_end().to_string()));
        }

        depth += delta;
    }

    if kept.is_empty() { None } else { Some(kept) }
}

// ── HTML ──────────────────────────────────────────────────────────────────────

fn html_keep_patterns() -> &'static Vec<Regex> {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| vec![
        Regex::new(r"(?i)^\s*<!doctype\b").unwrap(),
        Regex::new(r"(?i)<script\b[^>]*\bsrc\s*=").unwrap(),
        Regex::new(r"(?i)<link\b[^>]*\bhref\s*=").unwrap(),
        Regex::new(r"(?i)<meta\b[^>]*\bname\s*=").unwrap(),
        Regex::new(r"(?i)<h[1-6][\s>]").unwrap(),
        Regex::new(r#"(?i)<[a-z][\w-]*(?:\s[^<>]*)?\bid\s*="#).unwrap(),
    ])
}

pub fn extract_html_signatures(content: &str) -> Option<Vec<(usize, String)>> {
    static SCRIPT_ANY:  OnceLock<Regex> = OnceLock::new();
    static STYLE_OPEN:  OnceLock<Regex> = OnceLock::new();

    let pats       = html_keep_patterns();
    let script_any = SCRIPT_ANY.get_or_init(|| Regex::new(r"(?i)<script\b[^>]*>").unwrap());
    let style_open = STYLE_OPEN.get_or_init(|| Regex::new(r"(?i)<style\b").unwrap());

    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    // skip_until holds a lowercase close-tag substring instead of a Regex
    let mut skip_until: Option<&'static str> = None;
    let mut in_comment = false;

    for (i, &line) in lines.iter().enumerate() {
        let t     = line.trim();
        let lower = line.to_ascii_lowercase();

        if in_comment { if t.contains("-->") { in_comment = false; } continue; }
        if t.starts_with("<!--") && !t.contains("-->") { in_comment = true; continue; }

        if let Some(close_tag) = skip_until {
            if lower.contains(close_tag) { skip_until = None; }
            continue;
        }

        // Inline script (no src=): skip until </script>
        if script_any.is_match(line) && !lower.contains("src=") && !lower.contains("</script>") {
            skip_until = Some("</script>");
            continue;
        }
        // Inline style: skip until </style>
        if style_open.is_match(line) && !lower.contains("</style>") {
            skip_until = Some("</style>");
            continue;
        }

        if pats.iter().any(|p| p.is_match(line)) {
            kept.push((i + 1, line.trim_end().to_string()));
        }
    }

    if kept.is_empty() { None } else { Some(kept) }
}

// ── SQL ───────────────────────────────────────────────────────────────────────

fn sql_create_pattern() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(
        r"(?i)^\s*CREATE\s+(?:OR\s+REPLACE\s+)?(?:GLOBAL\s+|LOCAL\s+|TEMP(?:ORARY)?\s+|UNLOGGED\s+|UNIQUE\s+|MATERIALIZED\s+|DEFINER\s*=\s*\S+\s+)*(TABLE|VIEW|FUNCTION|PROCEDURE|INDEX|TRIGGER)\b"
    ).unwrap())
}

pub fn extract_sql(content: &str) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut in_block = false;
    let mut in_dollar = false;
    let mut begin_depth = 0i32;

    let dollar_count = |l: &str| l.matches("$$").count();

    for (i, &line) in lines.iter().enumerate() {
        let t = line.trim();
        let tl = t.to_lowercase();

        // Skip block comments
        if in_block { if t.contains("*/") { in_block = false; } continue; }
        if t.starts_with("/*") && !t.contains("*/") { in_block = true; continue; }
        if t.starts_with("--") { continue; }

        // Skip $$ body
        if dollar_count(line) % 2 == 1 { in_dollar = !in_dollar; }
        if in_dollar { continue; }

        // Skip BEGIN...END body
        if tl == "begin" || tl.starts_with("begin ") { begin_depth += 1; }
        if tl == "end" || tl.starts_with("end;") || tl.starts_with("end ") {
            begin_depth -= 1;
            if begin_depth < 0 { begin_depth = 0; }
            if begin_depth == 0 { continue; }
        }
        if begin_depth > 0 { continue; }

        if sql_create_pattern().is_match(line) {
            kept.push((i + 1, line.trim_end().to_string()));
            // Keep column definitions for TABLE
            if tl.contains("table") && line.contains('(') && !line.contains(')') {
                let mut depth = round_delta(line);
                let mut j = i + 1;
                while j < lines.len() && depth > 0 {
                    kept.push((j + 1, lines[j].trim_end().to_string()));
                    depth += round_delta(lines[j]);
                    j += 1;
                }
            }
        }
    }

    if kept.is_empty() { None } else { Some(kept) }
}

// ── Vue / Svelte ──────────────────────────────────────────────────────────────

/// Extract signatures from Vue/Svelte SFCs:
/// - `<script>` blocks run through the JS/TS heuristic with line-offset correction
/// - `<template>` root line kept
/// - Tags with `id=` kept
pub fn extract_vue_svelte(content: &str) -> Option<Vec<(usize, String)>> {
    static SCRIPT_OPEN:  OnceLock<Regex> = OnceLock::new();
    static SCRIPT_CLOSE: OnceLock<Regex> = OnceLock::new();
    static STYLE_OPEN:   OnceLock<Regex> = OnceLock::new();
    static ID_ATTR:      OnceLock<Regex> = OnceLock::new();

    let script_open  = SCRIPT_OPEN .get_or_init(|| Regex::new(r"(?i)^<script\b([^>]*)>").unwrap());
    let script_close = SCRIPT_CLOSE.get_or_init(|| Regex::new(r"(?i)</script>").unwrap());
    let style_open   = STYLE_OPEN  .get_or_init(|| Regex::new(r"(?i)^<style\b").unwrap());
    let id_attr      = ID_ATTR     .get_or_init(|| Regex::new(r#"(?i)<[a-z][\w-]*[^>]*\bid\s*="#).unwrap());

    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    let mut in_style   = false;
    let mut in_comment = false;

    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();

        if in_comment { if t.contains("-->") { in_comment = false; } i += 1; continue; }
        if t.starts_with("<!--") && !t.contains("-->") { in_comment = true; i += 1; continue; }
        if in_style {
            if line.to_ascii_lowercase().contains("</style>") { in_style = false; }
            i += 1;
            continue;
        }

        if let Some(caps) = script_open.captures(line) {
            kept.push((i + 1, line.trim_end().to_string()));
            let attrs  = caps.get(1).map_or("", |m| m.as_str());
            let offset = i + 1;
            i += 1;

            let mut script_lines: Vec<&str> = Vec::new();
            while i < lines.len() && !script_close.is_match(lines[i]) {
                script_lines.push(lines[i]);
                i += 1;
            }
            // Both TS and JS use the same heuristic extractor.
            let block = script_lines.join("\n");
            let _is_ts = attrs.to_ascii_lowercase().contains("lang=\"ts\"");
            if let Some(inner) = extract_ts_js_heuristic(&block) {
                for (line_num, text) in inner {
                    kept.push((line_num + offset, text));
                }
            }
            i += 1; // skip </script>
            continue;
        }

        if style_open.is_match(line) && !line.to_ascii_lowercase().contains("</style>") {
            in_style = true; i += 1; continue;
        }

        // Keep <template> opener and any tag carrying id=
        if t.starts_with("<template") || id_attr.is_match(line) {
            kept.push((i + 1, line.trim_end().to_string()));
        }

        i += 1;
    }

    if kept.is_empty() { None } else { Some(kept) }
}

// ── Python (upgraded from initial version) ────────────────────────────────────

pub fn extract_python(content: &str) -> Option<Vec<(usize, String)>> {
    static PY_IMPORT:    OnceLock<Regex> = OnceLock::new();
    static PY_DEF:       OnceLock<Regex> = OnceLock::new();
    static PY_CLASS:     OnceLock<Regex> = OnceLock::new();
    static PY_DECORATOR: OnceLock<Regex> = OnceLock::new();
    static PY_DUNDER:    OnceLock<Regex> = OnceLock::new();

    let py_import    = PY_IMPORT   .get_or_init(|| Regex::new(r"^(?:import|from)\s+\S").unwrap());
    let py_def       = PY_DEF      .get_or_init(|| Regex::new(r"^(?:async\s+)?def\s+\w").unwrap());
    let py_class     = PY_CLASS    .get_or_init(|| Regex::new(r"^class\s+\w").unwrap());
    let py_decorator = PY_DECORATOR.get_or_init(|| Regex::new(r"^@\w").unwrap());
    let py_dunder    = PY_DUNDER   .get_or_init(|| Regex::new(r"^__\w+__\s*=").unwrap());

    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut function_body_indent: Option<usize> = None;
    let mut i = 0;

    while i < lines.len() {
        let raw     = lines[i];
        let trimmed = raw.trim();

        if trimmed.is_empty() { i += 1; continue; }

        let indent = raw.len() - raw.trim_start().len();

        if let Some(body_indent) = function_body_indent {
            if indent > body_indent { i += 1; continue; }
            function_body_indent = None;
        }

        if py_import.is_match(trimmed) || py_dunder.is_match(trimmed)
            || py_decorator.is_match(trimmed) || py_class.is_match(trimmed)
        {
            kept.push((i + 1, raw.trim_end().to_string()));
            i += 1; continue;
        }

        if py_def.is_match(trimmed) {
            kept.push((i + 1, raw.trim_end().to_string()));
            // Multi-line signature
            let mut depth = round_delta(raw);
            while depth > 0 && i + 1 < lines.len() {
                i += 1;
                kept.push((i + 1, lines[i].trim_end().to_string()));
                depth += round_delta(lines[i]);
            }
            function_body_indent = Some(indent);
        }

        i += 1;
    }

    if kept.is_empty() { None } else { Some(kept) }
}

// ── Go ────────────────────────────────────────────────────────────────────────

pub fn extract_go(content: &str) -> Option<Vec<(usize, String)>> {
    static GO_TOP:        OnceLock<Regex> = OnceLock::new();
    static GO_PAREN:      OnceLock<Regex> = OnceLock::new();
    static GO_BRACE_TYPE: OnceLock<Regex> = OnceLock::new();

    let go_top   = GO_TOP       .get_or_init(|| Regex::new(r"^(?:package|import|func|type|const|var)\b").unwrap());
    let go_paren = GO_PAREN     .get_or_init(|| Regex::new(r"^(?:import|const|var)\s*\(").unwrap());
    let go_brace = GO_BRACE_TYPE.get_or_init(|| Regex::new(r"^type\s+\w+\s+(?:struct|interface)\b").unwrap());

    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if !go_top.is_match(line) { i += 1; continue; }
        kept.push((i + 1, line.trim_end().to_string()));

        if go_paren.is_match(line) {
            let mut depth = round_delta(line);
            while depth > 0 && i + 1 < lines.len() {
                i += 1;
                kept.push((i + 1, lines[i].trim_end().to_string()));
                depth += round_delta(lines[i]);
            }
            i += 1; continue;
        }

        if go_brace.is_match(line) && brace_delta(line) > 0 {
            let mut depth = brace_delta(line);
            while depth > 0 && i + 1 < lines.len() {
                i += 1;
                kept.push((i + 1, lines[i].trim_end().to_string()));
                depth += brace_delta(lines[i]);
            }
            i += 1; continue;
        }

        // Multi-line func signature
        let mut round = round_delta(line);
        while round > 0 && i + 1 < lines.len() {
            i += 1;
            kept.push((i + 1, lines[i].trim_end().to_string()));
            round += round_delta(lines[i]);
        }
        i += 1;
    }

    if kept.is_empty() { None } else { Some(kept) }
}

// ── C / C++ ───────────────────────────────────────────────────────────────────

pub fn extract_c_family(content: &str) -> Option<Vec<(usize, String)>> {
    static C_PREPROC:  OnceLock<Regex> = OnceLock::new();
    static C_TYPE:     OnceLock<Regex> = OnceLock::new();
    static C_EXTRA:    OnceLock<Regex> = OnceLock::new();
    static C_CONTROL:  OnceLock<Regex> = OnceLock::new();
    static C_FUNC:     OnceLock<Regex> = OnceLock::new();

    let c_preproc = C_PREPROC.get_or_init(|| Regex::new(r"^\s*#\s*(?:include|define)\b").unwrap());
    let c_type    = C_TYPE   .get_or_init(|| Regex::new(r"^(?:typedef\s+)?(?:struct|union|enum|class)\b").unwrap());
    let c_extra   = C_EXTRA  .get_or_init(|| Regex::new(r#"^(?:namespace\s+\w|template\s*<|extern\s+")"#).unwrap());
    let c_control = C_CONTROL.get_or_init(|| Regex::new(r"^(?:if|else|for|while|switch|return|do|case|goto|sizeof|break|continue)\b").unwrap());
    let c_func    = C_FUNC   .get_or_init(|| Regex::new(r"^[A-Za-z_][\w\s*&:<>,~]*\(").unwrap());

    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let t    = line.trim();

        if c_preproc.is_match(line) || c_extra.is_match(line) {
            kept.push((i + 1, line.trim_end().to_string()));
            i += 1; continue;
        }

        if c_type.is_match(line) {
            kept.push((i + 1, line.trim_end().to_string()));
            let is_enum = t.starts_with("typedef enum") || t.starts_with("enum ");
            let mut depth = brace_delta(line);
            while depth > 0 && i + 1 < lines.len() {
                i += 1;
                depth += brace_delta(lines[i]);
                if !is_enum || depth <= 0 {
                    kept.push((i + 1, lines[i].trim_end().to_string()));
                }
            }
            i += 1; continue;
        }

        if c_func.is_match(line) && !c_control.is_match(t) {
            kept.push((i + 1, line.trim_end().to_string()));
            let mut round = round_delta(line);
            while round > 0 && i + 1 < lines.len() {
                i += 1;
                kept.push((i + 1, lines[i].trim_end().to_string()));
                round += round_delta(lines[i]);
            }
        }

        i += 1;
    }

    if kept.is_empty() { None } else { Some(kept) }
}

// ── Shell ─────────────────────────────────────────────────────────────────────

pub fn extract_shell(content: &str) -> Option<Vec<(usize, String)>> {
    static SHELL_PATS: OnceLock<Vec<Regex>> = OnceLock::new();
    let pats = SHELL_PATS.get_or_init(|| vec![
        Regex::new(r"^(?:export\s+)?(?:function\s+\w+|\w+\s*\(\s*\))").unwrap(),
        Regex::new(r"^(?:readonly\s+|declare\s+(?:-[a-zA-Z]+\s+)*)?[A-Z_][A-Z0-9_]+=").unwrap(),
        Regex::new(r"^\.\s+\S|^source\s+\S").unwrap(),
    ]);
    extract_line_pattern(content, pats, hash_comment)
}

// ── Elixir ────────────────────────────────────────────────────────────────────

pub fn extract_elixir(content: &str) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut depth = 0i32;

    for (i, &line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') { continue; }

        let opens = t.starts_with("def ") || t.starts_with("defp ")
            || t.starts_with("defmodule ") || t.starts_with("defmacro ")
            || t.ends_with(" do") || t.ends_with(", do:");
        let closes = t == "end";

        if depth == 0 { kept.push((i + 1, line.trim_end().to_string())); }
        if opens  { depth += 1; }
        if closes {
            depth -= 1;
            if depth < 0 { depth = 0; }
            if depth == 0 { kept.push((i + 1, line.trim_end().to_string())); }
        }
    }

    if kept.is_empty() { None } else { Some(kept) }
}

// ── Haskell ───────────────────────────────────────────────────────────────────

pub fn extract_haskell(content: &str) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let kept: Vec<(usize, String)> = lines.iter().enumerate()
        .filter(|(_, l)| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with("--") && !t.starts_with("{-")
                && !l.starts_with(' ') && !l.starts_with('\t')
        })
        .map(|(i, l)| (i + 1, l.trim_end().to_string()))
        .collect();
    if kept.is_empty() { None } else { Some(kept) }
}

// ── TS/JS heuristic (for vue/svelte script blocks) ────────────────────────────

pub fn extract_ts_js_heuristic(content: &str) -> Option<Vec<(usize, String)>> {
    static PATS: OnceLock<Vec<Regex>> = OnceLock::new();
    let pats = PATS.get_or_init(|| vec![
        Regex::new(r"^\s*(export\s+)?(default\s+)?(async\s+)?function\s*\*?\s*\w+").unwrap(),
        Regex::new(r"^\s*(export\s+)?(abstract\s+)?class\s+\w+").unwrap(),
        Regex::new(r"^\s*(export\s+)?interface\s+\w+").unwrap(),
        Regex::new(r"^\s*(export\s+)?type\s+\w+").unwrap(),
        Regex::new(r"^\s*(import|export)\s+").unwrap(),
        Regex::new(r"^\s*(export\s+)?const\s+\w+[^=]*=\s*(\([^)]*\)|[^=>\n]+)\s*=>").unwrap(),
        Regex::new(r"^\s*(export\s+)?enum\s+\w+").unwrap(),
        Regex::new(r"^\s*(public|private|protected|static|abstract|readonly|override)\s+\w+").unwrap(),
    ]);
    extract_line_pattern(content, pats, c_comment)
}

// ── Generic brace-depth fallback ──────────────────────────────────────────────

pub fn extract_brace_depth_generic(content: &str) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut depth = 0i32;

    for (i, &line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        let open  = trimmed.chars().filter(|&c| c == '{').count() as i32;
        let close = trimmed.chars().filter(|&c| c == '}').count() as i32;

        if depth == 0 || (close > 0 && depth - close <= 0) {
            kept.push((i + 1, line.trim_end().to_string()));
        }

        depth += open - close;
        if depth < 0 { depth = 0; }
    }

    if kept.is_empty() { None } else { Some(kept) }
}

// ── Public router ─────────────────────────────────────────────────────────────

pub fn extract_heuristic(content: &str, ext: &str) -> Option<Vec<(usize, String)>> {
    match ext {
        "py"                      => extract_python(content),
        "go"                      => extract_go(content),
        "c" | "h"                 => extract_c_family(content),
        "cpp"|"hpp"|"cc"|"cxx"    => extract_c_family(content),
        "java"|"cs"|"kt"|"kotlin" => extract_kotlin_java_cs(content),
        "scala"                   => extract_scala(content),
        "rb"                      => extract_ruby(content),
        "php"                     => extract_php(content),
        "swift"                   => extract_swift(content),
        "css"|"scss"|"less"       => extract_css_signatures(content),
        "html"|"htm"              => extract_html_signatures(content),
        "sql"|"tsql"|"plsql"      => extract_sql(content),
        "vue"|"svelte"            => extract_vue_svelte(content),
        "sh"|"bash"|"zsh"|"fish"  => extract_shell(content),
        "ex"|"exs"                => extract_elixir(content),
        "hs"|"lhs"                => extract_haskell(content),
        "lua"                     => extract_brace_depth_generic(content),
        _                         => extract_brace_depth_generic(content),
    }
}
