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
                sources: Vec::new().into(),
                value: Arc::new(
                    pack(&TextSearchResult {
                        files: Vec::new(),
                        stats: Default::default(),
                    })
                    .expect("packed"),
                ),
                query_key: String::new(),
                redaction: None,
                skipped: Default::default(),
                prices: Default::default(),
                proof: Walked::settled(std::env::temp_dir(), Vec::new())
                    .proof()
                    .expect("proof"),
            },
        );
    }
    let (snapshots, bytes) = store.contents();
    assert!(bytes <= MAX_TOTAL_BYTES);
    assert_eq!(snapshots.last().map(String::as_str), Some("scan-39"));
    assert!(snapshots.iter().all(|snapshot| snapshot != "scan-0"));
}

/// `value` offered for storage, its walk the files it matched.
fn fresh(value: TextSearchResult) -> Fresh {
    let entries = value
        .files
        .iter()
        .map(|file| PathBuf::from(&file.path))
        .collect::<Vec<_>>();
    let root = entries
        .first()
        .and_then(|path| path.parent())
        .map_or_else(std::env::temp_dir, std::path::Path::to_path_buf);
    Fresh {
        value: pack(&value).expect("packed"),
        query_key: String::new(),
        redaction: Redaction::default(),
        skipped: Default::default(),
        walked: Walked::settled(root, entries),
        prices: Default::default(),
    }
}

fn one_match(path: &std::path::Path, line: u32, value: &str) -> TextSearchResult {
    TextSearchResult {
        files: vec![octocode_engine::types::TextSearchFile {
            path: path.to_string_lossy().into_owned(),
            match_count: 1,
            matches: vec![TextSearchMatch {
                deferred: None,
                line,
                last_line: None,
                column: 0,
                byte_offset: 0,
                value: value.into(),
                value_digest: TextSearchMatch::digest_of(value),
                count: None,
                rank: None,
                original_chars: None,
                original_bytes: None,
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
    assert_eq!(super::super::sources::digest_file(&path).ok(), Some(digest));
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

/// The re-read is bounded by the hash budget left: a file past it is
/// refused unread and spends nothing; one within it spends its size.
#[test]
fn a_reread_source_is_bounded_by_the_budget_left() {
    let dir = tempfile::tempdir().expect("fixture directory");
    let path = dir.path().join("ten.txt");
    std::fs::write(&path, "0123456789").expect("fixture");
    let path = path.to_string_lossy().into_owned();
    let mut budget = 9;
    assert!(read_source(path.clone(), &[], &mut budget).is_none());
    assert_eq!(budget, 9);
    let mut budget = 25;
    let source = read_source(path, &[], &mut budget).expect("within budget");
    assert_eq!(budget, 15);
    let digest: Digest = Sha256::digest(b"0123456789").into();
    assert_eq!(source.digest, digest);
    // A directory is not a regular file and is never read.
    assert!(read_source(dir.path().to_string_lossy().into_owned(), &[], &mut 100).is_none());
}

#[test]
fn a_stored_scan_is_only_served_under_its_own_policy() {
    let dir = tempfile::tempdir().expect("fixture directory");
    let snapshot = "manifest-policy-binding-test".to_owned();
    let empty = TextSearchResult {
        files: Vec::new(),
        stats: Default::default(),
    };
    let mut offered = fresh(empty);
    offered.walked = Walked::settled(dir.path().to_path_buf(), Vec::new());
    put(snapshot.clone(), "policy-a".into(), offered);
    assert!(get(&snapshot, "policy-a").is_some());
    assert!(get(&snapshot, "policy-b").is_none());
}
/// A stored scan is served only while everything its walk searched is
/// unchanged: an unmatched searched file that gains a match, or a file
/// created in a walked directory, drops it.
#[test]
fn a_stored_scan_is_dropped_when_its_walk_would_find_more() {
    let dir = tempfile::tempdir().expect("fixture directory");
    let matched = dir.path().join("a.txt");
    let unmatched = dir.path().join("c.txt");
    std::fs::write(&matched, "needle\n").expect("fixture");
    std::fs::write(&unmatched, "hay\n").expect("fixture");
    let store = |snapshot: &str| {
        let mut offered = fresh(one_match(&matched, 1, "needle"));
        offered.walked = Walked::settled(
            dir.path().to_path_buf(),
            vec![dir.path().to_path_buf(), matched.clone(), unmatched.clone()],
        );
        put(snapshot.into(), "policy".into(), offered);
        assert!(get(snapshot, "policy").is_some(), "stored");
    };
    store("manifest-walk-edit-test");
    std::fs::write(&unmatched, "needle\n").expect("edit");
    assert!(
        get("manifest-walk-edit-test", "policy").is_none(),
        "an unmatched file that gains a match drops the scan"
    );
    store("manifest-walk-create-test");
    std::fs::write(dir.path().join("0new.txt"), "needle\n").expect("create");
    assert!(
        get("manifest-walk-create-test", "policy").is_none(),
        "a file created in a walked directory drops the scan"
    );
}
