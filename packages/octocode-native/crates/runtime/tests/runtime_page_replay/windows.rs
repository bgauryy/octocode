//! Response budgets preserve complete numbered source evidence on both transports.
use super::SHA;
use crate::support::{self, Run as _, Workspace};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn retired_window_controls_are_rejected() {
    let workspace = Workspace::new();
    workspace.write("one.txt", "source\n");
    let runtime = workspace.runtime(&[]);
    for field in [
        "responseOffset",
        "responseLength",
        "responseScope",
        "responseSnapshot",
    ] {
        let mut input = json!({"queries":[{"path":"one.txt"}]});
        input[field] = json!(0);
        let error = runtime
            .run("retired".into(), "localFetch".into(), input)
            .await
            .expect_err("unknown envelope field");
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
    runtime.close().await;
}

fn body() -> String {
    (0..400)
        .map(|line| format!("evidence_{line:04} 界😀\n"))
        .collect()
}

fn source_lines(output: &Value) -> Vec<String> {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| {
            row["data"]["content"]
                .as_str()
                .into_iter()
                .flat_map(str::lines)
                .filter_map(|line| line.find("evidence_").map(|start| line[start..].to_owned()))
        })
        .collect()
}

async fn assert_complete(runtime: &octocode_native::runtime::ToolRuntime, tool: &str, row: Value) {
    use octocode_native::runtime::Channel;
    for channel in [Channel::Cli, Channel::Mcp] {
        for budget in [2500, 10000] {
            let mut walker =
                support::checks::Walker::new(tool, json!({"queries":[row],"responseChars":budget}));
            while let Some(input) = walker.next_query() {
                let admission = runtime.admit("source-page".into()).expect("admit");
                let output = runtime
                    .execute(admission, tool.into(), input, channel)
                    .await
                    .expect("read")
                    .structured_content
                    .into_value();
                let chars = output.to_string().encode_utf16().count();
                assert!(
                    chars <= budget || output["responsePagination"]["oversized"] == true,
                    "oversize must be explicit: {chars}/{budget} {output}"
                );
                walker.accept(support::checks::Page {
                    rows: source_lines(&output),
                    output,
                });
            }
            let walk = walker.finish();
            if budget == 2500 {
                assert!(walk.queries.len() > 1);
            }
            assert_eq!(
                walk.rows,
                body().lines().map(str::to_owned).collect::<Vec<_>>()
            );
        }
    }
}

#[tokio::test]
async fn local_source_pages_preserve_numbered_unicode_lines_on_both_transports() {
    let workspace = Workspace::new();
    let file = workspace.write("source.txt", body());
    let runtime = workspace.runtime(&[]);
    assert_complete(&runtime, "localFetch", json!({"path":file})).await;
    runtime.close().await;
}

#[tokio::test]
async fn github_source_pages_preserve_numbered_unicode_lines_on_both_transports() {
    let server = MockServer::builder().start().await;
    let body = body();
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/src%2Fsource.rs")).and(query_param("ref",SHA)).respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body),"size":body.len(),"sha":"f".repeat(40),"path":"src/source.rs"}))).mount(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    assert_complete(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","ref":SHA,"path":"src/source.rs"}),
    )
    .await;
    runtime.close().await;
}
