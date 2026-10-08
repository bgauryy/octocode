//! Preview pages of one unchanged scope share one prepare (scan, rewrite and
//! syntax check). Apply never reads the memo: it always prepares under its
//! lock with the caller's expected hashes, so a stale page fails closed there.

use super::{PreparedFile, RewriteCoverage};
use crate::tools::page_memo::PageMemo;
use std::sync::Arc;
use std::time::Duration;

/// Prepared previews kept, newest last.
const MAX_ENTRIES: usize = 4;
/// Source plus rewritten bytes the memo holds at most.
const MAX_BYTES: usize = 32 * 1024 * 1024;
/// How long a prepared preview serves later pages.
const TTL: Duration = Duration::from_secs(120);

pub(super) struct Prepared {
    pub snapshot: String,
    pub files: Arc<Vec<PreparedFile>>,
    pub coverage: RewriteCoverage,
}

/// Prepared previews by scope key, pinned by their preview snapshot and
/// weighed by the bytes they hold.
static MEMO: PageMemo<Arc<Prepared>> = PageMemo::new(TTL, MAX_ENTRIES, MAX_BYTES);

/// Remember `files` prepared for `key` under `snapshot`.
pub(super) fn store(
    key: String,
    snapshot: &str,
    files: Arc<Vec<PreparedFile>>,
    coverage: RewriteCoverage,
) {
    let bytes = files
        .iter()
        .map(|file| file.before.len() + file.after.len() + file.patch.len())
        .sum::<usize>();
    if bytes > MAX_BYTES {
        return;
    }
    let entry = Arc::new(Prepared {
        snapshot: snapshot.to_owned(),
        files,
        coverage,
    });
    MEMO.put(key, snapshot.to_owned(), bytes, entry);
}

/// The prepared preview for `key` when it was pinned by `snapshot`, is
/// fresh, and every file it rewrites is still `permitted` and still holds the
/// exact bytes it was prepared from (its pre-image hash).
pub(super) fn reuse(
    key: &str,
    snapshot: &str,
    permitted: impl Fn(&std::path::Path) -> bool,
) -> Option<Arc<Prepared>> {
    let entry = MEMO.get(key, snapshot, Arc::clone)?;
    let unchanged = entry.files.iter().all(|file| {
        permitted(&file.absolute)
            && std::fs::read(&file.absolute)
                .is_ok_and(|bytes| crate::digest::sha256(&bytes) == file.before_hash)
    });
    unchanged.then_some(entry)
}
