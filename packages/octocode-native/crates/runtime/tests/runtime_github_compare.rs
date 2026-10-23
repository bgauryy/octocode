//! ghGetHistoryItem compare views through the runtime against a mock
//! GitHub: file pages, patch windows, include scopes and both refs pinned.

use serde_json::json;
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod capped;
mod pages;
mod patches;

const BASE_SHA: &str = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const HEAD_SHA: &str = "2222222bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// A 150-commit comparison at GitHub's 300-file cap: commit pages of 100.
async fn mount_capped_compare(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/v2"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HEAD_SHA))
        .mount(server)
        .await;
    let files = (0..300)
        .map(|n| json!({"filename":format!("src/f{n:03}.rs"),"status":"modified","additions":1,"deletions":1,"sha":"3".repeat(40)}))
        .collect::<Vec<_>>();
    for (page, count) in [(1, 100), (2, 50)] {
        let commits = (0..count)
            .map(|n| json!({"sha":format!("{:040x}", page * 1000 + n),"commit":{"message":format!("c{n}\n\nWhy: detail {n}"),"author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}))
            .collect::<Vec<_>>();
        Mock::given(method("GET"))
            .and(path_regex(format!(
                "^/api/v3/repos/a/b/compare/(v1|{BASE_SHA})\\.\\.\\.{HEAD_SHA}$"
            )))
            .and(query_param("page", page.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status":"ahead","ahead_by":150,"behind_by":0,"total_commits":150,
                "permalink_url":"https://github.com/a/b/compare/a:1111111...a:2222222",
                "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":BASE_SHA},
                "commits":commits,"files":files
            })))
            .mount(server)
            .await;
    }
}
