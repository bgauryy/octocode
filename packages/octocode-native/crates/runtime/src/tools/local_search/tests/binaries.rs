use super::*;

/// An extensionless binary cannot be named by `extensions`, so the
/// listing continuation matches basenames instead and still reaches it.
#[test]
fn an_extensionless_leading_nul_binary_is_listed_by_basename() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("store"), b"SQLite format 3\0alpha").expect("fixture");
    fs::write(root.path().join("x.woff2"), b"wOF2\0\x01\0\0alpha").expect("fixture");
    fs::write(root.path().join("README"), "plain text\n").expect("fixture");
    fs::write(root.path().join("a.txt"), "alpha\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha".to_string(), "regex": LocalSearchQueryRegex::Literal}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(!result.is_partial);
    assert_eq!(
        result.stats.binary_skipped,
        Some(2),
        "{:?}",
        result.warnings
    );
    let query = result.next.as_ref().expect("next")["binarySkipped"]["query"]["queries"][0].clone();
    assert!(query.get("extensions").is_none(), "{query}");
    let listed = list_structure(query, &policy, &security);
    assert!(listed.contains("store"), "{listed}");
    assert!(listed.contains("x.woff2"), "{listed}");
    assert!(!listed.contains("a.txt"), "{listed}");
}

#[test]
fn opaque_binary_files_in_scope_do_not_make_a_search_partial() {
    // An object-file header puts a NUL before any text: such files are
    // skipped and nothing text-searchable is lost, so only their count is
    // disclosed.
    let body = search_fixture(
        &[
            ("addon.node", "\u{7f}ELF\u{2}\u{1}\u{1}\u{0}needle\n"),
            ("a.txt", "needle\n"),
        ],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string()}),
            None,
        ),
    );
    assert_ne!(body["isPartial"], true, "{body}");
    assert_ne!(body["terminalLimit"], true, "{body}");
    assert_ne!(body["status"], "partial", "{body}");
    assert!(body.get("warnings").is_none(), "{body}");
    assert_eq!(body["binarySkipped"], 1, "{body}");
    // An empty row says it in warnings, where the count may explain the miss.
    let empty = search_fixture(
        &[
            ("addon.node", "\u{7f}ELF\u{2}\u{1}\u{1}\u{0}needle\n"),
            ("a.txt", "other\n"),
        ],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string()}),
            None,
        ),
    );
    assert!(
        empty["warnings"]
            .to_string()
            .contains("binarySkipped: 1 binary file not searched (.node 1)."),
        "{empty}"
    );
    assert!(empty.get("binarySkipped").is_none(), "{empty}");
    assert!(body.get("partialReasons").is_none(), "{body}");

    // Targeted directly, the empty result still says why.
    let root = tempfile::tempdir().expect("fixture directory");
    let target = root.path().join("addon.node");
    fs::write(&target, b"\x7fELF\x02\x01\x01\0needle\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": target.to_string_lossy().into_owned(), "matchString": "needle".to_string()}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(
        result.hints.iter().any(|h| h.contains("binary")),
        "{:?}",
        result.hints
    );
}

#[test]
fn an_empty_result_with_a_binary_cut_is_partial_not_empty() {
    let body = search_fixture(
        &[(
            "blob.dat",
            "a text line\nanother\n\u{0}needle after the nul\n",
        )],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string()}),
            None,
        ),
    );
    assert_eq!(body["isPartial"], true, "{body}");
    // The binary cut is stated once, as a warning.
    assert!(
        body["warnings"].as_array().is_some_and(|warnings| warnings
            .iter()
            .any(|w| w.as_str().is_some_and(|w| w.contains("NUL byte")))),
        "{body}"
    );
    crate::tools::test_support::checks::assert_absent(&body["hints"], "NUL byte");
}

