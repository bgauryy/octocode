//! Coverage: verification failures and unfinished scans.
use super::*;
use octocode_test_support::json_fixture;

#[test]
fn failed_importer_verification_never_reports_complete_coverage() {
    let importers = Importers {
        failed: true,
        ..Importers::default()
    };
    let mut row = json_fixture(r#"{"status": "success"}"#);
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "references",
            "path": "/r/a.ts",
            "symbolName": "x",
            "line": 1
        }"#,
    ))
    .expect("query");
    importers.annotate(&mut row, &query, &Scope::new("/r".into(), Vec::new()));
    assert_eq!(row["coverage"]["importerScan"], SCAN_FAILED);
}

/// A scope the request's workspaceRoot narrowed says that importers outside
/// it were not scanned; the default scope says nothing.
#[test]
fn a_narrowed_scope_discloses_its_root() {
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "references",
            "path": "/r/p/a.ts",
            "symbolName": "x",
            "line": 1
        }"#,
    ))
    .expect("query");
    let mut scope = Scope::new("/r/p".into(), Vec::new());
    let mut row = json_fixture(r#"{"status": "success"}"#);
    Importers::default().annotate(&mut row, &query, &scope);
    assert!(row["coverage"].get("importerScanRoot").is_none(), "{row}");
    scope.narrowed = true;
    let mut row = json_fixture(r#"{"status": "success"}"#);
    Importers::default().annotate(&mut row, &query, &scope);
    assert_eq!(
        row["coverage"]["importerScanRoot"], "workspaceRoot",
        "{row}"
    );
}

/// Unverified candidates stay out of the answer, so `textOnlyFiles` and
/// the lexical lead disclose them, and the scan is never complete.
#[test]
fn unverified_candidates_are_not_answered_and_fail_the_scan() {
    let list = candidates(3);
    let Cut::Window(window) = Window::cut(list.clone(), WindowRequest::default()) else {
        panic!("window 1 is never stale");
    };
    let importers = Importers {
        window: Some(window),
        failed: true,
        unverified: vec![list[1].clone()],
        ..Importers::default()
    };
    assert_eq!(importers.covered(), vec![list[0].clone(), list[2].clone()]);
    let scope = Scope::new("/r".into(), Vec::new());
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "references",
            "path": "/r/a.ts",
            "symbolName": "x",
            "line": 1
        }"#,
    ))
    .expect("query");
    let mut row = json_fixture(r#"{"status": "success"}"#);
    importers.annotate(&mut row, &query, &scope);
    assert_eq!(row["coverage"]["importerScan"], SCAN_FAILED);
}

/// A failed scan names what failed: the candidate list, the anchor's own
/// declaration, or each candidate with its cause (unread or unopened, or
/// unresolved by the server), never only "could not read, open, or resolve
/// every candidate".
#[test]
fn a_failed_scan_names_its_step_and_each_candidate_with_its_cause() {
    let query: LspSearchQuery = serde_json::from_value(json_fixture(
        r#"{
            "operation": "references",
            "path": "/r/a.c",
            "symbolName": "dictFind",
            "line": 1
        }"#,
    ))
    .expect("query");
    let scope = Scope::new("/r".into(), Vec::new());
    let warning_of = |importers: Importers| {
        let mut row = json_fixture(r#"{"status": "success"}"#);
        importers.annotate(&mut row, &query, &scope);
        assert_eq!(row["coverage"]["importerScan"], SCAN_FAILED, "{row}");
        assert_eq!(row["isPartial"], true, "{row}");
        row["warnings"].to_string()
    };
    let list = warning_of(Importers {
        failed: true,
        failure: ScanFailure::CandidateList,
        ..Importers::default()
    });
    assert!(
        list.contains("could not list the files that spell this name"),
        "{list}"
    );
    let files = candidates(4);
    let Cut::Window(window) = Window::cut(files.clone(), WindowRequest::default()) else {
        panic!("window 1 is never stale");
    };
    let anchor = warning_of(Importers {
        window: Some(window),
        failed: true,
        failure: ScanFailure::AnchorDeclaration,
        ..Importers::default()
    });
    assert!(anchor.contains("anchor's own declaration"), "{anchor}");
    assert!(anchor.contains("none of the 4 candidate files"), "{anchor}");
    let Cut::Window(window) = Window::cut(files.clone(), WindowRequest::default()) else {
        panic!("window 1 is never stale");
    };
    let named = warning_of(Importers {
        window: Some(window),
        failed: true,
        failure: ScanFailure::Candidates,
        unverified: vec![files[1].clone(), files[2].clone()],
        unopened: vec![files[2].clone()],
        ..Importers::default()
    });
    let relative = |file: &str| {
        std::path::Path::new(file)
            .strip_prefix("/r")
            .map_or(file.to_owned(), |path| path.to_string_lossy().into_owned())
    };
    assert!(
        named.contains(&format!(
            "{} could not be read or opened",
            relative(&files[2])
        )),
        "{named}"
    );
    assert!(
        named.contains(&format!(
            "resolved no occurrence in {}",
            relative(&files[1])
        )),
        "{named}"
    );
}
