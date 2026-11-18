//! `path` names a folder or a file: a folder searches under it, a file searches that file.
use super::*;
use octocode_test_support::json_fixture;

fn item(path: &str) -> serde_json::Value {
    json!({"name":path.rsplit('/').next(),"path":path,"sha":"1","html_url":"https://x",
        "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
}

/// Mount GitHub's answer to the search `q`: `items` of `total` matches,
/// sent `times` times.
async fn answer(server: &MockServer, q: &str, total: usize, items: &[&str], times: u64) {
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param("q", q))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":total,"incomplete_results":false,
            "items":items.iter().map(|path| item(path)).collect::<Vec<_>>()
        })))
        .expect(times)
        .mount(server)
        .await;
}

async fn search(
    server: &MockServer,
    row: serde_json::Value,
) -> octocode_runtime_base::response::result::ToolData {
    let provider = one_shot_provider(server);
    let query: GhSearchCodeQuery = serde_json::from_value(row).expect("query");
    execute(
        &provider,
        &query,
        &default_context(),
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search")
}

/// GitHub's `path:` matches no file's own path: the empty folder search
/// runs once more as the file, which lists that file alone (a deeper path
/// that ends the same way is another file), with no repair hint.
#[tokio::test]
async fn a_file_path_searches_that_file() {
    let server = MockServer::builder().start().await;
    answer(
        &server,
        r#"main path:"src/bin/main.rs" repo:a/b in:path"#,
        0,
        &[],
        1,
    )
    .await;
    answer(
        &server,
        r#"main filename:"src/bin/main.rs" path:"src/bin" repo:a/b in:path"#,
        2,
        &["src/bin/main.rs", "src/bin/x/src/bin/main.rs"],
        1,
    )
    .await;
    let out = search(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["main"],
                "matchIn": ["path"],
                "path": "src/bin/main.rs"
            }"#,
        ),
    )
    .await;
    assert_eq!(paths_of(&out), ["src/bin/main.rs"], "{}", out.data);
    assert!(
        !out.data["hints"].to_string().contains("nameContains"),
        "{}",
        out.data
    );
    // A root file is anchored at the root.
    let server = MockServer::builder().start().await;
    answer(&server, "main path:Cargo.toml repo:a/b in:path", 0, &[], 1).await;
    answer(
        &server,
        "main filename:Cargo.toml path:/ repo:a/b in:path",
        1,
        &["Cargo.toml"],
        1,
    )
    .await;
    let out = search(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["main"],
                "matchIn": ["path"],
                "path": "Cargo.toml"
            }"#,
        ),
    )
    .await;
    assert_eq!(paths_of(&out), ["Cargo.toml"], "{}", out.data);
}

/// A folder `path` searches under it once; a dotted folder name is a folder.
#[tokio::test]
async fn a_folder_path_searches_under_it_once() {
    let server = MockServer::builder().start().await;
    answer(
        &server,
        r#"main path:"docs/v1.2" repo:a/b in:path"#,
        2,
        &["docs/v1.2/main.md", "docs/v1.2/a/main.rs"],
        1,
    )
    .await;
    answer(
        &server,
        r#"main filename:"docs/v1.2" path:docs repo:a/b in:path"#,
        0,
        &[],
        0,
    )
    .await;
    let out = search(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["main"],
                "matchIn": ["path"],
                "path": "docs/v1.2"
            }"#,
        ),
    )
    .await;
    let mut paths = paths_of(&out);
    paths.sort();
    assert_eq!(
        paths,
        ["docs/v1.2/a/main.rs", "docs/v1.2/main.md"],
        "{}",
        out.data
    );
}
