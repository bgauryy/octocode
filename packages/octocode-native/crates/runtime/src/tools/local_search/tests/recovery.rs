//! An empty or partial page's next step: a plain miss under a narrow path
//! leads to the narrowest folder above it where the same search matches
//! (LS-5); an unreadable file
//! names its reason in `partialReasons` (LS-7); a skipped target names a
//! reader only where that reader serves it.
use super::*;

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
        serde_json::json!({"matchString": ["absent-everywhere"]}),
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

/// A plain miss leads straight to the narrowest folder that holds every
/// match of the first folder above it with one, and says how many files
/// match there: one call, where one folder up at a time took one empty call
/// per level, and a lead to that ancestor left the agent to narrow again.
#[test]
fn a_plain_miss_leads_to_the_narrowest_folder_above_it_with_a_match() {
    let root = workspace();
    fs::create_dir_all(root.path().join("deep/a/b/c")).expect("dir");
    fs::create_dir_all(root.path().join("deep/other/sub")).expect("dir");
    fs::write(root.path().join("deep/a/b/c/x.txt"), "nothing\n").expect("fixture");
    fs::write(root.path().join("deep/other/hit.txt"), "needle\n").expect("fixture");
    fs::write(root.path().join("deep/other/sub/hit.txt"), "needle\n").expect("fixture");
    let lead_path = |row: &serde_json::Value| {
        let lead = crate::tools::test_support::checks::related_call(row, "localSearch", |_| true);
        let query = &lead["input"]["queries"][0];
        assert_eq!(query["matchString"], serde_json::json!(["needle"]), "{row}");
        query["path"].clone()
    };
    let deep = root
        .path()
        .join("deep/a/b/c")
        .to_string_lossy()
        .into_owned();
    for path in [deep.clone(), format!("{deep}/x.txt")] {
        let row = search(root.path(), &path, serde_json::json!({}));
        assert_eq!(row["files"], serde_json::json!([]), "{row}");
        assert_eq!(lead_path(&row), "deep/other", "{row}");
        assert!(
            row["hints"]
                .to_string()
                .contains("2 file(s) under deep/other match"),
            "{row}"
        );
    }
    // The lead runs, and finds what the narrow scope could not.
    let wider = search(root.path(), "deep/other", serde_json::json!({}));
    assert_eq!(wider["matchCount"], 2, "{wider}");
    // Matches in two folders (deep/other, and elsewhere.txt at the root):
    // the folder that holds them all.
    let narrow = root.path().join("narrow").to_string_lossy().into_owned();
    let row = search(root.path(), &narrow, serde_json::json!({}));
    assert_eq!(lead_path(&row), ".", "{row}");
    assert!(
        row["hints"].to_string().contains("3 file(s) under . match"),
        "{row}"
    );
}

/// When the probe is cut before any folder answers, the lead is the same
/// search one folder up, said as such.
#[test]
fn a_plain_miss_the_probe_cannot_place_leads_one_folder_up() {
    let root = workspace();
    let narrow = root.path().join("narrow").to_string_lossy().into_owned();
    for (path, parent) in [(narrow.clone(), "."), (format!("{narrow}/a.txt"), "narrow")] {
        let row = crate::tools::empty_walk::with_probe_limits(
            crate::tools::empty_walk::CUT_AT_ONCE,
            || search(root.path(), &path, serde_json::json!({})),
        );
        let lead = crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true);
        assert_eq!(lead["input"]["queries"][0]["path"], parent, "{row}");
        assert!(row["hints"].to_string().contains("one folder up"), "{row}");
    }
}

/// A miss the probe finds nowhere in the workspace offers no wider search:
/// every one would be empty.
#[test]
fn a_plain_miss_nowhere_in_the_workspace_offers_no_wider_search() {
    let root = workspace();
    let narrow = root.path().join("narrow").to_string_lossy().into_owned();
    let row = search(
        root.path(),
        &narrow,
        serde_json::json!({"matchString": ["absent-everywhere"]}),
    );
    assert_eq!(row["files"], serde_json::json!([]), "{row}");
    assert!(
        crate::tools::test_support::checks::find_related_call(&row, "localSearch", |_| true)
            .is_none(),
        "{row}"
    );
    assert!(
        row["hints"]
            .to_string()
            .contains("nor anywhere else in the workspace"),
        "{row}"
    );
}

