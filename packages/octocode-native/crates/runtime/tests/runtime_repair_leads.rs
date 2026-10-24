//! Admission rejects invalid shapes and constraints without guessing a replacement request.
use crate::support::{Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use serde_json::json;

#[tokio::test]
async fn invalid_inputs_preserve_caller_intent_by_rejecting_without_executable_repairs() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (tool, input, field) in [
        (
            "localFetch",
            json!({"queries":[{"path":"a.rs","fullContent":true,"length":100}]}),
            "fullContent",
        ),
        (
            "localFetch",
            json!({"queries":[{"queries":[{"path":"a.rs"}]}]}),
            "queries",
        ),
        (
            "localFetch",
            json!({"queries":"[{\"path\":\"a.rs\"}]"}),
            "queries",
        ),
        (
            "localFetch",
            json!({"queries":[{"path":"a.rs","ranges":"1-2"}]}),
            "ranges",
        ),
        (
            "localFetch",
            json!({"queries":[{"path":"a.rs","ranges":[{"startLine":5,"endLine":2}]}]}),
            "ranges",
        ),
        (
            "localSearch",
            json!({"queries":[{"path":".","matchString":["x"],"caseMode":"ignore"}]}),
            "caseMode",
        ),
        (
            "localSearch",
            json!({"queries":[{"path":".","matchstring":["x"]}]}),
            "matchstring",
        ),
        (
            "localSearch",
            json!({"queries":[{"path":".","matchString":["x"],"glob":"*.ts"}]}),
            "glob",
        ),
        (
            "lspSearch",
            json!({"queries":[{"operation":"refs","path":"a.ts","symbolName":"a","line":1}]}),
            "operation",
        ),
        (
            "lspSearch",
            json!({"queries":[{"operation":"definition","path":"a.ts","symbolName":"a","line":0}]}),
            "line",
        ),
        (
            "ghGetFileContent",
            json!({"queries":[{"path":"https://github.com/o/r/blob/main/a.rs"}]}),
            "owner",
        ),
        (
            "ghStructure",
            json!({"queries":[{"operation":"tree","owner":"o","repo":"r","ref":"v1","branch":"main"}]}),
            "branch",
        ),
        (
            "ghSearchCode",
            json!({"queries":[{"owner":"o","repo":"r","keywords":"[\"x\"]"}]}),
            "keywords",
        ),
        (
            "artifactSearch",
            json!({"queries":[{"operation":"resolve","ecosystem":"npm","packageName":"react","version":18}]}),
            "version",
        ),
        (
            "astSearch",
            json!({"queries":[{"operation":"symbols","path":".","pattern":"eval($X)"}]}),
            "pattern",
        ),
    ] {
        let error = runtime
            .run(tool.into(), tool.into(), input.clone())
            .await
            .expect_err("strict rejection");
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
        assert!(octocode_native::contracts::validate(tool, input).is_err());
    }
    runtime.close().await;
}
