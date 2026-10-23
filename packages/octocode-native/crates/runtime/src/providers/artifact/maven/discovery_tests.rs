//! Maven discovery: term syntax and the rows Central serves from its first window.
use super::super::test_http::RouteHttp;
use super::super::types::{artifact_query, test_budget};
use super::*;
use serde_json::json;

/// Maven Central's Solr search answers HTTP 400 to whitespace-
/// separated terms ("markdown parser"), which surfaced as an
/// `invalidInput` row for a schema-valid query. Words are ANDed, or
/// name the artifactId they spell hyphen-joined (Central indexes
/// `jackson-databind` as one token, so `jackson AND databind` alone
/// misses it); query-syntax characters split words.
#[tokio::test]
async fn maven_discovery_ands_terms_and_splits_query_syntax() {
    for (keywords, expected) in [
        (
            json!(["markdown parser"]),
            "(markdown AND parser) OR a:markdown-parser",
        ),
        (
            json!(["jackson", "databind"]),
            "(jackson AND databind) OR a:jackson-databind",
        ),
        (json!(["a:b (c)"]), "(a AND b AND c) OR a:a-b-c"),
        (json!(["spring-boot"]), "spring-boot"),
        (json!(["guava"]), "guava"),
    ] {
        let http = RouteHttp::always(json!({"response":{"numFound":0,"docs":[]}}));
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let query = artifact_query(
            json!({"ecosystem": ArtifactType::Maven, "keywords": keywords}),
            None,
        );
        maven(&query, &ArtifactProviderState::default(), &client)
            .await
            .expect("maven search");
        let q = http.seen()[0]
            .url
            .query_pairs()
            .find(|(key, _)| key == "q")
            .map(|(_, value)| value.into_owned());
        assert_eq!(q.as_deref(), Some(expected), "{keywords}");
    }
}

/// A Central double for `q=yaml`: 87 matches, rows only from start 0,
/// as many as `rows` asks.
fn yaml_central() -> RouteHttp {
    (1..=super::REACHABLE_END).fold(RouteHttp::new(), |http, rows| {
        let docs = (0..rows.min(87))
            .map(|n| json!({"g": "g", "a": format!("a{n}"), "latestVersion": "1"}))
            .collect::<Vec<_>>();
        http.json(
            &format!("/solrsearch/select?q=yaml&rows={rows}&start=0&wt=json"),
            json!({"response": {"numFound": 87, "docs": docs}}),
        )
    })
}

/// Central serves no row past start 0, yet every match within one
/// answer is reachable: 20-row pages walk all 87 rows, each page sliced
/// from a start-0 answer, then stop.
#[tokio::test]
async fn maven_pages_walk_every_row_central_serves_from_its_first() {
    let http = yaml_central();
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let mut names = Vec::new();
    let mut state = ArtifactProviderState::default();
    for page in 1.. {
        let query = artifact_query(
            json!({"ecosystem": ArtifactType::Maven, "keywords": ["yaml"],
                "pageSize": 20, "page": page}),
            None,
        );
        let served = maven(&query, &state, &client).await.expect("page");
        assert_eq!(served.terminal_limit, None, "page {page}");
        names.extend(served.artifacts.into_iter().map(|row| row.name));
        let Some(next) = served.next_state else {
            break;
        };
        state = next;
    }
    let unique = names.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!((names.len(), unique.len()), (87, 87), "{names:?}");
    assert!(
        http.seen()
            .iter()
            .filter(|seen| seen.url.path() == "/solrsearch/select")
            .all(|seen| seen
                .url
                .query_pairs()
                .any(|(key, value)| key == "start" && value == "0")),
        "every request starts at 0"
    );
}
