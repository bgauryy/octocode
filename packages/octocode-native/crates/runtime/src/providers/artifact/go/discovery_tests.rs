//! Go discovery: remembered tokens, bound cursors, the registry cap and deadline-bounded walks.
use super::super::query::discovery;
use super::super::test_http::RouteHttp;
use super::super::test_http::test_budget;
use super::*;
use serde_json::json;

/// Go pages by opaque token: each served page remembers the token that
/// starts the next, so following `nextPage` reads one page (not every
/// page before it), and a deeper jump walks from the deepest page known.
#[tokio::test]
async fn go_next_pages_start_from_the_remembered_token() {
    let item = |n: usize| json!({"packagePath": format!("example.com/p{n}")});
    let search = |token: Option<&str>| match token {
        None => "/v1/search?q=json&limit=20".to_owned(),
        Some(token) => format!("/v1/search?q=json&limit=20&token={token}"),
    };
    let http = RouteHttp::new()
        .json(
            &search(None),
            json!({"items": [item(1), item(2)], "nextPageToken": "t2", "total": 9}),
        )
        .json(
            &search(Some("t2")),
            json!({"items": [item(3), item(4)], "nextPageToken": "t3", "total": 9}),
        )
        .json(
            &search(Some("t3")),
            json!({"items": [item(5), item(6)], "nextPageToken": "t4", "total": 9}),
        )
        .json(
            &search(Some("t4")),
            json!({"items": [item(7), item(8)], "total": 9}),
        );
    let cache = super::super::ArtifactCache::new(None);
    let b = test_budget();
    let client = RegistryClient {
        cache: Some(&cache),
        ..RegistryClient::uncached(&http, &b)
    };
    let page_of = |page: u64| {
        (
            discovery(ArtifactType::Go, json!({"page": page})),
            ArtifactProviderState {
                page: Some(page),
                ..Default::default()
            },
        )
    };
    let (query, state) = page_of(1);
    let first = go(&query, &state, &client).await.expect("page 1");
    let cursor = first.next_state.and_then(|next| next.token);
    assert_eq!(
        cursor
            .as_deref()
            .and_then(|cursor| go_token(&go_page_key(&query, 2), cursor)),
        Some("t2")
    );
    let seen = http.targets().len();
    let (query, state) = page_of(2);
    let second = go(&query, &state, &client).await.expect("page 2");
    assert_eq!(second.artifacts[0].package_name, "example.com/p3");
    assert_eq!(http.targets()[seen..], [search(Some("t2"))], "one request");
    // Page 4 walks from page 3, the deepest page whose start is known.
    let seen = http.targets().len();
    let (query, state) = page_of(4);
    let fourth = go(&query, &state, &client).await.expect("page 4");
    assert_eq!(fourth.artifacts[0].package_name, "example.com/p7");
    assert_eq!(fourth.next_state, None);
    assert_eq!(
        http.targets()[seen..],
        [search(Some("t3")), search(Some("t4"))]
    );
}

/// pkg.go.dev search pages of two rows (`p1`…`p8`), tokens `t2`…`t4`.
fn go_pages() -> RouteHttp {
    let item = |n: usize| json!({"packagePath": format!("example.com/p{n}")});
    let search = |token: &str| format!("/v1/search?q=json&limit=20&token={token}");
    RouteHttp::new()
        .json(
            "/v1/search?q=json&limit=20",
            json!({"items": [item(1), item(2)], "nextPageToken": "t2", "total": 8}),
        )
        .json(
            &search("t2"),
            json!({"items": [item(3), item(4)], "nextPageToken": "t3", "total": 8}),
        )
        .json(
            &search("t3"),
            json!({"items": [item(5), item(6)], "nextPageToken": "t4", "total": 8}),
        )
        .json(
            &search("t4"),
            json!({"items": [item(7), item(8)], "total": 8}),
        )
}

fn go_page(page: u64, cursor: Option<String>) -> (ArtifactSearchQuery, ArtifactProviderState) {
    (
        discovery(ArtifactType::Go, json!({"page": page})),
        ArtifactProviderState {
            page: Some(page),
            token: cursor,
            ..Default::default()
        },
    )
}

/// pkg.go.dev answers a search with no match as `items: null`: an empty
/// page, not an invalid registry response.
#[tokio::test]
async fn a_go_search_with_no_match_is_an_empty_page() {
    let http = RouteHttp::always(json!({"items": null, "total": 0}));
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let (query, state) = go_page(1, None);
    let page = go(&query, &state, &client).await.expect("empty page");
    assert!(page.artifacts.is_empty());
    assert_eq!(page.total, Some(0));
    assert_eq!(page.next_state, None);
}

