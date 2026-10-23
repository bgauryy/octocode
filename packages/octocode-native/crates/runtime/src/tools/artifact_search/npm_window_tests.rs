//! npm search windows: restarted windows, terminal limits and page facts.
use super::*;
use crate::id::ToolId;

/// An npm search window: rows `p{n}` from `from` (or from 0 once `from`
/// passes `reachable`, as npm silently restarts a deep window), `size`
/// rows at most, out of `total`.
struct NpmWindow {
    total: u64,
    reachable: u64,
}

impl wiremock::Respond for NpmWindow {
    fn respond(&self, request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let param = |name: &str| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<u64>().ok())
        };
        let from = param("from").unwrap_or(0);
        let from = if from > self.reachable { 0 } else { from };
        let end = (from + param("size").unwrap_or(20)).min(self.total);
        let objects: Vec<Value> = (from..end)
            .map(|n| json!({"package": {"name": format!("p{n}"), "version": "1.0.0"}}))
            .collect();
        wiremock::ResponseTemplate::new(200)
            .set_body_json(json!({"objects": objects, "total": self.total}))
    }
}

async fn npm_window(total: u64, reachable: u64) -> wiremock::MockServer {
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(NpmWindow { total, reachable })
        .mount(&server)
        .await;
    server
}

fn npm_page(server: &wiremock::MockServer, page: u64, page_size: u64) -> ArtifactSearchQuery {
    typed(&json!({
        "ecosystem": "npm",
        "keywords": ["x"],
        "pageSize": page_size,
        "page": page,
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port()),
    }))
}

