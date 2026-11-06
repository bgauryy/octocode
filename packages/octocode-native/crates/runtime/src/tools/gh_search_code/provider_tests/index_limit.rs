//! A file-name search names GitHub's index size limit: a file it skips is absent without a trace.
use super::*;

async fn hints_for(query: serde_json::Value) -> Vec<String> {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"pnpm-lock.yaml","path":"extensions/pnpm-lock.yaml","sha":"1",
                 "html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}}
            ]
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(query).expect("query");
    let output = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("search");
    output.diagnostics.hints.clone()
}

#[tokio::test]
async fn a_file_name_search_names_the_384_kb_index_limit() {
    let hints = hints_for(
        json!({"owner":"a","repo":"b","keywords":["yaml"],"nameContains":"pnpm-lock.yaml"}),
    )
    .await;
    assert!(
        hints.iter().any(|hint| hint.contains("384 KB")),
        "{hints:?}"
    );
}

#[tokio::test]
async fn a_content_search_does_not_repeat_the_limit() {
    let hints = hints_for(json!({"owner":"a","repo":"b","keywords":["yaml"]})).await;
    assert!(
        !hints.iter().any(|hint| hint.contains("384 KB")),
        "{hints:?}"
    );
}

/// A name fragment names no one file: no larger copy can be missing, so the
/// page stays quiet.
#[tokio::test]
async fn a_name_fragment_does_not_repeat_the_limit() {
    let hints =
        hints_for(json!({"owner":"a","repo":"b","keywords":["yaml"],"nameContains":"lock"})).await;
    assert!(
        !hints.iter().any(|hint| hint.contains("384 KB")),
        "{hints:?}"
    );
}
