mod support;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

#[tokio::test]
async fn github_cache_survives_runtime_close_until_explicitly_cleared() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/source.rs"))
        .respond_with(|request: &Request| {
            if request.headers.get("if-none-match").is_some() {
                ResponseTemplate::new(304)
            } else {
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"v1\"")
                    .set_body_json(json!({
                        "type":"file", "encoding":"base64",
                        "content":STANDARD.encode("source body\n")
                    }))
            }
        })
        .expect(3)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let query = json!({"owner":"a", "repo":"b", "path":"source.rs", "branch":"a".repeat(40)});
    let first = workspace.runtime(&settings);
    let outcome = call(&first, "ghGetFileContent", query.clone())
        .await
        .unwrap();
    assert_eq!(row_status(&outcome), "success");
    first.close().await;

    let second = workspace.runtime(&settings);
    let outcome = call(&second, "ghGetFileContent", query.clone())
        .await
        .unwrap();
    assert_eq!(row_status(&outcome), "success");
    assert_eq!(row_data(&outcome)["files"][0]["content"], "source body\n");
    let requests = server.received_requests().await.unwrap();
    let contents: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
        .collect();
    assert!(contents[0].headers.get("if-none-match").is_none());
    assert_eq!(contents[1].headers.get("if-none-match").unwrap(), "\"v1\"");

    second.clear_github_cache();
    let outcome = call(&second, "ghGetFileContent", query).await.unwrap();
    assert_eq!(row_status(&outcome), "success");
    let requests = server.received_requests().await.unwrap();
    let last_content = requests
        .iter()
        .rev()
        .find(|request| request.url.path().contains("/contents/"))
        .unwrap();
    assert!(last_content.headers.get("if-none-match").is_none());
    second.close().await;
}
