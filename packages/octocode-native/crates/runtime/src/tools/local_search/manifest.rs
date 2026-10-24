//! Collected lexical scans kept for continuation pages, keyed by snapshot and
//! the path policy that produced them. A scan is stored only with the digest
//! of the bytes its values came from: the search hashes each file in the read
//! it searches with, and a file it did not hash is read here and must still
//! hold every value at its line. A scan is also stored only with the proof of
//! what its walk searched ([`Walked`]): every searched file, every walked
//! directory and the ignore rules, none written within the settle window
//! before the walk. A hit is only a candidate: it is served only while that
//! proof holds, so a file created after the scan, or an unmatched file that
//! gains a match, drops it and the page rescans (and its changed snapshot
//! restarts). The executor re-hashes each file a page shows (binding its
//! secret checks to those bytes) and restarts on any difference, and it
//! proves the submitted query still derives the stored snapshot before
//! serving it.
use super::super::page_memo::{PageMemo, Proof};
use super::layout::FilePrice;
use super::packed::Packed;
use super::sources::Digest;
use super::verify::Redaction;
use super::walked::Walked;
use crate::policy::discovery::WalkSkips;
use octocode_engine::types::{TextSearchMatch, TextSearchResult};
use sha2::{Digest as _, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

const MAX_ENTRIES: usize = 64;
/// Bytes one stored scan may keep: its packed matches, its sources' paths and
/// its redaction.
pub(super) const MAX_BYTES: usize = 4 * 1024 * 1024;
/// Bytes one stored scan's matches may take unpacked: what a page reads
/// back from it.
const MAX_RAW_BYTES: usize = 64 * 1024 * 1024;
/// Bytes all stored scans may hold together; the oldest are evicted first.
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
/// Matched-file bytes one stored scan may hash; a larger scan is not stored,
/// so its continuations rescan instead.
pub const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(60);

/// A matched file as it was when its scan was stored.
pub(super) struct Source {
    path: String,
    digest: Digest,
}

/// A stored scan. Its parts are shared, so a page takes them from the store
/// without copying them, and reads them outside the store's lock.
struct Scan {
    sources: Arc<[Source]>,
    value: Arc<Packed>,
    query_key: String,
    redaction: Option<Arc<Redaction>>,
    skipped: Arc<WalkSkips>,
    prices: Arc<NamePrices>,
    /// What the walk searched, unchanged since the walk.
    proof: Proof,
}

/// The enclosing-name prices the page cuts of one scan learned, by scanned
/// file path. Every page cut from the scan shares them, so a continuation
/// outlines only the files its own page adds, and cuts the earlier pages
/// with the prices they were cut with (see [`super::layout::NamePricing`]).
/// A stored scan weighs the prices its first page learned; a later page
/// adds at most one owner per hit line the stored value already holds.
#[derive(Default)]
pub struct NamePrices(std::sync::Mutex<HashMap<String, FilePrice>>);

impl NamePrices {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, FilePrice>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The prices of `files`, by index.
    pub(super) fn by_index(
        &self,
        files: &[octocode_engine::types::TextSearchFile],
    ) -> HashMap<usize, FilePrice> {
        let prices = self.lock();
        if prices.is_empty() {
            return HashMap::new();
        }
        files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| Some((index, prices.get(&file.path)?.clone())))
            .collect()
    }

    /// Keep `priced` (by index into `files`).
    pub(super) fn learn(
        &self,
        files: &[octocode_engine::types::TextSearchFile],
        priced: HashMap<usize, FilePrice>,
    ) {
        let mut prices = self.lock();
        for (index, price) in priced {
            if let Some(file) = files.get(index) {
                prices.entry(file.path.clone()).or_insert(price);
            }
        }
    }

    fn weight(&self) -> usize {
        self.lock()
            .iter()
            .map(|(path, price)| path.len() + price.weight())
            .sum()
    }
}

