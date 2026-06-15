//! Per-language heuristic signature extractors.
//! Used after tree-sitter misses and as the primary extractor for languages
//! without parser-backed support.
//!
//! All strategies are ported directly from the TS `extractSignatures.ts`
//! line-pattern / heuristic implementations.

use regex::Regex;
use std::sync::LazyLock;

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

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

fn c_comment(t: &str) -> bool {
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
}
fn hash_comment(t: &str) -> bool {
    t.starts_with('#') && !t.starts_with("#!")
}

// ── Kotlin / Java / C# ───────────────────────────────────────────────────────

static JAVA_CS_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(
            r"^\s*(public|private|protected|static|abstract|final|override|sealed|internal)\s+",
        )
        .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(class|interface|enum|record|object)\s+\w+")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(import|using|package|namespace)\s+")
            .expect("static heuristic regex must compile"),
    ]
});

fn java_cs_patterns() -> &'static Vec<Regex> {
    &JAVA_CS_PATTERNS
}

pub fn extract_kotlin_java_cs(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, java_cs_patterns(), c_comment)
}

static KOTLIN_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(
            r"^\s*(public|private|protected|internal|open|abstract|override|sealed|final|inline|suspend|actual|expect)\s+",
        )
        .expect("static heuristic regex must compile"),
        Regex::new(
            r"^\s*(class|interface|enum\s+class|data\s+class|sealed\s+class|abstract\s+class|companion\s+object|object)\b",
        )
        .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(import|package)\s+").expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(fun|val|var|const\s+val|typealias)\s+\w+")
            .expect("static heuristic regex must compile"),
    ]
});

fn kotlin_patterns() -> &'static Vec<Regex> {
    &KOTLIN_PATTERNS
}

pub fn extract_kotlin(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, kotlin_patterns(), c_comment)
}

// ── Scala ─────────────────────────────────────────────────────────────────────

static SCALA_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"^\s*(package|import)\s+").expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(sealed\s+|abstract\s+|final\s+|case\s+)*(class|object|trait|enum)\s+\w+")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(override\s+|private\s+|protected\s+|implicit\s+|given\s+)*(def|val|var|type)\s+\w+")
            .expect("static heuristic regex must compile"),
    ]
});

fn scala_patterns() -> &'static Vec<Regex> {
    &SCALA_PATTERNS
}

pub fn extract_scala(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, scala_patterns(), c_comment)
}

// ── Ruby ─────────────────────────────────────────────────────────────────────

static RUBY_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"^\s*(require|require_relative|include|extend|module_function|alias)\b")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*attr_(reader|writer|accessor)\b")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(def|class|module)\s+\S").expect("static heuristic regex must compile"),
    ]
});

fn ruby_patterns() -> &'static Vec<Regex> {
    &RUBY_PATTERNS
}

pub fn extract_ruby(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, ruby_patterns(), hash_comment)
}

// ── PHP ───────────────────────────────────────────────────────────────────────

static PHP_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"^\s*(use|namespace)\s+[\w\\]").expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(abstract\s+|final\s+)*(class|interface|trait|enum)\s+\w+")
            .expect("static heuristic regex must compile"),
        Regex::new(
            r"^\s*((public|private|protected|static|abstract|final)\s+)*function\s+&?\w+\s*\(",
        )
        .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*((public|private|protected)\s+)?const\s+\w+")
            .expect("static heuristic regex must compile"),
    ]
});

fn php_patterns() -> &'static Vec<Regex> {
    &PHP_PATTERNS
}

pub fn extract_php(content: &str) -> Option<Vec<(usize, String)>> {
    // PHP uses both // and # comments
    extract_line_pattern(content, php_patterns(), |t| {
        t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') || t.starts_with('#')
    })
}

// ── Swift ─────────────────────────────────────────────────────────────────────

static SWIFT_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"^\s*import\s+\w").expect("static heuristic regex must compile"),
        Regex::new(r"^\s*@\w+(\([^)]*\))?\s*$").expect("static heuristic regex must compile"),
        Regex::new(r"^\s*((public|private|fileprivate|internal|open|final|static|override|required|convenience|indirect|mutating|class)\s+)*(func|init|class|struct|protocol|enum|extension|subscript|typealias)\b")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*((public|private|fileprivate|internal|open|static|final)\s+)+(var|let)\s+\w")
            .expect("static heuristic regex must compile"),
    ]
});

