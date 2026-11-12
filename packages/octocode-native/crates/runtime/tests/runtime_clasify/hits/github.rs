//! The same hit rule on GitHub: a mock contents and code-search API serve the
//! dense file, and each judged route keeps exactly its KEEP hits.
use super::*;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn github_file(github: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/octo/demo/contents/dense.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"type":"file","encoding":"base64","content":STANDARD.encode(DENSE)}),
        ))
        .mount(github)
        .await;
}

/// The same hit rule on GitHub: a matchString read and a repo-scoped search.
#[tokio::test]
async fn github_reads_and_searches_judge_each_hit() {
    let server = provider().await;
    let github = MockServer::builder().start().await;
    github_file(&github).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).insert_header("etag", "\"page\"").set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[{
                "name":"dense.txt","path":"dense.txt","sha":"6",
                "html_url":format!("https://github.com/octo/demo/blob/{SHA}/dense.txt"),
                "repository":{"full_name":"octo/demo","html_url":"https://x","url":"https://x"},
                "text_matches":[{"fragment":"needle","matches":[{"text":"needle","indices":[0,6]}]}]}]
        })))
        .mount(&github)
        .await;
    let workspace = Workspace::new();
    let runtime = support::clasify_runtime(&workspace, &server, &support::github_api(&github));
    let read = support::run(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"octo","repo":"demo","path":"dense.txt","ref":SHA,"matchString":["needle"],
            "contextLines":{"before":2,"after":2},"classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    let data = read.data();
    assert_eq!(data["selection"]["unit"], "hits", "{data}");
    assert_eq!(data["selection"]["totalItems"], 6, "{data}");
    assert_eq!(data["selection"]["selectedItems"], 3, "{data}");
    let search = support::run(
        &runtime,
        "ghSearchCode",
        json!({"owner":"octo","repo":"demo","keywords":["needle"],"ref":SHA,
            "classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    let data = search.data();
    assert_eq!(data["selection"]["unit"], "hits", "{data}");
    assert_eq!(data["selection"]["totalItems"], 6, "{data}");
    assert_eq!(data["selection"]["selectedItems"], 3, "{data}");
    let matches = data["files"][0]["matches"]
        .as_array()
        .expect("kept matches");
    assert_eq!(matches.len(), 3, "{data}");
    assert!(
        matches.iter().all(|hit| hit["line"].is_u64()
            && hit["value"]
                .as_str()
                .is_some_and(|value| value.contains("KEEP"))),
        "{data}"
    );
    runtime.close().await;
}
