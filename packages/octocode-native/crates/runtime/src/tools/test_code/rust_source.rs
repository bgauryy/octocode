//! What a Rust file's text and place can say without a parse: whether it could
//! be test code whole, from where it could name a module only a `cfg` mounts,
//! and which files may mount it. The text is never lexed: a comment or a literal
//! can only widen what these allow, and the parse decides what they allow.
use super::WHOLE_FILE;
use std::path::{Path, PathBuf};

/// Whether `content` could be test code whole ([`whole_or`]): a span covering
/// every line starts on line 1, so it is an inner `#![cfg(...)]` (of the file,
/// or of a module opened on line 1) or an item its `cfg` opens on line 1.
pub(super) fn may_be_test_whole(content: &str) -> bool {
    content
        .lines()
        .next()
        .is_some_and(|first| first.contains("cfg"))
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
/// all of the text counts.
pub(super) fn first_gated_byte(content: &str) -> Option<usize> {
    if has_inner_cfg(content) {
        return Some(0);
    }
    let cfg = word_at(content, "cfg", 0)?;
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

/// The files that may declare a module of `dir`: its `mod.rs`, the sibling
/// `<dir>.rs`, and a crate root in it. A file is not its own parent: the
/// caller leaves out the module it asks about.
pub(super) fn candidates_in(dir: &Path) -> Vec<PathBuf> {
    let mut candidates = ["mod.rs", "lib.rs", "main.rs"]
        .into_iter()
        .map(|name| dir.join(name))
        .collect::<Vec<_>>();
    if let (Some(up), Some(name)) = (dir.parent(), dir.file_name()) {
        candidates.push(up.join(format!("{}.rs", name.to_string_lossy())));
    }
    candidates.retain(|candidate| candidate.is_file());
    candidates
}
