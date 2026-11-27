//! Hints by failure kind and the broadening leads of an empty search, read
//! from the serialized result.
use crate::support::{Workspace, call, row_data};
use base64::Engine as _;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn runtime(workspace: &Workspace, server: &MockServer) -> octocode_native::runtime::ToolRuntime {
    workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))])
}

fn related(data: &Value) -> Vec<&Value> {
    data["related"].as_array().into_iter().flatten().collect()
}

/// The static "confirm the owner" caution is said once for a batch of
/// missing repositories, not on every row.
#[tokio::test]
async fn a_batch_of_missing_repositories_states_the_owner_caution_once() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = runtime(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json_fixture(
            r#"{
                "queries": [
                    {"owner": "ghost", "repo": "one", "path": "README.md"},
                    {"owner": "ghost", "repo": "two", "path": "README.md"},
                    {"owner": "ghost", "repo": "three", "path": "README.md"}
                ]
            }"#,
        ),
    )
    .await
    .expect("batch");
    let served = outcome.structured_content.to_string();
    assert_eq!(served.matches("Confirm the owner").count(), 1, "{served}");
    runtime.close().await;
}

/// The owner-spelling check of a batch of missing owners is said once, not
/// on every row.
#[tokio::test]
async fn a_batch_of_missing_owners_states_the_spelling_check_once() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message":"Not Found"}"#)),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = runtime(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghSearchRepo",
        json_fixture(r#"{"queries": [{"operation":"list","owner":"ghost-one"}, {"operation":"list","owner":"ghost-two"}, {"operation":"list","owner":"ghost-three"}]}"#),
    )
    .await
    .expect("batch");
    let served = outcome.structured_content.to_string();
    assert_eq!(
        served.matches("Check the owner spelling").count(),
        1,
        "{served}"
    );
    assert!(served.matches("ghost-").count() >= 3, "{served}");
    runtime.close().await;
}

/// An issue number that is a pull request keeps its pull-request read and
/// gets no path-listing hint.
#[tokio::test]
async fn an_issue_number_that_is_a_pull_request_gets_no_path_hint() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "number": 5,
                "title": "t",
                "state": "open",
                "user": {"login": "x"},
                "pull_request": {"url": "u"},
                "body": "b",
                "labels": []
            }"#,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/issues/5/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = runtime(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json_fixture(
            r#"{
                "operation": "issue",
                "owner": "a",
                "repo": "b",
                "number": 5
            }"#,
        ),
    )
    .await
    .expect("issue row");
    let served = outcome.structured_content.to_string();
    assert!(!served.contains("parent directory"), "{served}");
    let data = row_data(&outcome);
    assert!(
        related(data)
            .iter()
            .any(|lead| lead["tool"] == "ghGetHistoryItem"
                && lead["input"]["queries"][0]["operation"] == "pullRequest"),
        "{data}"
    );
    runtime.close().await;
}

/// An empty search with several filters offers one variant per filter, each
/// without exactly that filter.
#[tokio::test]
async fn an_empty_search_drops_one_filter_per_variant() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"total_count": 0, "incomplete_results": false, "items": []}"#,
        )))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = runtime(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json_fixture(
            r#"{
                "operation": "issue",
                "owner": "a",
                "repo": "b",
                "keywords": ["zzz"],
                "filters": {"state": "open", "author": "x", "labels": ["bug"]}
            }"#,
        ),
    )
    .await
    .expect("empty search");
    let data = row_data(&outcome);
    let variants: Vec<&Value> = related(data)
        .into_iter()
        .flat_map(|lead| lead["input"]["queries"].as_array().into_iter().flatten())
        .filter(|row| row.get("filters").is_some() || row.get("keywords").is_some())
        .collect();
    let all = ["state", "author", "labels"];
    for key in all {
        assert!(
            variants.iter().any(|row| {
                let kept = row["filters"].as_object();
                all.iter().all(|other| {
                    kept.is_some_and(|map| map.contains_key(*other)) == (*other != key)
                })
            }),
            "a variant lacks exactly {key}: {data}"
        );
    }
    runtime.close().await;
}

/// A missed match in a source file leads first to the file's symbols. A read
/// at the default branch then leads to a code search of the repository; a
/// read at a pinned commit gets none, since code search indexes only the
/// default branch.
#[tokio::test]
async fn a_match_miss_in_a_rust_file_leads_to_its_outline_before_a_search() {
    let server = MockServer::builder().start().await;
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "a.rs").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/a.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64",
            "content": base64::engine::general_purpose::STANDARD.encode("fn one() {}\n")
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = runtime(&workspace, &server);
    for reference in [None, Some(SHA)] {
        let mut query = json!({"owner":"a","repo":"b","path":"a.rs","matchString":["absent"]});
        if let Some(reference) = reference {
            query["ref"] = json!(reference);
        }
        let outcome = call(&runtime, "ghGetFileContent", query)
            .await
            .expect("miss");
        let data = row_data(&outcome);
        let leads = related(data);
        let position = |pred: &dyn Fn(&Value) -> bool| leads.iter().position(|lead| pred(lead));
        let symbols = position(&|lead| {
            lead["tool"] == "ghGetFileContent" && lead["input"]["queries"][0]["view"] == "symbols"
        })
        .unwrap_or_else(|| panic!("an outline lead: {data}"));
        let search = position(&|lead| lead["tool"] == "ghSearchCode");
        match reference {
            None => {
                let search = search.unwrap_or_else(|| panic!("a search lead: {data}"));
                assert!(symbols < search, "{data}");
            }
            Some(_) => assert!(search.is_none(), "a pinned read searches no code: {data}"),
        }
    }
    runtime.close().await;
}
