//! A path-scoped commit listing that ends at the commit which renamed the
//! file says so and leads to the older path.
use crate::support::{Workspace, call, row_data, row_status};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const RENAME: &str = "079f91a2396c5c9264f34f95894259b7b15eb89c";
const PARENT: &str = "1111111111111111111111111111111111111111";

fn commit(sha: &str, message: &str) -> Value {
    json!({"sha": sha, "commit": {"message": message,
        "author": {"name": "A", "date": "2020-03-06T20:45:31Z"}},
        "author": {"login": "a"}})
}

async fn serve(server: &MockServer, files: Value) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"full_name": "a/b", "default_branch": "main"}"#,
        )))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .and(query_param("path", "src/core.py"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            commit("2222222222222222222222222222222222222222", "tweak"),
            commit(RENAME, "move to src directory"),
        ])))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{RENAME}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": RENAME,
            "parents": [{"sha": PARENT}],
            "commit": {"message": "move to src directory"},
            "files": files
        })))
        .mount(server)
        .await;
}

async fn list(server: &MockServer) -> Value {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json_fixture(
            r#"{
                "operation": "commit",
                "owner": "a",
                "repo": "b",
                "path": "src/core.py"
            }"#,
        ),
    )
    .await
    .expect("history");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome).clone();
    runtime.close().await;
    data
}

fn related(data: &Value) -> Vec<&Value> {
    data["related"].as_array().into_iter().flatten().collect()
}

#[tokio::test]
async fn a_listing_that_ends_at_a_rename_names_it_and_leads_to_the_old_path() {
    let server = MockServer::builder().start().await;
    serve(
        &server,
        json_fixture(
            r#"[
                {
                    "filename": "src/core.py",
                    "status": "renamed",
                    "previous_filename": "core.py",
                    "changes": 0
                }
            ]"#,
        ),
    )
    .await;
    let data = list(&server).await;
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("core.py") && warnings.contains(&RENAME[..7]),
        "{data}"
    );
    let older = related(&data)
        .into_iter()
        .find(|lead| {
            let query = &lead["input"]["queries"][0];
            lead["tool"] == "ghSearchHistory"
                && query["operation"] == "commit"
                && query["path"] == "core.py"
                && query["ref"] == PARENT
        })
        .unwrap_or_else(|| panic!("a lead to the older path: {data}"));
    assert_eq!(older["input"]["queries"][0]["owner"], "a");
    assert!(
        !related(&data)
            .iter()
            .any(|lead| lead["tool"] == "ghGetHistoryItem"
                && lead["input"]["queries"][0]["ref"] == RENAME),
        "a pure rename has no patch to read: {data}"
    );
}

#[tokio::test]
async fn a_listing_that_ends_at_another_change_adds_nothing() {
    let server = MockServer::builder().start().await;
    serve(
        &server,
        json_fixture(
            r#"[
                {
                    "filename": "src/core.py",
                    "status": "added",
                    "changes": 10
                }
            ]"#,
        ),
    )
    .await;
    let data = list(&server).await;
    assert!(data.get("warnings").is_none(), "{data}");
    assert!(
        !related(&data)
            .iter()
            .any(|lead| lead["input"]["queries"][0]["path"] == "core.py"),
        "{data}"
    );
}