/// A NUL byte in one visible file cuts that file's content, not the walk:
/// the empty row still names the match under an ignored path and leads to
/// it, and a row with that retry is not a terminal limit.
#[test]
fn a_binary_cut_keeps_the_ignored_path_disclosure_and_is_not_terminal() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    fs::write(root.path().join(".gitignore"), "ign/\n").expect("gitignore");
    fs::create_dir_all(root.path().join("ign")).expect("ignored dir");
    fs::write(root.path().join("ign/secret.ts"), "needleIgnored\n").expect("fixture");
    fs::write(root.path().join("nul.txt"), "needle one\n\u{0}needle two\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needleIgnored"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    assert!(result.is_partial, "the binary cut leaves a gap: {result:?}");
    let next = result.next.as_ref().expect("lead");
    assert_eq!(
        next["includeSkipped"]["query"]["queries"][0]["includeIgnored"], true,
        "{next}"
    );
    assert!(
        result
            .hints
            .iter()
            .any(|hint| hint.contains("that matches behind skipped paths")),
        "{:?}",
        result.hints
    );
    assert!(!result.terminal_limit, "{result:?}");
}

/// Skipped binaries are disclosed once, on the first page: a later page
/// repeats neither the warning nor the listing lead.
#[test]
fn skipped_binaries_are_disclosed_on_the_first_page_only() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(
        root.path().join("logo.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .expect("png");
    fs::write(root.path().join("a.txt"), "needle\n").expect("fixture");
    fs::write(root.path().join("b.txt"), "needle\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let first = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "pageSize": 1, "sort": "path"}),
        None,
    );
    let page1 = execute_local_search(&first, &policy, &security, &NeverCancel, None, None, None)
        .expect("page 1");
    assert_eq!(page1.stats.binary_skipped, Some(1), "{:?}", page1.warnings);
    assert!(
        page1
            .next
            .as_ref()
            .is_some_and(|next| next.get("binarySkipped").is_some())
    );
    let next = page1.next.as_ref().expect("next")["nextPage"]["query"]["queries"][0].clone();
    let second: LocalSearchQuery = serde_json::from_value(next).expect("page 2 query");
    let page2 = execute_local_search(&second, &policy, &security, &NeverCancel, None, None, None)
        .expect("page 2");
    assert_eq!(page2.stats.binary_skipped, None, "{:?}", page2.warnings);
    assert!(
        !page2
            .warnings
            .iter()
            .any(|w| w.starts_with("binarySkipped")),
        "{:?}",
        page2.warnings
    );
    assert!(
        page2
            .next
            .as_ref()
            .is_none_or(|next| next.get("binarySkipped").is_none())
    );
}

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
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "PNG"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.files.is_empty(), "{:?}", result.files);
    assert!(
        result
            .next
            .as_ref()
            .is_none_or(|next| next.get("read").is_none())
    );
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
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(body["next"]["read"]["tool"], "localFetch", "{body}");
    assert!(
        body["next"]["read"]["query"]["queries"][0]["path"]
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
        ls_query(serde_json::json!({"matchString": "zzqqxxnotthere"}), None),
    );
    assert_eq!(
        body["partialReasons"],
        serde_json::json!(["binaryQuit"]),
        "{body}"
    );
    assert_eq!(body["terminalLimit"], true, "{body}");
    assert_eq!(
        body["next"]["viewTree"]["tool"], "structureSearch",
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
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    crate::tools::test_support::checks::assert_absent(&body["warnings"], "hints.binarySkipped");
}

