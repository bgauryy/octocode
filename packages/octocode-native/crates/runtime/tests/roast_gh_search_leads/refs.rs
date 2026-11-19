//! ghStructure refs: `limit` counts each list, every row names its commit
//! `commitSha`, and a cursor walk lists each ref once.
use crate::support::{Workspace, call, row_data, row_status};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sha(n: usize) -> String {
    format!("{n:040x}")
}

async fn repository(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json_fixture(r#"{"default_branch": "b0"}"#)),
        )
        .mount(server)
        .await;
    // Ten branches b0..b9 served three per page.
    for page in 1..=4usize {
        let rows: Vec<Value> = (0..10usize)
            .skip((page - 1) * 3)
            .take(3)
            .map(|n| json!({"name": format!("b{n}"), "commit": {"sha": sha(n)}}))
            .collect();
        let mut response = ResponseTemplate::new(200).set_body_json(rows);
        if page < 4 {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/branches?per_page=3&page={}>; rel=\"next\"",
                    server.uri(),
                    page + 1
                ),
            );
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/branches"))
            .and(query_param("page", page.to_string()))
            .respond_with(response)
            .mount(server)
            .await;
    }
    let tags: Vec<Value> = (0..10usize)
        .map(|n| {
            json!({"ref": format!("refs/tags/v1.{n}.0"),
                   "object": {"sha": sha(100 + n), "type": "commit"}})
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/matching-refs/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tags))
        .mount(server)
        .await;
}

fn names(data: &Value, list: &str) -> Vec<String> {
    data[list]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let (name, sha) = row
                .as_str()
                .and_then(|row| row.split_once(' '))
                .unwrap_or_else(|| panic!("a \"name sha\" ref row: {row}"));
            assert!(octocode_github::is_full_sha(sha), "{row}");
            name.to_owned()
        })
        .collect()
}

#[tokio::test]
async fn limit_counts_each_list_and_every_ref_is_listed_once() {
    let server = MockServer::builder().start().await;
    repository(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let first = call(
        &runtime,
        "ghStructure",
        json_fixture(r#"{"owner": "a", "repo": "b", "operation": "refs", "limit": 3}"#),
    )
    .await
    .expect("refs");
    assert_eq!(
        row_status(&first),
        "success",
        "{}",
        first.structured_content
    );
    let data = row_data(&first);
    assert_eq!(names(data, "tags").len(), 3, "{data}");
    assert_eq!(names(data, "branches").len(), 3, "{data}");
    assert_eq!(data["pagination"]["hasMore"], true, "{data}");
    // Each ref is one `"name sha"` string: the name, a space, its commit.
    for row in data["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(data["branches"].as_array().into_iter().flatten())
    {
        let (_, sha) = row
            .as_str()
            .and_then(|row| row.split_once(' '))
            .unwrap_or_else(|| panic!("{row}"));
        assert!(octocode_github::is_full_sha(sha), "{row}");
    }
    let (mut tags, mut branches) = (names(data, "tags"), names(data, "branches"));
    let mut next = data["next"]["nextPage"]["input"].clone();
    while !next.is_null() {
        let page = call(&runtime, "ghStructure", next)
            .await
            .expect("next page");
        let data = row_data(&page);
        tags.extend(names(data, "tags"));
        branches.extend(names(data, "branches"));
        next = data["next"]["nextPage"]["input"].clone();
    }
    tags.sort();
    branches.sort();
    let unique = |list: &Vec<String>| {
        let mut once = list.clone();
        once.dedup();
        once.len()
    };
    assert_eq!((tags.len(), unique(&tags)), (10, 10), "{tags:?}");
    assert_eq!(
        (branches.len(), unique(&branches)),
        (10, 10),
        "{branches:?}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_tags_only_listing_states_only_tag_facts() {
    let server = MockServer::builder().start().await;
    repository(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let out = call(
        &runtime,
        "ghStructure",
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "operation": "refs",
                "limit": 3,
                "refKinds": ["tags"]
            }"#,
        ),
    )
    .await
    .expect("refs");
    let data = row_data(&out);
    assert_eq!(names(data, "tags").len(), 3, "{data}");
    assert!(data.get("branches").is_none(), "{data}");
    assert_eq!(data["pagination"]["hasMore"], true, "{data}");
    runtime.close().await;
}