fn swift_patterns() -> &'static Vec<Regex> {
    &SWIFT_PATTERNS
}

pub fn extract_swift(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, swift_patterns(), c_comment)
}

// ── CSS / SCSS / LESS ─────────────────────────────────────────────────────────

pub fn extract_css_signatures(content: &str) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut depth = 0i32;
    let mut in_comment = false;

    for (i, &line) in lines.iter().enumerate() {
        let t = line.trim();

        if in_comment {
            if t.contains("*/") {
                in_comment = false;
            }
            continue;
        }
        if t.starts_with("/*") && !t.contains("*/") {
            in_comment = true;
            continue;
        }
        if t.is_empty() {
            continue;
        }

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

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

// ── HTML ──────────────────────────────────────────────────────────────────────

static HTML_KEEP_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)^\s*<!doctype\b").expect("static heuristic regex must compile"),
        Regex::new(r"(?i)<script\b[^>]*\bsrc\s*=").expect("static heuristic regex must compile"),
        Regex::new(r"(?i)<link\b[^>]*\bhref\s*=").expect("static heuristic regex must compile"),
        Regex::new(r"(?i)<meta\b[^>]*\bname\s*=").expect("static heuristic regex must compile"),
        Regex::new(r"(?i)<h[1-6][\s>]").expect("static heuristic regex must compile"),
        Regex::new(r#"(?i)<[a-z][\w-]*(?:\s[^<>]*)?\bid\s*="#)
            .expect("static heuristic regex must compile"),
    ]
});

fn html_keep_patterns() -> &'static Vec<Regex> {
    &HTML_KEEP_PATTERNS
}

static HTML_SCRIPT_ANY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<script\b[^>]*>").expect("static heuristic regex must compile")
});
static HTML_STYLE_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<style\b").expect("static heuristic regex must compile"));

pub fn extract_html_signatures(content: &str) -> Option<Vec<(usize, String)>> {
    let pats = html_keep_patterns();
    let script_any = &*HTML_SCRIPT_ANY;
    let style_open = &*HTML_STYLE_OPEN;

    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    // skip_until holds a lowercase close-tag substring instead of a Regex
    let mut skip_until: Option<&'static str> = None;
    let mut in_comment = false;

    for (i, &line) in lines.iter().enumerate() {
        let t = line.trim();
        let lower = line.to_ascii_lowercase();

        if in_comment {
            if t.contains("-->") {
                in_comment = false;
            }
            continue;
        }
        if t.starts_with("<!--") && !t.contains("-->") {
            in_comment = true;
            continue;
        }

        if let Some(close_tag) = skip_until {
            if lower.contains(close_tag) {
                skip_until = None;
            }
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

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

// ── SQL ───────────────────────────────────────────────────────────────────────

static SQL_CREATE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^\s*CREATE\s+(?:OR\s+REPLACE\s+)?(?:GLOBAL\s+|LOCAL\s+|TEMP(?:ORARY)?\s+|UNLOGGED\s+|UNIQUE\s+|MATERIALIZED\s+|DEFINER\s*=\s*\S+\s+)*(TABLE|VIEW|FUNCTION|PROCEDURE|INDEX|TRIGGER)\b",
    )
    .expect("static heuristic regex must compile")
});

fn sql_create_pattern() -> &'static Regex {
    &SQL_CREATE_PATTERN
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
        if in_block {
            if t.contains("*/") {
                in_block = false;
            }
            continue;
        }
        if t.starts_with("/*") && !t.contains("*/") {
            in_block = true;
            continue;
        }
        if t.starts_with("--") {
            continue;
        }

        // Skip $$ body
        if dollar_count(line) % 2 == 1 {
            in_dollar = !in_dollar;
        }
        if in_dollar {
            continue;
        }

        // Skip BEGIN...END body
        if tl == "begin" || tl.starts_with("begin ") {
            begin_depth += 1;
        }
        if tl == "end" || tl.starts_with("end;") || tl.starts_with("end ") {
            begin_depth -= 1;
            if begin_depth < 0 {
                begin_depth = 0;
            }
            if begin_depth == 0 {
                continue;
            }
        }
        if begin_depth > 0 {
            continue;
        }

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

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

