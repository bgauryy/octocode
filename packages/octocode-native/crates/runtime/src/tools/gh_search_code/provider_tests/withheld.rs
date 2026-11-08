//! A plain search withholds a file the path policy withholds, as a classified
//! one does: the row lists its path only, nothing of it is read, and the page
//! says so. Under a policy that opens it, the same search reads its lines.
use super::*;

const VALUE: &str = "CowSaysMoo";

async fn mount(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":".env","path":".env","sha":"blob1",
                 "html_url":format!("https://github.com/a/b/blob/{SHA}/.env"),
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":format!("DB_PASS={VALUE}"),
                     "matches":[{"text":"DB_PASS","indices":[0,7]}]}]}
            ]
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/.env"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64",
            "content":STANDARD.encode(format!("APP=demo\nDB_PASS={VALUE}\n"))
        })))
        .mount(server)
        .await;
}

async fn search(server: &MockServer, withholds: super::super::Withholds<'_>) -> String {
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["DB_PASS"]}))
            .expect("query");
    let output = execute(
        &one_shot_provider(server),
        &query,
        &default_context(),
        &Passthrough,
        withholds,
    )
    .await
    .expect("search");
    serde_json::to_string(&output.data).expect("json")
}

async fn reads(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| {
            let path = request.url.path();
            path.contains("/contents/") || path.contains("/git/blobs/")
        })
        .count()
}

#[tokio::test]
async fn a_plain_search_lists_a_withheld_file_by_path_only_and_reads_nothing() {
    let server = MockServer::builder().start().await;
    mount(&server).await;
    let out = search(
        &server,
        &crate::policy::discovery::is_sensitive_repository_file,
    )
    .await;
    assert_eq!(reads(&server).await, 0, "no read of a withheld file: {out}");
    assert!(!out.contains(VALUE), "no value, line or fragment: {out}");
    assert!(!out.contains("\"lines\""), "path only: {out}");
    assert!(out.contains("\"lineResolved\":false"), "{out}");
    assert!(
        out.contains("withheld by the security path policy"),
        "the page says it is withheld: {out}"
    );
    assert!(!out.contains("ghGetFileContent"), "no read lead: {out}");
}

#[tokio::test]
async fn a_policy_that_opens_the_file_reads_its_lines() {
    let server = MockServer::builder().start().await;
    mount(&server).await;
    let out = search(&server, &|_: &str| false).await;
    assert!(reads(&server).await > 0, "{out}");
    assert!(out.contains("\"lines\""), "{out}");
    assert!(
        !out.contains("withheld by the security path policy"),
        "{out}"
    );
}
