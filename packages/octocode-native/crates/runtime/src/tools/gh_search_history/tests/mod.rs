//! ghSearchHistory tests: search [`terms`] (building, routing, validation),
//! page [`shaping`], and [`provider`]-backed searches and listings.
use super::filters::Filters;
use super::leads::{issue_links_read, issue_read_query, pr_read_query};
use super::query::{build_query_with_warnings, resolve_commit_window};
use super::rows::{
    is_completed, is_merged, item_number, map_commit, map_commit_list, map_issue, map_pr,
    read_target,
};
use super::shape::mark_empty;
use super::*;
use crate::security::scan::Passthrough;
use crate::tools::gh_shared::history::history_failure;
use crate::tools::gh_shared::pick_fields;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use octocode_github::ProviderError;
use octocode_github::mock_http::mount_json;
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

fn needs_issue_search_qualifiers(q: &GhSearchHistoryQuery) -> bool {
    query::needs_issue_search_qualifiers(&search(q).expect("qualifiers"))
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

mod provider;
mod shaping;
mod terms;

/// An empty search that sent no keywords is not told to broaden them.
#[test]
fn the_empty_fallback_names_keywords_only_when_the_search_had_them() {
    use crate::response::output::ToolOutput;
    let with = Output.fallback_hint(&json!({"keywords": ["fix"], "path": "a"}));
    assert!(with.contains("keywords"), "{with}");
    for query in [
        json!({"operation": "commit", "path": "a"}),
        json!({"keywords": []}),
    ] {
        let hint = Output.fallback_hint(&query);
        assert!(hint.contains("filter"), "{hint}");
        assert!(!hint.contains("Broaden keywords"), "{hint}");
    }
}
