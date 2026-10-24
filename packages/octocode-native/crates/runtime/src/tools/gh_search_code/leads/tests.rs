//! Top-hit reads, broadening variants and the leads a code page offers.
use super::*;

#[test]
fn top_hit_reads_its_matched_token() {
    let top = json!({"owner": "o", "repo": "r", "path": "src/a.rs",
        "matches": [{"value": "x\nfn find_all() {}", "matchIndices": [{"start": 5, "end": 13, "lineOffset": 1}]}]});
    let read = read_top_match(&top).expect("continuation");
    assert_eq!(read["tool"], "ghGetFileContent");
    assert_eq!(
        read["input"]["queries"][0]["matchString"],
        json!(["find_all"])
    );
    assert_eq!(read["input"]["queries"][0]["path"], "src/a.rs");
    assert!(read_top_match(&json!("o/r:src/a.rs")).is_none());
}

/// Only a source hit earns the top-match read; docs, changelogs, tests
/// and manifests (often ranked first) get none.
#[test]
fn read_top_match_is_offered_for_code_hits_only() {
    let top = |path: &str| {
        read_top_match(&json!({"owner": "o", "repo": "r", "path": path,
            "matches": [{"value": "needle", "matchIndices": [{"start": 0, "end": 6}]}]}))
    };
    assert!(top("src/a.rs").is_some());
    for path in [
        "GUIDE.md",
        "CHANGELOG.md",
        "tests/a.rs",
        "src/a.test.ts",
        "package.json",
    ] {
        assert!(top(path).is_none(), "{path}");
    }
}

/// An empty search offers one broader variant per narrowing it can
/// drop; a bare single-keyword search has nothing to drop.
#[test]
fn empty_search_broadens_by_dropping_one_narrowing() {
    let query: GhSearchCodeQuery = serde_json::from_value(
        json!({"owner":"o","repo":"r","keywords":["alpha","beta"],"pathPrefix":"src"}),
    )
    .expect("query");
    let lead = broaden_search(&query, 10).expect("variants");
    assert_eq!(lead["tool"], "ghSearchCode");
    let rows = lead["input"]["queries"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "at most two variants: {lead}");
    assert_eq!(rows[0].get("path"), None, "{lead}");
    assert_eq!(rows[1]["keywords"], json!(["alpha"]), "{lead}");
    assert!(rows.iter().all(|row| row.get("pageSize").is_none()));
    let bare: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"o","keywords":["alpha"]})).expect("query");
    assert!(broaden_search(&bare, 10).is_none());
}

/// Code search indexes only the default branch: a broader variant of a
/// ref-scoped empty search drops the `ref` instead of repeating it.
#[test]
fn broadened_rows_drop_the_requested_ref() {
    let query: GhSearchCodeQuery = serde_json::from_value(
        json!({"owner":"o","repo":"r","keywords":["alpha","beta"],"ref":"v2","pathPrefix":"src"}),
    )
    .expect("query");
    let lead = broaden_search(&query, 10).expect("variants");
    let rows = lead["input"]["queries"].as_array().expect("rows");
    assert!(!rows.is_empty(), "{lead}");
    assert!(rows.iter().all(|row| row.get("ref").is_none()), "{lead}");
}

/// Every variant is one more code search in a 10-per-minute window: a
/// variant is offered only when the window has a slot for it.
#[test]
fn broadening_never_offers_more_searches_than_the_window_has_room_for() {
    let query: GhSearchCodeQuery = serde_json::from_value(
        json!({"owner":"o","repo":"r","keywords":["alpha","beta"],"pathPrefix":"src"}),
    )
    .expect("query");
    let variants = |room| {
        broaden_search(&query, room)
            .map(|lead| lead["input"]["queries"].as_array().map_or(0, Vec::len))
    };
    assert_eq!(variants(0), None, "a full window offers no search");
    assert_eq!(variants(1), Some(1));
    assert_eq!(variants(2), Some(2));
    assert_eq!(variants(usize::MAX), Some(2));
}