#[test]
fn deferred_hit_before_a_large_binary_tail_remains_readable_and_binds_the_tail() {
    use std::io::{Seek, SeekFrom, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};

    let root = tempfile::tempdir().expect("fixture");
    let path = root.path().join("binary-tail.txt");
    let mut source = fs::File::create(&path).expect("create");
    source
        .write_all(b"needle before binary\n\0")
        .expect("prefix");
    let chunk = [b'x'; 64 * 1024];
    for _ in 0..(65 * 16) {
        source.write_all(&chunk).expect("newline-free tail");
    }
    drop(source);
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path":path,"matchString":"needle"}),
        None,
    );
    let scan = super::super::executor::scan_query(&request, &policy, &NeverCancel, Some(1), None)
        .expect("engine accepts the binary prefix");
    assert_eq!(scan.parsed.files.len(), 1);
    assert_eq!(scan.parsed.files[0].matches.len(), 1);
    assert!(scan.parsed.files[0].matches[0].deferred.is_some());
    assert!(
        scan.parsed
            .stats
            .cap_reasons
            .contains(&octocode_engine::types::CapReason::BinaryQuit)
    );
    let original = scan.parsed.files[0].clone();
    let expected = |_: &std::path::Path| None;
    let scope = super::super::verify::PageScope {
        query: &request,
        paths: &policy,
        output_root: root.path(),
        expected_digest: &expected,
        security: &security,
        cancel: &NeverCancel,
        walk_threads: Some(1),
    };
    let mut reconstructed = original.clone();
    super::super::verify::materialize(&scope, &mut reconstructed, 0..1)
        .expect("unchanged binary tail does not prevent reconstructing the earlier hit");
    assert_eq!(reconstructed.matches[0].value, "needle before binary");

    let result = super::super::executor::page_of(
        &request,
        &policy,
        &security,
        &NeverCancel,
        Some(1),
        scan,
        None,
    )
    .expect("unchanged binary tail is hashed without buffering it as a line");
    let mut owned_scan =
        super::super::executor::scan_query(&request, &policy, &NeverCancel, Some(1), None)
            .expect("baseline scan plan");
    let mut options = owned_scan.search_options.clone();
    options.values = octocode_engine::types::SourceValues::Hashed;
    owned_scan.parsed =
        super::super::executor::scan(&request, &policy, options, &policy.identity(), &NeverCancel)
            .expect("owned engine baseline")
            .parsed;
    assert_eq!(owned_scan.parsed.files[0].matches[0].deferred, None);
    assert_eq!(
        owned_scan.parsed.files[0].matches[0].value,
        "needle before binary"
    );
    owned_scan.reusable = None;
    let owned = super::super::executor::page_of(
        &request,
        &policy,
        &security,
        &NeverCancel,
        Some(1),
        owned_scan,
        None,
    )
    .expect("owned page preserves the existing conservative secret guard");
    assert_eq!(
        result, owned,
        "deferred reconstruction preserves the complete owned page, including redaction"
    );

    struct CancelDuringDrain(AtomicUsize);
    impl crate::execution::cancel::CancellationCheck for CancelDuringDrain {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, Ordering::Relaxed) >= 32 {
                Err("tail-drain-cancel".into())
            } else {
                Ok(())
            }
        }
    }
    let cancel = CancelDuringDrain(AtomicUsize::new(0));
    let expected = |_: &std::path::Path| None;
    let scope = super::super::verify::PageScope {
        query: &request,
        paths: &policy,
        output_root: root.path(),
        expected_digest: &expected,
        security: &security,
        cancel: &cancel,
        walk_threads: Some(1),
    };
    let error = super::super::verify::materialize(&scope, &mut original.clone(), 0..1)
        .expect_err("digest-only drain remains cancellable");
    assert!(format!("{error:?}").contains("tail-drain-cancel"));

    let mut source = fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open tail");
    source.seek(SeekFrom::End(-1)).expect("last byte");
    source.write_all(b"z").expect("same-size tail edit");
    drop(source);
    let scope = super::super::verify::PageScope {
        query: &request,
        paths: &policy,
        output_root: root.path(),
        expected_digest: &expected,
        security: &security,
        cancel: &NeverCancel,
        walk_threads: Some(1),
    };
    assert!(
        super::super::verify::materialize(&scope, &mut original.clone(), 0..1).is_err(),
        "even unshown tail bytes remain bound by the source digest"
    );
}
