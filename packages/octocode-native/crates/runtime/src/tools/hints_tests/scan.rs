//! Finds every prose template the production source can emit as a hint or
//! a notice: string literals (and `format!` templates) inside a hint or
//! notice emission (`push_hint`, `push_notice`, `.hint(`, `hints.push(`,
//! `diagnostics.add(`, a `"hints"` key, `let hint = …`, a classification
//! error's hint), a `*_HINT`/`*_TIP`/`*_NOTE`/`*_NOTICE` constant, or the
//! body of a function named for a hint, tip, note, notice or disclosure.
//! Test files and `#[cfg(test)]` modules are skipped. A literal without a
//! space (a key, a code, a lead name), or with fewer than two letters (a
//! separator), is not prose.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One emitted template: where it is written and its text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Template {
    pub(super) file: String,
    pub(super) text: String,
}

/// An emission and the argument its prose starts at (`0`: the whole call).
const EMITTERS: &[(&str, usize)] = &[
    ("push_hint(", 1),
    ("push_notice(", 1),
    (".hint(", 0),
    ("hints.push(", 0),
    ("diagnostics.add(", 1),
    ("[\"hints\"] =", 0),
    ("\"hints\":", 0),
    ("ClassificationError::new(", 2),
];
const CONST_MARKS: &[&str] = &["HINT", "TIP", "NOTE", "NOTICE"];
const FN_MARKS: &[&str] = &["hint", "tip", "note", "notice", "disclose"];

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

/// `#[cfg(test)]` module bodies.
fn test_modules(src: &str, skip: &[usize]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(found) = src[at..].find("#[cfg(test)]") {
        let start = at + found;
        let rest = src[start + 12..].trim_start();
        let item = start + 12 + (src[start + 12..].len() - rest.len());
        if rest.starts_with("mod ")
            && src[item..]
                .find('{')
                .is_some_and(|b| !src[item..item + b].contains(';'))
        {
            out.push((start, block_end(src, item, skip)));
        }
        at = start + 12;
    }
    out
}

/// Spans of `src` whose literals are hint or notice prose.
fn hint_spans(src: &str, skip: &[usize]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for (emitter, argument) in EMITTERS {
        for (at, _) in src.match_indices(emitter) {
            let after = at + emitter.len();
            if emitter.ends_with('(') {
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

/// Every hint or notice template under `src_root`, file paths relative to it.
pub(super) fn templates(src_root: &Path) -> BTreeSet<Template> {
    let mut files = Vec::new();
    source_files(src_root, &mut files);
    let mut out = BTreeSet::new();
    for path in files {
        let src = std::fs::read_to_string(&path).expect("read source");
        let literals = literals(&src);
        let skip = literal_spans(&src, &literals);
        let tests = test_modules(&src, &skip);
        let spans = hint_spans(&src, &skip);
        let file = path
            .strip_prefix(src_root)
            .expect("under src")
            .to_string_lossy()
            .replace('\\', "/");
        for literal in &literals {
            let inside = |spans: &[(usize, usize)]| {
                spans
                    .iter()
                    .any(|(a, b)| literal.start >= *a && literal.start < *b)
            };
            if inside(&tests) || !inside(&spans) || !literal.text.contains(' ') {
                continue;
            }
            // A separator (`", "`, `"; "`) or a bare slot is no wording.
            if literal.text.chars().filter(|c| c.is_alphabetic()).count() < 2 {
                continue;
            }
            out.insert(Template {
                file: file.clone(),
                text: literal.text.clone(),
            });
        }
    }
    out
}

/// The registry rows: `owner<TAB>file<TAB>template` (template escapes `\n`).
pub(super) fn registry(text: &str) -> BTreeMap<Template, String> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut cells = line.splitn(3, '\t');
            let owner = cells.next().expect("owner").to_owned();
            let file = cells.next().expect("file").to_owned();
            let text = cells.next().expect("template").replace("\\n", "\n");
            (Template { file, text }, owner)
        })
        .collect()
}
