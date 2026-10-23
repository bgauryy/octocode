//! Happy-path anchors: each operation's row carries the values the next hop
//! reads (`number`, `headSha`/`baseSha`/`mergeCommitSha`, `commitSha`,
//! changed files as path+stat, patch lines numbered on their own side).
use super::{HistoryItemRequest, execute};
use crate::id::ToolId;
use crate::security::scan::Passthrough;
use crate::tools::gh_shared::test_support::{fixture_context, mock_provider};
use crate::tools::test_support::{checks, published};
use octocode_github::RetryPolicy;
use octocode_github::mock_http::mount_json;
use serde_json::{Value, json};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod batches;
mod commit;
mod commit_match;
mod compare;
mod issue;
mod pull_request;

const BASE: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";
const MERGE_BASE: &str = "3333333333333333333333333333333333333333";

async fn read(server: &MockServer, row: Value) -> Value {
    let provider = mock_provider(server, RetryPolicy::default());
    let context = fixture_context(Duration::from_secs(10), 4 << 20);
    let request = HistoryItemRequest::from_row(row).expect("valid row");
    execute(&provider, &request, &context, &Passthrough)
        .await
        .expect("history item")
        .data
}

fn changed_file(name: &str) -> Value {
    json!({"filename":name,"status":"modified","additions":1,"deletions":1,
        "patch":"@@ -10,2 +10,2 @@\n keep\n-old\n+new"})
}

fn commit_raw(sha: &str, message: &str) -> Value {
    json!({"sha":sha,"commit":{"message":message,
        "author":{"name":"Ada","email":"a@x","date":"2024-01-01T00:00:00Z"}},
        "author":{"login":"ada"}})
}

fn assert_history_page_contract(query: &Value, data: &Value) {
    let shaped = crate::response::rows::result_row(
        crate::id::ToolId::GhGetHistoryItem,
        0,
        query,
        data.clone(),
        None,
    );
    crate::contracts::validate_output("ghGetHistoryItem", &json!({"results": [shaped]}))
        .expect("actual normalized history row satisfies the canonical output contract");
}

/// A provider list paged like GitHub: `per_page` items of `items` from
/// `page`, with a `next` link while more follow.
fn paged_like_github(
    base_url: String,
    items: Vec<Value>,
    wrap: fn(Vec<Value>) -> Value,
) -> impl Fn(&wiremock::Request) -> ResponseTemplate {
    move |request: &wiremock::Request| {
        let param = |key: &str| {
            request
                .url
                .query_pairs()
                .find(|(name, _)| name == key)
                .and_then(|(_, value)| value.parse::<usize>().ok())
        };
        let (page, per) = (param("page").unwrap_or(1), param("per_page").unwrap_or(30));
        let start = ((page - 1) * per).min(items.len());
        let end = (start + per).min(items.len());
        let mut response =
            ResponseTemplate::new(200).set_body_json(wrap(items[start..end].to_vec()));
        if end < items.len() {
            response = response.insert_header(
                "Link",
                format!(
                    "<{base_url}{}?per_page={per}&page={}>; rel=\"next\"",
                    request.url.path(),
                    page + 1
                ),
            );
        }
        response
    }
}
