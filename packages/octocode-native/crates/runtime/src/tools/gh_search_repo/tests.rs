//! Repository rows, empty pages and their broadening, and the missing-owner hint.
use super::*;

/// A missing owner is not told to broaden its keywords: the error names
/// the login, and no search change repairs it.

#[test]
fn zero_repositories_are_empty_and_only_a_complete_search_broadens() {
    let query: GhSearchRepoQuery = serde_json::from_value(
        json!({"keywords":["x"],"operation":"search","filters":{"language":"rust"}}),
    )
    .expect("query");
    let complete = repository_output(json!({"repositories":[]}), &query, true, false, false);
    assert_eq!(complete.status, Some("empty"));
    let rows =
        &crate::tools::test_support::checks::related_call(&complete.data, "ghSearchRepo", |_| true)
            ["input"]["queries"];
    assert_eq!(rows.as_array().map(Vec::len), Some(2));
    assert_eq!(rows[0]["keywords"], json!(["x"]));
    assert!(rows[0].get("filters").is_none());
    assert_eq!(rows[1]["filters"]["language"], "rust");
    assert!(rows[1].get("keywords").is_none());
    crate::tools::test_support::checks::assert_leads_valid(&complete.data);

    // GitHub's incomplete page: empty (exit 1), no broadening; its retry
    // is added with the partial marking.
    let incomplete = repository_output(json!({"repositories":[]}), &query, true, true, false);
    assert_eq!(incomplete.status, Some("empty"));
    assert!(incomplete.data.get("next").is_none());
    // A capped result has rows beyond the cap: not empty.
    let capped = repository_output(json!({"repositories":[]}), &query, true, false, true);
    assert_eq!(capped.status, None);
    assert!(capped.data.get("next").is_none());
}

/// An empty search inside one repository with a lone keyword broadens
/// to that repository's own row; each filter drops by its wire name.
#[test]
fn a_lone_keyword_in_one_repository_broadens_to_its_row() {
    let query: GhSearchRepoQuery = serde_json::from_value(
        json!({"owner":"o","repo":"r","keywords":["x"],"operation":"search"}),
    )
    .expect("query");
    let empty = repository_output(json!({"repositories":[]}), &query, true, false, false);
    let row = &crate::tools::test_support::checks::related_call(
        &empty.data,
        "ghSearchRepo",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(row["owner"], "o");
    assert_eq!(row["repo"], "r");
    assert!(row.get("keywords").is_none());
    crate::tools::test_support::checks::assert_leads_valid(&empty.data);
    let matched: GhSearchRepoQuery =
        serde_json::from_value(json!({"keywords":["x"],"matchIn":["name"],"operation":"search"}))
            .expect("query");
    let empty = repository_output(json!({"repositories":[]}), &matched, true, false, false);
    let row = &crate::tools::test_support::checks::related_call(
        &empty.data,
        "ghSearchRepo",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(row["keywords"], json!(["x"]));
    assert!(row.get("matchIn").is_none());
    crate::tools::test_support::checks::assert_leads_valid(&empty.data);
}

/// A topic that only repeats the repository's own name is dropped;
/// every other topic stays.
#[test]
fn topic_naming_the_repository_itself_is_dropped() {
    let item: octocode_github::RepositorySearchItem = serde_json::from_value(json!({
        "full_name":"harlanc/xiu","name":"xiu","html_url":"h","default_branch":"main",
        "topics":["rtmp","XIU","rust"]
    }))
    .expect("item");
    let row = repository_row(item, &[]);
    assert_eq!(row["topics"], json!(["rtmp", "rust"]), "{row}");
}

/// A topic that only repeats the row's `language` is dropped too.
#[test]
fn topic_naming_the_language_is_dropped() {
    let item: octocode_github::RepositorySearchItem = serde_json::from_value(json!({
        "full_name":"psf/requests","name":"requests","html_url":"h","default_branch":"main",
        "language":"Python","topics":["python","http"]
    }))
    .expect("item");
    let row = repository_row(item, &[]);
    assert_eq!(row["topics"], json!(["http"]), "{row}");
    assert_eq!(row["language"], "Python", "{row}");
}
