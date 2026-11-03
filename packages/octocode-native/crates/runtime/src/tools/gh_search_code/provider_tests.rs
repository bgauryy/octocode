//! Provider-backed ghSearchCode tests, one child module per behavior, and their shared fixtures.
use super::{GhSearchCodeQuery, execute};
use crate::security::scan::Passthrough;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_github::mock_http::Delayed;
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn paths_of(out: &crate::response::result::ToolData) -> Vec<String> {
    out.data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["path"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// GitHub's answer to one caller page of a code search (page `page` at
/// `limit` a page, listing `names` in `a/b` of `total` matches) as the wide
/// read asks for it: the `MAX_PER_PAGE` page holding that caller page, other
/// files before it. Served once, so mounts in walk order are the successive
/// answers of a search whose results move between page reads.
async fn mount_answer(
    server: &MockServer,
    page: usize,
    limit: usize,
    total: usize,
    names: &[&str],
) {
    let per = octocode_github::MAX_PER_PAGE;
    let offset = (page - 1) * limit;
    let items = (0..offset % per)
        .map(|slot| format!("slot{slot}.rs"))
        .chain(names.iter().map(|name| (*name).to_owned()))
        .map(|name| {
            json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
             "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param("page", (offset / per + 1).to_string()))
        .and(query_param("per_page", per.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":total,"incomplete_results":false,"items":items
        })))
        .up_to_n_times(1)
        .mount(server)
        .await;
}

/// The row `out`'s `next.nextPage` resumes, as the issued cursor replays it
/// (the saved-request path, which alone accepts private progress).
fn next_page(out: &crate::response::result::ToolData) -> GhSearchCodeQuery {
    let input = out.data["next"]["nextPage"]["input"].clone();
    assert!(input.is_object(), "no next.nextPage: {}", out.data);
    let prepared = crate::contracts::validate_built("ghSearchCode", input)
        .expect("the continuation is a valid saved request");
    serde_json::from_value(prepared["queries"][0].clone()).expect("query")
}

mod backed;
mod empty;
mod empty_leads;
mod owner_wide;
mod pages;
mod ref_commits;
mod requests;
mod walk;
mod window;
