//! The GraphQL branches of an item read against a mock GitHub (GraphQL beside
//! REST): a pull request's first page served by GraphQL, a collection GraphQL
//! left incomplete read by REST, a failed fast path falling back to REST, and
//! an issue's closing references.
use super::{HistoryItemRequest, execute};
use crate::security::scan::Passthrough;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use octocode_contract_types::ExtraKind;
use octocode_github::mock_http::{Delayed, mount_json};
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod fallback;
mod pages;
mod references;

const BASE: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";

/// The GraphQL pull request `a/b#7` with the given connections.
fn graphql_pr(connections: Value) -> Value {
    let mut pr = json!({
        "number": 7, "title": "Fix parser", "url": "https://x", "state": "OPEN",
        "body": "Fixes the parser.", "isDraft": false, "author": {"login": "bob"},
        "labels": {"pageInfo": {"hasNextPage": false}, "nodes": []},
        "baseRefName": "main", "baseRefOid": BASE, "headRefName": "feat", "headRefOid": HEAD,
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
        "closedAt": null, "mergedAt": null, "mergeCommit": null,
        "comments": {"totalCount": 1}, "changedFiles": 3, "additions": 3, "deletions": 0,
        "commitsCount": {"totalCount": 1}, "reviewThreads": {"totalCount": 0}
    });
    for (key, value) in connections.as_object().into_iter().flatten() {
        pr[key] = value.clone();
    }
    json!({"data": {"repository": {"pullRequest": pr}}})
}

async fn mount_graphql(server: &MockServer, answer: Value) {
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(server)
        .await;
}

/// The REST pull request itself; `reads` is how often a read may ask.
async fn mount_rest_pr(server: &MockServer, reads: u64) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Fix parser", "state": "open", "body": "Fixes the parser.",
            "user": {"login": "bob"}, "labels": [], "draft": false,
            "base": {"ref": "main", "sha": BASE}, "head": {"ref": "feat", "sha": HEAD},
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z",
            "comments": 1, "review_comments": 0, "commits": 1, "changed_files": 3,
            "additions": 3, "deletions": 0
        })))
        .expect(reads)
        .mount(server)
        .await;
}

async fn read(server: &MockServer, row: Value) -> crate::response::result::ToolData {
    let provider = one_shot_provider(server);
    let request = HistoryItemRequest::from_row(row).expect("valid row");
    execute(&provider, &request, &default_context(), &Passthrough)
        .await
        .expect("history item")
}

fn changed(name: &str) -> Value {
    json!({"filename": name, "status": "added", "additions": 1, "deletions": 0, "changes": 1})
}
