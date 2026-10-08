//! Collected lexical scans kept for continuation pages, keyed by snapshot and
//! the path policy that produced them. A scan is stored only with the digest
//! of the bytes its values came from: the search hashes each file in the read
//! it searches with, and a file it did not hash is read here and must still
//! hold every value at its line. A hit is only a candidate: every matched
//! file must keep its size and modification time, the executor re-hashes
//! each file a page shows (binding its secret checks to those bytes) and
//! restarts on any difference, and it proves the submitted query still
//! derives the stored snapshot before serving it. A file created
//! after the scan is not seen until the entry expires; a rescan after expiry
//! restarts a changed result.
use super::super::page_memo::PageMemo;
pub use super::verify::Redaction;
use octocode_engine::types::{TextSearchMatch, TextSearchResult};
use sha2::{Digest as _, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    path::PathBuf,
    time::{Duration, SystemTime},
};

const MAX_ENTRIES: usize = 64;
/// Bytes (paths plus match values) one stored scan may hold.
const MAX_BYTES: usize = 1024 * 1024;
/// Bytes all stored scans may hold together; the oldest are evicted first.
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
/// Matched-file bytes one stored scan may hash; a larger scan is not stored,
/// so its continuations rescan instead.
pub const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
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

struct Scan {
    sources: Vec<Source>,
    value: TextSearchResult,
    query_key: String,
    redaction: Option<Redaction>,
    skipped: crate::policy::discovery::WalkSkips,
}

/// A fresh scan offered for storage, with what its page derived from it.
pub struct Fresh {
    /// The scan as the walk returned it, before any page shaped it.
    pub value: TextSearchResult,
    /// The query key its snapshot was derived from (see `cursor::query_key`).
    pub query_key: String,
    /// What redacting `value` changed (see [`Redaction`]).
    pub redaction: Redaction,
    pub skipped: crate::policy::discovery::WalkSkips,
}

/// A stored scan whose matched files keep their stored size and time.
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
    pub redaction: Option<Redaction>,
    /// What the stored walk left out (policy-skipped, default-excluded).
    pub skipped: crate::policy::discovery::WalkSkips,
}

/// Stored scans, weighed by [`fits`] bytes.
static STORE: PageMemo<Scan> = PageMemo::new(TTL, MAX_ENTRIES, MAX_TOTAL_BYTES);

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
pub fn fits(value: &TextSearchResult) -> Option<usize> {
    let bytes = value
        .files
        .iter()
        .map(|file| file.path.len() + file.matches.iter().map(|m| m.value.len()).sum::<usize>())
        .sum::<usize>();
    (bytes <= MAX_BYTES).then_some(bytes)
}

pub fn get(snapshot: &str, policy: &str) -> Option<Stored> {
    let (value, sources, query_key, redaction, skipped) = STORE.get(snapshot, policy, |scan| {
        (
            scan.value.clone(),
            scan.sources.clone(),
            scan.query_key.clone(),
            scan.redaction.clone(),
            scan.skipped.clone(),
        )
    })?;
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
            query_key,
            redaction,
            skipped,
        });
    }
    evict(snapshot);
    None
}

/// Store `fresh` unless it is too large or a matched file no longer holds the
/// values it was scanned with (it changed between the scan and this call).
pub fn put(snapshot: String, policy: String, fresh: Fresh) {
    let Fresh {
        value,
        query_key,
        redaction,
        skipped,
    } = fresh;
    let Some(bytes) = fits(&value) else {
        return;
    };
    let mut budget = MAX_SOURCE_BYTES;
    let mut sources = Vec::with_capacity(value.files.len());
    for file in &value.files {
        // The search hashed the bytes it read the values from; only a file
        // it did not hash is read again here.
        let source = match file.source {
            Some(searched) => budget.checked_sub(searched.size).map(|left| {
                budget = left;
                Source {
                    path: file.path.clone(),
                    size: searched.size,
                    modified: searched.modified,
                    digest: searched.digest,
                }
            }),
            None => read_source(&file.path, &file.matches, &mut budget),
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
    let bytes = bytes + redaction.as_ref().map_or(0, Redaction::weight);
    STORE.put(
        snapshot,
        policy,
        bytes,
        Scan {
            sources,
            value,
            query_key,
            redaction,
            skipped,
        },
    );
}

/// Hash `path` and check every match value against the hashed bytes; `None`
/// when the file is unreadable, over `budget`, or no longer holds a value.
fn read_source(path: &str, matches: &[TextSearchMatch], budget: &mut u64) -> Option<Source> {
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
        let core = super::verify::strip_clip_markers(value_line);
        core.is_empty() || window.contains(core)
    })
}

