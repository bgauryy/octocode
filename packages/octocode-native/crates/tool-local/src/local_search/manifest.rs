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
use super::layout::{FilePrice, StreamCuts};
use super::packed::{Encoded, Packed};
use super::sources::Digest;
use super::verify::{JudgedWindows, Redaction};
use super::walked::Walked;
use octocode_engine::stamp::Proof;
use octocode_engine::types::{TextSearchMatch, TextSearchResult};
use octocode_runtime_base::policy::discovery::WalkSkips;
use octocode_tools_shared::page_memo::PageMemo;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

/// A matched file as it was when its scan was stored.
pub(super) struct Source {
    path: String,
    digest: Digest,
}

/// A stored scan. Its parts are shared, so a page takes them from the store
/// without copying them, and reads them outside the store's lock.
struct Scan {
    /// Digest of each matched file when stored, keyed by its scanned path;
    /// shared whole with each page, never rebuilt per page.
    digests: Arc<HashMap<PathBuf, Digest>>,
    value: Arc<Packed>,
    query_key: String,
    redaction: Option<Arc<Redaction>>,
    skipped: Arc<WalkSkips>,
    learned: Arc<Learned>,
    /// What the walk searched, unchanged since the walk.
    proof: Proof,
}

/// What the pages cut from one scan learned of it. Every page of the scan
/// shares it, so a continuation reuses what an earlier page derived from
/// the same stored value. A stored scan weighs what its first page learned;
/// a later page adds at most an entry per row, window or page the stored
/// value already holds.
#[derive(Default)]
pub(crate) struct Learned {
    /// The names its cuts priced.
    pub(super) prices: NamePrices,
    /// The row windows its pages judged (see [`JudgedWindows`]).
    pub(super) judged: JudgedWindows,
    /// Its stream cut (see [`StreamCuts`]).
    pub(super) cuts: StreamCuts,
}

impl Learned {
    fn weight(&self) -> usize {
        self.prices.weight() + self.judged.weight() + self.cuts.weight()
    }

    /// Drop the prices and the stream cut its first page made: a seekable
    /// scan's index holds the cut, so no later page reads them.
    fn forget_cut(&self) {
        self.prices.clear();
        self.cuts.clear();
    }

    /// Charge what its pages learn from now on to `bill`: the scan was stored
    /// and weighed, and a later entry must not grow it uncharged.
    fn bill_to(&self, bill: impl Fn(usize) -> bool + Send + Sync + 'static) {
        let bill: Arc<dyn Fn(usize) -> bool + Send + Sync> = Arc::new(bill);
        self.prices.1.set(Arc::clone(&bill));
        self.cuts.billing().set(Arc::clone(&bill));
        self.judged.billing().set(bill);
    }
}

/// Where entries a stored scan learns are charged; none before it is stored
/// (it is weighed whole then). Set once.
#[derive(Default)]
pub(super) struct Billing(std::sync::OnceLock<Arc<dyn Fn(usize) -> bool + Send + Sync>>);

impl Billing {
    pub(super) fn set(&self, bill: Arc<dyn Fn(usize) -> bool + Send + Sync>) {
        let _ = self.0.set(bill);
    }

    /// Whether `bytes` more may be kept: charged, or nothing bills them yet.
    pub(super) fn charge(&self, bytes: usize) -> bool {
        self.0.get().is_none_or(|bill| bill(bytes))
    }
}

/// The enclosing-name prices the page cuts of one scan learned, by scanned
/// file path. Every page cut from the scan shares them, so a continuation
/// outlines only the files its own page adds, and cuts the earlier pages
/// with the prices they were cut with (see [`super::layout::NamePricing`]).
/// A stored scan weighs the prices its first page learned; a later page
/// adds at most one owner per hit line the stored value already holds.
#[derive(Default)]
pub(super) struct NamePrices(std::sync::Mutex<HashMap<String, FilePrice>>, Billing);

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

    /// Keep `priced` (by index into `files`): each new price once charged, or
    /// priced again by a later page when the store refuses its bytes.
    pub(super) fn learn(
        &self,
        files: &[octocode_engine::types::TextSearchFile],
        priced: HashMap<usize, FilePrice>,
    ) {
        let mut prices = self.lock();
        for (index, price) in priced {
            if let Some(file) = files.get(index)
                && !prices.contains_key(&file.path)
                && self.1.charge(file.path.len() + price.weight())
            {
                prices.insert(file.path.clone(), price);
            }
        }
    }

    fn clear(&self) {
        self.lock().clear();
    }

    fn weight(&self) -> usize {
        self.lock()
            .iter()
            .map(|(path, price)| path.len() + price.weight())
            .sum()
    }
}

