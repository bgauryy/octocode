use super::*;
use octocode_test_support::json_fixture;

fn query() -> LspSearchQuery {
    serde_json::from_value(json_fixture(
        r#"{
            "operation": "definition",
            "path": "/repo/src/main.rs",
            "symbolName": "work",
            "line": 20,
            "workspaceRoot": "/repo"
        }"#,
    ))
    .expect("query")
}

#[test]
fn document_errors_are_not_limited_to_the_anchor_and_keep_context() {
    let mut row = json_fixture(r#"{"status": "empty", "path": "/repo/src/main.rs"}"#);
    let items = json_fixture(
        r#"[
            {
                "severity": "error",
                "message": "generated input unavailable",
                "line": 1
            }
        ]"#,
    );
    annotate_diagnostics(&mut row, &query(), Some(&items), 20);
    assert_eq!(
        row["lsp"]["analysisContext"],
        json_fixture(
            r#"{
                "diagnostics": "errors",
                "diagnosticsComplete": true,
                "errorCount": 1,
                "anchorErrorCount": 0
            }"#
        )
    );
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["documentAnalysisErrors"]"#)
    );
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&row, "lspSearch", |input| {
            input["queries"][0]["operation"] == "diagnostic"
        });
    assert_eq!(
        lead["input"]["queries"][0],
        json_fixture(
            r#"{
                "operation": "diagnostic",
                "path": "/repo/src/main.rs",
                "workspaceRoot": "/repo"
            }"#
        )
    );
    octocode_runtime_base::contracts::validate("lspSearch", lead["input"].clone())
        .expect("valid follow-up");
}

#[test]
fn unavailable_and_complete_clean_diagnostics_stay_distinct() {
    let q = query();
    let mut unavailable = json_fixture(r#"{"status": "empty"}"#);
    annotate_diagnostics(&mut unavailable, &q, None, 20);
    assert_eq!(
        unavailable["lsp"]["analysisContext"],
        json_fixture(r#"{"diagnostics": "unavailable"}"#)
    );
    assert!(unavailable.get("isPartial").is_none());

    let items = json!([]);
    let mut clean = json_fixture(r#"{"status": "empty"}"#);
    annotate_diagnostics(&mut clean, &q, Some(&items), 20);
    assert_eq!(
        clean["lsp"]["analysisContext"]["diagnostics"],
        "noErrorsReported"
    );
    assert_eq!(clean["lsp"]["analysisContext"]["diagnosticsComplete"], true);
    assert!(clean.get("isPartial").is_none());
}

#[test]
fn successful_answers_retain_their_evidence_when_diagnostics_degrade_coverage() {
    let mut row = json_fixture(
        r#"{
            "operation": "definition",
            "matches": [{"path": "/repo/target.rs", "line": 4}]
        }"#,
    );
    let expected = row["matches"].clone();
    let items = json_fixture(r#"[{"severity": "error", "message": "bad configuration"}]"#);
    annotate_diagnostics(&mut row, &query(), Some(&items), 20);
    assert_eq!(row["matches"], expected);
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["documentAnalysisErrors"]"#)
    );

    let mut error = json_fixture(r#"{"status": "error", "error": "provider unavailable"}"#);
    let before = error.clone();
    annotate_diagnostics(&mut error, &query(), None, 20);
    assert_eq!(error, before);
}

/// Final round, item 4: a diagnostic's message and source are the server's
/// text, quoted in the anchor-line warning as judged: a credential the
/// server quotes from the source shows as the placeholder a read shows, and
/// the server's words stay.
#[test]
fn an_anchor_errors_message_is_quoted_as_judged() {
    let token = format!("ghp_{}", "a1B2".repeat(9));
    let warning = |message: String| {
        let mut row = json_fixture(r#"{"status": "empty", "path": "/repo/src/main.rs"}"#);
        let items =
            json!([{"severity":"error", "message": message, "line": 20, "source": "rustc"}]);
        annotate_diagnostics(&mut row, &query(), Some(&items), 20);
        row["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .find(|warning| warning.contains("reports an error on line"))
            .map(str::to_owned)
            .unwrap_or_else(|| panic!("no anchor warning: {row}"))
    };
    let quoted = warning(format!("expected `;`, found `GITHUB_TOKEN = \"{token}\"`"));
    assert!(
        !quoted.contains(&token) && quoted.contains("[REDACTED-"),
        "{quoted}"
    );
    let plain = warning("mismatched types".into());
    assert!(
        plain.contains("line 20: mismatched types (rustc)."),
        "{plain}"
    );
}
