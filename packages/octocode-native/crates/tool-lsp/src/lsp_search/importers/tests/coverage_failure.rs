//! Lexical file agreement cannot verify an importer identity check that failed.
use super::*;
use octocode_test_support::json_fixture;

#[tokio::test]
async fn failed_anchor_verification_stays_partial_when_every_text_file_is_answered() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("canonical fixture");
    let file = root.join("a.ts");
    std::fs::write(&file, "export function target() {}\n").expect("source");
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    let paths = octocode_runtime_base::test_support::workspace_policy(&root);
    let files = scope
        .text_files(
            "target",
            &paths,
            &octocode_runtime_base::execution::cancel::NeverCancel,
        )
        .await
        .expect("scan")
        .expect("complete text scan");
    assert_eq!(files.len(), 1);
    scope.answer(files.iter().map(String::as_str));
    assert_eq!(scope.text_only_files("target"), Some(Vec::new()));

    let query = serde_json::from_value(json!({
        "operation": "references", "path": file.to_string_lossy(),
        "symbolName": "target", "line": 1
    }))
    .expect("query");
    let importers = Importers {
        failed: true,
        failure: ScanFailure::AnchorDeclaration,
        ..Importers::default()
    };
    let mut row = json_fixture(r#"{"operation": "references", "matches": []}"#);
    importers.annotate(&mut row, &query, &scope);
    crate::lsp_search::failure::stamp_coverage(&mut row);

    assert_eq!(row["coverage"]["textOnlyFiles"], 0, "{row}");
    assert_eq!(row["coverage"]["importerScan"], SCAN_FAILED, "{row}");
    assert_eq!(row["coverage"]["state"], "partial", "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["importerScanFailed"]"#),
        "{row}"
    );
    assert!(
        row["warnings"]
            .as_array()
            .expect("warning")
            .iter()
            .any(|warning| warning
                .as_str()
                .is_some_and(|text| text.contains("anchor's own declaration"))),
        "{row}"
    );
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&row, "localSearch", |_| true);
    assert_eq!(
        lead["input"]["queries"][0]["path"],
        root.to_string_lossy().as_ref()
    );
    assert_eq!(
        lead["input"]["queries"][0]["include"],
        json_fixture(r#"["*.ts"]"#)
    );
    octocode_runtime_base::contracts::validate("localSearch", lead["input"].clone())
        .expect("executable lexical recovery");
}
