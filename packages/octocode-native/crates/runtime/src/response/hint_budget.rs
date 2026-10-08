//! X11: the runtime never clips guidance ([`super::rows::concise`]), so
//! every hint and lead `why` source fits [`MAX_GUIDANCE_CHARS`] whole.
//!
//! Two sweeps cover all tools: the response stage's own recoveries (every
//! tool's fallback and every declared error code), and a scan of production
//! sources for the literals a hint or `why` is written from (`*HINT*`
//! constants, `fn *hint*` bodies, and the statement that names a hint or
//! `.why(`). A `{…}` placeholder counts as empty: its value is evidence.
use super::rows::MAX_GUIDANCE_CHARS;
use crate::tools::id::{ToolId, error_codes};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Visible characters of a literal's source text: escapes resolved,
/// `format!` placeholders dropped.
fn visible_chars(source: &str) -> usize {
    let mut count = 0;
    let mut chars = source.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\\' => {
                chars.next();
                count += 1;
            }
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                count += 1;
            }
            '{' => {
                for inner in chars.by_ref() {
                    if inner == '}' {
                        break;
                    }
                }
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                count += 1;
            }
            _ => count += 1,
        }
    }
    count
}

/// One source line split into its code (literals blanked) and its string
/// literals.
fn split_line(line: &str) -> (String, Vec<String>) {
    let mut code = String::new();
    let mut literals = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '/' && chars.peek() == Some(&'/') {
            break;
        }
        if character == '\'' {
            // A char literal ('"', '\'') never opens a string.
            let rest: String = chars.clone().take(3).collect();
            if rest.starts_with("\\") && rest.chars().nth(2) == Some('\'') {
                chars.nth(2);
                code.push_str("' '");
                continue;
            }
            if rest.chars().nth(1) == Some('\'') {
                chars.nth(1);
                code.push_str("' '");
                continue;
            }
        }
        if character != '"' {
            code.push(character);
            continue;
        }
        let mut literal = String::new();
        while let Some(inner) = chars.next() {
            match inner {
                '\\' => {
                    literal.push(inner);
                    if let Some(escaped) = chars.next() {
                        literal.push(escaped);
                    }
                }
                '"' => break,
                _ => literal.push(inner),
            }
        }
        code.push_str("\"\"");
        literals.push(literal);
    }
    (code, literals)
}

fn bracket_depth(code: &str) -> i32 {
    code.chars()
        .map(|character| match character {
            '(' | '[' | '{' => 1,
            ')' | ']' | '}' => -1,
            _ => 0,
        })
        .sum()
}

/// Production text of one source file: its `#[cfg(test)] mod` cut off.
fn production(text: &str) -> &str {
    let mut offset = 0;
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    for (index, line) in lines.iter().enumerate() {
        if line.trim() == "#[cfg(test)]"
            && lines
                .get(index + 1)
                .is_some_and(|next| next.trim_start().starts_with("mod ") || next.contains(" mod "))
        {
            return &text[..offset];
        }
        offset += line.len();
    }
    text
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("source dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "tests") {
                rust_sources(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name != "tests.rs" && !name.ends_with("_tests.rs")
            })
        {
            out.push(path);
        }
    }
}

/// Guidance literals of one production file, with their line numbers.
fn guidance_literals(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    // Open statement or `fn *hint*` body: the bracket depth it closes at.
    let mut statement: Option<i32> = None;
    let mut hint_fn: Option<i32> = None;
    let mut depth = 0;
    for (index, line) in production(text).lines().enumerate() {
        let (code, literals) = split_line(line);
        let lower = code.to_ascii_lowercase();
        let names_hint = lower.contains("hint") || lower.contains(".why(");
        if hint_fn.is_none() && lower.contains("fn ") && lower.contains("hint") {
            hint_fn = Some(depth);
        }
        if statement.is_none() && names_hint && !lower.contains("warn") {
            statement = Some(depth);
        }
        if statement.is_some() || hint_fn.is_some() {
            found.extend(literals.into_iter().map(|literal| (index + 1, literal)));
        }
        depth += bracket_depth(&code);
        if let Some(open) = statement
            && depth <= open
            && (code.trim_end().ends_with(';')
                || code.trim_end().ends_with(',')
                || code.trim_end().ends_with('}')
                || code.trim_end().ends_with(')'))
        {
            statement = None;
        }
        if let Some(open) = hint_fn
            && depth <= open
            && code.contains('}')
        {
            hint_fn = None;
        }
    }
    found
}

#[test]
fn every_hint_source_fits_the_guidance_budget() {
    let mut over = Vec::new();
    // The response stage's recoveries, for every tool and declared code.
    let queries = [
        json!({}),
        json!({"path": "/etc"}),
        json!({"owner": "o", "repo": "r"}),
    ];
    for tool in ToolId::ALL {
        for query in &queries {
            let hint = tool.output().fallback_hint(query);
            if hint.chars().count() > MAX_GUIDANCE_CHARS {
                over.push(format!("{tool:?} fallback: {hint}"));
            }
        }
        for (code, _) in error_codes::ALL {
            for hint in [
                super::rows::error_code_hint(tool, code),
                tool.output().error_hint(code),
            ]
            .into_iter()
            .flatten()
            {
                if hint.chars().count() > MAX_GUIDANCE_CHARS {
                    over.push(format!("{tool:?} {code}: {hint}"));
                }
            }
        }
    }
    // Every guidance literal in production sources.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    assert!(files.len() > 100, "the scan reads the runtime sources");
    for file in files {
        let text = std::fs::read_to_string(&file).expect("source");
        for (line, literal) in guidance_literals(&text) {
            let chars = visible_chars(&literal);
            if chars > MAX_GUIDANCE_CHARS {
                over.push(format!(
                    "{}:{line} ({chars} chars): {literal}",
                    file.strip_prefix(&root).unwrap_or(&file).display()
                ));
            }
        }
    }
    assert!(
        over.is_empty(),
        "hint sources over {MAX_GUIDANCE_CHARS} chars (the runtime no longer clips them):\n{}",
        over.join("\n")
    );
}

#[test]
fn the_scan_reads_hint_statements_and_skips_warnings() {
    let source = r#"
const LONG_HINT: &str = "aaaa";
fn f() {
    warnings.push(format!("warning text {x}"));
    let hint = format!(
        "hint text {x}"
    );
    call().why("why text");
    let message = "plain message";
}
fn recovery_hint(code: &str) -> &'static str {
    match code {
        "x" => "arm text",
        _ => "other arm",
    }
}
fn after() { let message = "after"; }
"#;
    let texts = guidance_literals(source)
        .into_iter()
        .map(|(_, literal)| literal)
        .collect::<Vec<_>>();
    assert_eq!(
        texts,
        [
            "aaaa",
            "hint text {x}",
            "why text",
            "x",
            "arm text",
            "other arm"
        ]
    );
    assert_eq!(visible_chars("a {x} \\\"b\\\" {{c}}"), 10);
    // Hints keep their text whole.
    let long = "word ".repeat(40);
    assert_eq!(
        super::rows::concise(&long).chars().count(),
        long.trim_end().len() + 1
    );
}
