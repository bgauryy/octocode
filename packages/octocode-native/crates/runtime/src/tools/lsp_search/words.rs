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
        // Carry the tail an undecided match may span, with the byte before.
        let keep = window.len().min(word.len() + 1);
        opened &= keep == window.len();
        window.drain(..window.len() - keep);
    }
}

/// Whether `bytes` holds `word` with no identifier character joined to
/// either end (any non-ASCII byte counts as one); an end of `word` that is
/// itself punctuation (`.name`) needs no boundary. Unless `opened` (the
/// text starts there), a match at the very start is undecided; unless
/// `closed` (the text ends there), so is one at the very end.
pub(super) fn spells_word(bytes: &[u8], word: &[u8], opened: bool, closed: bool) -> bool {
    let is_word =
        |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80;
    if word.is_empty() || bytes.len() < word.len() {
        return false;
    }
    let (first, last) = (word[0], word[word.len() - 1]);
    (0..=bytes.len() - word.len()).any(|at| {
        bytes[at..].starts_with(word)
            && (!is_word(first)
                || if at == 0 {
                    opened
                } else {
                    !is_word(bytes[at - 1])
                })
            && (!is_word(last)
                || bytes
                    .get(at + word.len())
                    .map_or(closed, |byte| !is_word(*byte)))
    })
}

#[cfg(test)]
mod tests;
