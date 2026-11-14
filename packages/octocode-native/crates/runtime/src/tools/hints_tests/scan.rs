//! Finds every prose template the production source can emit as a hint or
//! a notice: string literals (and `format!` templates) inside a hint or
//! notice emission (`push_notice`, `add_hints`, `set_hints`, `prepend_hints`,
//! `.hint(`, `hints.push(`, a `hints: vec![` or `.hints = vec![` list,
//! a `let hints…` binding,
//! `diagnostics.add(`, a `"hints"` key, `let hint = …`, a classification
//! error's hint), a `*_HINT`/`*_TIP`/`*_NOTE`/`*_NOTICE` constant, or the
//! body of a function named for a hint, tip, note, notice or disclosure.
//! Test files, `test_support` and test-only items
//! ([`production_text`]) are skipped. A literal without a space (a key, a
//! code, a lead name), or with fewer than two letters (a separator), is not
//! prose.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::tools::test_support::production_text;

/// One emitted template: where it is written and its text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Template {
    pub(super) file: String,
    pub(super) text: String,
}

/// An emission and the argument its prose starts at (`0`: the whole call).
const EMITTERS: &[(&str, usize)] = &[
    ("push_notice(", 1),
    (".hint(", 0),
    ("hints.push(", 0),
    ("hints: vec![", 0),
    (".hints = vec![", 0),
    ("add_hints(", 1),
    ("set_hints(", 1),
    ("prepend_hints(", 1),
    ("diagnostics.add(", 1),
    ("[\"hints\"] =", 0),
    ("\"hints\":", 0),
    ("ClassificationError::new(", 2),
];
const CONST_MARKS: &[&str] = &["HINT", "TIP", "NOTE", "NOTICE"];
/// A name part starting with one marks the function (`disclos`: disclose,
/// disclosed, disclosure).
const FN_MARKS: &[&str] = &["hint", "tip", "note", "notice", "disclos"];

