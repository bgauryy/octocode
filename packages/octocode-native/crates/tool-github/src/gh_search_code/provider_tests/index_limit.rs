//! Every page that searched states GitHub's index file-size boundary as a
//! coverage fact: a file the index skips is absent without a trace.
use super::*;
use octocode_test_support::json_fixture;

async fn search(
    query: serde_json::Value,
    incomplete: bool,
    names: &[&str],
) -> octocode_runtime_base::response::result::ToolData {
    let server = MockServer::builder().start().await;
    let items = names
        .iter()
        .map(|name| {
            json!({"name":name,"path":format!("extensions/{name}"),"sha":"1",
             "html_url":format!("https://github.com/a/b/blob/{SHA}/extensions/{name}"),
             "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":items.len(),"incomplete_results":incomplete,"items":items
        })))
        .mount(&server)
        .await;
    for name in names {
        octocode_test_support::mock_http::list_in_trees(
            &server,
            "a",
            "b",
            &format!("extensions/{name}"),
        )
        .await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/extensions%2F{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode("lockfileVersion: yaml\n")
            })))
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(query).expect("query");
    execute(
        &provider,
        &query,
        &default_context(),
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search")
}

/// The fact replaces the old free-text hint: a file-name search, a name
/// fragment, a content search and an empty page all state the same
/// boundary, in bytes; an empty page also states complete coverage, and a
/// page of hits only its boundary.
#[tokio::test]
async fn every_searched_page_states_the_index_limit_as_coverage() {
    for (query, names) in [
        (
            json_fixture(
                r#"{
                    "owner": "a",
                    "repo": "b",
                    "keywords": ["yaml"],
                    "nameContains": "pnpm-lock.yaml"
                }"#,
            ),
            &["pnpm-lock.yaml"][..],
        ),
        (
            json_fixture(
                r#"{
                    "owner": "a",
                    "repo": "b",
                    "keywords": ["yaml"],
                    "nameContains": "lock"
                }"#,
            ),
            &["pnpm-lock.yaml"][..],
        ),
        (
            json_fixture(r#"{"owner": "a", "repo": "b", "keywords": ["yaml"]}"#),
            &["pnpm-lock.yaml"][..],
        ),
        (
            json_fixture(r#"{"owner": "a", "keywords": ["yaml"]}"#),
            &[][..],
        ),
    ] {
        let out = search(query.clone(), false, names).await;
        let coverage = &out.data["coverage"];
        // The mock server is not github.com: an Enterprise Server host.
        assert_eq!(
            coverage["indexLimit"],
            json!({"maxFileBytes": 5 * 1024 * 1024}),
            "{query}: {}",
            out.data
        );
        let hits = !names.is_empty();
        assert_eq!(
            coverage.get("state") == Some(&json!("complete")),
            !hits,
            "{query}: {}",
            out.data
        );
        assert!(
            !out.diagnostics
                .hints
                .iter()
                .any(|hint| hint.contains("384 KB")),
            "{query}: {:?}",
            out.diagnostics.hints
        );
    }
}

/// A page GitHub reports incomplete is partial, and its coverage says so.
#[tokio::test]
async fn an_incomplete_page_states_partial_coverage() {
    let out = search(
        json_fixture(r#"{"owner": "a", "repo": "b", "keywords": ["yaml"]}"#),
        true,
        &["pnpm-lock.yaml"],
    )
    .await;
    assert!(!out.diagnostics.partial_reasons.is_empty(), "{}", out.data);
    assert_eq!(out.data["coverage"]["state"], "partial", "{}", out.data);
}

/// github.com's index skips files over 384 KiB; an Enterprise Server's
/// over 5 MiB.
#[test]
fn the_index_limit_follows_the_host() {
    let endpoint = |base: &str| {
        octocode_github::GitHubEndpoint::new(url::Url::parse(base).expect("url")).expect("endpoint")
    };
    assert_eq!(
        super::super::index_max_file_bytes(&endpoint("https://api.github.com")),
        384 * 1024
    );
    assert_eq!(
        super::super::index_max_file_bytes(&endpoint("https://ghe.example.com/api/v3")),
        5 * 1024 * 1024
    );
    // GHE.com is GitHub's cloud: github.com's index.
    assert_eq!(
        super::super::index_max_file_bytes(&endpoint("https://api.acme.ghe.com")),
        384 * 1024
    );
    // A loopback test server stays a Server.
    assert_eq!(
        super::super::index_max_file_bytes(&endpoint("http://127.0.0.1:8080/api/v3")),
        5 * 1024 * 1024
    );
}

/// A github.com page omits the limit (the tool description states it); an
/// Enterprise Server page states its own.
#[test]
fn coverage_states_the_index_limit_only_where_the_host_differs() {
    let endpoint = |base: &str| {
        octocode_github::GitHubEndpoint::new(url::Url::parse(base).expect("url")).expect("endpoint")
    };
    assert_eq!(
        super::super::page_facts::coverage(false, true, &endpoint("https://api.github.com")),
        Some(json_fixture(r#"{"state": "complete"}"#))
    );
    assert_eq!(
        super::super::page_facts::coverage(false, false, &endpoint("https://api.github.com")),
        None
    );
    assert_eq!(
        super::super::page_facts::coverage(true, true, &endpoint("https://ghe.example.com/api/v3")),
        Some(json!({"state":"partial","indexLimit":{"maxFileBytes":5*1024*1024}}))
    );
}
