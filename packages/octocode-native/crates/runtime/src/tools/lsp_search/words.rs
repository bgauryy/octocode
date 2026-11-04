//! Whole-word search of text and files: the lexical checks importer
//! recovery and the ignored-file index make.
use std::borrow::Cow;
use std::path::Path;

/// Bytes a file search reads at a time.
pub(super) const MENTION_CHUNK_BYTES: usize = 1024 * 1024;

/// Files one search thread takes at least: a scoped thread costs tens of
/// microseconds, about what a small file's open and read take.
const FILES_PER_WORKER: usize = 64;

/// The `files` `spells` holds for, in order, on as many threads as the request
/// has a share of; and whether every one was checked (only `stopped`, the
/// request's end, cuts it).
pub(super) fn files_spelling<'a, T: AsRef<Path> + Sync>(
    files: &'a [T],
    stopped: &(dyn Fn() -> bool + Sync),
    spells: impl Fn(&Path) -> bool + Sync,
) -> (Vec<&'a T>, bool) {
    let mut items = files.iter().collect::<Vec<_>>();
    let workers = crate::tools::workers::workers(items.len() / FILES_PER_WORKER, None);
    let answers = crate::tools::workers::each_parallel(&mut items, workers, |file| {
        (!stopped()).then(|| spells((*file).as_ref()))
    });
    let read_all = answers.iter().all(Option::is_some);
    let found = files
        .iter()
        .zip(answers)
        .filter(|(_, answer)| *answer == Some(true))
        .map(|(file, _)| file)
        .collect();
    (found, read_all)
}

/// Whether the file at `path` spells `word` ([`spells_word`]) in its text, read
/// in [`MENTION_CHUNK_BYTES`] chunks whatever its size, as a text search reads
/// it: a UTF-16 file by its characters, and any other only up to a first NUL
/// (past it the file is binary, and a binary prefix holds no text at all). A
/// file gone since the index listed it spells nothing; `None` when it cannot be
/// read. A file over `limit` bytes is not searched: it spells nothing, as a text
/// search would not list it.
pub(super) fn file_spells(path: &Path, word: &[u8], limit: Option<u64>) -> Option<bool> {
    use std::io::Read;
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(false),
        Err(_) => return None,
    };
    let meta = file.metadata();
    if let (Some(limit), Ok(meta)) = (limit, &meta)
        && meta.len() > limit
    {
        return Some(false);
    }
    let chunk = meta.map_or(MENTION_CHUNK_BYTES, |meta| {
        usize::try_from(meta.len()).map_or(MENTION_CHUNK_BYTES, |len| {
            len.saturating_add(1).min(MENTION_CHUNK_BYTES)
        })
    });
    let mut window = Vec::with_capacity(chunk + word.len() + 4);
    // Whether the window starts at the file's start (later windows start
    // with a carried boundary byte).
    let mut opened = true;
    loop {
        let carried = window.len();
        let read = (&mut file)
            .take(MENTION_CHUNK_BYTES as u64)
            .read_to_end(&mut window)
            .ok()?;
        let closed = read < MENTION_CHUNK_BYTES;
        let utf16 = opened && octocode_engine::text::has_utf16_bom(&window);
        if utf16 && !closed {
            file.read_to_end(&mut window).ok()?;
        }
        if utf16 || (opened && closed) {
            return Some(spells_word(&searched_text(&window), word, true, true));
        }
        if let Some(nul) = memchr::memchr(0, &window[carried..]) {
            let text = &window[..carried + nul];
            return Some(
                spells_word(text, word, opened, true)
                    && octocode_engine::text::sniff(text)
                        != octocode_engine::text::BinaryVerdict::Binary,
            );
        }
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

/// The text a search reads in a whole file's `bytes`: a UTF-16 file's
/// characters; else the bytes up to a first NUL, none when that prefix is
/// binary.
pub(super) fn searched_text(bytes: &[u8]) -> Cow<'_, [u8]> {
    use octocode_engine::text::{BinaryVerdict, decode, has_utf16_bom, sniff};
    if has_utf16_bom(bytes) {
        return match decode(bytes).text {
            Cow::Borrowed(text) => Cow::Borrowed(text.as_bytes()),
            Cow::Owned(text) => Cow::Owned(text.into_bytes()),
        };
    }
    Cow::Borrowed(match memchr::memchr(0, bytes) {
        Some(nul) if sniff(&bytes[..nul]) == BinaryVerdict::Binary => &[],
        Some(nul) => &bytes[..nul],
        None => bytes,
    })
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
    memchr::memmem::find_iter(bytes, word).any(|at| {
        let end = at + word.len();
        (opened || at > 0 || !starts_ident)
            && (closed || end < bytes.len() || !ends_ident)
            && is_whole_word(bytes, at, end)
    })
}

#[cfg(test)]
mod tests;
