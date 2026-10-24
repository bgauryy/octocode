//! Fixtures of the provider-backed GitHub search-tool tests (ghSearchCode,
//! ghSearchRepo, ghStructure, and their shared paging table) against one
//! mock server.
use crate::response::result::ToolData;
use crate::security::scan::Passthrough;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use crate::tools::{gh_search_code, gh_search_repo, gh_structure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_github::ProviderError;
use octocode_github::mock_http::mount_json;
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

pub(crate) async fn run_code(server: &MockServer, query: Value) -> Result<ToolData, ProviderError> {
    let provider = one_shot_provider(server);
    let query = serde_json::from_value(query).expect("canonical code query");
    gh_search_code::execute(&provider, &query, &default_context(), &Passthrough).await
}

pub(crate) async fn run_repo(server: &MockServer, query: Value) -> Result<ToolData, ProviderError> {
    let provider = one_shot_provider(server);
    let query = serde_json::from_value(query).expect("canonical repository query");
    gh_search_repo::execute(&provider, &query, &default_context()).await
}

pub(crate) async fn run_tree(server: &MockServer, query: Value) -> Result<ToolData, ProviderError> {
    let home = crate::tools::test_support::Fixture::new();
    run_at(server, query, &home.0).await
}

pub(crate) async fn run_at(
    server: &MockServer,
    query: Value,
    home: &std::path::Path,
) -> Result<ToolData, ProviderError> {
    let provider = one_shot_provider(server);
    let query = serde_json::from_value(query).expect("canonical tree query");
    gh_structure::execute(&provider, &query, &default_context(), home).await
}

/// An empty code search result, `incomplete` or not.
pub(crate) async fn empty_code_search(server: &MockServer, incomplete: bool) {
    mount_json(
        server,
        "/api/v3/search/code",
        200,
        json!({"total_count":0,"incomplete_results":incomplete,"items":[]}),
    )
    .await;
}

/// Repository `a/b` now lives at `c/d`.
pub(crate) async fn renamed_to_c_d(server: &MockServer) {
    mount_json(
        server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main","full_name":"c/d"}),
    )
    .await;
}

/// A code search with one `wrap_app` hit in each `src/<name>.rs`.
pub(crate) async fn wrap_app_hits(server: &MockServer, names: &[&str]) {
    let items = names
        .iter()
        .map(|name| {
            json!({"name":format!("{name}.rs"),"path":format!("src/{name}.rs"),"sha":"1",
                    "html_url":"https://x",
                    "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                    "text_matches":[{"fragment":"fn wrap_app() {}",
                        "matches":[{"text":"wrap_app","indices":[3,11]}]}]})
        })
        .collect::<Vec<_>>();
    mount_json(
        server,
        "/api/v3/search/code",
        200,
        json!({
            "total_count":items.len(),"incomplete_results":false,"items":items
        }),
    )
    .await;
}

/// A code search whose one hit is `fn wrap_app() {}` in `app.rs`.
pub(crate) async fn app_rs_hit(server: &MockServer) {
    mount_json(server, "/api/v3/search/code", 200, json!({
        "total_count":1,"incomplete_results":false,"items":[
            {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
             "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
             "text_matches":[{"fragment":"fn wrap_app() {}","matches":[{"text":"wrap_app","indices":[3,11]}]}]}
        ]
    }))
    .await;
}

/// Org `o`'s repositories, most recently pushed first.
pub(crate) async fn recently_pushed_org_repos(server: &MockServer, repos: Value) {
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .and(query_param("sort", "pushed"))
        .and(query_param("direction", "desc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(repos))
        .mount(server)
        .await;
}

pub(crate) const TREE_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Resolve `reference` to [`TREE_SHA`], as GitHub's commits endpoint does.
pub(crate) async fn mount_ref(server: &MockServer, reference: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{reference}")))
        .respond_with(ResponseTemplate::new(200).set_body_string(TREE_SHA))
        .mount(server)
        .await;
}

pub(crate) fn repo_item(name: &str, archived: bool) -> Value {
    json!({
        "full_name": format!("o/{name}"), "name": name,
        "html_url": "https://x", "default_branch": "main",
        "archived": archived
    })
}

/// Search items for `a/b` with one GitHub fragment each (GitHub caps
/// fragments, so routing.py's second call is not in the index text).
pub(crate) async fn mount_code_search(server: &MockServer) {
    mount_json(server, "/api/v3/search/code", 200, json!({
                "total_count":2,"incomplete_results":false,"items":[
                    {"name":"handler.py","path":"src/handler.py","sha":"1","html_url":"https://x",
                     "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                     "text_matches":[{"fragment":"def wrap_app(app):","matches":[{"text":"wrap_app","indices":[4,12]}]}]},
                    {"name":"routing.py","path":"src/routing.py","sha":"2","html_url":"https://x",
                     "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                     "text_matches":[{"fragment":"    await wrap_app(app)(scope)","matches":[{"text":"wrap_app","indices":[10,18]}]}]}
                ]
            })).await;
}

pub(crate) async fn mount_content(server: &MockServer, file: &str, body: &str) {
    Mock::given(method("GET"))
        // The contents route encodes the path as one segment.
        .and(path(format!(
            "/api/v3/repos/a/b/contents/{}",
            file.replace('/', "%2F")
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode(body)
        })))
        .mount(server)
        .await;
}

pub(crate) const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

pub(crate) async fn mount_listing(server: &MockServer, dir: &str, entries: Value) {
    let route = if dir.is_empty() {
        "/api/v3/repos/a/b/contents".to_owned()
    } else {
        format!("/api/v3/repos/a/b/contents/{}", dir.replace('/', "%2F"))
    };
    mount_json(server, route, 200, entries).await;
}

pub(crate) fn code_item(name: &str) -> Value {
    json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
        "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
        "text_matches":[{"fragment":"needle","matches":[{"text":"needle","indices":[0,6]}]}]})
}
