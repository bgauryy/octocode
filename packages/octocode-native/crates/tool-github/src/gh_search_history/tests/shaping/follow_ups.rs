//! The reads and broader searches a page leads to: the best row to read,
//! the issue-links read, and one-filter-at-a-time broadening of an empty
//! search.
use super::*;
use octocode_test_support::json_fixture;

/// A bare issue number in the keywords links straight to that issue's
/// closing pull requests.
#[test]
fn bare_issue_number_keywords_offer_the_issue_links_read() {
    let q = parse(r##"{"operation":"pullRequest","owner":"o","repo":"r","keywords":["#13786"]}"##);
    let read = issue_links_read(&q).expect("bare number");
    assert_eq!(read["input"]["queries"][0]["operation"], "issue");
    assert_eq!(read["input"]["queries"][0]["number"], 13786);
    let words =
        parse(r#"{"operation":"pullRequest","owner":"o","repo":"r","keywords":["fix 13786"]}"#);
    assert!(issue_links_read(&words).is_none());
}

/// commits, pull requests and issues all take `since`/`until`).
#[test]
fn empty_searches_offer_a_broader_search() {
    let broadened = |row: Value| {
        let query: GhSearchHistoryQuery = serde_json::from_value(row).expect("query");
        super::leads::broaden_search(&query).map(|lead| lead["input"]["queries"][0].clone())
    };
    let scoped: GhSearchHistoryQuery = serde_json::from_value(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"],
            "page": 2,
            "filters": {"author": "x"}
        }"#,
    ))
    .expect("query");
    let scoped = super::leads::broaden_search(&scoped).expect("lead")["input"]["queries"].clone();
    // A variant keeps at least one keyword: dropping the only one would
    // list the scope, unrelated to the topic searched.
    assert_eq!(scoped.as_array().map(Vec::len), Some(1), "{scoped}");
    assert!(scoped[0].get("filters").is_none(), "{scoped}");
    assert!(scoped[0].get("page").is_none(), "{scoped}");
    assert_eq!(
        scoped[0]["keywords"],
        json_fixture(r#"["fix"]"#),
        "{scoped}"
    );
    // Every narrowing filter gets its own variant: a scoped keyword search
    // that is empty because of its date window is recoverable too.
    let both: GhSearchHistoryQuery = serde_json::from_value(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"],
            "filters": {"createdAt": {"from": "2030-01-01T00:00:00Z"}}
        }"#,
    ))
    .expect("query");
    let both = super::leads::broaden_search(&both).expect("lead")["input"]["queries"].clone();
    assert_eq!(both.as_array().map(Vec::len), Some(1), "{both}");
    assert!(
        both[0].get("filters").is_none() && both[0]["keywords"] == json_fixture(r#"["fix"]"#),
        "{both}"
    );
    let qualifiers = broadened(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "filters": {"labels": ["bug"]}
        }"#,
    ))
    .expect("qualifiers dropped");
    assert!(qualifiers.get("filters").is_none(), "{qualifiers}");
    let window = broadened(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "path": "src",
            "filters": {"committedAt": {"from": "2026-09-24T00:00:00Z"}}
        }"#,
    ))
    .expect("window dropped");
    assert!(window.get("filters").is_none(), "{window}");
    assert_eq!(window["path"], "src", "{window}");
    for operation in ["pullRequest", "issue"] {
        let window = broadened(json!({"operation":operation,"owner":"o","repo":"r",
            "filters":{"createdAt":{"from":"2026-10-01T00:00:00Z","to":"2026-10-03T23:59:59Z"}}}))
        .expect("window dropped");
        assert!(window.get("filters").is_none(), "{operation}: {window}");
        assert!(window.get("filters").is_none(), "{operation}: {window}");
    }
    assert!(
        broadened(json_fixture(
            r#"{"operation": "commit", "owner": "o", "repo": "r"}"#
        ))
        .is_none()
    );
}

/// An empty search with several keywords offers one drop-one variant
/// per keyword (at most 5, the last keyword dropped first) instead of
/// dropping them all; a lone keyword is never dropped.
#[test]
fn empty_multi_keyword_search_broadens_one_keyword_at_a_time() {
    let variants = |keywords: Value| {
        let query: GhSearchHistoryQuery = serde_json::from_value(json!({"operation":"pullRequest",
            "owner":"o","repo":"r","keywords":keywords}))
        .expect("query");
        super::leads::broaden_search(&query).expect("lead")["input"]["queries"].clone()
    };
    let three = variants(json_fixture(r#"["a", "b", "c"]"#));
    let kept = three
        .as_array()
        .expect("queries")
        .iter()
        .map(|row| row["keywords"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        kept,
        vec![
            json_fixture(r#"["a", "b"]"#),
            json_fixture(r#"["a", "c"]"#),
            json_fixture(r#"["b", "c"]"#)
        ]
    );
    // A variant is a fresh search from its first page.
    assert!(
        three
            .as_array()
            .expect("queries")
            .iter()
            .all(|row| row.get("pageSize").is_none())
    );
    // The only keyword is never dropped: nothing broader is offered.
    let one: GhSearchHistoryQuery = serde_json::from_value(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "keywords": ["a"]
        }"#,
    ))
    .expect("query");
    assert!(super::leads::broaden_search(&one).is_none());
    let seven = variants(json_fixture(r#"["a", "b", "c", "d", "e", "f", "g"]"#));
    assert_eq!(seven.as_array().map(Vec::len), Some(5), "{seven}");
    assert_eq!(
        seven[0]["keywords"],
        json_fixture(r#"["a", "b", "c", "d", "e", "f"]"#)
    );
    assert_eq!(
        seven[4]["keywords"],
        json_fixture(r#"["a", "b", "d", "e", "f", "g"]"#)
    );
}
