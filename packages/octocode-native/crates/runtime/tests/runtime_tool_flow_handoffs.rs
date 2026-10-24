//! Search and fetch never hand off to public clasify: relevance runs inside
//! retrieval (`classifiers`), so no response offers a second relevance pass,
//! and none makes a provider request without classifiers.

use crate::support::{Run as _, Workspace};

use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn wide_searches_and_unfinished_reads_offer_no_clasify_handoff() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for index in 0..40 {
        workspace.write(
            &format!("src/file_{index}.c"),
            (0..80)
                .map(|line| format!("housekeeping timer {line}\n"))
                .collect::<String>(),
        );
    }
    let runtime = workspace.runtime(&[
        ("OCTOCODE_BETA", "true".into()),
        ("OCTOCODE_CLASSIFICATION_API", "fixture-key".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
    ]);
    for (tool, row) in [
        (
            "localSearch",
            json!({"path":"src","matchString":["housekeeping"]}),
        ),
        ("localFetch", json!({"path":"src/file_0.c"})),
    ] {
        let out = runtime
            .run(
                "handoff".into(),
                tool.into(),
                json!({"queries":[row],"responseChars":1000}),
            )
            .await
            .expect("result")
            .structured_content;
        let mut required = 0;
        let owner = octocode_native::id::ToolId::from_name(tool).expect("known tool");
        octocode_native::response::continuations::visit_calls(owner, &out, &mut |call, _| {
            assert_ne!(
                call["tool"], "clasify",
                "retrieval does not request implicit classification: {call}"
            );
            if call["kind"] == "continue" {
                crate::support::assert_cursor_only(&call["input"]);
                required += 1;
            }
        })
        .expect("schema-directed calls");
        assert!(
            required > 0,
            "unfinished retrieval exposes its remaining work: {out}"
        );
    }
    runtime.close().await;
}
