//! A streamed source's latest proof, kept by path and scanner policy. Later
//! reads use its windows while the handle shows the same settled file. A
//! continuation also requires its snapshot's digest; a changed file passes
//! again, and its new digest fails the snapshot, so a stale page is never served.
//!
//! A match scan's hit lines are kept beside its pass. They are a pure function
//! of the bytes the digest names and of the whole read, so their key holds all
//! of it, with the scanner; they are served only while [`current`] holds.
use super::hits::KeptHits;
use super::proven::Proven;
use super::window::Opened;
use crate::page_memo::PageMemo;
use crate::source_view::local_fetch_query;
use octocode_contract_types::LocalFetchQuery;
use octocode_engine::stamp::{SourceStamp, cache_stamp};
use octocode_runtime_base::security::scan::ContentScan;
use std::path::Path;
use std::sync::Arc;

pub(super) struct Kept {
    pub(super) proven: Proven,
    pub(super) total_lines: usize,
    stamp: SourceStamp,
}

static STORE: PageMemo<Arc<Kept>> = PageMemo::new();
static HITS: PageMemo<Arc<KeptHits>> = PageMemo::new();

fn policy(scan: &dyn ContentScan) -> Option<String> {
    Some(format!("{:?}", scan.kept()?.limits()))
}

/// Where `path`'s value for a policy or a match scan is kept. Two files with
/// the same bytes keep separate proofs and neither evicts the other's.
fn slot(key: &str, path: &Path) -> String {
    serde_json::json!([key, path.to_string_lossy()]).to_string()
}

/// The current pass, also matching `q`'s snapshot when this is a continuation.
pub(super) fn current(
    q: &LocalFetchQuery,
    path: &Path,
    meta: &std::fs::Metadata,
    scan: &dyn ContentScan,
) -> Option<Arc<Kept>> {
    let kept = load(path, meta, scan)?;
    local_fetch_query::snapshot(q)
        .is_none_or(|digest| digest == kept.proven.digest)
        .then_some(kept)
}

/// The pass over the bytes `digest` names, while the opened file's stamp
/// still stands in for them.
pub(super) fn of_digest(
    digest: &str,
    path: &Path,
    meta: &std::fs::Metadata,
    scan: &dyn ContentScan,
) -> Option<Arc<Kept>> {
    load(path, meta, scan).filter(|kept| digest == kept.proven.digest)
}

fn load(path: &Path, meta: &std::fs::Metadata, scan: &dyn ContentScan) -> Option<Arc<Kept>> {
    let policy = policy(scan)?;
    let slot = slot(&policy, path);
    let kept = STORE.get(&slot, &policy, Arc::clone)?;
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
    let Some(policy) = policy(source.scan) else {
        return false;
    };
    let Some(stamp) = cache_stamp(source.meta)
        .and_then(|_| SourceStamp::settled(source.meta, proven.started))
        .filter(|_| proven.unchanged(source.file, source.meta))
    else {
        return false;
    };
    let bytes = proven.weight() + std::mem::size_of::<Kept>();
    STORE.put(
        slot(&policy, source.path),
        policy,
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
fn scan_key(q: &LocalFetchQuery, digest: &str, policy: &str) -> Option<String> {
    let mut read = q.clone();
    local_fetch_query::set_progress(&mut read, None);
    local_fetch_query::set_snapshot(&mut read, None).ok()?;
    local_fetch_query::set_path(&mut read, ".").ok()?;
    serde_json::to_string(&serde_json::json!({
        "digest": digest,
        "read": read,
        "scan": policy,
    }))
    .ok()
}

/// The hits of `q`'s scan of the bytes its snapshot names.
pub(super) fn hits(
    q: &LocalFetchQuery,
    path: &Path,
    scan: &dyn ContentScan,
) -> Option<Arc<KeptHits>> {
    let policy = policy(scan)?;
    let key = scan_key(q, local_fetch_query::snapshot(q)?, &policy)?;
    HITS.get(&slot(&key, path), &policy, Arc::clone)
}

/// Keep the hits of `q`'s scan of the bytes `digest` names.
pub(super) fn keep_hits(
    q: &LocalFetchQuery,
    path: &Path,
    digest: &str,
    hits: KeptHits,
    scan: &dyn ContentScan,
) {
    let Some(policy) = policy(scan) else {
        return;
    };
    if let Some(key) = scan_key(q, digest, &policy) {
        let bytes = hits.weight();
        HITS.put(slot(&key, path), policy, bytes, Arc::new(hits));
    }
}