/// pkg.go.dev reports any broader match count as its 1,000-result cap:
/// a total at the cap is a lower bound, so it stays unknown.
#[tokio::test]
async fn a_go_total_at_the_registry_cap_is_unknown() {
    for (reported, total) in [(1000, None), (293, Some(293))] {
        let http = RouteHttp::always(json!({"items": [{"packagePath": "example.com/p1"}],
            "nextPageToken": "t2", "total": reported}));
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let (query, state) = go_page(1, None);
        let page = go(&query, &state, &client).await.expect("page");
        assert_eq!(page.total, total, "reported {reported}");
    }
}

/// A deep page without a cursor walks the registry's tokens one request
/// at a time: a deadline mid-walk names the walk and how to reach the
/// page, not registry availability; a single request keeps the plain
/// timeout.
#[tokio::test]
async fn a_go_walk_past_the_deadline_names_the_walk() {
    use super::super::http::{ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest};
    /// The first search page answers; the deadline passes before any
    /// token-started page does.
    struct DeadlineAfterFirst(RouteHttp);
    impl ArtifactHttp for DeadlineAfterFirst {
        fn get<'a>(
            &'a self,
            req: ArtifactHttpRequest,
            budget: &'a octocode_io::budget::RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            if req
                .url
                .query()
                .is_some_and(|query| query.contains("token="))
            {
                return Box::pin(async {
                    Err(ArtifactError::new(
                        ErrorCode::Timeout,
                        "Artifact registry request exceeded its deadline.",
                    ))
                });
            }
            self.0.get(req, budget)
        }
    }
    let http = DeadlineAfterFirst(go_pages());
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let (query, state) = go_page(3, None);
    let error = go(&query, &state, &client).await.expect_err("deadline");
    assert_eq!(error.code, ErrorCode::Timeout);
    assert!(error.message.contains("page 2 of 3"), "{error:?}");
    assert!(
        error.hints.iter().any(|hint| hint.contains("nextPage")),
        "{error:?}"
    );
    // A page read by its own cursor is one request: the plain timeout.
    let cursor = go_cursor(&go_page_key(&query, 3), "t3");
    let (query, state) = go_page(3, Some(cursor));
    let error = go(&query, &state, &client).await.expect_err("deadline");
    assert_eq!(error.code, ErrorCode::Timeout);
    assert!(error.hints.is_empty(), "{error:?}");
}

/// The cursor a page hands its next page is all that page needs: with no
/// registry cache (memory mode, a new daemon, a gap past the cache's
/// lifetime), page 4 is one request, not a walk from page 1.
#[tokio::test]
async fn a_go_cursor_serves_its_page_in_one_request_without_a_cache() {
    let http = go_pages();
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let (query, state) = go_page(3, None);
    let third = go(&query, &state, &client).await.expect("page 3 walks");
    assert_eq!(third.artifacts[0].package_name, "example.com/p5");
    let cursor = third.next_state.and_then(|next| next.token);
    assert!(
        cursor.as_deref().is_some_and(|cursor| cursor != "t4"),
        "{cursor:?}"
    );
    let seen = http.targets().len();
    let (query, state) = go_page(4, cursor);
    let fourth = go(&query, &state, &client).await.expect("page 4");
    assert_eq!(fourth.artifacts[0].package_name, "example.com/p7");
    assert_eq!(
        http.targets()[seen..],
        ["/v1/search?q=json&limit=20&token=t4"],
        "one request"
    );
    assert_eq!(fourth.next_state, None);
}

/// A cursor that is not its page's, or one the registry rejects, is no
/// shortcut: the page walks and serves exactly the rows it owns.
#[tokio::test]
async fn a_foreign_or_rejected_go_cursor_walks_to_the_same_rows() {
    let other = discovery(ArtifactType::Go, json!({"keywords": ["yaml"]}));
    for cursor in [
        // Bound to another query's page 3.
        go_cursor(&go_page_key(&other, 3), "t2"),
        // Bound to page 2 of this query, copied onto page 3.
        go_cursor(&go_page_key(&go_page(2, None).0, 2), "t2"),
        // This page's, but the registry no longer honors it (404).
        go_cursor(&go_page_key(&go_page(3, None).0, 3), "expired"),
        "not-a-cursor".to_owned(),
    ] {
        let http = go_pages();
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let (query, state) = go_page(3, Some(cursor.clone()));
        let third = go(&query, &state, &client).await.expect("page 3");
        let names = third
            .artifacts
            .iter()
            .map(|row| row.package_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["example.com/p5", "example.com/p6"], "{cursor}");
        assert!(
            http.targets()
                .iter()
                .any(|target| target == "/v1/search?q=json&limit=20&token=t2"),
            "walked: {cursor}"
        );
    }
}
