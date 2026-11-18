use super::*;

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
        value: encode(&value).expect("encoded"),
        query_key: String::new(),
        redaction: Redaction::default(),
        skipped: Default::default(),
        walked: Walked::settled(root, entries),
        learned: Default::default(),
    }
}

fn one_match(path: &std::path::Path, line: u32, value: &str) -> TextSearchResult {
    TextSearchResult {
        files: vec![octocode_engine::types::TextSearchFile {
            term_counts: Vec::new(),
            path: path.to_string_lossy().into_owned(),
            match_count: 1,
            counted: Default::default(),
            matches: vec![TextSearchMatch {
                deferred: None,
                line,
                first_line: line,
                last_line: None,
                column: 0,
                byte_offset: 0,
                value: value.into(),
                value_digest: TextSearchMatch::digest_of(value),
                count: None,
                rank: None,
                window_start: None,
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
    let digest: Digest = octocode_engine::digest::sha256_digest(held.as_bytes());
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
        lines: None,
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

/// Storing a scan reads each file's path and search digest, not its hits:
/// a file the search hashed decodes none, so a scan of any size is stored
/// without a second copy of its rows beside the one it was encoded from.
#[test]
fn storing_a_scan_decodes_no_hit_of_a_file_the_search_hashed() {
    let dir = tempfile::tempdir().expect("fixture directory");
    let path = dir.path().join("hashed.ts");
    std::fs::write(&path, "// header\nconst k = 1;\n").expect("fixture");
    let mut value = one_match(&path, 2, "const k = 1;");
    let hit = value.files[0].matches[0].clone();
    value.files[0].matches = vec![hit; 1000];
    value.files[0].match_count = 1000;
    value.files[0].source = Some(octocode_engine::types::SearchedSource {
        size: 1,
        modified: None,
        digest: [9; 32],
        lines: None,
    });
    let fresh = fresh(value);
    let before = super::super::packed::decoded_hits_on_this_thread();
    put("manifest-no-decode-test".into(), "policy".into(), fresh);
    assert_eq!(
        super::super::packed::decoded_hits_on_this_thread(),
        before,
        "storing decoded rows of a file whose digest the search made"
    );
    let stored = get("manifest-no-decode-test", "policy").expect("stored");
    let StoredValue::Whole(value) = stored.value else {
        panic!("a whole encoding is stored whole");
    };
    assert_eq!(value.files[0].matches.len(), 1000);
    assert_eq!(stored.digests.get(&path), Some(&[9; 32]));
}

/// A re-read file larger than `max_file` is refused unread; one within it
/// is hashed.
#[test]
fn a_reread_source_is_bounded_per_file() {
    let dir = tempfile::tempdir().expect("fixture directory");
    let path = dir.path().join("ten.txt");
    std::fs::write(&path, "0123456789").expect("fixture");
    let path = path.to_string_lossy().into_owned();
    assert!(read_source(path.clone(), &[], 9).is_none());
    let source = read_source(path, &[], 10).expect("within the bound");
    let digest: Digest = octocode_engine::digest::sha256_digest(b"0123456789");
    assert_eq!(source.digest, digest);
    // A directory is not a regular file and is never read.
    assert!(read_source(dir.path().to_string_lossy().into_owned(), &[], 100).is_none());
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

/// Prices a page learns once its scan is stored are charged their bytes, once
/// each; a refused one is not kept, and a later page prices that file again.
#[test]
fn learned_prices_are_charged_or_not_kept() {
    let files = one_match(std::path::Path::new("/src/a.rs"), 1, "a").files;
    let learned = Learned::default();
    let charged = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let total = Arc::clone(&charged);
    learned.bill_to(move |bytes| {
        total.fetch_add(bytes, std::sync::atomic::Ordering::SeqCst);
        true
    });
    for _ in 0..2 {
        learned
            .prices
            .learn(&files, HashMap::from([(0, FilePrice::Unread)]));
    }
    assert_eq!(
        charged.load(std::sync::atomic::Ordering::SeqCst),
        "/src/a.rs".len()
    );
    assert_eq!(learned.prices.weight(), "/src/a.rs".len());
    let refused = Learned::default();
    refused.bill_to(|_| false);
    refused
        .prices
        .learn(&files, HashMap::from([(0, FilePrice::Unread)]));
    assert_eq!(refused.prices.weight(), 0);
    assert!(refused.prices.by_index(&files).is_empty());
}

/// A stream cut a page makes once its scan is stored is charged its bytes; a
/// refused one is not kept, and a later page cuts the scan again.
#[test]
fn a_learned_stream_cut_is_charged_or_not_kept() {
    let pages = Arc::new(vec![vec![(0, 0..2), (1, 0..1)], vec![(2, 0..3)]]);
    let learned = Learned::default();
    let charged = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let total = Arc::clone(&charged);
    learned.bill_to(move |bytes| {
        total.fetch_add(bytes, std::sync::atomic::Ordering::SeqCst);
        true
    });
    learned.cuts.keep(&pages);
    learned.cuts.keep(&pages);
    let weight = learned.cuts.weight();
    assert!(weight > 0);
    assert_eq!(charged.load(std::sync::atomic::Ordering::SeqCst), weight);
    let refused = Learned::default();
    refused.bill_to(|_| false);
    refused.cuts.keep(&pages);
    assert!(!refused.cuts.kept());
    assert_eq!(refused.cuts.weight(), 0);
}
