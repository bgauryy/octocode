use crate::support::{Workspace, call, row_data, row_status, walk};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "abc123def456abc123def456abc123def456abc1";

fn patch_tokens(output: &Value) -> Vec<String> {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| row["data"]["files"].as_array().into_iter().flatten())
        .filter_map(|file| file["patch"].as_str())
        .flat_map(str::lines)
        .filter_map(|line| line.find("added-line-").map(|at| line[at..].to_owned()))
        .collect()
}

#[tokio::test]
async fn summary_and_patch_walks_terminate_without_requiring_each_other() {
    let server = MockServer::builder().start().await;
    let expected = (0..1200)
        .map(|i| format!("added-line-{i:04}-independent-evidence"))
        .collect::<Vec<_>>();
    let patch = format!(
        "@@ -0,0 +1,{} @@\n+{}",
        expected.len(),
        expected.join("\n+")
    );
    let message = "Add evidence\n\nThe full rationale belongs to the summary.";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{SHA}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha":SHA, "commit":{"message":message,"author":{"name":"A","date":"2024-01-01T00:00:00Z"}},
            "files":[{"filename":"src/a.rs","status":"added","additions":1200,"deletions":0,"patch":patch}]
        })))
        .mount(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let summary = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"queries":[{"operation":"commit","owner":"a","repo":"b","ref":SHA}],"responseChars":4000}),
    )
    .await
    .expect("summary");
    assert_eq!(
        row_status(&summary),
        "success",
        "{}",
        summary.structured_content
    );
    assert_eq!(row_data(&summary)["message"], message);
    // The summary is finished. Files and patches are independently readable.
    assert!(
        summary.structured_content.get("next").is_none(),
        "{}",
        summary.structured_content
    );
    assert!(
        row_data(&summary).get("files").is_none(),
        "{}",
        summary.structured_content
    );
    assert_eq!(
        row_data(&summary)["next"]["files"]["tool"],
        "ghGetHistoryItem",
        "{}",
        summary.structured_content
    );
    let summary_data = row_data(&summary);
    crate::support::checks::assert_leads_valid(summary_data);
    let input = summary_data["next"]["patches"]["input"].clone();
    assert!(input.is_object(), "{summary_data}");
    let walked = walk(&runtime, "ghGetHistoryItem", input, patch_tokens).await;
    assert!(walked.queries.len() > 1, "patch must exercise continuation");
    assert_eq!(walked.rows, expected);
    runtime.close().await;
}
