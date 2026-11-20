//! ghSearchHistory tests: search [`terms`] (building, routing, validation),
//! page [`shaping`], and [`provider`]-backed searches and listings.
use super::leads::{issue_links_read, issue_read_query, pr_read_query};
use super::query::{build_query_with_warnings, resolve_commit_window};
use super::rows::{
    is_completed, is_merged, item_number, map_commit_list, map_issue, map_pr, read_target,
};
use super::shape::mark_empty;
use super::*;
use octocode_github::ProviderError;
use octocode_runtime_base::security::scan::Passthrough;
use octocode_test_support::mock_http::mount_json;
use octocode_tools_shared::gh_shared::history::history_failure;
use octocode_tools_shared::gh_shared::pick_fields;
use octocode_tools_shared::gh_shared::test_support::{default_context, one_shot_provider};
use serde_json::{Value, json};

fn search(q: &GhSearchHistoryQuery) -> Result<HistorySearch, ProviderError> {
    HistorySearch::new(q.clone())
}

fn build_query(q: &GhSearchHistoryQuery) -> Result<String, ProviderError> {
    build_query_with_warnings(&search(q)?).map(|(terms, _)| terms)
}

fn should_use_search_for_prs(q: &GhSearchHistoryQuery) -> bool {
    query::should_use_search_for_prs(&search(q).expect("qualifiers"))
}

async fn execute(
    provider: &octocode_github::GitHubProvider,
    query: &GhSearchHistoryQuery,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Value, ProviderError> {
    super::execute(provider, query.clone(), context, security)
        .await
        .map_err(|failure| *failure.error)
}

fn parse(json: &str) -> GhSearchHistoryQuery {
    serde_json::from_str(json).expect("GitHub history search test data should be valid")
}

mod judged_text;
mod provider;
mod shaping;
mod terms;

/// Redacts `key = "<value>"` only where that context shows, as the real
/// scanner does for a context-named secret; a bare value is invisible to it.
struct ContextOnly;

impl ContentScan for ContextOnly {
    fn sanitize(
        &self,
        text: &str,
        _: &std::path::Path,
    ) -> Result<(String, Vec<String>), (octocode_contract_types::ErrorCode, String)> {
        let clean = text.replace(
            "key = \"c7Tvo+hBKqFYY+kv5ZJr3J1TWDtk\"",
            "key = \"[REDACTED-ENVVARSECRETS]\"",
        );
        Ok((clean, Vec::new()))
    }
}

/// A secret the body names by context is redacted where a title repeats it
/// bare, though no row shows the body: the rows alone would leak it.
#[test]
fn a_secret_named_in_a_body_is_redacted_in_the_title_that_repeats_it() {
    let secret = "c7Tvo+hBKqFYY+kv5ZJr3J1TWDtk";
    let mut items = vec![
        json!({"number": 1, "title": format!("rotate {secret}"), "body": format!("key = \"{secret}\"")}),
        json!({"number": 2, "title": "unrelated", "labels": [{"name": secret}]}),
    ];
    super::rows::sanitize_items(&mut items, &ContextOnly).expect("sanitized");
    let rendered = Value::Array(items.clone()).to_string();
    assert!(!rendered.contains(secret), "{rendered}");
    assert_eq!(items[1]["title"], "unrelated");
    assert_eq!(items[0]["number"], 1);
}

/// A keyword spelled as a qualifier points to the `filters` field that
/// carries it; plain keywords get no hint.
#[test]
fn a_qualifier_keyword_points_to_its_filter() {
    let hints = super::shape::qualifier_keyword_hints(&["author:sharkdp", "parser", "url:x"]);
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].contains("filters.author"), "{hints:?}");
}