// ── Vue / Svelte ──────────────────────────────────────────────────────────────

/// Extract signatures from Vue/Svelte SFCs:
/// - `<script>` blocks run through the JS/TS heuristic with line-offset correction
/// - `<template>` root line kept
/// - Tags with `id=` kept
static VUE_SCRIPT_OPEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^<script\b([^>]*)>").expect("static heuristic regex must compile")
});
static VUE_SCRIPT_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</script>").expect("static heuristic regex must compile"));
static VUE_STYLE_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^<style\b").expect("static heuristic regex must compile"));
static VUE_ID_ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<[a-z][\w-]*[^>]*\bid\s*="#).expect("static heuristic regex must compile")
});

pub fn extract_vue_svelte(content: &str) -> Option<Vec<(usize, String)>> {
    let script_open = &*VUE_SCRIPT_OPEN;
    let script_close = &*VUE_SCRIPT_CLOSE;
    let style_open = &*VUE_STYLE_OPEN;
    let id_attr = &*VUE_ID_ATTR;

    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    let mut in_style = false;
    let mut in_comment = false;

    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();

        if in_comment {
            if t.contains("-->") {
                in_comment = false;
            }
            i += 1;
            continue;
        }
        if t.starts_with("<!--") && !t.contains("-->") {
            in_comment = true;
            i += 1;
            continue;
        }
        if in_style {
            if line.to_ascii_lowercase().contains("</style>") {
                in_style = false;
            }
            i += 1;
            continue;
        }

        if let Some(caps) = script_open.captures(line) {
            kept.push((i + 1, line.trim_end().to_string()));
            let attrs = caps.get(1).map_or("", |m| m.as_str());
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
            in_style = true;
            i += 1;
            continue;
        }

        // Keep <template> opener and any tag carrying id=
        if t.starts_with("<template") || id_attr.is_match(line) {
            kept.push((i + 1, line.trim_end().to_string()));
        }

        i += 1;
    }

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

// ── Python (upgraded from initial version) ────────────────────────────────────

static PY_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:import|from)\s+\S").expect("static heuristic regex must compile")
});
static PY_DEF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:async\s+)?def\s+\w").expect("static heuristic regex must compile")
});
static PY_CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^class\s+\w").expect("static heuristic regex must compile"));
static PY_DECORATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^@\w").expect("static heuristic regex must compile"));
static PY_DUNDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^__\w+__\s*=").expect("static heuristic regex must compile"));

pub fn extract_python(content: &str) -> Option<Vec<(usize, String)>> {
    let py_import = &*PY_IMPORT;
    let py_def = &*PY_DEF;
    let py_class = &*PY_CLASS;
    let py_decorator = &*PY_DECORATOR;
    let py_dunder = &*PY_DUNDER;

    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut function_body_indent: Option<usize> = None;
    let mut i = 0;

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        let indent = raw.len() - raw.trim_start().len();

        if let Some(body_indent) = function_body_indent {
            if indent > body_indent {
                i += 1;
                continue;
            }
            function_body_indent = None;
        }

        if py_import.is_match(trimmed)
            || py_dunder.is_match(trimmed)
            || py_decorator.is_match(trimmed)
            || py_class.is_match(trimmed)
        {
            kept.push((i + 1, raw.trim_end().to_string()));
            i += 1;
            continue;
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

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

// ── Go ────────────────────────────────────────────────────────────────────────

static GO_TOP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:package|import|func|type|const|var)\b")
        .expect("static heuristic regex must compile")
});
static GO_PAREN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:import|const|var)\s*\(").expect("static heuristic regex must compile")
});
static GO_BRACE_TYPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^type\s+\w+\s+(?:struct|interface)\b")
        .expect("static heuristic regex must compile")
});

