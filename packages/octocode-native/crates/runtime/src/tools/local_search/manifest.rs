//! Collected lexical scans kept for continuation pages, keyed by snapshot and
//! the path policy that produced them. A scan is stored only when every match
//! value is still present at its line in the bytes hashed at store time. A hit
//! is only a candidate: every matched file must keep its size and modification
//! time, the executor re-hashes each file a page shows (binding its secret
//! checks to those bytes) and restarts on any difference, and it recomputes
//! the snapshot from the submitted query before serving it. A file created
//! after the scan is not seen until the entry expires; a rescan after expiry
//! restarts a changed result.
use octocode_engine::types::{RipgrepMatch, RipgrepParseResult};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    io::Read,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant, SystemTime},
};

const MAX_ENTRIES: usize = 64;
/// Bytes (paths plus match values) one stored scan may hold.
const MAX_BYTES: usize = 1024 * 1024;
/// Bytes all stored scans may hold together; the oldest are evicted first.
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
/// Matched-file bytes one stored scan may hash; a larger scan is not stored,
/// so its continuations rescan instead.
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(60);

/// SHA-256 of a matched file's bytes.
pub type Digest = [u8; 32];

/// A matched file as it was when its scan was stored.
#[derive(Clone)]
struct Source {
    path: String,
    size: u64,
    modified: Option<SystemTime>,
    digest: Digest,
}

struct Entry {
    snapshot: String,
    policy: String,
    stored: Instant,
    bytes: usize,
    sources: Vec<Source>,
    value: RipgrepParseResult,
}

/// A stored scan whose matched files keep their stored size and time.
pub struct Stored {
    pub value: RipgrepParseResult,
    /// Digest of each matched file when stored, keyed by its scanned path.
    /// A page may show a file's values only while it still hashes to this,
    /// and its secret checks must read those bytes.
    pub digests: HashMap<PathBuf, Digest>,
}

static STORE: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());

fn stamp(path: &str) -> Option<(u64, Option<SystemTime>)> {
    std::fs::metadata(path)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()))
}

/// Streamed SHA-256 of `path`.
pub fn digest_file(path: &std::path::Path) -> std::io::Result<Digest> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            return Ok(hasher.finalize().into());
        }
        hasher.update(&buf[..read]);
    }
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

pub fn get(snapshot: &str, policy: &str) -> Option<Stored> {
    let (value, sources) = {
        let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
        store.retain(|entry| entry.stored.elapsed() < TTL);
        let entry = store
            .iter()
            .find(|entry| entry.snapshot == snapshot && entry.policy == policy)?;
        (entry.value.clone(), entry.sources.clone())
    };
    // Size and time reject most edits without a read; the digest that proves
    // a file's bytes is checked when a page shows it, so a walk hashes each
    // file once rather than every file on every page.
    let unchanged = sources
        .iter()
        .all(|source| stamp(&source.path) == Some((source.size, source.modified)));
    if unchanged {
        return Some(Stored {
            value,
            digests: sources
                .into_iter()
                .map(|source| (PathBuf::from(source.path), source.digest))
                .collect(),
        });
    }
    evict(snapshot);
    None
}

/// Store `value` unless it is too large or a matched file no longer holds the
/// values it was scanned with (it changed between the scan and this call).
pub fn put(snapshot: String, policy: String, value: RipgrepParseResult) {
    let Some(bytes) = fits(&value) else {
        return;
    };
    let mut budget = MAX_SOURCE_BYTES;
    let mut sources = Vec::with_capacity(value.files.len());
    for file in &value.files {
        let Some(source) = read_source(&file.path, &file.matches, &mut budget) else {
            return;
        };
        sources.push(source);
    }
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

/// Hash `path` and check every match value against the hashed bytes; `None`
/// when the file is unreadable, over `budget`, or no longer holds a value.
fn read_source(path: &str, matches: &[RipgrepMatch], budget: &mut u64) -> Option<Source> {
    let meta = std::fs::metadata(path).ok()?;
    *budget = budget.checked_sub(meta.len())?;
    let bytes = std::fs::read(path).ok()?;
    if u64::try_from(bytes.len()).ok()? != meta.len() {
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
        path: path.to_owned(),
        size: meta.len(),
        modified: meta.modified().ok(),
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
        let core = super::executor::strip_clip_markers(value_line);
        core.is_empty() || window.contains(core)
    })
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

    fn one_match(path: &std::path::Path, line: u32, value: &str) -> RipgrepParseResult {
        RipgrepParseResult {
            files: vec![octocode_engine::types::RipgrepFile {
                path: path.to_string_lossy().into_owned(),
                match_count: 1,
                matches: vec![RipgrepMatch {
                    line,
                    column: 0,
                    value: value.into(),
                    count: None,
                    kind: None,
                    score_hint: None,
                    rank: None,
                    original_chars: None,
                }],
            }],
            stats: Default::default(),
        }
    }

    /// A file rewritten between the scan and the store no longer holds the
    /// scanned value, so the scan is not stored; a consistent scan is stored
    /// with the digest of the bytes it was checked against.
    #[test]
    fn a_scan_is_stored_only_with_the_bytes_that_hold_its_values() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let path = dir.path().join("b.ts");
        let snapshot = "manifest-content-binding-test";
        std::fs::write(&path, "// header\nZZZZZZZZZZZZ\n").expect("fixture");
        put(
            snapshot.into(),
            "policy".into(),
            one_match(&path, 2, "QUJDQUJDQUJD"),
        );
        assert!(
            get(snapshot, "policy").is_none(),
            "inconsistent scan stored"
        );
        let held = "// header\nconst k = QUJDQUJDQUJD;\n";
        std::fs::write(&path, held).expect("fixture");
        put(
            snapshot.into(),
            "policy".into(),
            one_match(&path, 2, "…QUJDQUJ…"),
        );
        let stored = get(snapshot, "policy").expect("consistent scan stored");
        let digest: Digest = Sha256::digest(held.as_bytes()).into();
        assert_eq!(stored.digests.get(&path), Some(&digest));
        assert_eq!(digest_file(&path).ok(), Some(digest));
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
