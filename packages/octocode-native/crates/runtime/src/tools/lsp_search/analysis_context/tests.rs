use super::*;

fn query() -> LspSearchQuery {
    serde_json::from_value(json!({
        "operation":"definition", "path":"/repo/src/main.rs", "symbolName":"work", "line":20,
        "workspaceRoot":"/repo", "rustContext":{"features":["custom"],"buildScripts":false}
    }))
    .expect("query")
}

#[test]
fn document_errors_are_not_limited_to_the_anchor_and_keep_context() {
    let mut row = json!({"status":"empty", "path":"/repo/src/main.rs"});
    let items = json!([
        {"severity":"error","message":"generated input unavailable",
         "displayRange":{"startLine":1,"endLine":1}}
    ]);
    annotate_diagnostics(&mut row, &query(), Some((&items, false)), 20);
    assert_eq!(
        row["lsp"]["analysisContext"],
        json!({
            "diagnostics":"errors", "diagnosticsComplete":true,
            "errorCount":1, "anchorErrorCount":0
        })
    );
    assert_eq!(row["partialReasons"], json!(["documentAnalysisErrors"]));
    let lead = crate::tools::test_support::checks::related_call(&row, "lspSearch", |input| {
        input["queries"][0]["operation"] == "diagnostic"
    });
    assert_eq!(
        lead["input"]["queries"][0],
        json!({
            "operation":"diagnostic", "path":"/repo/src/main.rs", "workspaceRoot":"/repo",
            "rustContext":{"features":["custom"],"buildScripts":false}
        })
    );
    crate::contracts::validate("lspSearch", lead["input"].clone()).expect("valid follow-up");
}

#[test]
fn unavailable_clean_and_truncated_diagnostics_stay_distinct() {
    let q = query();
    let mut unavailable = json!({"status":"empty"});
    annotate_diagnostics(&mut unavailable, &q, None, 20);
    assert_eq!(
        unavailable["lsp"]["analysisContext"],
        json!({"diagnostics":"unavailable"})
    );
    assert!(unavailable.get("isPartial").is_none());

    let items = json!([]);
    let mut clean = json!({"status":"empty"});
    annotate_diagnostics(&mut clean, &q, Some((&items, false)), 20);
    assert_eq!(
        clean["lsp"]["analysisContext"]["diagnostics"],
        "noErrorsReported"
    );
    assert_eq!(clean["lsp"]["analysisContext"]["diagnosticsComplete"], true);
    assert!(clean.get("isPartial").is_none());

    let mut truncated = json!({"status":"empty"});
    annotate_diagnostics(&mut truncated, &q, Some((&items, true)), 20);
    assert_eq!(
        truncated["lsp"]["analysisContext"]["diagnosticsComplete"],
        false
    );
    assert_eq!(truncated["partialReasons"], json!(["diagnosticsTruncated"]));
}

#[test]
fn successful_answers_retain_their_evidence_when_diagnostics_degrade_coverage() {
    let mut row =
        json!({"operation":"definition", "matches":[{"path":"/repo/target.rs","line":4}]});
    let expected = row["matches"].clone();
    let items = json!([{"severity":"error", "message":"bad configuration"}]);
    annotate_diagnostics(&mut row, &query(), Some((&items, false)), 20);
    assert_eq!(row["matches"], expected);
    assert_eq!(row["partialReasons"], json!(["documentAnalysisErrors"]));

    let mut error = json!({"status":"error", "error":"provider unavailable"});
    let before = error.clone();
    annotate_diagnostics(&mut error, &query(), None, 20);
    assert_eq!(error, before);
}
