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
use crate::lsp_search::words::{MENTION_CHUNK_BYTES, file_spells, searched_text, spells_word};
use octocode_engine::stamp::{Held, Link, SourceStamp, cache_stamp};
use octocode_engine::text::is_ident_char;
use octocode_tools_shared::page_memo::PageMemo;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
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
    /// What the request's held proofs show ([`octocode_engine::stamp::held`]):
    /// a file whose kept entry they prove current costs no stat.
    held: Held,
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
            held: octocode_engine::stamp::held(),
        }
    }

    /// [`file_spells`] of `path` under this search's limit, reading the file
    /// only when its entry cannot rule the name out. An entry whose stamp a
    /// proof that held in this request shows current (the scope
    /// fingerprint's, which stamps every file) answers without a stat.
    pub(super) fn file_spells(&self, path: &Path) -> Option<bool> {
        let (Some(hash), Some(key)) = (self.hash, path.to_str()) else {
            return self.search_text(path);
        };
        if let Some((stamp, spelled)) = SPELLED.get(key, ANY_POLICY, |kept| {
            (kept.stamp, kept.tokens.binary_search(&hash).is_ok())
        }) && self.held.proves(path, Link::Follow, stamp)
        {
            return if self.limit.is_some_and(|limit| stamp.size > limit) {
                Some(false)
            } else if spelled {
                self.search_text(path)
            } else {
                Some(false)
            };
        }
        #[cfg(test)]
        STATS.note(path);
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
        READS.note(path);
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
        let Some((bytes, settled)) = read_window(path, meta, stamp, self.clock) else {
            return self.search_text(path);
        };
        let text = searched_text(&bytes);
        if settled {
            let tokens = token_hashes(&text);
            // The entry just built answers a miss: the text is searched only
            // to confirm a hit, as on any later request.
            let held = self
                .hash
                .is_none_or(|hash| tokens.binary_search(&hash).is_ok());
            keep(key, stamp, tokens);
            if !held {
                return Some(false);
            }
        }
        Some(spells_word(&text, self.word, true, true))
    }
}

/// The bytes of `path`, which one read window holds, and whether
/// its settled stamp `stamp` (from `meta`, before the read) still describes
/// it after; `None` when it cannot be read or outgrows the window.
fn read_window(
    path: &Path,
    meta: &std::fs::Metadata,
    stamp: SourceStamp,
    clock: SystemTime,
) -> Option<(Vec<u8>, bool)> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    #[cfg(test)]
    READS.note(path);
    let capacity = usize::try_from(meta.len()).unwrap_or(0).saturating_add(1);
    let mut bytes = Vec::with_capacity(capacity);
    let window = (&mut file)
        .take(MENTION_CHUNK_BYTES as u64)
        .read_to_end(&mut bytes);
    if !window.is_ok_and(|read| read < MENTION_CHUNK_BYTES) {
        return None;
    }
    let settled = SourceStamp::settled(meta, clock).is_some()
        && file.metadata().ok().and_then(|now| cache_stamp(&now)) == Some(stamp);
    Some((bytes, settled))
}

/// Requests of this tool in flight, and the event of the count reaching
/// zero: the entries built after an answer ([`build_after_answer`]) wait for
/// it and yield to the next request.
static IN_FLIGHT: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());
/// Whether a build of entries after an answer is under way: one at a time.
static BUILDING: AtomicBool = AtomicBool::new(false);

fn in_flight_count() -> std::sync::MutexGuard<'static, usize> {
    IN_FLIGHT
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One request of this tool in flight, until dropped.
pub(in crate::lsp_search) struct InFlight(());

impl Drop for InFlight {
    fn drop(&mut self) {
        let mut count = in_flight_count();
        *count = count.saturating_sub(1);
        if *count == 0 {
            IN_FLIGHT.1.notify_all();
        }
    }
}

/// Count a request in flight while the guard lives.
pub(in crate::lsp_search) fn in_flight() -> InFlight {
    *in_flight_count() += 1;
    InFlight(())
}

/// Build the entries of a scope's newly listed `files` once no request is in
/// flight, on a background thread, so the next request for another name
/// rules most files out by their entries instead of reading the whole scope
/// (O3c). The thread waits for the requests in flight to end (the event, no
/// timer) and stops at the next request, which reads what is left as it does
/// today; an entry is kept only for a settled file, as
/// [`WordSearch::file_spells`] keeps one.
pub(super) fn build_after_answer(files: Arc<[PathBuf]>) {
    if cfg!(test) || BUILDING.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("octocode-lsp-spelling".into())
        .spawn(move || {
            struct Done;
            impl Drop for Done {
                fn drop(&mut self) {
                    BUILDING.store(false, Ordering::Release);
                }
            }
            let _done = Done;
            {
                let mut count = in_flight_count();
                while *count > 0 {
                    count = IN_FLIGHT
                        .1
                        .wait(count)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
            }
            let clock = octocode_engine::stamp::settle_clock();
            for file in files.iter() {
                if *in_flight_count() > 0 {
                    return;
                }
                build_entry(file, clock);
            }
        });
    if spawned.is_err() {
        BUILDING.store(false, Ordering::Release);
    }
}

/// Keep the entry of `path` unless a current one is kept: a settled file one
/// read window holds.
fn build_entry(path: &Path, clock: SystemTime) {
    let Some(key) = path.to_str() else {
        return;
    };
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let Some(stamp) = cache_stamp(&meta).filter(|_| meta.is_file()) else {
        return;
    };
    if SPELLED.get(key, ANY_POLICY, |kept| kept.stamp == stamp) == Some(true)
        || !usize::try_from(meta.len()).is_ok_and(|len| len < MENTION_CHUNK_BYTES)
    {
        return;
    }
    if let Some((bytes, true)) = read_window(path, &meta, stamp, clock) {
        keep(key, stamp, token_hashes(&searched_text(&bytes)));
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
        let bytes = valid.as_bytes();
        let undecodable_after = !chunk.invalid().is_empty();
        let mut run = None;
        let mut close = |start: usize, end: usize| {
            if start > 0 || !undecodable_before {
                hashes.push(word_hash(&bytes[start..end]));
            }
        };
        // Source text is mostly ASCII: an ASCII byte is judged as it is, and
        // only a multi-byte character is decoded.
        let mut at = 0;
        while at < bytes.len() {
            let byte = bytes[at];
            let (ident, width) = if byte.is_ascii() {
                (
                    byte == b'_' || byte == b'$' || byte.is_ascii_alphanumeric(),
                    1,
                )
            } else {
                let Some(ch) = valid.get(at..).and_then(|rest| rest.chars().next()) else {
                    break;
                };
                (is_ident_char(ch), ch.len_utf8())
            };
            match (ident, run) {
                (true, None) => run = Some(at),
                (false, Some(start)) => {
                    close(start, at);
                    run = None;
                }
                _ => {}
            }
            at += width;
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
pub(super) static READS: super::path_counter::PathCounter = super::path_counter::PathCounter::new();

/// Test-only count of the times a search stat'ed each file.
#[cfg(test)]
pub(super) static STATS: super::path_counter::PathCounter = super::path_counter::PathCounter::new();

#[cfg(test)]
mod tests;
