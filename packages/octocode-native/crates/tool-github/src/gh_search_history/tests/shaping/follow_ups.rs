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

/// A capped date-ordered search continues past the window from its last
/// reachable page only: before it, the page walk reaches the same rows.
#[test]
fn a_capped_search_omits_unchanged_narrowing_and_keeps_its_page_walk() {
    let boundary = "2026-04-01T08:00:00Z";
    let last = octocode_github::SEARCH_RESULT_CAP;
    let read = |row: Value, at: usize| {
        let query =
            HistorySearch::new(serde_json::from_value(row).expect("query")).expect("search");
        let page = octocode_github::HistoryPage {
            total_count: octocode_github::SEARCH_RESULT_CAP + 1,
            incomplete_results: false,
            items: vec![json!({
                "number": 1, "title": "boundary result", "state": "closed",
                "created_at": boundary,
            })],
            warnings: Vec::new(),
            listed: false,
            has_more: false,
            list_total: None,
        };
        let paging = super::shape::Paging::new(&page, at, 1);
        super::shape::shape(&query, page, &paging, String::new())
    };
    for operation in ["pullRequest", "issue"] {
        let search = json!({
            "operation": operation, "owner": "o", "repo": "r", "limit": 1,
            "filters": {"state": "closed", "createdAt": {"to": "2026-04-02T00:00:00Z"}},
        });
        let route = json!(super::leads::PastWindow::FromLastRow.warning());
        let states_route = |value: &Value| {
            value["warnings"]
                .as_array()
                .is_some_and(|warnings| warnings.contains(&route))
        };
        let first = read(search.clone(), 1);
        assert!(first["calls"].get("narrowScope").is_none(), "{first}");
        assert!(!states_route(&first), "{first}");
        let end = read(search, last);
        let narrowed = end["calls"]["narrowScope"]["input"]["queries"][0].clone();
        assert_eq!(narrowed["filters"]["createdAt"]["to"], boundary, "{end}");
        assert!(states_route(&end), "{end}");

        let second = read(narrowed, 1);
        assert!(second["calls"].get("narrowScope").is_none(), "{second}");
        assert_eq!(
            second["calls"]["nextPage"]["tool"], "ghSearchHistory",
            "{second}"
        );
        assert_eq!(
            second["partialReasons"],
            json!(["providerResultCap"]),
            "{second}"
        );
    }
}

/// An empty search broadens by its filters alone, one filter dropped per
/// variant: commits, pull requests and issues alike. `path` and `keywords`
/// stay in every variant: a listing without its path answers no question
/// about the path, and the terms' own hint covers the keywords.
#[test]
fn empty_searches_broaden_by_dropping_one_filter_at_a_time() {
    let variants = |row: Value| {
        let query: GhSearchHistoryQuery = serde_json::from_value(row).expect("query");
        super::leads::broaden_search(&query).map(|lead| {
            lead["input"]["queries"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
    };
    let scoped = variants(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"],
            "page": 2,
            "filters": {"author": "x"}
        }"#,
    ))
    .expect("lead");
    assert_eq!(scoped.len(), 1, "{scoped:?}");
    assert!(scoped[0].get("filters").is_none(), "{scoped:?}");
    assert!(scoped[0].get("page").is_none(), "{scoped:?}");
    assert_eq!(scoped[0]["keywords"], json_fixture(r#"["fix"]"#));
    // Each filter gets its own variant, which keeps every other one.
    let two = variants(json!({"operation":"issue","owner":"o","repo":"r",
        "keywords":["a","b"],
        "filters":{"labels":["bug"],"createdAt":{"from":"2030-01-01T00:00:00Z"}}}))
    .expect("lead");
    let mut kept = two
        .iter()
        .map(|row| {
            let filters = row["filters"].as_object().cloned().unwrap_or_default();
            assert_eq!(row["keywords"], json!(["a", "b"]), "{row}");
            filters.keys().cloned().collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    kept.sort();
    assert_eq!(
        kept,
        vec![vec!["createdAt".to_owned()], vec!["labels".to_owned()]]
    );
    let window = variants(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "path": "src",
            "filters": {"committedAt": {"from": "2026-09-24T00:00:00Z"}}
        }"#,
    ))
    .expect("window dropped");
    assert_eq!(window.len(), 1, "{window:?}");
    assert!(window[0].get("filters").is_none(), "{window:?}");
    assert_eq!(window[0]["path"], "src", "{window:?}");
    // Nothing to drop without a filter: a path, a lone keyword or several
    // keywords are never dropped.
    for unfiltered in [
        json_fixture(r#"{"operation": "commit", "owner": "o", "repo": "r"}"#),
        json_fixture(r#"{"operation": "commit", "owner": "o", "repo": "r", "path": "src/a.rs"}"#),
        json_fixture(
            r#"{"operation": "pullRequest", "owner": "o", "repo": "r", "keywords": ["a"]}"#,
        ),
        json_fixture(
            r#"{"operation": "pullRequest", "owner": "o", "repo": "r", "keywords": ["a", "b", "c"]}"#,
        ),
    ] {
        assert_eq!(variants(unfiltered.clone()), None, "{unfiltered}");
    }
}

/// An empty search's hints change the next call and never contradict each
/// other: dropped filters beside their related call, the ANDed terms only
/// when no keyword spells a qualifier (whose own hint names the filter), and
/// nothing about terms when the search has none.
#[test]
fn an_empty_search_hints_only_what_its_premise_holds() {
    let hints = |row: Value| {
        let query =
            HistorySearch::new(serde_json::from_value(row).expect("query")).expect("search");
        let page = octocode_github::HistoryPage {
            total_count: 0,
            incomplete_results: false,
            items: Vec::new(),
            warnings: Vec::new(),
            listed: false,
            has_more: false,
            list_total: None,
        };
        let paging = super::shape::Paging::new(&page, 1, 10);
        let value = super::shape::shape(&query, page, &paging, String::new());
        assert_eq!(value["status"], "empty", "{value}");
        let hints = value["hints"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        // One short sentence each, never explaining what related[] does.
        for hint in &hints {
            assert!(!hint.contains("related"), "{hint}");
            assert!(hint.chars().count() <= 150, "{hint}");
        }
        hints
    };
    assert_eq!(
        hints(json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["zzqx"]})),
        [super::leads::TERMS_ANDED]
    );
    let qualifier =
        hints(json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["head:main"]}));
    assert_eq!(qualifier.len(), 1, "{qualifier:?}");
    assert!(qualifier[0].contains("filters.headRef"), "{qualifier:?}");
    assert_eq!(
        hints(json!({"operation":"issue","owner":"o","repo":"r","filters":{"labels":["bug"]}})),
        [super::leads::DROP_ONE_FILTER]
    );
    assert_eq!(
        hints(
            json!({"operation":"issue","owner":"o","repo":"r","keywords":["zzqx"],
            "filters":{"labels":["bug"]}})
        ),
        [super::leads::DROP_ONE_FILTER, super::leads::TERMS_ANDED]
    );
}
