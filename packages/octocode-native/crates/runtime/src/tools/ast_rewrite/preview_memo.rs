//! Preview pages of one unchanged scope share one prepare (scan, rewrite and
//! syntax check). Apply never reads the memo: it always prepares under its
//! lock with the caller's expected hashes, so a stale page fails closed there.

use super::{PreparedFile, RewriteCoverage};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    bytes: usize,
    stored_at: Instant,
}

static MEMO: Mutex<VecDeque<(String, Arc<Prepared>)>> = Mutex::new(VecDeque::new());

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
        bytes,
        stored_at: Instant::now(),
    });
    let mut memo = MEMO.lock().unwrap_or_else(|error| error.into_inner());
    memo.retain(|(held, _)| *held != key);
    memo.push_back((key, entry));
    while memo.len() > MAX_ENTRIES
        || memo.iter().map(|(_, entry)| entry.bytes).sum::<usize>() > MAX_BYTES
    {
        memo.pop_front();
    }
}

/// The prepared preview for `key` when it was pinned by `snapshot`, is
/// fresh, and every file it rewrites is still `permitted` and still holds the
/// exact bytes it was prepared from (its pre-image hash).
pub(super) fn reuse(
    key: &str,
    snapshot: &str,
    permitted: impl Fn(&std::path::Path) -> bool,
) -> Option<Arc<Prepared>> {
    let entry = {
        let mut memo = MEMO.lock().unwrap_or_else(|error| error.into_inner());
        memo.retain(|(_, entry)| entry.stored_at.elapsed() < TTL);
        memo.iter()
            .find(|(held, _)| held == key)
            .map(|(_, entry)| Arc::clone(entry))?
    };
    let unchanged = entry.snapshot == snapshot
        && entry.files.iter().all(|file| {
            permitted(&file.absolute)
                && std::fs::read(&file.absolute)
                    .is_ok_and(|bytes| crate::digest::sha256(&bytes) == file.before_hash)
        });
    unchanged.then_some(entry)
}
