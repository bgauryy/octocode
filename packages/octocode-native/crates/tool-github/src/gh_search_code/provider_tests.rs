//! Provider-backed ghSearchCode tests, one child module per behavior, and their shared fixtures.
use super::{GhSearchCodeQuery, execute};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_runtime_base::security::scan::Passthrough;
use octocode_test_support::mock_http::Delayed;
use octocode_tools_shared::gh_shared::test_support::{default_context, one_shot_provider};
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{method, path, query_param, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn paths_of(out: &octocode_runtime_base::response::result::ToolData) -> Vec<String> {
    out.data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["path"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// GitHub's answer to one caller page of a code search (page `page` at
/// `limit` a page, listing `names` in `a/b` of `total` matches) as the wide
/// read asks for it: a first page at its `limit`, a later one in the
/// provider page holding it (GitHub's second page at `limit` times the largest
/// power of two no greater than `page - 1`, or a `MAX_PER_PAGE` page once that
/// passes it), other files before it. Served once, so mounts in walk order are the successive
/// answers of a search whose results move between page reads.
async fn mount_answer(
    server: &MockServer,
    page: usize,
    limit: usize,
    total: usize,
    names: &[&str],
) {
    let offset = (page - 1) * limit;
    // A first page asks for itself; a walk reads provider pages that double.
    let largest = octocode_github::MAX_PER_PAGE;
    let (provider_page, per) = match page.checked_sub(1).filter(|before| *before > 0) {
        None => (1, limit),
        Some(before) => match limit * (1 << before.ilog2()) {
            size if size <= largest => (2, size),
            _ => (offset / largest + 1, largest),
        },
    };
    let items = (0..offset - (provider_page - 1) * per)
        .map(|slot| format!("slot{slot}.rs"))
        .chain(names.iter().map(|name| (*name).to_owned()))
        .map(|name| {
            json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
             "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param("page", provider_page.to_string()))
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
fn next_page(out: &octocode_runtime_base::response::result::ToolData) -> GhSearchCodeQuery {
    let input = out.data["next"]["nextPage"]["input"].clone();
    assert!(input.is_object(), "no next.nextPage: {}", out.data);
    let prepared = octocode_runtime_base::contracts::validate_saved("ghSearchCode", input)
        .expect("the continuation is a valid saved request");
    serde_json::from_value(prepared["queries"][0].clone()).expect("query")
}

mod backed;
mod empty;
mod empty_leads;
mod index_limit;
mod oversized;
mod owner_wide;
mod pages;
mod path_scope;
mod ref_commits;
mod repeat_reads;
mod requests;
mod shared_rows;
mod walk;
mod window;
mod withheld;

/// A search the code-search quota refused that names a file also leads to
/// reading that file for the same terms, which spends no search quota; a
/// folder path leads to no read.
#[test]
fn a_quota_refused_search_of_a_file_leads_to_reading_it() {
    use octocode_github::{ProviderError, ProviderErrorKind};
    use octocode_tools_shared::gh_shared::GhFailure;
    let refused = || {
        GhFailure::new(
            ProviderError::new(ProviderErrorKind::RateLimited, "quota"),
            "quota",
        )
    };
    let query = |path: &str| -> GhSearchCodeQuery {
        serde_json::from_value(
            json!({"owner": "o", "repo": "r", "keywords": ["onRequest"], "path": path}),
        )
        .expect("query")
    };
    let failure = super::with_file_read(refused(), &query("docs/Lifecycle.md"));
    let calls = failure.calls.expect("a lead");
    assert_eq!(calls["readFile"]["tool"], "ghGetFileContent", "{calls}");
    let row = &calls["readFile"]["input"]["queries"][0];
    assert_eq!(row["path"], "docs/Lifecycle.md", "{calls}");
    assert_eq!(row["matchString"], json!(["onRequest"]), "{calls}");
    let folder = super::with_file_read(refused(), &query("docs"));
    assert!(folder.calls.is_none(), "{:?}", folder.calls);
}