/// Drop one stored scan, as expiry or eviction would.
pub fn evict(snapshot: &str) {
    STORE.evict(snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_scans_stay_within_the_aggregate_budget() {
        let store = PageMemo::new(TTL, MAX_ENTRIES, MAX_TOTAL_BYTES);
        for tag in 0..40 {
            store.put(
                format!("scan-{tag}"),
                "policy".into(),
                MAX_BYTES,
                Scan {
                    sources: Vec::new(),
                    value: TextSearchResult {
                        files: Vec::new(),
                        stats: Default::default(),
                    },
                    query_key: String::new(),
                    redaction: None,
                    skipped: Default::default(),
                },
            );
        }
        let (snapshots, bytes) = store.contents();
        assert!(bytes <= MAX_TOTAL_BYTES);
        assert_eq!(snapshots.last().map(String::as_str), Some("scan-39"));
        assert!(snapshots.iter().all(|snapshot| snapshot != "scan-0"));
    }

    fn fresh(value: TextSearchResult) -> Fresh {
        Fresh {
            value,
            query_key: String::new(),
            redaction: Redaction::default(),
            skipped: Default::default(),
        }
    }

    fn one_match(path: &std::path::Path, line: u32, value: &str) -> TextSearchResult {
        TextSearchResult {
            files: vec![octocode_engine::types::TextSearchFile {
                path: path.to_string_lossy().into_owned(),
                match_count: 1,
                matches: vec![TextSearchMatch {
                    line,
                    column: 0,
                    value: value.into(),
                    count: None,
                    kind: None,
                    score_hint: None,
                    rank: None,
                    original_chars: None,
                }],
                source: None,
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
            fresh(one_match(&path, 2, "QUJDQUJDQUJD")),
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
            fresh(one_match(&path, 2, "…QUJDQUJ…")),
        );
        let stored = get(snapshot, "policy").expect("consistent scan stored");
        let digest: Digest = Sha256::digest(held.as_bytes()).into();
        assert_eq!(stored.digests.get(&path), Some(&digest));
        assert_eq!(digest_file(&path).ok(), Some(digest));
    }

    /// A file the search hashed is stored with the search's digest and is
    /// not read again: a value the file no longer holds, which the read
    /// path would refuse, is still stored under that digest.
    #[test]
    fn a_searched_digest_is_stored_without_reading_the_file() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let path = dir.path().join("searched.ts");
        std::fs::write(&path, "// header\nconst k = 1;\n").expect("fixture");
        let meta = std::fs::metadata(&path).expect("meta");
        let snapshot = "manifest-searched-digest-test";
        let mut value = one_match(&path, 2, "not in the file");
        value.files[0].source = Some(octocode_engine::types::SearchedSource {
            size: meta.len(),
            modified: meta.modified().ok(),
            digest: [7; 32],
        });
        put(snapshot.into(), "policy".into(), fresh(value));
        let stored = get(snapshot, "policy").expect("stored from the search digest");
        assert_eq!(stored.digests.get(&path), Some(&[7; 32]));
        // The same scan without a search digest takes the read path.
        evict(snapshot);
        put(
            snapshot.into(),
            "policy".into(),
            fresh(one_match(&path, 2, "not in the file")),
        );
        assert!(get(snapshot, "policy").is_none());
    }

    #[test]
    fn a_stored_scan_is_only_served_under_its_own_policy() {
        let snapshot = "manifest-policy-binding-test".to_owned();
        let empty = TextSearchResult {
            files: Vec::new(),
            stats: Default::default(),
        };
        put(snapshot.clone(), "policy-a".into(), fresh(empty));
        assert!(get(&snapshot, "policy-a").is_some());
        assert!(get(&snapshot, "policy-b").is_none());
    }
}