/// A skipped binary holds no text the search could match: an empty page
/// that skipped one still leads first to the folder above where the same
/// search matches; the binary stays a warning, with its listing after.
#[test]
fn a_plain_miss_beside_a_skipped_binary_still_leads_to_the_match() {
    let root = workspace();
    fs::write(
        root.path().join("narrow/icon.png"),
        b"\x89PNG\r\n\x1a\n\0\0needle",
    )
    .expect("fixture");
    let narrow = root.path().join("narrow").to_string_lossy().into_owned();
    let row = search(root.path(), &narrow, serde_json::json!({}));
    assert_eq!(row["files"], serde_json::json!([]), "{row}");
    assert!(
        row["warnings"].to_string().contains("binarySkipped"),
        "{row}"
    );
    let related = row["related"].as_array().expect("related");
    assert_eq!(related[0]["tool"], "localSearch", "{row}");
    assert_eq!(related[0]["input"]["queries"][0]["path"], ".", "{row}");
    assert!(
        related.iter().any(|call| call["tool"] == "structureSearch"),
        "{row}"
    );
    assert!(
        row["hints"].to_string().contains("file(s) under . match"),
        "{row}"
    );
}

/// Outside the workspace the probe climbs only to the top of the tree the
/// path lies in: a clone's checkout, never the store's other checkouts.
#[test]
fn a_plain_miss_in_a_clone_leads_to_the_match_in_its_checkout_only() {
    let workspace = tempfile::tempdir().expect("workspace");
    let store = tempfile::tempdir().expect("store");
    let checkout = store.path().join("owner/repo/c1");
    let other = store.path().join("owner/repo/c2");
    fs::create_dir_all(checkout.join("lib")).expect("dir");
    fs::create_dir_all(checkout.join("test")).expect("dir");
    fs::create_dir_all(&other).expect("dir");
    fs::write(checkout.join("lib/a.js"), "nothing\n").expect("fixture");
    fs::write(checkout.join("test/a.js"), "needle\n").expect("fixture");
    fs::write(other.join("b.js"), "unique-to-other\n").expect("fixture");
    let policy = PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(workspace.path().to_path_buf()),
        additional_roots: vec![store.path().to_path_buf()],
        tree_stores: vec![(store.path().to_path_buf(), 3)],
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new();
    let run = |path: &std::path::Path, term: &str| {
        let request = ls_query(
            serde_json::json!({"path": path.to_string_lossy(), "matchString": [term]}),
            None,
        );
        search_in(&request, &policy, &security, None)
            .expect("search")
            .to_row()
            .expect("row")
    };
    let lib = fs::canonicalize(checkout.join("lib")).expect("lib");
    let row = run(&lib, "needle");
    let lead = crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true);
    let test_dir = fs::canonicalize(checkout.join("test")).expect("test");
    assert_eq!(
        lead["input"]["queries"][0]["path"],
        test_dir.to_string_lossy().as_ref(),
        "{row}"
    );
    assert!(
        row["hints"].to_string().contains("1 file(s) under"),
        "{row}"
    );
    // A term only another checkout holds is nowhere in this one.
    let row = run(&lib, "unique-to-other");
    assert!(
        crate::tools::test_support::checks::find_related_call(&row, "localSearch", |_| true)
            .is_none(),
        "{row}"
    );
    let top = fs::canonicalize(&checkout).expect("checkout");
    assert!(
        row["hints"].to_string().contains(&format!(
            "nor anywhere else under {} under the same filters",
            top.display()
        )),
        "{row}"
    );
}

#[test]
fn a_miss_at_the_workspace_root_has_nothing_wider() {
    let root = workspace();
    let row = search(
        root.path(),
        &root.path().to_string_lossy(),
        serde_json::json!({"matchString": ["absent-everywhere"]}),
    );
    assert_eq!(row["files"], serde_json::json!([]), "{row}");
    assert!(
        crate::tools::test_support::checks::find_related_call(&row, "localSearch", |_| true)
            .is_none(),
        "{row}"
    );
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
        serde_json::json!(["filesSkipped"]),
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
        serde_json::json!({"matchString": ["absent"]}),
    );
    let hit = search(
        root.path(),
        &narrow,
        serde_json::json!({"matchString": ["nothing"]}),
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
            serde_json::json!(["filesSkipped"]),
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
    let stream = crate::tools::source_view::prepare::MAX_STREAM_SOURCE_BYTES;
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
            crate::tools::test_support::checks::find_related_call(&row, "localSearch", |_| true)
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
        serde_json::json!({"matchString": ["(a+)+$"], "regexEngine": "pcre2"}),
    );
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["regexMatchLimit"]),
        "{row}"
    );
}
