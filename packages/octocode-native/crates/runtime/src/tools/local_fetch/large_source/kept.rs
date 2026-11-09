//! A streamed source's proof, kept between the pages of one read. The first
//! page passes over every byte; the pages after it name that pass's digest in
//! their snapshot and read only their windows while the handle shows the same
//! file. A changed file misses here, passes again, and its new digest fails
//! the snapshot, so a stale page is never served.
//!
//! A match scan's hit lines are kept beside its pass. They are a pure function
//! of the bytes the digest names and of the whole read, so their key holds all
//! of it, with the scanner; they are served only while [`current`] holds.
use super::hits::KeptHits;
use super::proven::Proven;
use super::window::Opened;
use crate::security::scan::ContentScan;
use crate::tools::page_memo::PageMemo;
use crate::tools::source_view::local_fetch_query;
use octocode_contract_types::LocalFetchQuery;
use octocode_engine::stamp::{SourceStamp, cache_stamp};
use std::path::Path;
use std::sync::Arc;

pub(super) struct Kept {
    pub(super) proven: Proven,
    pub(super) total_lines: usize,
    stamp: SourceStamp,
}

static STORE: PageMemo<Arc<Kept>> = PageMemo::new();
static HITS: PageMemo<Arc<KeptHits>> = PageMemo::new();

fn policy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Where `path`'s value for `key` (a digest, or a scan key holding one) is
/// kept. A digest names bytes, not a file: two files with the same bytes
/// are read apart, so each keeps its own value and neither evicts the other.
fn slot(key: &str, path: &Path) -> String {
    serde_json::json!([key, policy(path)]).to_string()
}

/// The pass `q`'s snapshot names, while the opened file's stamp still stands
/// in for its bytes.
pub(super) fn current(
    q: &LocalFetchQuery,
    path: &Path,
    meta: &std::fs::Metadata,
) -> Option<Arc<Kept>> {
    let slot = slot(local_fetch_query::snapshot(q)?, path);
    let kept = STORE.get(&slot, &policy(path), Arc::clone)?;
    if cache_stamp(meta) == Some(kept.stamp) {
        return Some(kept);
    }
    STORE.evict(&slot);
    None
}

/// Keep a finished pass over `source`, unless the file was written too
/// recently for its stamp to stand in for its bytes, or changed during the pass:
/// whether it was kept.
pub(super) fn keep(source: &Opened<'_>, proven: Proven, total_lines: usize) -> bool {
    let Some(stamp) = cache_stamp(source.meta)
        .and_then(|_| SourceStamp::settled(source.meta, proven.started))
        .filter(|_| proven.unchanged(source.file, source.meta))
    else {
        return false;
    };
    let bytes = proven.weight() + std::mem::size_of::<Kept>();
    STORE.put(
        slot(&proven.digest, source.path),
        policy(source.path),
        bytes,
        Arc::new(Kept {
            proven,
            total_lines,
            stamp,
        }),
    );
    true
}

/// What a scan depends on besides the path: the bytes, the read without its
/// position or the spelling of its path, and the scanner.
fn scan_key<S: ContentScan>(q: &LocalFetchQuery, digest: &str) -> Option<String> {
    let mut read = q.clone();
    local_fetch_query::set_progress(&mut read, None);
    local_fetch_query::set_snapshot(&mut read, None).ok()?;
    local_fetch_query::set_path(&mut read, ".").ok()?;
    serde_json::to_string(&serde_json::json!({
        "digest": digest,
        "read": read,
        "scan": std::any::type_name::<S>(),
    }))
    .ok()
}

/// The hits of `q`'s scan of the bytes its snapshot names.
pub(super) fn hits<S: ContentScan>(q: &LocalFetchQuery, path: &Path) -> Option<Arc<KeptHits>> {
    let key = scan_key::<S>(q, local_fetch_query::snapshot(q)?)?;
    HITS.get(&slot(&key, path), &policy(path), Arc::clone)
}

/// Keep the hits of `q`'s scan of the bytes `digest` names.
pub(super) fn keep_hits<S: ContentScan>(
    q: &LocalFetchQuery,
    path: &Path,
    digest: &str,
    hits: KeptHits,
) {
    if let Some(key) = scan_key::<S>(q, digest) {
        let bytes = hits.weight();
        HITS.put(slot(&key, path), policy(path), bytes, Arc::new(hits));
    }
}
