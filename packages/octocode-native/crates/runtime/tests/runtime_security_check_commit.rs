//! securityCheck `commit` with `repo` through the runtime's real GitHub reader
//! and OSV client, against a fake GitHub and a fake OSV at the HTTP boundary:
//! what each answer reads as, from the reads joined in `repository_read.rs`.

use crate::support::{FIXTURE_LIMITS, Workspace, call, row_data};
use octocode_runtime_base::execution::RuntimeLimits;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "4f0b76e2eca13de1c1fe8b4305abc1f7d63f4b86";

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn server_error() -> ResponseTemplate {
    ResponseTemplate::new(500).set_body_json(json_fixture(r#"{"message": "Server Error"}"#))
}

/// A refusal that is no rate limit: the read is denied, not waited for.
fn denied() -> ResponseTemplate {
    ResponseTemplate::new(403).set_body_json(json_fixture(
        r#"{"message": "Resource not accessible by integration"}"#,
    ))
}

async fn get(server: &MockServer, route: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r{route}")))
        .respond_with(response)
        .mount(server)
        .await;
}

/// A public repository whose commit `SHA` is signed, is no branch head, is in
/// no pull request and is not on the default branch's history; no advisories.
/// Each `(route, response)` of `over` replaces the route's default answer.
async fn github(over: &[(&str, ResponseTemplate)]) -> MockServer {
    let server = MockServer::builder().start().await;
    for (route, response) in over {
        get(&server, route, response.clone()).await;
    }
    let defaults = [
        (
            String::new(),
            ok(json_fixture(
                r#"{
                    "full_name": "o/r",
                    "name": "r",
                    "default_branch": "main",
                    "html_url": "https://github.com/o/r",
                    "private": false,
                    "visibility": "public",
                    "stargazers_count": 5000,
                    "forks_count": 10,
                    "archived": false,
                    "fork": false,
                    "pushed_at": "2999-01-01T00:00:00Z"
                }"#,
            )),
        ),
        (
            format!("/commits/{SHA}"),
            ok(json_fixture(
                r#"{
                    "commit": {"verification": {"verified": true, "reason": "valid"}}
                }"#,
            )),
        ),
        (format!("/commits/{SHA}/branches-where-head"), ok(json!([]))),
        (format!("/commits/{SHA}/pulls"), ok(json!([]))),
        ("/tags".to_owned(), ok(json!([]))),
        (
            format!("/compare/main...{SHA}"),
            ok(json_fixture(r#"{"status": "ahead"}"#)),
        ),
        (
            "/branches".to_owned(),
            ok(json_fixture(
                r#"[{"name": "main", "commit": {"sha": "a1"}}]"#,
            )),
        ),
        ("/security-advisories".to_owned(), ok(json!([]))),
    ];
    for (route, response) in defaults {
        get(&server, &route, response).await;
    }
    server
}

/// OSV knows nothing of the commit.
async fn osv() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ok(json_fixture(r#"{"results": [{}]}"#)))
        .mount(&server)
        .await;
    server
}

/// One `securityCheck` commit row with `repo: o/r`, as the row's data.
async fn read(github: &MockServer) -> Value {
    let osv = osv().await;
    let osv_api: &'static str = Box::leak(format!("{}/v1", osv.uri()).into_boxed_str());
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[
            ("GITHUB_API_URL", format!("{}/api/v3", github.uri())),
            ("OCTOCODE_SECURITY_CHECKS", "true".into()),
        ],
        RuntimeLimits {
            osv_api,
            ..FIXTURE_LIMITS
        },
    );
    let outcome = call(
        &runtime,
        "securityCheck",
        // The whole row: these tests read each source's entry and facts.
        json!({"operation": "commit", "ref": SHA, "owner": "o", "repo": "r", "view": "details"}),
    )
    .await
    .expect("securityCheck row");
    let data = row_data(&outcome).clone();
    runtime.close().await;
    data
}

fn github_entry(data: &Value) -> &Value {
    data["sources"]
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["source"] == "github"))
        .unwrap_or_else(|| panic!("a GitHub entry: {data}"))
}