/// A fresh scan offered for storage, with what its page derived from it.
pub(super) struct Fresh {
    /// The scan as its page encoded it (see [`Encoded`]); it is deflated
    /// only when stored.
    pub value: Encoded,
    /// The query key its snapshot was derived from (see `cursor::query_key`).
    pub query_key: String,
    /// What redacting `value` changed (see [`Redaction`]).
    pub redaction: Redaction,
    pub skipped: Arc<WalkSkips>,
    /// What the walk searched (see [`Walked`]).
    pub walked: Walked,
    /// What its page learned (see [`Learned`]).
    pub(super) learned: Arc<Learned>,
}

/// A stored scan as a continuation reads it.
pub(super) enum StoredValue {
    /// Decoded whole: the page is cut from it again.
    Whole(Box<TextSearchResult>),
    /// Read by page: only the page's frames are decoded (see
    /// [`super::page_view::PageView::stored`]).
    Seek(Arc<Packed>),
}

/// A stored scan whose walk is unchanged since it was stored.
pub(super) struct Stored {
    pub value: StoredValue,
    /// Digest of each matched file when stored, keyed by its scanned path.
    /// A page may show a file's values only while it still hashes to this,
    /// and its secret checks must read those bytes.
    pub digests: Arc<HashMap<PathBuf, Digest>>,
    /// The query key the stored snapshot was derived from: a query with the
    /// same key derives the same snapshot from `value`.
    pub query_key: String,
    /// The fresh page's redaction of `value`, kept only when its key-block
    /// reads hashed to the stored digests.
    pub redaction: Option<Arc<Redaction>>,
    /// What the stored walk left out (policy-withheld, ignored,
    /// default-excluded, hidden).
    pub skipped: Arc<WalkSkips>,
    /// What its pages learned so far.
    pub(super) learned: Arc<Learned>,
}

/// Stored scans, weighed by their kept bytes against the one process budget
/// ([`octocode_engine::memory::MEMORY`]): their packed matches, their sources' paths,
/// their redactions and what their walks recorded. One scan may use all of
/// it, since a scan that is not stored costs a whole walk per page, which
/// outweighs the entries it would displace; unpacked, a page reads back at
/// most as much.
static STORE: PageMemo<Scan> = PageMemo::new();

/// `value` encoded for storage, or `None` when it is too large to store.
#[cfg(test)]
pub(super) fn encode(value: &TextSearchResult) -> Option<Encoded> {
    encode_checked(value, &|| false)
}

#[cfg(test)]
pub(super) fn get(snapshot: &str, policy: &str) -> Option<Stored> {
    get_checked(snapshot, policy, &|| false)
}

/// `value` encoded as a page found it, for a cursor that may keep it; it is
/// deflated only by [`put_checked`].
pub(super) fn encode_checked(
    value: &TextSearchResult,
    cancelled: &dyn Fn() -> bool,
) -> Option<Encoded> {
    Encoded::encode_checked(value, octocode_engine::memory::MEMORY.limit(), cancelled)
}

pub(super) fn get_checked(
    snapshot: &str,
    policy: &str,
    cancelled: &dyn Fn() -> bool,
) -> Option<Stored> {
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
                    Arc::clone(&scan.digests),
                    scan.query_key.clone(),
                    scan.redaction.clone(),
                    Arc::clone(&scan.skipped),
                    Arc::clone(&scan.learned),
                ),
                scan.proof.clone(),
            )
        },
        |(value, digests, query_key, redaction, skipped, learned)| {
            Some(Stored {
                value: if value.index().is_some() {
                    StoredValue::Seek(value)
                } else {
                    StoredValue::Whole(Box::new(value.unpack_checked(cancelled)?))
                },
                digests,
                query_key,
                redaction,
                skipped,
                learned,
            })
        },
    )
}

/// Store `fresh` unless it is too large, a matched file no longer holds the
/// values it was scanned with (it changed between the scan and this call),
/// or what the walk searched cannot be proved unchanged from here on.
#[cfg(test)]
pub(super) fn put(snapshot: String, policy: String, fresh: Fresh) {
    put_checked(snapshot, policy, fresh, &|| false);
}