/// A string literal: its byte span in the source and its text.
struct Literal {
    start: usize,
    text: String,
}

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries = std::fs::read_dir(dir)
        .expect("read src")
        .map(|entry| entry.expect("entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let test_name = name == "tests"
            || name == "test_support"
            || name == "tests.rs"
            || name.ends_with("_tests")
            || name.ends_with("_tests.rs");
        if test_name {
            continue;
        }
        if path.is_dir() {
            source_files(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// Every string literal outside comments, with escapes and line
/// continuations resolved.
fn literals(src: &str) -> Vec<Literal> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i = src[i..].find('\n').map_or(bytes.len(), |end| i + end);
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = src[i + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |end| i + end + 4);
            }
            b'r' if matches!(bytes.get(i + 1), Some(b'"' | b'#'))
                && !bytes
                    .get(i.wrapping_sub(1))
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_') =>
            {
                let hashes = bytes[i + 1..].iter().take_while(|b| **b == b'#').count();
                let open = i + 1 + hashes;
                if bytes.get(open) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                let close = format!("\"{}", "#".repeat(hashes));
                let end = src[open + 1..]
                    .find(&close)
                    .map_or(bytes.len(), |e| open + 1 + e);
                out.push(Literal {
                    start: i,
                    text: src[open + 1..end].to_owned(),
                });
                i = end + close.len();
            }
            b'\'' => {
                // A char literal ('x', '\n', '"') or a lifetime.
                if bytes.get(i + 1) == Some(&b'\\') {
                    i = src[i + 3..].find('\'').map_or(bytes.len(), |e| i + 4 + e);
                } else if bytes.get(i + 2) == Some(&b'\'') {
                    i += 3;
                } else {
                    i += src[i + 1..].chars().next().map_or(1, |c| 1 + c.len_utf8());
                }
            }
            b'"' => {
                let (text, end) = string_body(src, i + 1);
                out.push(Literal { start: i, text });
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

/// The text of a `"…"` literal whose body starts at `from`, and the byte
/// after its closing quote.
fn string_body(src: &str, from: usize) -> (String, usize) {
    let mut text = String::new();
    let mut chars = src[from..].char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return (text, from + at + 1),
            '\\' => match chars.next() {
                Some((_, '\n')) => {
                    let rest = chars.as_str();
                    let skipped = rest.len() - rest.trim_start().len();
                    for _ in 0..rest[..skipped].chars().count() {
                        chars.next();
                    }
                }
                Some((_, 'n')) => text.push('\n'),
                Some((_, 't')) => text.push('\t'),
                Some((_, other)) => text.push(other),
                None => break,
            },
            other => text.push(other),
        }
    }
    (text, src.len())
}

/// The end of the expression starting at `from`: the first `;` or `,` at
/// depth 0, or the bracket that closes an enclosing one.
fn expression_end(src: &str, from: usize, skip: &[usize]) -> usize {
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut i = from;
    while i < bytes.len() {
        if skip[i] != 0 {
            i = skip[i];
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth < 0 {
                    return i;
                }
            }
            b';' | b',' if depth == 0 => return i,
            _ => {}
        }
        i += 1;
    }
    i
}

/// The end of the braced block opening at or after `from`.
fn block_end(src: &str, from: usize, skip: &[usize]) -> usize {
    let Some(open) = src[from..].find('{').map(|at| from + at) else {
        return src.len();
    };
    (matching_close(src, open, skip) + 1).min(src.len())
}

/// The bracket that closes the one at `open`.
fn matching_close(src: &str, open: usize, skip: &[usize]) -> usize {
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        if skip[i] != 0 {
            i = skip[i];
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    i
}

fn literal_spans(src: &str, literals: &[Literal]) -> Vec<usize> {
    let mut jump = vec![0; src.len() + 1];
    for literal in literals {
        let body = src[literal.start..]
            .find('"')
            .map_or(literal.start, |q| literal.start + q);
        let end = if src.as_bytes()[literal.start] == b'r' {
            let hashes = body - literal.start - 1;
            src[body + 1..]
                .find(&format!("\"{}", "#".repeat(hashes)))
                .map_or(src.len(), |e| body + 1 + e + 1 + hashes)
        } else {
            string_body(src, body + 1).1
        };
        jump[literal.start] = end;
    }
    jump
}

/// Spans of `src` whose literals are hint or notice prose.
fn hint_spans(src: &str, skip: &[usize]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for (emitter, argument) in EMITTERS {
        for (at, _) in src.match_indices(emitter) {
            let after = at + emitter.len();
            if emitter.ends_with(['(', '[']) {
                let close = matching_close(src, after - 1, skip);
                let mut from = after;
                for _ in 0..*argument {
                    from = (expression_end(src, from, skip) + 1).min(close);
                }
                spans.push((from, close));
            } else {
                spans.push((at, expression_end(src, after, skip)));
            }
        }
    }
    for (at, _) in src.match_indices("let ") {
        let rest = src[at + 4..].trim_start_matches("mut ");
        if rest.starts_with("hint") {
            spans.push((at, expression_end(src, at + 4, skip)));
        }
    }
    for (at, _) in src.match_indices("const ") {
        let name = src[at + 6..].split(':').next().unwrap_or_default().trim();
        let named = CONST_MARKS
            .iter()
            .any(|mark| name.split('_').any(|part| part == *mark));
        if named && name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
            spans.push((at, expression_end(src, at + 6, skip)));
        }
    }
    for (at, _) in src.match_indices("fn ") {
        let name = src[at + 3..]
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default();
        if FN_MARKS
            .iter()
            .any(|mark| name.split('_').any(|part| part.starts_with(mark)))
        {
            spans.push((at, block_end(src, at, skip)));
        }
    }
    spans
}

/// The production source files under `src_root`, each with its path
/// relative to it and its [`production_text`].
fn production_files(src_root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    source_files(src_root, &mut files);
    files
        .into_iter()
        .map(|path| {
            let file = path
                .strip_prefix(src_root)
                .expect("under src")
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&path).expect("read source");
            (file, production_text(&src))
        })
        .collect()
}

/// Every hint or notice template under `src_root`, file paths relative to it.
pub(super) fn templates(src_root: &Path) -> BTreeSet<Template> {
    production_files(src_root)
        .iter()
        .flat_map(|(file, src)| file_templates(file, src))
        .collect()
}

/// The hint or notice templates of one file's production text.
fn file_templates(file: &str, src: &str) -> Vec<Template> {
    let literals = literals(src);
    let skip = literal_spans(src, &literals);
    let spans = hint_spans(src, &skip);
    literals
        .iter()
        .filter(|literal| {
            spans
                .iter()
                .any(|(a, b)| literal.start >= *a && literal.start < *b)
        })
        .filter(|literal| literal.text.contains(' '))
        // A separator (`", "`, `"; "`) or a bare slot is no wording.
        .filter(|literal| literal.text.chars().filter(|c| c.is_alphabetic()).count() >= 2)
        .map(|literal| Template {
            file: file.to_owned(),
            text: literal.text.clone(),
        })
        .collect()
}

