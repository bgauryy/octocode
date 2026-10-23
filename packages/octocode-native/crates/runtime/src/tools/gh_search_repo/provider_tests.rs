//! Provider-backed ghSearchRepo tests: typed filters, archived disclosure,
//! and row facts.
use super::{GhSearchRepoQuery, execute};
use crate::response::result::ToolData;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use octocode_github::mock_http::mount_json;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn run(server: &MockServer, query: Value) -> ToolData {
    let provider = one_shot_provider(server);
    let query: GhSearchRepoQuery = serde_json::from_value(query).expect("query");
    let context = default_context();
    execute(&provider, &query, &context).await.expect("search")
}

fn repo_item(name: &str, archived: bool) -> Value {
    json!({
        "full_name": format!("o/{name}"), "name": name,
        "html_url": "https://x", "default_branch": "main",
        "archived": archived
    })
}

fn page(items: Vec<Value>) -> Value {
    json!({"total_count": items.len(), "incomplete_results": false, "items": items})
}

mod backed;
mod owner_listing;
mod rows;
mod search;
