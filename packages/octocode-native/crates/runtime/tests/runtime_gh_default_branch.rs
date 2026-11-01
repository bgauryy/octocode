//! A ref that does not exist: every GitHub tool that takes `ref` names the
//! repository's actual default branch and offers the same call at it. There is
//! no fallback: the row stays an error.

use crate::support::{Workspace, call, checks, row_data};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// `a/b` whose default branch is `default`; `missing` is no ref of it.
async fn repository(default: &str, missing: &str) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"default_branch":default,"full_name":"a/b","private":false,"archived":false}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{missing}")))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":format!("No commit found for SHA: {missing}")})),
        )
        .mount(&server)
        .await;
    server
}

fn calls(missing: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"README.md","ref":missing,"refresh":true}),
        ),
        (
            "ghStructure",
            json!({"operation":"tree","owner":"a","repo":"b","ref":missing,"path":"src"}),
        ),
        (
            "ghStructure",
            json!({"owner":"a","repo":"b","operation":"refs","ref":missing}),
        ),
        (
            "ghGetHistoryItem",
            json!({"operation":"commit","owner":"a","repo":"b","ref":missing}),
        ),
    ]
}

/// The default branch is named in the message, and the lead is the same
/// query at it, whatever the branch is called; `viewRefs` stays.
#[tokio::test]
async fn a_missing_ref_names_the_default_branch_and_leads_to_it() {
    for (default, missing) in [("master", "main"), ("trunk", "develop"), ("main", "master")] {
        let server = repository(default, missing).await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        for (tool, query) in calls(missing) {
            let outcome = call(&runtime, tool, query.clone())
                .await
                .expect("error row");
            let data = row_data(&outcome);
            assert_eq!(data["errorCode"], "notFound", "{tool}: {data}");
            let message = data["error"].as_str().unwrap_or_default();
            assert!(message.contains(&format!("\"{missing}\"")), "{message}");
            assert!(
                message.ends_with(&format!("; the default branch is \"{default}\".")),
                "{tool}: {message}"
            );
            let lead =
                checks::related_call(data, tool, |input| input["queries"][0].get("ref").is_some());
            assert_eq!(lead["tool"], tool, "{data}");
            let mut expected = query.clone();
            expected["ref"] = json!(default);
            let row = &lead["input"]["queries"][0];
            for (field, value) in expected.as_object().expect("object") {
                assert_eq!(&row[field], value, "{tool}.{field}: {data}");
            }
            assert_eq!(
                crate::support::checks::related_call(data, "ghStructure", |input| input["queries"]
                    [0]["operation"]
                    == "refs")["tool"],
                "ghStructure"
            );
        }
        runtime.close().await;
    }
}

/// A commit listing and a clone at a missing ref take the same lead.
#[tokio::test]
async fn history_listing_and_clone_lead_to_the_default_branch() {
    let server = repository("master", "main").await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/commits$"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_STORAGE_MODE", "persistent".into()),
    ]);
    for (tool, query) in [
        (
            "ghSearchHistory",
            json!({"operation":"commit","owner":"a","repo":"b","ref":"main"}),
        ),
        (
            "ghCloneRepo",
            json!({"owner":"a","repo":"b","ref":"main","paths":["src"]}),
        ),
    ] {
        let outcome = call(&runtime, tool, query.clone())
            .await
            .expect("error row");
        let data = row_data(&outcome);
        let message = data["error"].as_str().unwrap_or_default();
        assert!(
            message.ends_with("; the default branch is \"master\"."),
            "{tool}: {data}"
        );
        let lead =
            checks::related_call(data, tool, |input| input["queries"][0].get("ref").is_some());
        assert_eq!(lead["tool"], tool, "{data}");
        let row = &lead["input"]["queries"][0];
        assert_eq!(row["ref"], "master", "{data}");
        assert_eq!(row["owner"], "a", "{data}");
        assert_eq!(
            crate::support::checks::related_call(data, "ghStructure", |input| input["queries"][0]
                ["operation"]
                == "refs")["tool"],
            "ghStructure"
        );
    }
    runtime.close().await;
}

/// Without the repository's metadata, or when the failed ref is the default
/// branch itself, the error is exactly what it was.
#[tokio::test]
async fn without_a_different_default_branch_the_error_is_unchanged() {
    // The default branch is the ref that failed.
    let same = repository("main", "main").await;
    // Metadata unreadable: the repository may be the problem.
    let unreadable = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: main"})),
        )
        .mount(&unreadable)
        .await;
    for server in [same, unreadable] {
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        for (tool, query) in calls("main") {
            let outcome = call(&runtime, tool, query).await.expect("error row");
            let data = row_data(&outcome);
            let message = data["error"].as_str().unwrap_or_default();
            assert!(!message.contains("default branch"), "{tool}: {data}");
            assert!(
                !data["related"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|call| call["input"]["queries"][0].get("ref").is_some()
                        || call["input"]["queries"][0].get("baseRef").is_some()),
                "{data}"
            );
            assert!(
                data["related"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|call| call["tool"] == "ghStructure"
                        && (call["input"]["queries"][0]["operation"] == "refs"))
                    || tool == "ghGetHistoryItem"
            );
        }
        runtime.close().await;
    }
}

/// A comparison GitHub answers with one bare not-found names the side that
/// is missing (each named side is probed) and leads to the same comparison
/// with that side at the default branch. A retry that would compare the
/// default branch with itself is not offered.
#[tokio::test]
async fn a_comparison_names_its_missing_side_and_leads_to_the_default_branch() {
    let server = repository("master", "main").await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/compare/"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    for resolved in ["next", "master"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/commits/{resolved}")))
            .respond_with(ResponseTemplate::new(200).set_body_string("a".repeat(40)))
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let query =
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"main","headRef":"next"});
    let outcome = call(&runtime, "ghGetHistoryItem", query)
        .await
        .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    let message = data["error"].as_str().expect("error message");
    for fact in ["main...next", "a/b", "baseRef", "master"] {
        assert!(message.contains(fact), "missing {fact}: {data}");
    }
    let row = &checks::related_call(data, "ghGetHistoryItem", |input| {
        input["queries"][0].get("baseRef").is_some()
    })["input"]["queries"][0];
    assert_eq!(row["baseRef"], "master", "{data}");
    assert_eq!(row["headRef"], "next", "{data}");
    assert_eq!(
        crate::support::checks::related_call(
            data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "refs"
        )["tool"],
        "ghStructure"
    );

    // The other side already is the default branch: no retry to offer.
    let query =
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"main","headRef":"master"});
    let outcome = call(&runtime, "ghGetHistoryItem", query)
        .await
        .expect("error row");
    let data = row_data(&outcome);
    assert!(
        !data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["input"]["queries"][0].get("ref").is_some()
                || call["input"]["queries"][0].get("baseRef").is_some()),
        "{data}"
    );
    assert!(
        data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "ghStructure"
                && (call["input"]["queries"][0]["operation"] == "refs")),
        "{data}"
    );
    runtime.close().await;
}
