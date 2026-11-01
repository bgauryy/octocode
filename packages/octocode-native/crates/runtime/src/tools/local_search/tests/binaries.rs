use super::*;
use octocode_contract_types::LocalSearchQuery;

mod tail;

/// Bytes of a binary header that happen to match are not hits: no row and
/// no read lead a text read would fail on.
#[test]
fn a_match_inside_a_binary_header_yields_no_row_and_no_read() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(
        root.path().join("logo.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .expect("png");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": ["PNG"]}),
        None,
    );
    let result = search_in(&request, &policy, &security, None).expect("search");
    assert!(result.files.is_empty(), "{:?}", result.files);
    assert!(
        result
            .next
            .as_ref()
            .is_none_or(|next| next.get("read").is_none())
    );
}

/// The skipped-binary warning reads as a count and the files' extensions.
#[test]
fn the_binary_warning_names_counts_and_extensions_plainly() {
    let group = |extension: &str, count: u32| octocode_engine::types::BinaryExtensionCount {
        extension: extension.into(),
        count,
        names: Vec::new(),
    };
    let warning = crate::tools::local_search::binaries::leading_binary_warning;
    assert_eq!(
        warning(&[group("", 1)]),
        "binarySkipped: 1 binary file not searched (no extension)."
    );
    assert_eq!(
        warning(&[group("png", 2)]),
        "binarySkipped: 2 binary files not searched (all .png)."
    );
    assert_eq!(
        warning(&[group("woff2", 54), group("", 1)]),
        "binarySkipped: 55 binary files not searched (54 .woff2, 1 with no extension)."
    );
}

/// A binary file named alone searched nothing: refused as
/// `binaryFileUnsupported` (as astSearch and localFetch refuse it), never
/// an empty row that reads as absence.
#[test]
fn a_binary_file_named_alone_is_refused_not_empty() {
    let root = tempfile::tempdir().expect("fixture directory");
    let logo = root.path().join("logo.png");
    fs::write(&logo, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").expect("png");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": logo.to_string_lossy().into_owned(), "matchString": ["PNG"]}),
        None,
    );
    let error = search_in(&request, &policy, &security, None).expect_err("binary target");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::BinaryFileUnsupported,
        "{error:?}"
    );
    assert!(error.message.contains("logo.png"), "{error:?}");
}

/// A page cut by a binary file (`binaryQuit`) is partial, but its shown
/// text hits stay one read away.
#[test]
fn partial_binary_cut_page_keeps_read_lead() {
    let body = search_fixture(
        &[
            (
                "a.rs",
                "fn a() {}\nlet needle = 1;\nlet needle2 = needle;\n",
            ),
            ("blob.dat", "needle\u{0}needle\n"),
        ],
        ls_query(serde_json::json!({"matchString": ["needle"]}), None),
    );
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(
        crate::tools::test_support::checks::related_call(&body, "localFetch", |_| true)["tool"],
        "localFetch",
        "{body}"
    );
    assert!(
        crate::tools::test_support::checks::related_call(&body,"localFetch", |_|true)["input"]["queries"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.rs")),
        "{body}"
    );
}

/// A search that found nothing in the readable text but was cut by a
/// binary file (`binaryQuit`) is partial and terminal, so the bare
/// `No matches` fact cannot stand alone: it leads to the scope's tree to
/// narrow the search.
#[test]
fn zero_match_binary_cut_search_leads_to_narrow_the_scope() {
    // The NUL sits past the file's leading bytes, as in a text file with one
    // stray NUL: the scan quits there instead of skipping the file.
    let blob = format!("{}\u{0}tail\n", "line of text\n".repeat(2000));
    let body = search_fixture(
        &[("a.rs", "fn a() {}\n"), ("blob.dat", blob.as_str())],
        ls_query(serde_json::json!({"matchString": ["zzqqxxnotthere"]}), None),
    );
    assert_eq!(
        body["partialReasons"],
        serde_json::json!(["binaryQuit"]),
        "{body}"
    );
    assert_eq!(body["terminalLimit"], true, "{body}");
    assert_eq!(
        crate::tools::test_support::checks::related_call(&body, "structureSearch", |_| true)["tool"],
        "structureSearch",
        "{body}"
    );
}

/// A warning never names a lead the lead cap may drop.
#[test]
fn binary_warning_never_names_a_capped_lead() {
    let body = search_fixture(
        &[
            ("a.rs", "fn needle() {}\nneedle();\n"),
            ("img.png", "\u{0}\u{1}needle"),
        ],
        ls_query(serde_json::json!({"matchString": ["needle"]}), None),
    );
    crate::tools::test_support::checks::assert_absent(&body["warnings"], "hints.binarySkipped");
}
