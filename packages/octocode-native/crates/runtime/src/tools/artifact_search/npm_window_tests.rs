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

fn npm_page(server: &wiremock::MockServer, page: u64) -> ArtifactSearchQuery {
    typed(&json!({"operation":"search",
        "ecosystem": "npm",
        "keywords": ["x"],
        "page": page,
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port())}))
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
    for page in [1, 2, 3] {
        let data = execute(npm_page(&server, page), call(deadline, true))
            .await
            .expect("discovery succeeds");
        let first = (page - 1) * 20;
        assert_eq!(
            data["artifacts"][0]["packageName"],
            format!("p{first}"),
            "{data}"
        );
        assert!(data["next"]["nextPage"].is_object(), "{data}");
        assert!(data.get("terminalLimit").is_none(), "{data}");
    }
    // Past the reach, the window repeats from the first row.
    let data = execute(npm_page(&server, 4), call(deadline, true))
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
    let data = execute(npm_page(&server, 4), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(data["artifacts"], json!([]), "{data}");
    assert_eq!(data["terminalLimit"], true, "{data}");
}

/// A next page the registry cannot serve (its first result past the
/// deepest offset a discovery pages) is no lead either: the last
/// reachable page has no more (`hasMore:false`) and ends the row at the
/// registry's result cap, naming the reach without a hidden field.
#[tokio::test]
async fn a_next_page_past_the_registry_reach_states_a_terminal_limit() {
    let server = npm_window(100_000, 100_000).await;
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let inside = execute(npm_page(&server, 250), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(
        inside["next"]["nextPage"]["input"]["queries"][0]["page"], 251,
        "{inside}"
    );
    let data = execute(npm_page(&server, 251), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(data["artifacts"][0]["packageName"], "p5000", "{data}");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
    assert_eq!(data["pagination"]["hasMore"], false, "{data}");
    // The matched total, not the reach: the warning states the reach.
    assert_eq!(data["pagination"]["totalItems"], 100_000, "{data}");
    assert_eq!(data["terminalLimit"], true, "{data}");
    assert_eq!(
        data["partialReasons"],
        json!(["providerResultCap"]),
        "{data}"
    );
    assert_eq!(
        data["warnings"],
        json!([
            "npm matched 100000 results; paging reaches only the first 5020, which end on this page."
        ]),
        "{data}"
    );
    crate::tools::test_support::validate_sealed(
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
            "total": 29
        })))
        .mount(&server)
        .await;
    let query = json!({"operation":"search",
        "ecosystem": "npm",
        "keywords": ["x"],
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port())});
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let data = execute(typed(&query), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(
        data["pagination"],
        json!({"page": 1, "pageSize": 20, "totalItems": 29, "hasMore": true}),
        "{data}"
    );
    assert_eq!(
        data["next"]["nextPage"]["input"]["queries"][0]["page"], 2,
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
    let data = execute(npm_page(&server, 1), call(deadline, true))
        .await
        .expect("discovery succeeds");
    // The matched total whatever the page size; the notice states the reach.
    assert_eq!(data["pagination"]["totalItems"], 146_295, "{data}");
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
            "npm matched 146295 results; paging reaches only the first 5020 (through page 251)."
        ]),
        "{data}"
    );
    // The response stage places the notice (here: in `warnings`).
    let mut placed = json!({"results": [{"index": 0, "data": data}]});
    crate::response::notices::place(&mut placed, "results");
    assert_eq!(
        placed["results"][0]["data"]["warnings"],
        json!([
            "npm matched 146295 results; paging reaches only the first 5020 (through page 251)."
        ])
    );
    crate::tools::test_support::validate_sealed(ToolId::ArtifactSearch.as_str(), &placed)
        .expect("contract-valid capped page");
}

/// npm ranks rows that match one term of several, so 2+ `keywords` keep only
/// rows holding every term in `name`, `description` or `keywords`. Windows
/// stay npm's: `matching` holds the row positions that hold both terms, each
/// through a different field.
struct NpmTerms {
    total: u64,
    matching: &'static [u64],
}