/// A wire field only its owner writes, and how a writer builds it raw.
/// `response/` owns them all: `add_hints`/`prepend_hints`/`set_hints`
/// (`hints`), `mark_partial`/`terminal_limit` (`isPartial`) and
/// `Continuation` (`"kind":"continue"`).
const OWNED_FIELDS: &[&str] = &["hints", "isPartial"];

/// A raw write of an owned field outside the output owner.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RawWrite {
    pub(super) file: String,
    pub(super) field: &'static str,
}

/// Whether the literal spanning `start..end` is written as a key: assigned
/// (`row["k"] = …`), a map or `json!` key (`"k": …`), or inserted
/// (`insert("k", …)`, `entry("k")`). A comparison or a read is not a write.
fn is_written(src: &str, start: usize, end: usize) -> bool {
    let after = src[end..].trim_start();
    let before = src[..start].trim_end();
    (after.starts_with(':') && !after.starts_with("::"))
        || (after.starts_with("] =") && !after.starts_with("] =="))
        || before.ends_with("insert(")
        || before.ends_with("entry(")
}

/// Every raw write of `hints`, `isPartial` or `"kind":"continue"` in the
/// production source under `src_root` outside `response/`.
pub(super) fn raw_writes(src_root: &Path) -> BTreeSet<RawWrite> {
    let mut out = BTreeSet::new();
    for (file, src) in production_files(src_root) {
        if file.starts_with("response/") {
            continue;
        }
        let literals = literals(&src);
        let skip = literal_spans(&src, &literals);
        for literal in &literals {
            let end = skip[literal.start];
            if let Some(field) = OWNED_FIELDS.iter().find(|f| **f == literal.text)
                && is_written(&src, literal.start, end)
            {
                out.insert(RawWrite {
                    file: file.clone(),
                    field,
                });
            }
            if literal.text == "kind" && is_written(&src, literal.start, end) {
                let value_end = expression_end(&src, end, &skip);
                let builds_continue = literals
                    .iter()
                    .any(|l| l.text == "continue" && l.start >= end && l.start < value_end);
                if builds_continue {
                    out.insert(RawWrite {
                        file: file.clone(),
                        field: "kind:continue",
                    });
                }
            }
        }
    }
    out
}

/// Each emission form the scan reads hint prose from yields its literal;
/// the same literal in test-only code, outside an emission, or without
/// wording yields none.
#[test]
fn every_emission_form_yields_its_prose_and_test_code_none() {
    const PROSE: &str = "read the file first";
    let texts = |src: &str| {
        file_templates("f.rs", &production_text(src))
            .into_iter()
            .map(|template| template.text)
            .collect::<Vec<_>>()
    };
    let mut forms = EMITTERS
        .iter()
        .map(|(emitter, argument)| {
            let args = "x, ".repeat(*argument);
            let close = if emitter.ends_with('[') { "]" } else { ")" };
            let open = emitter.ends_with(['(', '[']);
            let tail = if open { close } else { "" };
            format!("fn f() {{ {emitter}{args}\"{PROSE}\"{tail}; }}\n")
        })
        .collect::<Vec<_>>();
    forms.extend(
        CONST_MARKS
            .iter()
            .map(|mark| format!("const A_{mark}: &str = \"{PROSE}\";\n")),
    );
    forms.extend(
        FN_MARKS
            .iter()
            .map(|mark| format!("fn {mark}_text() -> &'static str {{ \"{PROSE}\" }}\n")),
    );
    forms.push(format!("fn f() {{ let hint = \"{PROSE}\"; }}\n"));
    for form in &forms {
        assert_eq!(texts(form), [PROSE], "{form}");
        for gate in [
            "#[test]",
            "#[tokio::test]",
            "#[tokio::test(flavor = \"multi_thread\")]",
            "#[cfg(test)]",
        ] {
            let gated = format!("{gate}\n{form}");
            assert!(texts(&gated).is_empty(), "{gated}");
        }
        let in_tests = format!("#[cfg(test)]\nmod tests {{\n{form}}}\n");
        assert!(texts(&in_tests).is_empty(), "{in_tests}");
        for production in ["#[cfg(not(test))]", "#[must_use]", "#[expect(dead_code)]"] {
            let kept = format!("{production}\n{form}");
            assert_eq!(texts(&kept), [PROSE], "{kept}");
        }
    }
    for not_prose in [
        "fn f() { x.hint(\"code\"); }",
        "fn f() { x.hint(\", \"); }",
        "fn f() { let s = \"read the file first\"; }",
    ] {
        assert!(texts(not_prose).is_empty(), "{not_prose}");
    }
}