pub fn extract_go(content: &str) -> Option<Vec<(usize, String)>> {
    let go_top = &*GO_TOP;
    let go_paren = &*GO_PAREN;
    let go_brace = &*GO_BRACE_TYPE;

    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if !go_top.is_match(line) {
            i += 1;
            continue;
        }
        kept.push((i + 1, line.trim_end().to_string()));

        if go_paren.is_match(line) {
            let mut depth = round_delta(line);
            while depth > 0 && i + 1 < lines.len() {
                i += 1;
                kept.push((i + 1, lines[i].trim_end().to_string()));
                depth += round_delta(lines[i]);
            }
            i += 1;
            continue;
        }

        if go_brace.is_match(line) && brace_delta(line) > 0 {
            let mut depth = brace_delta(line);
            while depth > 0 && i + 1 < lines.len() {
                i += 1;
                kept.push((i + 1, lines[i].trim_end().to_string()));
                depth += brace_delta(lines[i]);
            }
            i += 1;
            continue;
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

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

// ── C / C++ ───────────────────────────────────────────────────────────────────

static C_PREPROC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*#\s*(?:include|define)\b").expect("static heuristic regex must compile")
});
static C_TYPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:typedef\s+)?(?:struct|union|enum|class)\b")
        .expect("static heuristic regex must compile")
});
static C_EXTRA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(?:namespace\s+\w|template\s*<|extern\s+")"#)
        .expect("static heuristic regex must compile")
});
static C_CONTROL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:if|else|for|while|switch|return|do|case|goto|sizeof|break|continue)\b")
        .expect("static heuristic regex must compile")
});
static C_FUNC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z_][\w\s*&:<>,~]*\(").expect("static heuristic regex must compile")
});

pub fn extract_c_family(content: &str) -> Option<Vec<(usize, String)>> {
    let c_preproc = &*C_PREPROC;
    let c_type = &*C_TYPE;
    let c_extra = &*C_EXTRA;
    let c_control = &*C_CONTROL;
    let c_func = &*C_FUNC;

    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();

        if c_preproc.is_match(line) || c_extra.is_match(line) {
            kept.push((i + 1, line.trim_end().to_string()));
            i += 1;
            continue;
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
            i += 1;
            continue;
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

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

// ── Shell ─────────────────────────────────────────────────────────────────────

static SHELL_PATS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"^(?:export\s+)?(?:function\s+\w+|\w+\s*\(\s*\))")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^(?:readonly\s+|declare\s+(?:-[a-zA-Z]+\s+)*)?[A-Z_][A-Z0-9_]+=")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^\.\s+\S|^source\s+\S").expect("static heuristic regex must compile"),
    ]
});

pub fn extract_shell(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, &SHELL_PATS, hash_comment)
}

// ── Elixir ────────────────────────────────────────────────────────────────────

pub fn extract_elixir(content: &str) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut depth = 0i32;

    for (i, &line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }

        let signature = t.starts_with("def ")
            || t.starts_with("defp ")
            || t.starts_with("defmodule ")
            || t.starts_with("defmacro ");
        let opens = signature || t.ends_with(" do") || t.ends_with(", do:");
        let closes = t == "end";

        if depth == 0 || signature {
            kept.push((i + 1, line.trim_end().to_string()));
        }
        if opens {
            depth += 1;
        }
        if closes {
            depth -= 1;
            if depth < 0 {
                depth = 0;
            }
            if depth == 0 {
                kept.push((i + 1, line.trim_end().to_string()));
            }
        }
    }

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

// ── Haskell ───────────────────────────────────────────────────────────────────

pub fn extract_haskell(content: &str) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let kept: Vec<(usize, String)> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with("--")
                && !t.starts_with("{-")
                && !l.starts_with(' ')
                && !l.starts_with('\t')
        })
        .map(|(i, l)| (i + 1, l.trim_end().to_string()))
        .collect();
    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

// ── TS/JS heuristic (for vue/svelte script blocks) ────────────────────────────

