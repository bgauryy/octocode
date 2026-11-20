//! The answer of a row whose OSV read failed states only what GitHub's
//! database answered: a read that failed is never "matches none", and a read
//! that stopped part way names its matches as a floor.
use super::*;
use crate::security_check::AdvisoryRead;
use crate::security_check::repository::RepoFailure;
use octocode_contract_types::ErrorCode;

fn rate_limited() -> AdvisoryRead {
    AdvisoryRead::Failed(RepoFailure {
        code: ErrorCode::RateLimited,
        why: "GitHub rate limit reached.".into(),
    })
}

/// The answer's sentence about GitHub's database, after OSV's.
fn github_clause(data: &Value) -> String {
    let answer = data["answer"].as_str().expect("answer");
    let (_, github) = answer
        .split_once("GitHub's advisory database")
        .unwrap_or_else(|| panic!("names GitHub's database: {answer}"));
    github.to_owned()
}

/// OSV and GitHub's database both fail: the answer says GitHub's database
/// was not read, never that it matches none.
#[tokio::test]
async fn a_failed_github_read_is_never_stated_as_matching_none() {
    let http = monorepo_registry().osv_pages(vec![(503, json!({}))]);
    let github = monorepo_github("GHSA-mono-twin-aaaa").global_page(None, rate_limited());
    let data = monorepo_page(&http, &github, None).await;
    let clause = github_clause(&data);
    assert!(!clause.contains("matches"), "{data}");
    assert!(clause.contains("not read"), "{data}");
    assert_eq!(data["isPartial"], true, "{data}");
    octocode_runtime_base::test_support::assert_row_contract(ToolId::SecurityCheck, &data);
}

/// GitHub's database answers one page, then fails: the answer names what was
/// read as a floor of a list that is not whole.
#[tokio::test]
async fn a_github_read_that_stopped_names_its_matches_as_a_floor() {
    let http = monorepo_registry().osv_pages(vec![(503, json!({}))]);
    let github = monorepo_github("GHSA-mono-twin-aaaa")
        .global_page(
            None,
            AdvisoryRead::Page(octocode_github::AdvisoryPage {
                advisories: vec![global_record("GHSA-glob-only-bbbb", json!({}))],
                next: Some("page-2".into()),
            }),
        )
        .global_page(Some("page-2"), rate_limited());
    let data = monorepo_page(&http, &github, None).await;
    let clause = github_clause(&data);
    assert!(clause.contains("GHSA-glob-only-bbbb"), "{data}");
    assert!(clause.contains("at least 1"), "{data}");
    assert!(clause.contains("in part"), "{data}");
    octocode_runtime_base::test_support::assert_row_contract(ToolId::SecurityCheck, &data);
}

/// GitHub's database answers its whole list, empty: the answer says it
/// matches none, which proves only that database.
#[tokio::test]
async fn a_whole_empty_github_read_matches_none() {
    let http = monorepo_registry().osv_pages(vec![(503, json!({}))]);
    let github = monorepo_github("GHSA-mono-twin-aaaa").global_page(
        None,
        AdvisoryRead::Page(octocode_github::AdvisoryPage {
            advisories: Vec::new(),
            next: None,
        }),
    );
    let data = monorepo_page(&http, &github, None).await;
    assert!(github_clause(&data).contains("matches none"), "{data}");
}
