//! What each listed file spells, kept across requests, so a search of the
//! scope for a name reads only the files that may hold it.
//!
//! A file's entry is the hash of every whole word in its text, as
//! [`file_spells`] decides whole words, under the [`SourceStamp`] of the bytes
//! it was built from. A hash miss at an equal stamp proves the file does not
//! spell the name. A hit is confirmed by reading the file, so a collision costs
//! one read, never a false claim. Only a settled file is kept, so any later
//! write moves its stamp; a file written since is read, and kept again once
//! settled.
//!
//! Hashes only, in memory only: no entry holds text, and none reaches disk.
use crate::tools::lsp_search::words::{
    MENTION_CHUNK_BYTES, file_spells, searched_text, spells_word,
};
use crate::tools::page_memo::PageMemo;
use octocode_engine::stamp::{SourceStamp, cache_stamp};
use octocode_engine::text::is_ident_char;
use std::path::Path;
use std::time::SystemTime;

/// One file's whole words, sorted.
struct Spelled {
    stamp: SourceStamp,
    tokens: Box<[u64]>,
}

/// Entries by canonical path, against the one process budget.
static SPELLED: PageMemo<Spelled> = PageMemo::new();

/// What a file spells depends on its bytes alone; which files a request may
/// search is its listing's decision.
const ANY_POLICY: &str = "";

/// One name's search of a request's listed files.
pub(super) struct WordSearch<'a> {
    word: &'a [u8],
    /// The hash of a name a file's entry can answer for: one made only of
    /// identifier characters is a whole token wherever it is a whole word.
    /// Any other name crosses tokens, and is searched for in the text.
    hash: Option<u64>,
    /// A larger file is not searched.
    limit: Option<u64>,
    /// The request's settle clock, read before any file.
    clock: SystemTime,
}

impl<'a> WordSearch<'a> {
    pub(super) fn new(symbol: &'a str, limit: Option<u64>, clock: SystemTime) -> Self {
        let word = symbol.as_bytes();
        let identifier = !symbol.is_empty() && symbol.chars().all(is_ident_char);
        Self {
            word,
            hash: identifier.then(|| word_hash(word)),
            limit,
            clock,
        }
    }

    /// [`file_spells`] of `path` under this search's limit, reading the file
    /// only when its entry cannot rule the name out.
    pub(super) fn file_spells(&self, path: &Path) -> Option<bool> {
        let (Some(hash), Some(key)) = (self.hash, path.to_str()) else {
            return self.search_text(path);
        };
        let Ok(meta) = std::fs::metadata(path) else {
            return self.search_text(path);
        };
        if self.limit.is_some_and(|limit| meta.len() > limit) {
            return Some(false);
        }
        let Some(stamp) = cache_stamp(&meta).filter(|_| meta.is_file()) else {
            return self.search_text(path);
        };
        let kept = SPELLED.get(key, ANY_POLICY, |kept| {
            (kept.stamp == stamp).then(|| kept.tokens.binary_search(&hash).is_ok())
        });
        match kept {
            Some(Some(true)) => return self.search_text(path),
            Some(Some(false)) => return Some(false),
            Some(None) => SPELLED.evict(key),
            None => {}
        }
        // A search crossing read windows sees less around each one than a whole
        // text does, so only a file one window holds gets an entry.
        if usize::try_from(meta.len()).is_ok_and(|len| len < MENTION_CHUNK_BYTES) {
            self.read_and_keep(path, key, &meta, stamp)
        } else {
            self.search_text(path)
        }
    }

    fn search_text(&self, path: &Path) -> Option<bool> {
        #[cfg(test)]
        reads::note(path);
        file_spells(path, self.word, self.limit)
    }

    /// Search one window's file, keeping its entry when `meta`, its settled
    /// stamp from before the read, still describes it after.
    fn read_and_keep(
        &self,
        path: &Path,
        key: &str,
        meta: &std::fs::Metadata,
        stamp: SourceStamp,
    ) -> Option<bool> {
        use std::io::Read;
        let Ok(mut file) = std::fs::File::open(path) else {
            return self.search_text(path);
        };
        #[cfg(test)]
        reads::note(path);
        let capacity = usize::try_from(meta.len()).unwrap_or(0).saturating_add(1);
        let mut bytes = Vec::with_capacity(capacity);
        let window = (&mut file)
            .take(MENTION_CHUNK_BYTES as u64)
            .read_to_end(&mut bytes);
        if !window.is_ok_and(|read| read < MENTION_CHUNK_BYTES) {
            return self.search_text(path);
        }
        let text = searched_text(&bytes);
        let settled = SourceStamp::settled(meta, self.clock).is_some()
            && file.metadata().ok().and_then(|now| cache_stamp(&now)) == Some(stamp);
        if settled {
            keep(key, stamp, token_hashes(&text));
        }
        Some(spells_word(&text, self.word, true, true))
    }
}

fn keep(key: &str, stamp: SourceStamp, tokens: Box<[u64]>) {
    let bytes = std::mem::size_of::<Spelled>() + std::mem::size_of_val(&*tokens);
    SPELLED.put(
        key.to_owned(),
        ANY_POLICY.to_owned(),
        bytes,
        Spelled { stamp, tokens },
    );
}

/// The hashes of the whole words of `text`, sorted, one each: the maximal runs
/// of identifier characters that no undecodable byte touches, which are exactly
/// the names [`octocode_engine::text::is_whole_word`] accepts in it.
fn token_hashes(text: &[u8]) -> Box<[u64]> {
    let mut hashes = Vec::new();
    let mut undecodable_before = false;
    for chunk in text.utf8_chunks() {
        let valid = chunk.valid();
        let undecodable_after = !chunk.invalid().is_empty();
        let mut run = None;
        let mut close = |start: usize, end: usize| {
            if start > 0 || !undecodable_before {
                hashes.push(word_hash(&valid.as_bytes()[start..end]));
            }
        };
        for (at, ch) in valid.char_indices() {
            match (is_ident_char(ch), run) {
                (true, None) => run = Some(at),
                (false, Some(start)) => {
                    close(start, at);
                    run = None;
                }
                _ => {}
            }
        }
        if let Some(start) = run
            && !undecodable_after
        {
            close(start, valid.len());
        }
        undecodable_before = undecodable_after;
    }
    hashes.sort_unstable();
    hashes.dedup();
    hashes.into_boxed_slice()
}

/// FNV-1a: fast on short names; a collision costs one read.
fn word_hash(word: &[u8]) -> u64 {
    word.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Test-only count of the times each file was opened to search its text.
#[cfg(test)]
pub(super) mod reads {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{LazyLock, Mutex, PoisonError};

    static READS: LazyLock<Mutex<HashMap<PathBuf, usize>>> = LazyLock::new(Mutex::default);

    pub(in super::super) fn note(path: &Path) {
        *READS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(path.to_path_buf())
            .or_default() += 1;
    }

    pub(in super::super) fn of(path: &Path) -> usize {
        READS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .copied()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