/// A fresh scan offered for storage, with what its page derived from it.
pub struct Fresh {
    /// The scan as the walk returned it, before any page shaped it.
    pub value: Packed,
    /// The query key its snapshot was derived from (see `cursor::query_key`).
    pub query_key: String,
    /// What redacting `value` changed (see [`Redaction`]).
    pub redaction: Redaction,
    pub skipped: Arc<WalkSkips>,
    /// What the walk searched (see [`Walked`]).
    pub walked: Walked,
    /// The names its page cut priced (see [`NamePrices`]).
    pub prices: Arc<NamePrices>,
}

/// A stored scan whose walk is unchanged since it was stored.
pub struct Stored {
    pub value: TextSearchResult,
    /// Digest of each matched file when stored, keyed by its scanned path.
    /// A page may show a file's values only while it still hashes to this,
    /// and its secret checks must read those bytes.
    pub digests: HashMap<PathBuf, Digest>,
    /// The query key the stored snapshot was derived from: a query with the
    /// same key derives the same snapshot from `value`.
    pub query_key: String,
    /// The fresh page's redaction of `value`, kept only when its key-block
    /// reads hashed to the stored digests.
    pub redaction: Option<Arc<Redaction>>,
    /// What the stored walk left out (policy-withheld, ignored,
    /// default-excluded, hidden).
    pub skipped: Arc<WalkSkips>,
    /// The names its page cuts priced so far.
    pub prices: Arc<NamePrices>,
}

/// Stored scans, weighed by their kept bytes.
static STORE: PageMemo<Scan> = PageMemo::new(TTL, MAX_ENTRIES, MAX_TOTAL_BYTES);

/// `value` packed for storage, or `None` when it is too large to store.
#[cfg(test)]
pub fn pack(value: &TextSearchResult) -> Option<Packed> {
    pack_checked(value, &|| false)
}

#[cfg(test)]
pub fn get(snapshot: &str, policy: &str) -> Option<Stored> {
    get_checked(snapshot, policy, &|| false)
}

pub fn pack_checked(value: &TextSearchResult, cancelled: &dyn Fn() -> bool) -> Option<Packed> {
    Packed::pack_checked(value, MAX_RAW_BYTES, MAX_BYTES, cancelled)
}

pub fn get_checked(snapshot: &str, policy: &str, cancelled: &dyn Fn() -> bool) -> Option<Stored> {
    // The stamps of everything the walk searched reject an edit, a new file
    // or a changed ignore rule without a read; the digest that proves a
    // file's bytes is checked when a page shows it, so a walk hashes each
    // file once rather than every file on every page.
    STORE.get_current(
        snapshot,
        policy,
        |scan| {
            (
                (
                    Arc::clone(&scan.value),
                    Arc::clone(&scan.sources),
                    scan.query_key.clone(),
                    scan.redaction.clone(),
                    Arc::clone(&scan.skipped),
                    Arc::clone(&scan.prices),
                ),
                scan.proof.clone(),
            )
        },
        |(value, sources, query_key, redaction, skipped, prices)| {
            Some(Stored {
                value: value.unpack_checked(cancelled)?,
                digests: sources
                    .iter()
                    .map(|source| (PathBuf::from(&source.path), source.digest))
                    .collect(),
                query_key,
                redaction,
                skipped,
                prices,
            })
        },
    )
}

/// Store `fresh` unless it is too large, a matched file no longer holds the
/// values it was scanned with (it changed between the scan and this call),
/// or what the walk searched cannot be proved unchanged from here on.
#[cfg(test)]
pub fn put(snapshot: String, policy: String, fresh: Fresh) {
    put_checked(snapshot, policy, fresh, &|| false);
}