/// npm restarts a window whose `from` passes its undocumented reach,
/// serving earlier rows as if they were the page. A page of repeated
/// rows lists none of them: it keeps the registry's total and states
/// how many rows are reachable, with no next page.
#[tokio::test]
async fn a_repeated_npm_window_is_a_terminal_limit_with_the_reachable_count() {
    let server = npm_window(100_000, 50).await;
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    // Pages inside the reach are their own rows and continue.
    for page in [1, 3, 6] {
        let data = execute(npm_page(&server, page, 10), call(deadline, true))
            .await
            .expect("discovery succeeds");
        let first = (page - 1) * 10;
        assert_eq!(data["artifacts"][0]["name"], format!("p{first}"), "{data}");
        assert!(data["next"]["nextPage"].is_object(), "{data}");
        assert!(data.get("terminalLimit").is_none(), "{data}");
    }
    // Past the reach, the window repeats from the first row.
    let data = execute(npm_page(&server, 7, 10), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(data["artifacts"], json!([]), "{data}");
    assert_eq!(data["terminalLimit"], true, "{data}");
    assert_eq!(data["pagination"]["totalItems"], 60, "{data}");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
    assert_eq!(
        data["partialReasons"],
        json!(["providerResultCap"]),
        "{data}"
    );
    let warning = data["warnings"][0].as_str().unwrap_or_default();
    assert!(warning.contains("at most 60 of 100000"), "{data}");
    // The boundary page: its probe is still honored, the page is not.
    let server = npm_window(100_000, 59).await;
    let data = execute(npm_page(&server, 7, 10), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(data["artifacts"], json!([]), "{data}");
    assert_eq!(data["terminalLimit"], true, "{data}");
}

/// The last page the contract accepts offers no next page: it states
/// the terminal limit instead of a lead the tool would reject.
#[tokio::test]
async fn the_last_requestable_page_states_a_terminal_limit_not_a_next_page() {
    let server = npm_window(100_000, 100_000).await;
    let query = json!({
        "ecosystem": "npm",
        "keywords": ["x"],
        "pageSize": 1,
        "page": 1000,
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port()),
    });
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let data = execute(typed(&query), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
    assert_eq!(data["pagination"]["hasMore"], false, "{data}");
    assert_eq!(data["terminalLimit"], true, "{data}");
    assert_eq!(
        data["partialReasons"],
        json!([paging::CURSOR_MAXIMUM]),
        "{data}"
    );
    let warning = data["warnings"][0].as_str().unwrap_or_default();
    assert!(warning.contains("page:1000"), "{data}");
}

/// A next page the registry cannot serve (its first result past the
/// deepest offset a discovery pages) is no lead either: the last
/// reachable page has no more (`hasMore:false`) and ends the row at the
/// registry's result cap, naming the reach without a hidden field.
#[tokio::test]
async fn a_next_page_past_the_registry_reach_states_a_terminal_limit() {
    let server = npm_window(100_000, 100_000).await;
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let inside = execute(npm_page(&server, 50, 100), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(
        inside["next"]["nextPage"]["query"]["queries"][0]["page"], 51,
        "{inside}"
    );
    let data = execute(npm_page(&server, 51, 100), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(data["artifacts"][0]["name"], "p5000", "{data}");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
    assert_eq!(data["pagination"]["hasMore"], false, "{data}");
    assert_eq!(data["pagination"]["totalItems"], 5100, "{data}");
    assert_eq!(data["terminalLimit"], true, "{data}");
    assert_eq!(
        data["partialReasons"],
        json!(["providerResultCap"]),
        "{data}"
    );
    assert_eq!(
        data["warnings"],
        json!([
            "npm matched 100000 results; paging reaches only the first 5100, which end on this page."
        ]),
        "{data}"
    );
    crate::contracts::validate_output(
        ToolId::ArtifactSearch.as_str(),
        &json!({"results": [{"index": 0, "data": data}]}),
    )
    .expect("contract-valid terminal page");
}

/// A discovery page states the shared page facts: the page, its size,
/// the registry's total (no page count) and whether more exist.
#[tokio::test]
async fn discovery_pagination_states_the_shared_page_facts() {
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "objects": [{"package": {"name": "a", "version": "1.0.0"}},
                        {"package": {"name": "b", "version": "1.0.0"}}],
            "total": 9
        })))
        .mount(&server)
        .await;
    let query = json!({
        "ecosystem": "npm",
        "keywords": ["x"],
        "pageSize": 2,
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port()),
    });
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let data = execute(typed(&query), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(
        data["pagination"],
        json!({"currentPage": 1, "pageSize": 2, "totalItems": 9, "hasMore": true}),
        "{data}"
    );
    assert_eq!(
        data["next"]["nextPage"]["query"]["queries"][0]["pageSize"], 2,
        "{data}"
    );
    // Every row of this total is reachable: nothing is partial.
    assert!(data.get("isPartial").is_none(), "{data}");
}

/// A registry total past the reachable rows is said once: the total
/// stays the registry's, every continuing page keeps the typed
/// `providerResultCap` marker, and the reach is a static notice (the
/// response says it on the first page only).
#[tokio::test]
async fn a_total_past_the_reach_states_the_reachable_window() {
    let server = npm_window(146_295, 146_295).await;
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let data = execute(npm_page(&server, 1, 100), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(data["pagination"]["totalItems"], 5100, "{data}");
    assert!(data["next"]["nextPage"].is_object(), "{data}");
    assert!(data.get("terminalLimit").is_none(), "{data}");
    assert_eq!(data["isPartial"], true, "{data}");
    assert_eq!(
        data["partialReasons"],
        json!(["providerResultCap"]),
        "{data}"
    );
    assert!(data.get("warnings").is_none(), "{data}");
    assert_eq!(
        data[crate::response::paging::NOTICES],
        json!([
            "npm matched 146295 results; paging reaches only the first 5100 (through page 51)."
        ]),
        "{data}"
    );
    // The response stage places the notice (here: in `warnings`).
    let mut placed = json!({"results": [{"index": 0, "data": data}]});
    crate::response::notices::place(&mut placed, "results");
    assert_eq!(
        placed["results"][0]["data"]["warnings"],
        json!([
            "npm matched 146295 results; paging reaches only the first 5100 (through page 51)."
        ])
    );
    crate::contracts::validate_output(ToolId::ArtifactSearch.as_str(), &placed)
        .expect("contract-valid capped page");
}
