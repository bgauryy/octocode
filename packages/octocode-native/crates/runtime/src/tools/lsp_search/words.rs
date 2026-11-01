//! Whole-word search of text and files: the lexical checks importer
//! recovery and the ignored-file index make.
use std::path::Path;

/// Bytes a file search reads at a time.
pub(super) const MENTION_CHUNK_BYTES: usize = 1024 * 1024;

/// Whether the file at `path` spells `word` ([`spells_word`]), read in
/// [`MENTION_CHUNK_BYTES`] chunks whatever its size: a file gone since the
/// index listed it spells nothing; `None` when it cannot be read.
pub(super) fn file_spells(path: &Path, word: &[u8]) -> Option<bool> {
    use std::io::Read;
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(false),
        Err(_) => return None,
    };
    let mut window = Vec::with_capacity(MENTION_CHUNK_BYTES + word.len() + 1);
    let mut chunk = vec![0u8; MENTION_CHUNK_BYTES];
    // Whether the window starts at the file's start (later windows start
    // with a carried boundary byte).
    let mut opened = true;
    loop {
        let read = file.read(&mut chunk).ok()?;
        let closed = read == 0;
        window.extend_from_slice(&chunk[..read]);
        if spells_word(&window, word, opened, closed) {
            return Some(true);
        }
        if closed {
            return Some(false);
        }
        // Carry the tail an undecided match may span, with the character
        // (up to 4 bytes) before it.
        let keep = window.len().min(word.len() + 4);
        opened &= keep == window.len();
        window.drain(..window.len() - keep);
    }
}

/// Whether `bytes` holds `word` as a whole word
/// ([`octocode_engine::text::is_whole_word`]). Unless `opened` (the text
/// starts there), a match at the very start is undecided; unless `closed`
/// (the text ends there), so is one at the very end.
pub(super) fn spells_word(bytes: &[u8], word: &[u8], opened: bool, closed: bool) -> bool {
    use octocode_engine::text::{is_ident_char, is_whole_word};
    let Ok(name) = std::str::from_utf8(word) else {
        return false;
    };
    if name.is_empty() || bytes.len() < word.len() {
        return false;
    }
    // Whether each end of `word` needs a boundary (it is an identifier end).
    let starts_ident = name.chars().next().is_some_and(is_ident_char);
    let ends_ident = name.chars().next_back().is_some_and(is_ident_char);
    (0..=bytes.len() - word.len()).any(|at| {
        let end = at + word.len();
        bytes[at..].starts_with(word)
            && (opened || at > 0 || !starts_ident)
            && (closed || end < bytes.len() || !ends_ident)
            && is_whole_word(bytes, at, end)
    })
}

#[cfg(test)]
mod tests;
