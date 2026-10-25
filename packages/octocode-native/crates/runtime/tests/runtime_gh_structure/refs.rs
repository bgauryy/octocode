//! Refs listings: branches and tags paged together, the default branch,
//! one resolved ref, and the leads of a refs page.
use super::*;

/// Branches and tags page together: each page
/// holds the same page number of both lists, and `next.nextPage` continues
/// while either has more, so every ref is listed exactly once. Tags are
/// names (each is a ref); the default branch's head is `commitSha`.
#[tokio::test]
async fn refs_list_every_branch_and_tag_once_with_the_default_branch() {
    let server = MockServer::builder().start().await;
    mount_default_branch(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string("9".repeat(40)))
        .mount(&server)
        .await;
    let sha = |n: u8| format!("{n}").repeat(40);
    let pages = [
        (
            "branches",
            "1",
            json!([{"name":"dev","commit":{"sha":sha(1)}},{"name":"main","commit":{"sha":sha(2)}}]),
            true,
        ),
        (
            "branches",
            "2",
            json!([{"name":"release","commit":{"sha":sha(3)}}]),
            false,
        ),
        (
            "tags",
            "1",
            json!([{"name":"v1.0.0","commit":{"sha":sha(4)}}]),
            false,
        ),
        ("tags", "2", json!([]), false),
    ];
    for (kind, page, body, more) in pages {
        let mut response = ResponseTemplate::new(200).set_body_json(body);
        if more {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/{kind}?per_page=100&page=2>; rel=\"next\"",
                    server.uri()
                ),
            );
        }
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/{kind}")))
            .and(query_param("per_page", "100"))
            .and(query_param("page", page))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let first = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "operation": "refs"}),
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
    assert_eq!(data["defaultBranch"], "main", "{data}");
    assert_eq!(data["commitSha"], sha(9), "{data}");
    assert_eq!(
        data["branches"],
        json!({"dev":sha(1),"main":sha(2)}),
        "{data}"
    );
    assert_eq!(data["tags"], json!(["v1.0.0"]), "{data}");
    let next = data["next"]["nextPage"]["input"].clone();
    support::assert_cursor_only(&next);
    let second = call(&runtime, "ghStructure", next)
        .await
        .expect("refs page 2");
    let data = row_data(&second);
    assert_eq!(data["branches"], json!({"release":sha(3)}), "{data}");
    assert_eq!(data["tags"], json!([]), "{data}");
    assert!(data.get("next").is_none(), "{data}");
    runtime.close().await;
}

/// refs + ref resolves that one ref to its commit and says what kind of ref
/// it is: no tag or branch pages.
#[tokio::test]
async fn refs_with_ref_resolves_one_ref() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/v1.0.0"))
        .respond_with(ResponseTemplate::new(200).set_body_string("7".repeat(40)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/ref/tags/v1.0.0"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"object":{"sha":"7".repeat(40)}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/ref/heads/v1.0.0"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "operation": "refs", "ref": "v1.0.0"}),
    )
    .await
    .expect("one ref");
    let data = row_data(&outcome);
    assert_eq!(data["owner"], "a");
    assert_eq!(data["repo"], "b");
    assert_eq!(data["commitSha"], "7".repeat(40));
    assert_eq!(data["refKind"], "tag");
    assert!(
        data.get("next").is_none(),
        "a resolved ref is complete: {data}"
    );
    runtime.close().await;
}