static TS_JS_HEURISTIC_PATS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"^\s*(export\s+)?(default\s+)?(async\s+)?function\s*\*?\s*\w+")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(export\s+)?(abstract\s+)?class\s+\w+")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(export\s+)?interface\s+\w+")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(export\s+)?type\s+\w+").expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(import|export)\s+").expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(export\s+)?const\s+\w+[^=]*=\s*(\([^)]*\)|[^=>\n]+)\s*=>")
            .expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(export\s+)?enum\s+\w+").expect("static heuristic regex must compile"),
        Regex::new(r"^\s*(public|private|protected|static|abstract|readonly|override)\s+\w+")
            .expect("static heuristic regex must compile"),
    ]
});

pub fn extract_ts_js_heuristic(content: &str) -> Option<Vec<(usize, String)>> {
    extract_line_pattern(content, &TS_JS_HEURISTIC_PATS, c_comment)
}

// ── Markdown ─────────────────────────────────────────────────────────────────

pub fn extract_markdown(content: &str) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut fence: Option<MarkdownFence> = None;
    let mut in_html_comment = false;
    let mut in_generated_toc = false;
    let mut previous_plain: Option<(usize, String)> = None;

    let first_content = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(0);
    let mut in_frontmatter =
        first_content == 0 && lines.first().is_some_and(|line| line.trim_end() == "---");

    for (idx, raw) in lines.iter().enumerate() {
        let line_no = idx + 1;

        if let Some(ref active_fence) = fence {
            if markdown_fence_close(raw, active_fence) {
                fence = None;
            }
            previous_plain = None;
            continue;
        }

        if let Some(next_fence) = markdown_fence_start(raw) {
            let info = next_fence.info.trim();
            if !info.is_empty() {
                kept.push((
                    line_no,
                    format!("code fence: {}", compact_markdown_text(info, 80)),
                ));
            }
            fence = Some(next_fence);
            previous_plain = None;
            continue;
        }

        if raw.starts_with("    ") || raw.starts_with('\t') {
            previous_plain = None;
            continue;
        }

        if in_frontmatter {
            let trimmed = raw.trim();
            if idx > first_content && (trimmed == "---" || trimmed == "...") {
                in_frontmatter = false;
            } else if let Some(key) = markdown_frontmatter_key(trimmed) {
                kept.push((line_no, format!("frontmatter: {key}")));
            }
            previous_plain = None;
            continue;
        }

        if in_generated_toc {
            if markdown_toc_end(raw) {
                in_generated_toc = false;
            }
            previous_plain = None;
            continue;
        }
        if markdown_toc_start(raw) {
            in_generated_toc = !markdown_toc_end(raw);
            previous_plain = None;
            continue;
        }

        let (without_comments, still_in_comment) =
            strip_markdown_html_comment(raw, in_html_comment);
        in_html_comment = still_in_comment;
        let trimmed = without_comments.trim();

        if trimmed.is_empty() {
            previous_plain = None;
            continue;
        }

        if let Some(heading) = markdown_atx_heading(trimmed) {
            kept.push((line_no, heading));
            previous_plain = None;
            continue;
        }

        if let Some(level) = markdown_setext_level(trimmed) {
            if let Some((heading_line, text)) = previous_plain.take() {
                let prefix = if level == 1 { "# " } else { "## " };
                kept.push((heading_line, format!("{prefix}{text}")));
            }
            continue;
        }

        if let Some(reference) = markdown_reference_definition(trimmed) {
            kept.push((line_no, reference));
            previous_plain = None;
            continue;
        }

        if markdown_list_item(trimmed) {
            kept.push((line_no, compact_markdown_text(trimmed, 160)));
            previous_plain = None;
            continue;
        }

        let links = markdown_inline_links(trimmed);
        if !links.is_empty() {
            kept.push((line_no, format!("links: {}", links.join(", "))));
            previous_plain = None;
            continue;
        }

        previous_plain = Some((line_no, compact_markdown_text(trimmed, 120)));
    }

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

struct MarkdownFence {
    marker: char,
    length: usize,
    info: String,
}

