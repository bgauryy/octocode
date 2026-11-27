//! Guidance by row type: a row that returns evidence states no hint and
//! offers no related call, whatever its view, paging, skipped paths or the
//! binaries beside its hits; an empty row states at most one hint and offers
//! at most one related call, the first cause that explains the miss.
use super::*;

/// A workspace with code and test code, a file with hits enough to page,
/// a binary, and an ignored and a hidden directory the walk skips.
fn workspace() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("fixture");
    for (path, body) in [
        (
            "src/lib.rs",
            "pub fn getUser() {}\nconst MAX_RETRIES: u32 = 3;\nfn run() { getUser(); }\n"
                .to_owned(),
        ),
        ("src/more.rs", "let needle = 1;\n".repeat(200)),
        ("src/logo.png", "\u{0}\u{1}needle".to_owned()),
        ("tests/it.rs", "fn it() { only_in_tests(); }\n".to_owned()),
        (".ignore", "generated/\n".to_owned()),
        ("generated/out.rs", "let needle = 2;\n".to_owned()),
        (".cache/kept.rs", "let needle = 3;\n".to_owned()),
    ] {
        let at = root.path().join(path);
        fs::create_dir_all(at.parent().expect("parent")).expect("dir");
        fs::write(at, body).expect("file");
    }
    root
}

/// One page of the search `fields` describe, as its status and its row.
fn page(root: &std::path::Path, fields: serde_json::Value) -> (SearchStatus, serde_json::Value) {
    let (policy, security) = policy_for(root);
    let result = search_in(&ls_query(fields, None), &policy, &security, None).expect("search");
    (result.status, result.to_row().expect("row"))
}

fn has_hits(row: &serde_json::Value) -> bool {
    row["files"]
        .as_array()
        .is_some_and(|files| !files.is_empty())
}

#[test]
fn a_row_with_hits_states_no_hint_and_offers_no_related_call() {
    let root = workspace();
    let at = root.path().to_string_lossy().into_owned();
    let mut skipped = 0;
    for fields in [
        serde_json::json!({"path": at, "matchString": ["needle"]}),
        serde_json::json!({"path": at, "matchString": ["needle"], "view": "files"}),
        serde_json::json!({"path": at, "matchString": ["needle"], "view": "countMatches"}),
        serde_json::json!({
            "path": at, "matchString": [r"need\w+"], "regexEngine": "rust", "view": "matchOnly"
        }),
        // Beside a binary alone, with nothing skipped.
        serde_json::json!({"path": "src", "matchString": ["getUser"]}),
        serde_json::json!({"path": "src/lib.rs", "matchString": ["MAX_RETRIES"]}),
    ] {
        let (status, row) = page(root.path(), fields.clone());
        assert_ne!(status, SearchStatus::Empty, "{fields}: {row}");
        assert!(has_hits(&row), "{fields}: {row}");
        assert_evidence_row_guides_nothing(&row);
        skipped += usize::from(row.get("skippedPaths").is_some());
    }
    assert!(skipped > 0, "a row with hits beside skipped paths");
    // Every page of a paged answer: its continuation reads on, and nothing
    // else is offered.
    let (_, pages) = walk_search(
        root.path(),
        serde_json::json!({"path": at, "matchString": ["needle"]}),
        Some(1500),
    );
    assert!(pages.len() > 1, "the fixture pages");
    for row in pages.iter().filter(|row| has_hits(row)) {
        assert_evidence_row_guides_nothing(row);
    }
}

#[test]
fn an_empty_row_states_one_hint_and_offers_one_related_call_at_most() {
    let root = workspace();
    let at = root.path().to_string_lossy().into_owned();
    let empty = |fields: serde_json::Value| {
        let (status, row) = page(root.path(), fields.clone());
        assert_eq!(status, SearchStatus::Empty, "{fields}: {row}");
        assert!(!has_hits(&row), "{fields}: {row}");
        let call = one_recovery(&row);
        (row, call)
    };
    // A plain miss and a miss of a term of one case throughout: no cause
    // explains either, so neither states a hint or offers a call, the case
    // reading least of all.
    for term in ["zzqqxx", "MAX_TRIES", "zz_qq_42"] {
        let (row, call) = empty(serde_json::json!({"path": "src", "matchString": [term]}));
        assert!(row.get("hints").is_none(), "{term}: {row}");
        assert_eq!(call, None, "{term}: {row}");
    }
    // A term that mixes case: the one call ignores case, and finds it.
    let (row, call) = empty(serde_json::json!({"path": "src", "matchString": ["GetUser"]}));
    let call = call.unwrap_or_else(|| panic!("a call: {row}"));
    assert_eq!(call["caseMode"], "insensitive", "{row}");
    let (_, found) = page(root.path(), call);
    assert!(has_hits(&found), "{found}");
    // Skipped paths a flag lifts: the row keeps their ledger, and the one
    // call lifts their causes and reads what they hold.
    let (row, call) = empty(serde_json::json!({"path": at, "matchString": ["zzqqxx"]}));
    assert!(row["skippedPaths"]["entries"].is_array(), "{row}");
    let call = call.unwrap_or_else(|| panic!("a call: {row}"));
    assert_eq!(call["includeIgnored"], true, "{row}");
    assert_eq!(call["includeHidden"], true, "{row}");
    let (_, wider) = page(root.path(), call);
    assert!(wider["filesScanned"].as_u64() > row["filesScanned"].as_u64());
    // A test-code scope that left the hit out: the one call includes it.
    let (row, call) = empty(serde_json::json!({
        "path": at, "matchString": ["only_in_tests"], "testCode": "exclude"
    }));
    let call = call.unwrap_or_else(|| panic!("a call: {row}"));
    assert_eq!(call["testCode"], "include", "{row}");
    let (_, found) = page(root.path(), call);
    assert!(has_hits(&found), "{found}");
    // Globs that match no file: one hint, and at most the listing of the
    // names that exist.
    let (row, _) = empty(serde_json::json!({
        "path": "src", "matchString": ["needle"], "include": ["*.zig"]
    }));
    assert_eq!(row["hints"].as_array().map(Vec::len), Some(1), "{row}");
}
