use super::*;
use octocode_test_support::json_fixture;

/// An unresolved anchor's read lead runs only when it finds the name: a
/// name the file holds only in another case is named as the file spells it
/// at its nearest line and read case-insensitively; a name the file lacks gets no read.
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
    // The error names the file's own spelling on the nearest line, which
    // the next request can ask for; the requested spelling is in no file.
    let (source_line, spelling) = (2, "runtime");
    let error = row["error"].as_str().expect("error");
    assert!(error.starts_with(message), "{row}");
    let added = &error[message.len()..];
    assert!(
        added.contains(&format!("'{spelling}'")) && added.contains(&format!("line {source_line}")),
        "{row}"
    );
    assert!(!added.contains("Runtime"), "{row}");
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

/// A kind named in a sentence takes the article its sound needs.
#[test]
fn a_kind_takes_the_article_its_sound_needs() {
    use crate::lsp_search::render::with_article;
    for (kind, phrase) in [
        ("enum", "an enum"),
        ("interface", "an interface"),
        ("struct", "a struct"),
        ("type", "a type"),
        ("object", "an object"),
    ] {
        assert_eq!(with_article(kind), phrase);
    }
}

/// An empty answer for a named symbol (a definition of generated code the
/// server does not index) leads with a text search of the workspace root for
/// the symbol, not a read of the anchor file, which only uses the name.
#[test]
fn an_empty_named_symbol_leads_with_a_workspace_text_search() {
    let dir = tempfile::tempdir().expect("workspace");
    let root = dir.path().to_string_lossy().into_owned();
    let file = dir.path().join("use.rs");
    std::fs::write(&file, "fn run() { generated_symbol(); }\n").expect("source");
    let query: LspSearchQuery = serde_json::from_value(json!({
        "operation": "definition",
        "path": file.to_string_lossy(),
        "symbolName": "generated_symbol",
        "line": 1
    }))
    .expect("query");
    let row = with_next(
        &query,
        json!({"status": "empty", "category": "noLocations", "workspaceRoot": root}),
    );
    let related = row["related"].as_array().expect("a related lead");
    let search = related
        .iter()
        .find(|call| call["tool"] == "localSearch")
        .unwrap_or_else(|| panic!("a workspace text search: {row}"));
    let search_query = &search["input"]["queries"][0];
    assert_eq!(search_query["path"], root, "{row}");
    assert!(
        search_query["matchString"][0]
            .as_str()
            .is_some_and(|pattern| pattern.contains("generated_symbol")),
        "{row}"
    );
    assert!(
        related.iter().all(|call| call["tool"] != "localFetch"),
        "{row}"
    );
}