fn rules(data: &Value) -> Vec<&str> {
    data["cautions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|caution| caution["rule"].as_str())
        .collect()
}

/// A list GitHub did not answer is absent from `facts` and never stated as an
/// absence in the answer; a list it answered empty is.
#[tokio::test]
async fn a_list_github_did_not_answer_is_not_read_never_empty() {
    for (route, field, absence, unread) in [
        ("/tags", "inTag", "tag", "tags not read"),
        (
            &*format!("/commits/{SHA}/branches-where-head"),
            "heads",
            "branch head",
            "branch heads not read",
        ),
        (
            &*format!("/commits/{SHA}/pulls"),
            "pulls",
            "pull request",
            "pull requests not read",
        ),
    ] {
        let server = github(&[(route, server_error())]).await;
        let data = read(&server).await;
        let facts = &github_entry(&data)["facts"];
        assert!(facts.get(field).is_none(), "{field} was not read: {data}");
        let answer = data["answer"].as_str().unwrap_or_default();
        assert!(
            !answer.contains(&format!("no {absence}")) && !answer.contains(&format!(", {absence}")),
            "no absence of a {absence} is stated: {data}"
        );
        assert!(
            answer.contains(unread),
            "the answer names what was not read: {data}"
        );
        assert!(rules(&data).is_empty(), "{data}");
        // A partial page states it once: partialReasons or a floor countScope, no isPartial.
        assert!(data.get("isPartial").is_none(), "{data}");
        assert!(
            !data["partialReasons"].is_null()
                || data["pagination"]["countScope"]
                    .as_str()
                    .is_some_and(|scope| scope != "complete"),
            "{data}"
        );
    }
}

/// The tag list is read once: it gives the tag tips and the tags the other-ref
/// walk compares. A tag whose history holds the commit names it, and no fork
/// caution fires.
#[tokio::test]
async fn the_tag_list_read_once_serves_the_tag_tips_and_the_other_ref_walk() {
    let server = github(&[
        (
            "/branches",
            ok(json_fixture(
                r#"[
                    {"name": "main", "commit": {"sha": "a1"}},
                    {"name": "dev", "commit": {"sha": "b2"}}
                ]"#,
            )),
        ),
        (
            &format!("/compare/b2...{SHA}"),
            ok(json_fixture(r#"{"status": "ahead"}"#)),
        ),
        (
            &format!("/compare/c3...{SHA}"),
            ok(json_fixture(r#"{"status": "behind"}"#)),
        ),
    ])
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/tags"))
        .respond_with(ok(json_fixture(
            r#"[{"name": "v1", "commit": {"sha": "c3"}}]"#,
        )))
        .expect(1)
        .with_priority(1)
        .mount(&server)
        .await;
    let data = read(&server).await;
    let facts = &github_entry(&data)["facts"];
    assert_eq!(facts["inTag"], json_fixture(r#"["v1"]"#), "{data}");
    assert_eq!(facts["inRefsComplete"], true, "{data}");
    assert_eq!(facts["onDefaultBranch"], false, "{data}");
    assert!(rules(&data).is_empty(), "{data}");
    assert!(data.get("isPartial").is_none(), "{data}");
    server.verify().await;
}

/// A branch list GitHub refused leaves the other refs unread: never the fork
/// caution, and the reason is named.
#[tokio::test]
async fn a_refused_branch_list_is_unread_and_draws_no_fork_caution() {
    let server = github(&[("/branches", server_error())]).await;
    let data = read(&server).await;
    assert!(rules(&data).is_empty(), "{data}");
    let why = github_entry(&data)["why"].as_str().unwrap_or_default();
    assert!(
        why.contains("the history of the other branches and tags not read"),
        "{data}"
    );
}

/// A comparison that failed beside a holder keeps the holder, says the list may
/// be short because of that failure, and names the failure.
#[tokio::test]
async fn a_failed_comparison_beside_a_holder_is_named_with_its_reason() {
    let server = github(&[
        (
            "/branches",
            ok(json_fixture(
                r#"[
                    {"name": "main", "commit": {"sha": "a1"}},
                    {"name": "x", "commit": {"sha": "b2"}},
                    {"name": "y", "commit": {"sha": "c3"}}
                ]"#,
            )),
        ),
        (
            &format!("/compare/b2...{SHA}"),
            ok(json_fixture(r#"{"status": "behind"}"#)),
        ),
        (&format!("/compare/c3...{SHA}"), server_error()),
    ])
    .await;
    let data = read(&server).await;
    let facts = &github_entry(&data)["facts"];
    assert_eq!(facts["inBranch"], json_fixture(r#"["x"]"#), "{data}");
    assert_eq!(facts["inRefsComplete"], false, "{data}");
    let answer = data["answer"].as_str().unwrap_or_default();
    assert!(
        !answer.contains("the walk ended"),
        "the reason is the failure: {data}"
    );
    let why = github_entry(&data)["why"].as_str().unwrap_or_default();
    assert!(
        why.contains("not read"),
        "the failed comparison is named: {data}"
    );
    // A partial page states it once: partialReasons or a floor countScope, no isPartial.
    assert!(data.get("isPartial").is_none(), "{data}");
    assert!(
        !data["partialReasons"].is_null()
            || data["pagination"]["countScope"]
                .as_str()
                .is_some_and(|scope| scope != "complete"),
        "{data}"
    );
}

/// Two reads that failed for different reasons each keep their own reason.
#[tokio::test]
async fn each_read_that_failed_keeps_its_own_reason() {
    let server = github(&[
        (
            &format!("/commits/{SHA}/branches-where-head"),
            server_error(),
        ),
        (&format!("/commits/{SHA}/pulls"), denied()),
    ])
    .await;
    let data = read(&server).await;
    let why = github_entry(&data)["why"].as_str().unwrap_or_default();
    assert!(why.contains("branch heads not read"), "{data}");
    assert!(why.contains("pull requests not read"), "{data}");
    assert_eq!(
        why.matches("GitHub did not answer").count(),
        2,
        "each read keeps its own reason: {data}"
    );
    assert!(
        why.contains("permissionDenied"),
        "the second reason is kept: {data}"
    );
}
