//! An empty or partial page's next step: a plain miss states none; an
//! unreadable file names its reason in `partialReasons` (LS-7); a skipped
//! target names a reader only where that reader serves it.
use super::*;
use octocode_test_support::json_fixture;

fn workspace() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("narrow")).expect("dir");
    fs::write(root.path().join("narrow/a.txt"), "nothing here\n").expect("fixture");
    fs::write(root.path().join("elsewhere.txt"), "needle\n").expect("fixture");
    root
}

fn search(root: &std::path::Path, path: &str, extra: serde_json::Value) -> serde_json::Value {
    let (policy, security) = policy_for(root);
    let mut fields = serde_json::json!({"path": path, "matchString": ["needle"]});
    for (key, value) in extra.as_object().expect("fields") {
        fields[key] = value.clone();
    }
    let request = ls_query(fields, None);
    search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("row")
}

/// An empty row is its empty list and what the scan covered: `files: []`
/// and `filesScanned`, with no counter that says zero and no byte total.
#[test]
fn an_empty_row_states_coverage_and_no_zero_counter() {
    let root = workspace();
    let row = search(
        root.path(),
        &root.path().to_string_lossy(),
        json_fixture(r#"{"matchString": ["absent-everywhere"]}"#),
    );
    assert_eq!(row["files"], serde_json::json!([]), "{row}");
    assert_eq!(row["filesScanned"], 2, "{row}");
    let data = row.as_object().expect("row");
    for (key, value) in data {
        assert_ne!(value.as_u64(), Some(0), "zero counter {key}: {row}");
    }
    assert!(!data.contains_key("bytesSearched"), "{row}");
    // A row with hits keeps its counters, and states no byte total either.
    let hit = search(
        root.path(),
        &root.path().to_string_lossy(),
        serde_json::json!({}),
    );
    assert_eq!(hit["matchCount"], 1, "{hit}");
    assert_eq!(hit["matchedFiles"], 1, "{hit}");
    assert!(hit.get("bytesSearched").is_none(), "{hit}");
}

/// A plain miss, under a narrow folder, at one file or at the workspace root,
/// is its empty list and its coverage: no cause explains it, so the row
/// states no hint and offers no call. Widening is the caller's call.
#[test]
fn a_plain_miss_states_no_hint_and_offers_no_call() {
    let root = workspace();
    let narrow = root.path().join("narrow").to_string_lossy().into_owned();
    for (path, pattern) in [
        (narrow.clone(), "needle"),
        (format!("{narrow}/a.txt"), "needle"),
        (
            root.path().to_string_lossy().into_owned(),
            "absent-everywhere",
        ),
    ] {
        let row = search(
            root.path(),
            &path,
            serde_json::json!({"matchString": [pattern]}),
        );
        assert_eq!(row["files"], serde_json::json!([]), "{path}: {row}");
        assert!(row.get("hints").is_none(), "{path}: {row}");
        assert!(related_calls(&row).is_empty(), "{path}: {row}");
    }
}

#[cfg(unix)]
#[test]
fn an_unreadable_file_is_named_in_partial_reasons() {
    use std::os::unix::fs::PermissionsExt;
    let root = workspace();
    let locked = root.path().join("narrow/locked.txt");
    fs::write(&locked, "needle\n").expect("fixture");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    if fs::read(&locked).is_ok() {
        return; // the process reads it anyway (root)
    }
    let narrow = root.path().join("narrow").to_string_lossy().into_owned();
    let row = search(root.path(), &narrow, serde_json::json!({}));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).expect("restore");
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["filesSkipped"]"#),
        "{row}"
    );
}

/// The first unreadable file is named as result rows name files (from the
/// workspace, not the host layout), and the gap is stated by its partial
/// reason alone, whether or not the page found a match.
#[cfg(unix)]
#[test]
fn an_unreadable_file_is_named_from_the_workspace_and_stated_once() {
    use std::os::unix::fs::PermissionsExt;
    let root = workspace();
    let locked = root.path().join("narrow/locked.txt");
    fs::write(&locked, "needle\n").expect("fixture");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    if fs::read(&locked).is_ok() {
        return; // the process reads it anyway (root)
    }
    let narrow = root.path().join("narrow").to_string_lossy().into_owned();
    let miss = search(
        root.path(),
        &narrow,
        json_fixture(r#"{"matchString": ["absent"]}"#),
    );
    let hit = search(
        root.path(),
        &narrow,
        json_fixture(r#"{"matchString": ["nothing"]}"#),
    );
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).expect("restore");
    for row in [&miss, &hit] {
        let first = row["fileErrors"]["first"].as_str().expect("first error");
        let host = root.path().to_string_lossy();
        assert!(!first.contains(host.as_ref()), "{first}");
        assert!(first.starts_with("narrow/locked.txt"), "{first}");
        // `partialReasons: filesSkipped` (the contract's own warning) states
        // the gap; the search adds no second warning for the same files.
        assert_eq!(
            row["partialReasons"],
            json_fixture(r#"["filesSkipped"]"#),
            "{row}"
        );
        assert!(!row.to_string().contains("unreadable"), "{row}");
    }
}

/// A sparse file of `len` bytes.
fn sized(path: &std::path::Path, len: u64) {
    fs::File::create(path)
        .and_then(|file| file.set_len(len))
        .expect("sparse fixture");
}

#[test]
fn a_skipped_target_names_localfetch_only_where_it_reads() {
    let root = workspace();
    let ceiling = octocode_engine::search::text_search::DEFAULT_MAX_LINE_SEARCH_FILE_BYTES;
    let stream = octocode_tools_shared::source_view::prepare::MAX_STREAM_SOURCE_BYTES;
    for (name, len, reads) in [
        ("skipped.txt", ceiling + 1, true),
        ("vast.txt", stream + 1, false),
    ] {
        let path = root.path().join(name);
        sized(&path, len);
        let row = search(root.path(), &path.to_string_lossy(), serde_json::json!({}));
        let hints = row["hints"].to_string();
        assert!(hints.contains("Nothing searched"), "{row}");
        assert_eq!(hints.contains("localFetch"), reads, "{name}: {row}");
        // The skip, not the scope, explains the miss: the same search from
        // the root would skip the same file again, so no wider search leads.
        assert!(
            octocode_runtime_base::test_support::checks::find_related_call(
                &row,
                "localSearch",
                |_| true
            )
            .is_none(),
            "{name}: {row}"
        );
    }
}

#[test]
fn a_regex_match_limit_is_named_in_partial_reasons() {
    let root = workspace();
    // Nested repetition over a long run that fails at its end: PCRE2
    // backtracks past its match limit here, and reads the other file.
    fs::write(
        root.path().join("narrow/runaway.txt"),
        format!("{}!\n", "a".repeat(40)),
    )
    .expect("fixture");
    let narrow = root.path().join("narrow").to_string_lossy().into_owned();
    let row = search(
        root.path(),
        &narrow,
        json_fixture(r#"{"matchString": ["(a+)+$"], "regexEngine": "pcre2"}"#),
    );
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["regexMatchLimit"]"#),
        "{row}"
    );
}
