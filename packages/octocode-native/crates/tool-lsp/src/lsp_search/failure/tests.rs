use super::*;
use octocode_test_support::json_fixture;

/// An unresolved anchor's read lead runs only when it finds the name: a
/// name the file holds only in another case is named at its nearest line
/// and read case-insensitively; a name the file lacks gets no read.
#[test]
fn anchor_read_lead_only_when_it_finds_the_name() {
    let query: LspSearchQuery = serde_json::from_value(json_fixture(
        r#"{
            "operation": "definition",
            "path": "/w/lib.rs",
            "symbolName": "Runtime",
            "line": 1
        }"#,
    ))
    .expect("query");
    let message = "Could not find symbol 'Runtime' at or near line 1";
    let reads = |row: &Value| {
        row["related"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|call| call["tool"] == "localFetch")
            .map(|call| call["input"]["queries"][0].clone())
            .collect::<Vec<_>>()
    };
    let workspace_search = |row: &Value| {
        row["related"].as_array().is_some_and(|calls| {
            calls.iter().any(|call| {
                call["tool"] == "lspSearch"
                    && call["input"]["queries"][0]["operation"] == "workspaceSymbol"
            })
        })
    };
    let run = |source: &str| {
        let mut row = failure(&query, "", ErrorCode::NotFound, message, true);
        anchor_recovery(&mut row, &query, Some(source));
        row
    };

    let row = run("// x\nstruct Runtime;\n");
    assert_eq!(reads(&row).len(), 1, "{row}");
    assert_eq!(reads(&row)[0]["caseMode"], "sensitive", "{row}");
    assert_eq!(row["error"], message);
    assert!(workspace_search(&row), "{row}");

    let row = run("mod a;\nfn runtime() {}\nfn b() {}\nlet RUNTIME = 1;\n");
    assert_eq!(
        row["error"],
        format!("{message}; the file has 'Runtime' only in another case, nearest at line 2")
    );
    assert_eq!(reads(&row).len(), 1, "{row}");
    assert_eq!(reads(&row)[0]["caseMode"], "insensitive", "{row}");
    assert!(workspace_search(&row), "{row}");

    let row = run("fn other() {}\n");
    assert!(reads(&row).is_empty(), "{row}");
    assert_eq!(row["error"], message);
    assert!(workspace_search(&row), "{row}");
}

/// A row another reason made partial states no complete importer scan and
/// no zero count of verified importers, as a complete row does not: what
/// stays is what a reader acts on, such as the root that bounded the scan
/// or an importer scan that did not complete.
#[test]
fn a_partial_row_states_no_complete_importer_scan() {
    let mut partial = json_fixture(
        r#"{
            "operation": "references",
            "isPartial": true,
            "partialReasons": ["documentAnalysisErrors"],
            "coverage": {"importerScan": "complete", "verifiedImporterFiles": 0}
        }"#,
    );
    stamp_coverage(&mut partial);
    assert!(partial.get("coverage").is_none(), "{partial}");
    let mut bounded = json_fixture(
        r#"{
            "operation": "references",
            "isPartial": true,
            "partialReasons": ["documentAnalysisErrors"],
            "coverage": {"importerScan": "complete", "verifiedImporterFiles": 0, "importerScanRoot": "workspaceRoot"}
        }"#,
    );
    stamp_coverage(&mut bounded);
    assert_eq!(
        bounded["coverage"],
        json_fixture(r#"{"importerScanRoot": "workspaceRoot", "state": "partial"}"#),
        "{bounded}"
    );
    let mut capped = json_fixture(
        r#"{
            "operation": "references",
            "isPartial": true,
            "partialReasons": ["importerScanCapped"],
            "coverage": {"importerScan": "capped", "verifiedImporterFiles": 0, "importerScanRoot": "workspaceRoot"}
        }"#,
    );
    stamp_coverage(&mut capped);
    assert_eq!(
        capped["coverage"],
        json_fixture(
            r#"{"importerScan": "capped", "importerScanRoot": "workspaceRoot", "state": "partial"}"#
        ),
        "{capped}"
    );
}

/// A complete row bounded by the request's workspaceRoot states the root
/// alone: no complete importer scan and no zero count.
#[test]
fn a_complete_bounded_row_states_only_its_root() {
    let mut bounded = json_fixture(
        r#"{
            "operation": "references",
            "coverage": {"importerScan": "complete", "verifiedImporterFiles": 0, "textOnlyFiles": 0, "importerScanRoot": "workspaceRoot"}
        }"#,
    );
    stamp_coverage(&mut bounded);
    assert_eq!(
        bounded["coverage"],
        json_fixture(r#"{"importerScanRoot": "workspaceRoot", "state": "complete"}"#),
        "{bounded}"
    );
}