impl wiremock::Respond for NpmTerms {
    fn respond(&self, request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let param = |name: &str| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<u64>().ok())
        };
        let from = param("from").unwrap_or(0);
        let end = (from + param("size").unwrap_or(20)).min(self.total);
        let objects: Vec<Value> = (from..end)
            .map(|n| {
                let package = if !self.matching.contains(&n) {
                    json!({"name": format!("json-only-{n}"), "description": "just the one term", "version": "1.0.0"})
                } else if n % 3 == 0 {
                    json!({"name": format!("@acme/json-parser-{n}"), "version": "1.0.0"})
                } else if n % 3 == 1 {
                    json!({"name": format!("by-keywords-{n}"), "keywords": ["json", "parser"], "version": "1.0.0"})
                } else {
                    json!({"name": format!("by-text-{n}"), "description": "A JSON parser.", "version": "1.0.0"})
                };
                json!({"package": package})
            })
            .collect();
        wiremock::ResponseTemplate::new(200)
            .set_body_json(json!({"objects": objects, "total": self.total}))
    }
}

async fn npm_terms_page(server: &wiremock::MockServer, page: u64) -> Value {
    let query = typed(&json!({"operation":"search",
        "ecosystem": "npm",
        "keywords": ["json", "parser"],
        "page": page,
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port())}));
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    execute(query, call(deadline, true))
        .await
        .expect("discovery succeeds")
}

async fn npm_terms(total: u64, matching: &'static [u64]) -> wiremock::MockServer {
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(NpmTerms { total, matching })
        .mount(&server)
        .await;
    server
}

/// A window holding no row with every term is not absence: it says more
/// remains, and the walk reaches each match exactly once, whichever of
/// `name`, `description` or `keywords` spells the terms.
#[tokio::test]
async fn npm_keywords_are_anded_and_a_window_without_a_match_is_not_absence() {
    let server = npm_terms(60, &[21, 25, 45]).await;
    let first = npm_terms_page(&server, 1).await;
    assert_eq!(first["artifacts"], json!([]), "{first}");
    assert!(first.get("status").is_none(), "{first}");
    assert_eq!(first["pagination"]["hasMore"], true, "{first}");
    assert!(first["next"]["nextPage"].is_object(), "{first}");
    assert!(first["pagination"].get("totalItems").is_none(), "{first}");
    let mut names = Vec::new();
    for page in 1..=3 {
        let data = npm_terms_page(&server, page).await;
        for row in data["artifacts"].as_array().expect("rows") {
            names.push(row["packageName"].as_str().expect("name").to_owned());
        }
        assert_eq!(data["next"]["nextPage"].is_object(), page < 3, "{data}");
    }
    // Position 21 % 3 == 0: scoped name; 25 % 3 == 1: keywords; 45 % 3 == 0: scoped name.
    assert_eq!(
        names,
        [
            "@acme/json-parser-21",
            "by-keywords-25",
            "@acme/json-parser-45"
        ]
    );
}

/// The walk's last window with no match ends it: that is what empty means.
#[tokio::test]
async fn npm_keywords_with_no_match_left_end_the_walk_as_empty() {
    let server = npm_terms(10, &[]).await;
    let data = npm_terms_page(&server, 1).await;
    assert_eq!(data["artifacts"], json!([]), "{data}");
    assert_eq!(data["status"], "empty", "{data}");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
}

/// A description holding both terms keeps its row, and one keyword is npm's
/// own answer: a lone term filters nothing.
#[tokio::test]
async fn npm_description_matches_and_a_lone_keyword_filters_nothing() {
    let server = npm_terms(3, &[2]).await;
    let both = npm_terms_page(&server, 1).await;
    assert_eq!(both["artifacts"][0]["packageName"], "by-text-2", "{both}");
    let lone = typed(
        &json!({"operation":"search", "ecosystem":"npm", "keywords":["json"],
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port())}),
    );
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let data = execute(lone, call(deadline, true))
        .await
        .expect("discovery");
    assert_eq!(
        data["artifacts"].as_array().expect("rows").len(),
        3,
        "{data}"
    );
}