pub fn put_checked(snapshot: String, policy: String, fresh: Fresh, cancelled: &dyn Fn() -> bool) {
    let Fresh {
        value,
        query_key,
        redaction,
        skipped,
        walked,
        prices,
    } = fresh;
    if walked.untracked || walked.oversize {
        return;
    }
    let Some(scan) = value.unpack_checked(cancelled) else {
        return;
    };
    let mut budget = MAX_SOURCE_BYTES;
    let mut sources = Vec::with_capacity(scan.files.len());
    for file in scan.files {
        if cancelled() {
            return;
        }
        let path = if std::path::Path::new(&file.path).is_absolute() {
            file.path.clone()
        } else {
            let root = if walked.root.is_file() {
                walked.root.parent().unwrap_or(&walked.root)
            } else {
                &walked.root
            };
            root.join(&file.path).to_string_lossy().into_owned()
        };
        // The search hashed the bytes it read the values from; only a file
        // it did not hash is read again here.
        let source = match file.source {
            Some(searched) => Some(Source {
                path,
                digest: searched.digest,
            }),
            None => read_source(path, &file.matches, &mut budget),
        };
        let Some(source) = source else {
            return;
        };
        sources.push(source);
    }
    // The redaction replays on a later page only when every key-block scan
    // it made read the stored bytes; otherwise each page redacts afresh.
    let redaction = redaction
        .key_files
        .iter()
        .all(|(index, read)| {
            read.is_some() && sources.get(*index).map(|source| source.digest) == *read
        })
        .then_some(redaction);
    let bytes = value.weight()
        + sources
            .iter()
            .map(|source| source.path.len())
            .sum::<usize>()
        + redaction.as_ref().map_or(0, Redaction::weight)
        + walked.weight()
        + prices.weight();
    if bytes > MAX_BYTES {
        return;
    }
    let Some(proof) = walked.proof() else {
        return;
    };
    STORE.put(
        snapshot,
        policy,
        bytes,
        Scan {
            sources: sources.into(),
            value: Arc::new(value),
            query_key,
            redaction: redaction.map(Arc::new),
            skipped,
            prices,
            proof,
        },
    );
}

/// Hash `path` and check every match value against the hashed bytes; `None`
/// when the file is unreadable, over `budget`, no longer a regular file (a
/// FIFO or link swapped in since the walk), or no longer holds a value.
pub(super) fn read_source(
    path: String,
    matches: &[TextSearchMatch],
    budget: &mut u64,
) -> Option<Source> {
    // Bounded by the budget left: a file grown past it is refused unread.
    let octocode_engine::text::Source { bytes, metadata } =
        octocode_engine::text::read_regular(std::path::Path::new(&path), *budget).ok()?;
    let size = u64::try_from(bytes.len()).ok()?;
    // A length other than the handle's own means it was written meanwhile.
    if size != metadata.len() {
        return None;
    }
    *budget -= size;
    if !matches.is_empty() {
        let text = String::from_utf8_lossy(&bytes);
        let lines = text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .collect::<Vec<_>>();
        if !matches
            .iter()
            .all(|matched| value_matches_source(&lines, matched.line, &matched.value))
        {
            return None;
        }
    }
    Some(Source {
        path,
        digest: Sha256::digest(&bytes).into(),
    })
}

/// True when each line of `value`, without its clip markers, appears in the
/// source lines around `line` (the window the secret check re-reads).
fn value_matches_source(lines: &[&str], line: u32, value: &str) -> bool {
    if line == 0 {
        return true;
    }
    let line = line as usize;
    if line > lines.len() {
        return false;
    }
    let span = value.lines().count().max(1);
    let lo = line.saturating_sub(span).max(1);
    let hi = line.saturating_add(span).min(lines.len());
    let window = lines[lo - 1..hi].join("\n");
    value.lines().all(|value_line| {
        let core = crate::security::cut::strip_clip_markers(value_line);
        core.is_empty() || window.contains(core)
    })
}

/// Drop one stored scan, as expiry or eviction would.
pub fn evict(snapshot: &str) {
    STORE.evict(snapshot);
}

#[cfg(test)]
mod tests;
