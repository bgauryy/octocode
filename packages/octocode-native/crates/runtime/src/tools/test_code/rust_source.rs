//! What a Rust file's text and place can say without a parse: whether it could
//! be test code whole, from where it could name a module only a `cfg` mounts,
//! and which files may mount it. The text is never lexed: a comment or a literal
//! can only widen what these allow, and the parse decides what they allow.
use super::WHOLE_FILE;
use std::path::{Path, PathBuf};

/// Whether `content` could be test code whole ([`whole_or`]): a span covering
/// every line starts on line 1, so it is an inner `#![cfg(...)]` (of the file,
/// or of a module opened on line 1), or an item whose `cfg` or first outer
/// attribute (`#[derive(...)]` above `#[cfg(test)]`) is on line 1: an item's
/// span starts at its attributes.
pub(super) fn may_be_test_whole(content: &str) -> bool {
    content
        .lines()
        .next()
        .is_some_and(|first| first.contains("cfg") || first.trim_start().starts_with('#'))
        || has_inner_cfg(content)
}

/// Whether `content` spells what the parse reads as an inner `cfg`: it gates
/// the whole node around it, so text before it as well as after.
fn has_inner_cfg(content: &str) -> bool {
    content.match_indices("#![").any(|(at, opening)| {
        content[at + opening.len()..]
            .trim_start()
            .strip_prefix("cfg")
            .is_some_and(|after| {
                !after.starts_with(|next: char| next == '_' || next.is_alphanumeric())
            })
    })
}

/// The byte of `content` from which it could name a module it mounts only
/// under test; `None` when it could mount none. An outer `#[cfg(...)]` that
/// requires test spells `cfg`, then `test`, before the item it gates, and a
/// test span holds whole lines, so such a `mod` is named no earlier than the
/// start of that `test` word's line. The first `test` after the first `cfg`
/// comes no later than any gate's. An inner `cfg` can gate text before it, so
/// all of the text counts. A `cfg_attr` under test gates a `path` it gives the
/// same way; one giving none mounts nothing.
pub(super) fn first_gated_byte(content: &str) -> Option<usize> {
    if has_inner_cfg(content) {
        return Some(0);
    }
    let conditional_path =
        || word_at(content, "cfg_attr", 0).filter(|_| spells_path_attribute(content.as_bytes()));
    let cfg = [word_at(content, "cfg", 0), conditional_path()]
        .into_iter()
        .flatten()
        .min()?;
    let test = word_at(content, "test", cfg + "cfg".len())?;
    Some(content[..test].rfind('\n').map_or(0, |newline| newline + 1))
}

/// Where the identifier `word` first occurs in `content` at or after `from`, a
/// char boundary. Only ASCII beside it rules a spelling out, so what a
/// non-ASCII neighbor means can only add a spelling.
pub(super) fn word_at(content: &str, word: &str, from: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let in_word =
        |byte: Option<&u8>| byte.is_some_and(|byte| *byte == b'_' || byte.is_ascii_alphanumeric());
    content[from..]
        .match_indices(word)
        .map(|(at, _)| from + at)
        .find(|at| {
            !in_word(at.checked_sub(1).and_then(|before| bytes.get(before)))
                && !in_word(bytes.get(at + word.len()))
        })
}

/// Whether `content` at or after the char boundary `from` could spell the head
/// of the module `name`: the keyword `mod`, then `name` or `r#name`, with only
/// blanks or a comment between them.
pub(super) fn may_declare_module(content: &str, name: &str, from: usize) -> bool {
    let mut at = from;
    while let Some(found) = word_at(content, name, at) {
        at = found + name.len();
        let before = content[..found]
            .strip_suffix("r#")
            .unwrap_or(&content[..found]);
        if ends_in_module_keyword(before) {
            return true;
        }
    }
    false
}

/// Whether `before` could end in the keyword `mod` and trivia. A line comment
/// is never read back to what precedes it, so any such line counts.
fn ends_in_module_keyword(before: &str) -> bool {
    let before = before.trim_end();
    let line = before
        .rfind('\n')
        .map_or(before, |newline| &before[newline + 1..]);
    before.ends_with("*/")
        || line.contains("//")
        || before.strip_suffix("mod").is_some_and(|head| {
            !head
                .as_bytes()
                .last()
                .is_some_and(|byte| *byte == b'_' || byte.is_ascii_alphanumeric())
        })
}

/// Whether `content` could give a `mod` a `#[path]`: the word `path` right
/// after the `[` or `,` an attribute (or a `cfg_attr`) lists it behind, then `=`.
/// Read as bytes, so most of a directory's files are ruled out unconverted; any
/// non-ASCII byte beside the word counts as a blank, which can only widen this.
pub(super) fn spells_path_attribute(content: &[u8]) -> bool {
    let blank = |byte: &u8| byte.is_ascii_whitespace() || *byte == 0x0b || !byte.is_ascii();
    let in_word = |byte: Option<&u8>| byte.is_some_and(|b| *b == b'_' || b.is_ascii_alphanumeric());
    memchr::memmem::find_iter(content, b"path").any(|at| {
        let end = at + b"path".len();
        !in_word(at.checked_sub(1).and_then(|before| content.get(before)))
            && !in_word(content.get(end))
            && content[..at]
                .iter()
                .rev()
                .find(|byte| !blank(byte))
                .is_some_and(|byte| matches!(byte, b'[' | b','))
            && content[end..].iter().find(|byte| !blank(byte)) == Some(&b'=')
    })
}

/// `spans` of `content`, or [`WHOLE_FILE`] when one covers every line
/// (`#![cfg(test)]` spans the file).
pub(super) fn whole_or(spans: Vec<[u32; 2]>, content: &str) -> Vec<[u32; 2]> {
    let last = octocode_engine::num::saturating_u32(content.lines().count().max(1));
    if spans.iter().any(|[start, end]| *start <= 1 && *end >= last) {
        vec![WHOLE_FILE]
    } else {
        spans
    }
}

/// The module a Rust file defines: its stem, or its directory for `mod.rs`;
/// `None` for a crate root (`lib.rs`, `main.rs`), which nothing mounts.
pub(super) fn module_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    match stem.as_ref() {
        "lib" | "main" => None,
        "mod" => Some(path.parent()?.file_name()?.to_string_lossy().into_owned()),
        _ => Some(stem.into_owned()),
    }
}

/// The directory whose [`candidates_in`] may declare the module `path`
/// defines: its own, or the one above it for `mod.rs`.
pub(super) fn candidate_dir(path: &Path) -> Option<&Path> {
    let dir = path.parent()?;
    if path.file_stem().is_some_and(|stem| stem == "mod") {
        dir.parent()
    } else {
        Some(dir)
    }
}

/// The names of the files that may declare a module of `dir`, present or not:
/// its `mod.rs`, the sibling `<dir>.rs`, and a crate root in it. A file is not
/// its own parent: the caller leaves out the module it asks about.
pub(super) fn candidates_in(dir: &Path) -> Vec<PathBuf> {
    let mut candidates = ["mod.rs", "lib.rs", "main.rs"]
        .into_iter()
        .map(|name| dir.join(name))
        .collect::<Vec<_>>();
    if let (Some(up), Some(name)) = (dir.parent(), dir.file_name()) {
        candidates.push(up.join(format!("{}.rs", name.to_string_lossy())));
    }
    candidates
}
