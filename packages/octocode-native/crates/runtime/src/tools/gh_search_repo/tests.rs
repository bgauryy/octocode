//! Repository rows, empty pages and their broadening, and the missing-owner hint.

/// A missing owner is not told to broaden its keywords: the error names
/// the login, and no search change repairs it.
#[test]
fn a_missing_owner_gets_no_search_fallback_hint() {
    use crate::id::ToolId;
    let query = serde_json::json!({"owner": "no-such-owner"});
    let mut row = serde_json::json!({"status": "error", "data": {
        "errorCode": "notFound",
        "error": "Owner \"no-such-owner\" not found: no GitHub user or organization has this login"
    }});
    crate::response::rows::ensure_recovery(&mut row, ToolId::GhSearchRepo, &query);
    assert!(row["data"].get("hints").is_none(), "{row}");
    // An empty search still gets its fallback.
    let mut empty = serde_json::json!({"status": "empty", "data": {}});
    crate::response::rows::ensure_recovery(&mut empty, ToolId::GhSearchRepo, &query);
    assert!(empty["data"]["hints"][0].is_string(), "{empty}");
}

use super::*;

#[test]
fn zero_repositories_are_empty_and_only_a_complete_search_broadens() {
    let query: GhSearchRepoQuery =
        serde_json::from_value(json!({"keywords":["x"],"language":"rust"})).expect("query");
    let complete = repository_output(json!({"repositories":[]}), &query, true, false, false);
    assert_eq!(complete.status, Some("empty"));
    assert_eq!(
        complete.data["hints"],
        json!(["broadenSearch rows: queries[0] drops language; queries[1] drops keywords"])
    );
    assert_eq!(
        complete.data["next"]["broadenSearch"]["query"]["queries"],
        json!([{"keywords":["x"]}, {"language":"rust"}])
    );

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
    let query: GhSearchRepoQuery =
        serde_json::from_value(json!({"owner":"o","repo":"r","keywords":["x"]})).expect("query");
    let empty = repository_output(json!({"repositories":[]}), &query, true, false, false);
    assert_eq!(
        empty.data["next"]["broadenSearch"]["query"]["queries"],
        json!([{"owner":"o","repo":"r"}]),
        "{}",
        empty.data
    );
    let matched: GhSearchRepoQuery =
        serde_json::from_value(json!({"keywords":["x"],"matchIn":["name"]})).expect("query");
    let empty = repository_output(json!({"repositories":[]}), &matched, true, false, false);
    assert_eq!(
        empty.data["next"]["broadenSearch"]["query"]["queries"],
        json!([{"keywords":["x"]}]),
        "{}",
        empty.data
    );
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
