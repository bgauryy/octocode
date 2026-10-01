//! Collected lexical scans kept for continuation pages, keyed by snapshot and
//! the path policy that produced them. A hit is only a candidate: every
//! matched file must still have the size and modification time it had when
//! scanned, and the executor recomputes the snapshot from the submitted query
//! before serving it. A file created after the scan is not seen until the
//! entry expires; a rescan after expiry restarts a changed result.
use octocode_engine::types::RipgrepParseResult;
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant, SystemTime},
};

const MAX_ENTRIES: usize = 64;
/// Bytes (paths plus match values) one stored scan may hold.
const MAX_BYTES: usize = 1024 * 1024;
/// Bytes all stored scans may hold together; the oldest are evicted first.
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(60);

/// Size and modification time of a matched file when it was scanned.
type Stamp = Option<(u64, Option<SystemTime>)>;

struct Entry {
    snapshot: String,
    policy: String,
    stored: Instant,
    bytes: usize,
    sources: Vec<(String, Stamp)>,
    value: RipgrepParseResult,
}

static STORE: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());

fn stamp(path: &str) -> Stamp {
    std::fs::metadata(path)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()))
}

/// Bytes a scan would occupy, or `None` when it exceeds the per-scan budget.
pub fn fits(value: &RipgrepParseResult) -> Option<usize> {
    let bytes = value
        .files
        .iter()
        .map(|file| file.path.len() + file.matches.iter().map(|m| m.value.len()).sum::<usize>())
        .sum::<usize>();
    (bytes <= MAX_BYTES).then_some(bytes)
}

pub fn get(snapshot: &str, policy: &str) -> Option<RipgrepParseResult> {
    let (value, sources) = {
        let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
        store.retain(|entry| entry.stored.elapsed() < TTL);
        let entry = store
            .iter()
            .find(|entry| entry.snapshot == snapshot && entry.policy == policy)?;
        (entry.value.clone(), entry.sources.clone())
    };
    if sources
        .iter()
        .all(|(path, scanned)| stamp(path) == *scanned)
    {
        return Some(value);
    }
    STORE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|entry| entry.snapshot != snapshot);
    None
}

pub fn put(snapshot: String, policy: String, value: RipgrepParseResult) {
    let Some(bytes) = fits(&value) else {
        return;
    };
    let sources = value
        .files
        .iter()
        .map(|file| (file.path.clone(), stamp(&file.path)))
        .collect();
    let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    insert(
        &mut store,
        Entry {
            snapshot,
            policy,
            stored: Instant::now(),
            bytes,
            sources,
            value,
        },
    );
}

/// Add `entry`, first dropping expired scans, an older scan with the same
/// snapshot, and the oldest scans beyond the entry and byte budgets.
fn insert(store: &mut VecDeque<Entry>, entry: Entry) {
    store.retain(|kept| kept.stored.elapsed() < TTL && kept.snapshot != entry.snapshot);
    let mut total = store.iter().map(|kept| kept.bytes).sum::<usize>();
    while store.len() >= MAX_ENTRIES || total + entry.bytes > MAX_TOTAL_BYTES {
        let Some(oldest) = store.pop_front() else {
            break;
        };
        total -= oldest.bytes;
    }
    store.push_back(entry);
}

/// Drop one stored scan, as expiry or eviction would.
#[cfg(test)]
pub fn evict(snapshot: &str) {
    STORE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|entry| entry.snapshot != snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_scans_stay_within_the_aggregate_budget() {
        let mut store = VecDeque::new();
        for tag in 0..40 {
            insert(
                &mut store,
                Entry {
                    snapshot: format!("scan-{tag}"),
                    policy: "policy".into(),
                    stored: Instant::now(),
                    bytes: MAX_BYTES,
                    sources: Vec::new(),
                    value: RipgrepParseResult {
                        files: Vec::new(),
                        stats: Default::default(),
                    },
                },
            );
        }
        assert!(store.iter().map(|entry| entry.bytes).sum::<usize>() <= MAX_TOTAL_BYTES);
        assert_eq!(
            store.back().map(|entry| entry.snapshot.as_str()),
            Some("scan-39")
        );
        assert!(store.iter().all(|entry| entry.snapshot != "scan-0"));
    }

    #[test]
    fn a_stored_scan_is_only_served_under_its_own_policy() {
        let snapshot = "manifest-policy-binding-test".to_owned();
        let empty = RipgrepParseResult {
            files: Vec::new(),
            stats: Default::default(),
        };
        put(snapshot.clone(), "policy-a".into(), empty);
        assert!(get(&snapshot, "policy-a").is_some());
        assert!(get(&snapshot, "policy-b").is_none());
    }
}
