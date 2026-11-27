//! Admission rejects invalid shapes and constraints without guessing a replacement request.
use crate::support::{Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use octocode_test_support::json_fixture;

#[tokio::test]
async fn invalid_inputs_preserve_caller_intent_by_rejecting_without_executable_repairs() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (tool, input, field) in [
        (
            "localFetch",
            json_fixture(
                r#"{
                    "queries": [{"path": "a.rs", "fullContent": true, "length": 100}]
                }"#,
            ),
            "fullContent",
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"queries": [{"path": "a.rs"}]}]}"#),
            "queries",
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"path": "a.rs", "ranges": "1-2"}]}"#),
            "ranges",
        ),
        (
            "localFetch",
            json_fixture(
                r#"{
                    "queries": [
                        {"path": "a.rs", "ranges": [{"startLine": 5, "endLine": 2}]}
                    ]
                }"#,
            ),
            "ranges",
        ),
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [{"path": ".", "matchString": ["x"], "caseMode": "ignore"}]
                }"#,
            ),
            "caseMode",
        ),
        (
            "localSearch",
            json_fixture(r#"{"queries": [{"path": ".", "matchstring": ["x"]}]}"#),
            "matchstring",
        ),
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [{"path": ".", "matchString": ["x"], "glob": "*.ts"}]
                }"#,
            ),
            "glob",
        ),
        (
            "lspSearch",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "refs",
                            "path": "a.ts",
                            "symbolName": "a",
                            "line": 1
                        }
                    ]
                }"#,
            ),
            "operation",
        ),
        (
            "lspSearch",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "definition",
                            "path": "a.ts",
                            "symbolName": "a",
                            "line": 0
                        }
                    ]
                }"#,
            ),
            "line",
        ),
        (
            "ghGetFileContent",
            json_fixture(
                r#"{
                    "queries": [{"path": "https://github.com/o/r/blob/main/a.rs"}]
                }"#,
            ),
            "owner",
        ),
        (
            "ghStructure",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "tree",
                            "owner": "o",
                            "repo": "r",
                            "ref": "v1",
                            "branch": "main"
                        }
                    ]
                }"#,
            ),
            "branch",
        ),
        (
            "astSearch",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "symbols",
                            "path": ".",
                            "rule": {"pattern": "eval($X)"}
                        }
                    ]
                }"#,
            ),
            "rule",
        ),
    ] {
        let error = runtime
            .run(tool.into(), tool.into(), input.clone())
            .await
            .expect_err(&format!("strict rejection of {input}"));
        assert_eq!(error.code, ErrorCode::InvalidInput, "{tool}: {input}");
        let payload = error.payload.as_deref().expect("structured rejection");
        assert!(
            payload["error"]
                .as_str()
                .is_some_and(|message| message.contains(field)),
            "{tool}: {payload}"
        );
        assert!(
            crate::support::checks::leads(payload).is_empty(),
            "no guessed executable request: {payload}"
        );
        assert!(octocode_runtime_base::contracts::validate(tool, input).is_err());
    }
    runtime.close().await;
}
