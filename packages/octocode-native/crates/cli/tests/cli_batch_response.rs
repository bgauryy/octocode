#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;
use serde_json::json;
use support::Workspace;

fn search(root: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut query = json!({"path":root,"searchText":"needle","goal": "test", "reasoning":"Find needles."});
    for (key, value) in extra.as_object().unwrap() {
        query[key] = value.clone();
    }
    query
}

#[tokio::test]
async fn cli_exit_is_two_when_any_batch_row_is_rejected() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let input =
        json!({"queries":[search(&root, json!({})), search(&root, json!({"serchText":"x"}))]});
    let output = workspace
        .cli()
        .args(["localSearch", &input.to_string()])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["results"][1]["data"]["errorCode"], "invalidInput");
    assert_ne!(value["results"][0]["status"], "error");
}

#[test]
fn cli_exit_is_six_while_response_pages_remain() {
    let workspace = Workspace::new();
    let mut file = None;
    for index in 0..10 {
        file = Some(workspace.write(&format!("src/p{index}.txt"), "needle ".repeat(400) + "\n"));
    }
    let root = file
        .unwrap()
        .parent()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let query = search(&root, json!({"pageSize":10,"matchContentLength":2000}));
    let output = workspace
        .cli()
        .env("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", "2000")
        .args(["localSearch", &query.to_string()])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["responsePagination"]["hasMore"], true, "{value}");
    assert_eq!(output.status.code(), Some(6));
}