fn markdown_fence_start(line: &str) -> Option<MarkdownFence> {
    let leading = line.len() - line.trim_start().len();
    if leading > 3 {
        return None;
    }

    let rest = line.trim_start();
    let marker = rest.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let length = rest.chars().take_while(|&c| c == marker).count();
    if length < 3 {
        return None;
    }

    Some(MarkdownFence {
        marker,
        length,
        info: rest[length..].trim().to_owned(),
    })
}

fn markdown_fence_close(line: &str, fence: &MarkdownFence) -> bool {
    let rest = line.trim_start();
    let count = rest.chars().take_while(|&c| c == fence.marker).count();
    count >= fence.length && rest[count..].trim().is_empty()
}

fn markdown_frontmatter_key(line: &str) -> Option<String> {
    let (key, _value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return None;
    }
    Some(key.to_owned())
}

fn markdown_atx_heading(line: &str) -> Option<String> {
    let hash_count = line.chars().take_while(|&c| c == '#').count();
    if hash_count == 0 || hash_count > 6 {
        return None;
    }
    let rest = &line[hash_count..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim();
    if text.is_empty() {
        return None;
    }
    Some(format!(
        "{} {}",
        "#".repeat(hash_count),
        compact_markdown_text(text, 160)
    ))
}

fn markdown_setext_level(line: &str) -> Option<u8> {
    if line.len() < 2 {
        return None;
    }
    if line.chars().all(|c| c == '=') {
        Some(1)
    } else if line.chars().all(|c| c == '-') {
        Some(2)
    } else {
        None
    }
}

static MD_REF_DEF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\[([^\]]+)\]:\s*(\S+)"#).expect("static heuristic regex must compile")
});

fn markdown_reference_definition(line: &str) -> Option<String> {
    let captures = MD_REF_DEF.captures(line)?;
    let label = captures.get(1)?.as_str().trim();
    let target = captures.get(2)?.as_str().trim();
    Some(format!("link ref: [{label}]: {target}"))
}

static MD_LIST_ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(?:[-+*]|\d+[.)])\s+\S"#).expect("static heuristic regex must compile")
});

fn markdown_list_item(line: &str) -> bool {
    MD_LIST_ITEM.is_match(line)
}

static MD_DIRECT_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"!?\[([^\]\n]+)\]\(([^)\s]+)(?:\s+[^)]*)?\)"#)
        .expect("static heuristic regex must compile")
});
static MD_REF_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"!?\[([^\]\n]+)\]\[([^\]\n]*)\]"#).expect("static heuristic regex must compile")
});

fn markdown_inline_links(line: &str) -> Vec<String> {
    let direct = &*MD_DIRECT_LINK;
    let reference = &*MD_REF_LINK;

    let mut links = Vec::new();
    for captures in direct.captures_iter(line) {
        let label = compact_markdown_text(captures.get(1).map_or("", |m| m.as_str()), 48);
        let target = compact_markdown_text(captures.get(2).map_or("", |m| m.as_str()), 96);
        if !label.is_empty() && !target.is_empty() {
            links.push(format!("[{label}]({target})"));
        }
    }
    for captures in reference.captures_iter(line) {
        let label = compact_markdown_text(captures.get(1).map_or("", |m| m.as_str()), 48);
        let reference = captures.get(2).map_or("", |m| m.as_str()).trim().to_owned();
        let target = if reference.is_empty() {
            label.clone()
        } else {
            compact_markdown_text(&reference, 48)
        };
        if !label.is_empty() && !target.is_empty() {
            links.push(format!("[{label}][{target}]"));
        }
    }
    links
}

fn strip_markdown_html_comment(line: &str, mut in_comment: bool) -> (String, bool) {
    let mut output = String::new();
    let mut current = 0usize;
    while current < line.len() {
        if in_comment {
            if let Some(end) = line[current..].find("-->") {
                current += end + 3;
                in_comment = false;
            } else {
                return (output, true);
            }
        } else if let Some(start) = line[current..].find("<!--") {
            output.push_str(&line[current..current + start]);
            let comment_start = current + start;
            if let Some(end) = line[comment_start..].find("-->") {
                current = comment_start + end + 3;
            } else {
                return (output, true);
            }
        } else {
            output.push_str(&line[current..]);
            break;
        }
    }
    (output, in_comment)
}

