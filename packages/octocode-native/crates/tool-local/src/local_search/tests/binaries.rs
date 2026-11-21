use super::*;
use octocode_contract_types::LocalSearchQuery;
use octocode_test_support::json_fixture;

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

/// A page cut by a binary file (`binaryQuit`) is partial and keeps its shown
/// text hits; as a row with evidence it states no hint and offers no call.
#[test]
fn a_partial_binary_cut_page_keeps_its_hits_and_guides_nothing() {
    let body = search_fixture(
        &[
            (
                "a.rs",
                "fn a() {}\nlet needle = 1;\nlet needle2 = needle;\n",
            ),
            ("blob.dat", "needle\u{0}needle\n"),
        ],
        ls_query(json_fixture(r#"{"matchString": ["needle"]}"#), None),
    );
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(
        body["partialReasons"],
        json_fixture(r#"["binaryQuit"]"#),
        "{body}"
    );
    assert!(
        body["files"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|file| file["path"] == "a.rs"
                && file["matches"].as_array().map(Vec::len) == Some(2)),
        "{body}"
    );
    assert_evidence_row_guides_nothing(&body);
}

/// A search that found nothing in the readable text but was cut by a
/// binary file (`binaryQuit`) is partial and terminal, never `empty`: its
/// warning names the cut file, so the miss does not read as absence.
#[test]
fn a_zero_match_binary_cut_search_is_partial_and_names_the_cut_file() {
    // The NUL sits past the file's leading bytes, as in a text file with one
    // stray NUL: the scan quits there instead of skipping the file.
    let blob = format!("{}\u{0}tail\n", "line of text\n".repeat(2000));
    let body = search_fixture(
        &[("a.rs", "fn a() {}\n"), ("blob.dat", blob.as_str())],
        ls_query(json_fixture(r#"{"matchString": ["zzqqxxnotthere"]}"#), None),
    );
    assert_eq!(
        body["partialReasons"],
        json_fixture(r#"["binaryQuit"]"#),
        "{body}"
    );
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(body["terminalLimit"], true, "{body}");
    assert!(
        body["warnings"][0]
            .as_str()
            .is_some_and(|warning| warning.contains("blob.dat")),
        "{body}"
    );
}

/// Binaries skipped whole from their leading bytes hold no text a search
/// could match: an empty row states their count in `binarySkipped`, a row
/// with hits (which they change nothing of) does not, and no row lists them.
#[test]
fn an_empty_row_counts_whole_binaries_and_no_row_lists_them() {
    let files = [("a.txt", "alpha\n"), ("img.jpg", "\u{0}\u{1}alpha")];
    for (pattern, empty) in [("alpha", false), ("zzqqxx", true)] {
        let body = search_fixture(
            &files,
            ls_query(serde_json::json!({"matchString": [pattern]}), None),
        );
        assert_eq!(body["files"].as_array().is_some_and(Vec::is_empty), empty);
        if empty {
            assert_eq!(body["binarySkipped"], 1, "{body}");
        } else {
            assert!(body.get("binarySkipped").is_none(), "{body}");
        }
        // Neither row is guided anywhere: one holds evidence, the other is a
        // plain miss no cause explains.
        assert!(body.get("hints").is_none(), "{body}");
        assert!(related_calls(&body).is_empty(), "{body}");
    }
}

/// A warning never names a lead the lead cap may drop.
#[test]
fn binary_warning_never_names_a_capped_lead() {
    let body = search_fixture(
        &[
            ("a.rs", "fn needle() {}\nneedle();\n"),
            ("img.png", "\u{0}\u{1}needle"),
        ],
        ls_query(json_fixture(r#"{"matchString": ["needle"]}"#), None),
    );
    octocode_runtime_base::test_support::checks::assert_absent(
        &body["warnings"],
        "hints.binarySkipped",
    );
}
