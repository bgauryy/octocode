//! A streamed source's proof, kept between the pages of one read. The first
//! page passes over every byte; the pages after it name that pass's digest in
//! their snapshot and read only their windows while the handle shows the same
//! file. A changed file misses here, passes again, and its new digest fails
//! the snapshot, so a stale page is never served.
use super::proven::Proven;
use super::window::Opened;
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

fn policy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The pass `q`'s snapshot names, while the opened file's stamp still stands
/// in for its bytes.
pub(super) fn current(
    q: &LocalFetchQuery,
    path: &Path,
    meta: &std::fs::Metadata,
) -> Option<Arc<Kept>> {
    let snapshot = local_fetch_query::snapshot(q)?;
    let kept = STORE.get(snapshot, &policy(path), Arc::clone)?;
    if cache_stamp(meta) == Some(kept.stamp) {
        return Some(kept);
    }
    STORE.evict(snapshot);
    None
}

/// Keep a finished pass over `source`, unless the file was written too
/// recently for its stamp to stand in for its bytes, or changed during the pass.
pub(super) fn keep(source: &Opened<'_>, proven: Proven, total_lines: usize) {
    let Some(stamp) = cache_stamp(source.meta)
        .and_then(|_| SourceStamp::settled(source.meta, proven.started))
        .filter(|_| proven.unchanged(source.file, source.meta))
    else {
        return;
    };
    let bytes = proven.weight() + std::mem::size_of::<Kept>();
    let snapshot = proven.digest.clone();
    STORE.put(
        snapshot,
        policy(source.path),
        bytes,
        Arc::new(Kept {
            proven,
            total_lines,
            stamp,
        }),
    );
}