fn markdown_toc_start(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    !markdown_toc_end(line)
        && (lower.contains("<!-- toc")
            || lower.contains("<!-- table of contents")
            || lower.contains("<!-- doctoc")
            || lower.contains("<!-- markdown-toc"))
}

fn markdown_toc_end(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("<!-- end") || lower.contains("<!-- /toc") || lower.contains("tocstop")
}

fn compact_markdown_text(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    for (idx, ch) in compact.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

// ── Generic brace-depth heuristic ─────────────────────────────────────────────

pub fn extract_brace_depth_generic(content: &str) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<(usize, String)> = Vec::new();
    let mut depth = 0i32;

    for (i, &line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let open = trimmed.chars().filter(|&c| c == '{').count() as i32;
        let close = trimmed.chars().filter(|&c| c == '}').count() as i32;

        if depth == 0 || (close > 0 && depth - close <= 0) {
            kept.push((i + 1, line.trim_end().to_string()));
        }

        depth += open - close;
        if depth < 0 {
            depth = 0;
        }
    }

    if kept.is_empty() {
        None
    } else {
        Some(kept)
    }
}

// ── Public router ─────────────────────────────────────────────────────────────

pub fn extract_heuristic(content: &str, ext: &str) -> Option<Vec<(usize, String)>> {
    match ext {
        "py" => extract_python(content),
        "go" => extract_go(content),
        "c" | "h" => extract_c_family(content),
        "cpp" | "hpp" | "cc" | "cxx" => extract_c_family(content),
        "java" | "cs" => extract_kotlin_java_cs(content),
        "kt" | "kotlin" => extract_kotlin(content),
        "scala" => extract_scala(content),
        "rb" => extract_ruby(content),
        "php" => extract_php(content),
        "swift" => extract_swift(content),
        "css" | "scss" | "less" => extract_css_signatures(content),
        "html" | "htm" => extract_html_signatures(content),
        "sql" | "tsql" | "plsql" => extract_sql(content),
        "vue" | "svelte" => extract_vue_svelte(content),
        "sh" | "bash" | "zsh" | "fish" => extract_shell(content),
        "ex" | "exs" => extract_elixir(content),
        "hs" | "lhs" => extract_haskell(content),
        "md" | "markdown" => extract_markdown(content),
        "lua" => extract_brace_depth_generic(content),
        _ => extract_brace_depth_generic(content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(rows: &[(usize, String)], needle: &str) -> bool {
        rows.iter().any(|(_, text)| text.contains(needle))
    }

    fn assert_has(rows: &[(usize, String)], needle: &str) {
        assert!(has(rows, needle), "expected '{needle}' in {rows:?}");
    }

    fn assert_not_has(rows: &[(usize, String)], needle: &str) {
        assert!(!has(rows, needle), "did not expect '{needle}' in {rows:?}");
    }

    #[test]
    fn kotlin_extracts_class_functions_and_companion_without_bodies() {
        let src = "class Foo {\n    fun bar(): Int {\n        return 1\n    }\n\n    companion object {\n        const val Name = \"foo\"\n    }\n}\n";
        let rows = extract_heuristic(src, "kt").expect("kotlin must extract");
        assert_has(&rows, "class Foo");
        assert_has(&rows, "fun bar");
        assert_has(&rows, "companion object");
        assert_not_has(&rows, "return 1");
    }

    #[test]
    fn scala_extracts_object_def_and_val_without_body_expressions() {
        let src = "package example\n\ncase class User(id: String)\n\nobject Service {\n  def load(id: String): User = {\n    User(id)\n  }\n\n  val value: Int = 1\n}\n";
        let rows = extract_heuristic(src, "scala").expect("scala must extract");
        assert_has(&rows, "case class User");
        assert_has(&rows, "object Service");
        assert_has(&rows, "def load");
        assert_has(&rows, "val value");
        assert_not_has(&rows, "User(id)");
    }

    #[test]
    fn vue_and_svelte_extract_template_ids_and_script_signatures_without_style_or_bodies() {
        let src = "<template>\n  <section id=\"app\">Ready</section>\n</template>\n\n<style>\n.red { color: red; }\n</style>\n\n<script lang=\"ts\">\nexport function load() {\n  return 1;\n}\n</script>\n";
        for ext in ["vue", "svelte"] {
            let rows = extract_heuristic(src, ext).expect("sfc must extract");
            assert_has(&rows, "<template>");
            assert_has(&rows, "id=\"app\"");
            assert_has(&rows, "<script");
            assert_has(&rows, "export function load");
            assert_not_has(&rows, "color: red");
            assert_not_has(&rows, "return 1");
        }
    }

    #[test]
    fn elixir_extracts_module_and_defs_without_function_bodies() {
        let src =
            "defmodule App do\n  def run do\n    IO.puts(\"run\")\n  end\n\n  defp hidden do\n    :ok\n  end\nend\n";
        let rows = extract_heuristic(src, "ex").expect("elixir must extract");
        assert_has(&rows, "defmodule App");
        assert_has(&rows, "def run");
        assert_has(&rows, "defp hidden");
        assert_not_has(&rows, "IO.puts");
        assert_not_has(&rows, ":ok");
    }

    #[test]
    fn haskell_extracts_top_level_declarations_without_indented_bodies() {
        let src = "module App where\n\nanswer :: Int\nanswer =\n  1\n";
        let rows = extract_heuristic(src, "hs").expect("haskell must extract");
        assert_has(&rows, "module App");
        assert_has(&rows, "answer :: Int");
        assert_has(&rows, "answer =");
        assert_not_has(&rows, "  1");
    }

    #[test]
    fn sql_extracts_schema_statements_without_procedure_body() {
        let src = "CREATE TABLE users (\n  id INT,\n  name TEXT\n);\n\nCREATE PROCEDURE refresh_users()\nBEGIN\n  INSERT INTO users VALUES (1);\nEND;\n";
        let rows = extract_heuristic(src, "sql").expect("sql must extract");
        assert_has(&rows, "CREATE TABLE users");
        assert_has(&rows, "id INT");
        assert_has(&rows, "CREATE PROCEDURE");
        assert_not_has(&rows, "INSERT INTO");
    }

    #[test]
    fn php_extracts_namespace_class_and_methods_without_bodies() {
        let src = "<?php\nnamespace App;\n\nclass Foo {\n    public function bar() {\n        return 1;\n    }\n}\n";
        let rows = extract_heuristic(src, "php").expect("php must extract");
        assert_has(&rows, "namespace App");
        assert_has(&rows, "class Foo");
        assert_has(&rows, "function bar");
        assert_not_has(&rows, "return 1");
    }

    #[test]
    fn ruby_extracts_requires_classes_methods_and_attrs_without_bodies() {
        let src = "require 'json'\n\nclass Foo\n  attr_reader :name\n\n  def bar\n    puts 'body'\n  end\nend\n";
        let rows = extract_heuristic(src, "rb").expect("ruby must extract");
        assert_has(&rows, "require 'json'");
        assert_has(&rows, "class Foo");
        assert_has(&rows, "attr_reader");
        assert_has(&rows, "def bar");
        assert_not_has(&rows, "puts 'body'");
    }

    #[test]
    fn swift_extracts_imports_types_and_functions_without_bodies() {
        let src =
            "import Foundation\n\nstruct User {\n  let id: String\n}\n\npublic func load() -> User {\n  return User(id: \"1\")\n}\n";
        let rows = extract_heuristic(src, "swift").expect("swift must extract");
        assert_has(&rows, "import Foundation");
        assert_has(&rows, "struct User");
        assert_has(&rows, "public func load");
        assert_not_has(&rows, "return User");
    }

    #[test]
    fn generic_brace_depth_keeps_top_level_boundaries_without_nested_body_lines() {
        let src = "resource App {\n  nested value\n}\n\nresource Other {}\n";
        let rows = extract_heuristic(src, "unknown").expect("generic must extract");
        assert_has(&rows, "resource App");
        assert_has(&rows, "resource Other");
        assert_not_has(&rows, "nested value");
    }
}