pub(super) fn put_checked(
    snapshot: String,
    policy: String,
    fresh: Fresh,
    cancelled: &dyn Fn() -> bool,
) {
    let Fresh {
        value,
        query_key,
        redaction,
        skipped,
        walked,
        learned,
    } = fresh;
    if walked.untracked || walked.oversize {
        return;
    }
    // Each file's path and source only; its hits stay in the encoding.
    let Some(files) = value.sourced_files(cancelled) else {
        return;
    };
    // One file at a time, each at most what the budget could keep; the
    // request's cancellation bounds the whole pass.
    let max_file = u64::try_from(octocode_engine::memory::MEMORY.limit()).unwrap_or(u64::MAX);
    let root = if walked.root.is_file() {
        walked.root.parent().unwrap_or(&walked.root)
    } else {
        &walked.root
    };
    // A row path as the store keys it: under the walk's root.
    let stored_path = |path: &str| {
        if std::path::Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            root.join(path)
        }
    };
    let mut digests = HashMap::with_capacity(files.len());
    for file in files {
        if cancelled() {
            return;
        }
        let path = stored_path(&file.path).to_string_lossy().into_owned();
        // The search hashed the bytes it read the values from; only a file
        // it did not hash is read again here.
        let source = match file.source {
            Some(searched) => Some(Source {
                path,
                digest: searched.digest,
            }),
            None => read_source(path, &file.matches, max_file),
        };
        let Some(source) = source else {
            return;
        };
        digests.insert(PathBuf::from(source.path), source.digest);
    }
    // Whether every key-block scan the page made read the stored bytes.
    let holds = redaction
        .key_files
        .iter()
        .all(|key| key.read.is_some() && digests.get(&stored_path(&key.path)) == key.read.as_ref());
    // A seekable scan's frames hold what its page redacted: kept only when
    // that redaction read the stored bytes. A whole scan's redaction
    // replays on a later page only then; otherwise each page redacts afresh.
    if value.seekable() && !holds {
        return;
    }
    let redaction = (!value.seekable() && holds).then_some(redaction);
    // A seekable scan's pages are its index: what its first page learned to
    // cut them is not read again.
    if value.seekable() {
        learned.forget_cut();
    }
    // Verdicts a later page learns grow the stored scan: each is charged to
    // it when learned, as what is learned up to here is weighed below.
    {
        let snapshot = snapshot.clone();
        learned.bill_to(move |bytes| STORE.charge(&snapshot, bytes));
    }
    let derived = digests
        .keys()
        .map(|path| path.as_os_str().len() + std::mem::size_of::<Digest>())
        .sum::<usize>()
        + redaction.as_ref().map_or(0, Redaction::weight)
        + walked.weight()
        + learned.weight();
    let Some(proof) = walked.proof() else {
        return;
    };
    // Deflated last: the only step whose cost grows with the scan's rows.
    let budget = octocode_engine::memory::MEMORY.limit();
    let Some(value) = value.deflate_checked(budget.saturating_sub(derived), cancelled) else {
        return;
    };
    let bytes = value.weight() + derived;
    STORE.put(
        snapshot,
        policy,
        bytes,
        Scan {
            digests: Arc::new(digests),
            value: Arc::new(value),
            query_key,
            redaction: redaction.map(Arc::new),
            skipped,
            learned,
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
    max_file: u64,
) -> Option<Source> {
    // A file grown past `max_file` is refused unread.
    let octocode_engine::text::Source { bytes, metadata } =
        octocode_engine::text::read_regular(std::path::Path::new(&path), max_file).ok()?;
    let size = u64::try_from(bytes.len()).ok()?;
    // A length other than the handle's own means it was written meanwhile.
    if size != metadata.len() {
        return None;
    }
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
        digest: octocode_engine::digest::sha256_digest(&bytes),
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
        // A long line's row shows a secret's place by its placeholder.
        octocode_engine::security::placeholder_regex()
            .split(octocode_runtime_base::security::cut::strip_clip_markers(
                value_line,
            ))
            .all(|segment| segment.is_empty() || window.contains(segment))
    })
}

/// Drop one stored scan, as expiry or eviction would.
pub(super) fn evict(snapshot: &str) {
    STORE.evict(snapshot);
}

#[cfg(test)]
mod tests;
