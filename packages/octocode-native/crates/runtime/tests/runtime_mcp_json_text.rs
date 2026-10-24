//! The MCP text channel is JSON only: every `CallToolResult` carries exactly
//! one text block, the serialized JSON of that same result's final
//! `structuredContent` (MCP server/tools: "a tool that returns structured
//! content SHOULD also return the serialized JSON in a TextContent block").

use crate::support::{Run as _, Workspace, clasify_runtime};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// `result`'s one text block, asserted to be the serialized JSON of its
/// `structuredContent`, byte for byte.
fn assert_json_text(result: &Value) {
    let content = result["content"].as_array().expect("content array");
    assert_eq!(content.len(), 1, "{result}");
    assert_eq!(content[0]["type"], "text", "{result}");
    let structured = &result["structuredContent"];
    assert!(structured.is_object(), "{result}");
    assert_eq!(
        content[0]["text"].as_str().expect("text"),
        serde_json::to_string(structured).expect("serializable"),
        "{result}"
    );
}

async fn mcp(runtime: &ToolRuntime, tool: &str, input: Value) -> Value {
    let result = runtime
        .run_mcp("mcp-json".into(), tool.into(), input)
        .await
        .expect("an MCP result");
    assert_json_text(&result);
    result
}

fn lines(word: &str) -> String {
    (0..200)
        .map(|index| format!("needle {word} {index}\n"))
        .collect()
}

#[tokio::test]
async fn an_unpaged_row_is_its_structured_json() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let file = workspace.write("a.txt", "alpha\nbeta\n");
    let result = mcp(&runtime, "localFetch", json!({"queries":[{"path":file}]})).await;
    assert!(
        result["structuredContent"]
            .get("responsePagination")
            .is_none()
    );
    assert_eq!(result["isError"], false);
    runtime.close().await;
}

/// Every page scope, its saved replay included, answers in JSON text.
#[tokio::test]
async fn every_page_and_its_saved_replay_is_its_structured_json() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let file = workspace.write("pages.txt", lines("paged"));
    let mut page = mcp(
        &runtime,
        "localSearch",
        json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":900}),
    )
    .await;
    let mut pages = 1;
    while let Some(input) = page["structuredContent"]
        .pointer("/responsePagination/next/input")
        .cloned()
    {
        crate::support::assert_cursor_only(&input);
        page = mcp(&runtime, "localSearch", input).await;
        assert_eq!(
            page["structuredContent"]["responsePagination"]["replayed"],
            true
        );
        pages += 1;
        assert!(pages < 300, "finite response pages");
    }
    assert!(pages > 1);
    runtime.close().await;
}

#[tokio::test]
async fn a_tool_error_is_its_structured_json() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let result = mcp(
        &runtime,
        "localFetch",
        json!({"queries":[{"path":"a.txt","matchstring":"x"}]}),
    )
    .await;
    assert_eq!(result["isError"], true);
    assert_eq!(result["structuredContent"]["errorCode"], "invalidInput");
    runtime.close().await;
}

#[tokio::test]
async fn a_batch_with_rejected_rows_is_its_structured_json() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let file = workspace.write("a.txt", lines("batch"));
    for length in [None, Some(1_500)] {
        let mut input = json!({"queries":[
            {"path":file},
            {"path":file,"matchstring":"x"}
        ]});
        if let Some(length) = length {
            input["responseChars"] = json!(length);
        }
        let result = mcp(&runtime, "localFetch", input).await;
        let rows = result["structuredContent"]["results"]
            .as_array()
            .expect("rows");
        assert!(
            rows.iter().any(|row| row["status"] == "error"),
            "{length:?}: {result}"
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn clasify_receipts_are_their_structured_json() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| (id.clone(), json!({"type":"noul","noul":0.9})))
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-1.13.0",
                "answers":answers,
                "usage":{"input_tokens":40,"output_tokens":7}
            }))
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let result = mcp(
        &runtime,
        "clasify",
        json!({"queries":[{
            "operation":"evaluate",
            "content":"Update the configuration, then restart the service.",
            "questions":{"restart":{"type":"noul","instructions":"Does the content require a restart?"}}
        }]}),
    )
    .await;
    assert!(
        result["structuredContent"]["results"][0]["data"]["answers"].is_object(),
        "{result}"
    );
    runtime.close().await;
}
