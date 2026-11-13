//! Refs listings: branches and tags paged together, the default branch,
//! one resolved ref, and the leads of a refs page.
use super::*;

/// Branches and tags page together: each page
/// holds the same page number of both lists, and `next.nextPage` continues
/// while either has more, so every ref is listed exactly once. Tags are
/// names (each is a ref) from the repository's whole git tag-ref list,
/// paged in one order; the default branch's head is `commitSha`.
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
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/matching-refs/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"ref":"refs/tags/v1.0.0","object":{"sha":sha(4),"type":"commit"}}
        ])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let first = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "operation": "refs", "limit": 100}),
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
        json!([{"name":"dev","sha":sha(1)},{"name":"main","sha":sha(2)}]),
        "{data}"
    );
    assert_eq!(data["tags"], json!(["v1.0.0"]), "{data}");
    let next = data["next"]["nextPage"]["input"].clone();
    support::assert_cursor_only(&next);
    let second = call(&runtime, "ghStructure", next)
        .await
        .expect("refs page 2");
    let data = row_data(&second);
    assert_eq!(
        data["branches"],
        json!([{"name":"release","sha":sha(3)}]),
        "{data}"
    );
    assert_eq!(data["tags"], json!([]), "{data}");
    assert!(data.get("next").is_none(), "{data}");
    // The fresh first response leaves out the echoes of its query; the
    // page read through a cursor keeps them, because its request names none.
    for echo in ["owner", "repo"] {
        assert!(row_data(&first).get(echo).is_none(), "{echo} echoed");
        assert!(row_data(&second).get(echo).is_some(), "{echo} dropped");
    }
    runtime.close().await;
}

/// A branch name is data, never a JSON key: a branch whose name ends in
/// `-token-refresh` keeps its commit SHA verbatim through the output
/// secret gate, with no redaction (the tester's bgauryy/octocode branch).
#[tokio::test]
async fn a_credential_worded_branch_keeps_its_commit_sha() {
    let server = MockServer::builder().start().await;
    mount_default_branch(&server).await;
    let head = "df5e4353e6dbef3b8a6e70441e7322092dbe110b";
    let name = "execute-plan/f5008669-pr-3-featocto-github-device-login-and-token-refresh";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/branches"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([{"name":name,"commit":{"sha":head}}])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/matching-refs/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let out = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "operation": "refs"}),
    )
    .await
    .expect("refs");
    let data = row_data(&out);
    assert_eq!(
        data["branches"],
        json!([{"name":name,"sha":head}]),
        "{}",
        out.structured_content
    );
    assert!(
        !out.structured_content.to_string().contains("REDACTED"),
        "{}",
        out.structured_content
    );
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
    assert!(data.get("owner").is_none() && data.get("repo").is_none());
    assert_eq!(data["commitSha"], "7".repeat(40));
    assert_eq!(data["refKind"], "tag");
    assert!(
        data.get("next").is_none(),
        "a resolved ref is complete: {data}"
    );
    runtime.close().await;
}

/// In a batch whose rows all fail with the same `notFound`, every row names
/// its own ref and carries its own related[] leads; the generic recovery hint
/// is stated once, on the first row (static notes once).
#[tokio::test]
async fn every_not_found_row_of_a_batch_carries_its_own_recovery_hint() {
    let server = MockServer::builder().start().await;
    mount_default_branch(&server).await;
    Mock::given(method("GET"))
        .and(path_regex(
            "/api/v3/repos/a/b/(commits|git/ref|git/trees)/.*nope-",
        ))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: nope"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let bad = |name: &str| json!({"owner":"a","repo":"b","operation":"tree","ref":name});
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"queries":[bad("nope-1"), bad("nope-2")]}),
    )
    .await
    .expect("batch");
    let rows = outcome.structured_content["results"]
        .as_array()
        .expect("rows");
    assert_eq!(rows.len(), 2);
    for (at, row) in rows.iter().enumerate() {
        assert_eq!(row["data"]["errorCode"], "notFound", "{row}");
        // The row's message names its own ref and where it looked; its
        // related[] leads (the refs listing) are the calls. The generic
        // `notFound` hint (retry with a listed name) is stated once, on row 0.
        let name = format!("nope-{}", at + 1);
        let error = row["data"]["error"].as_str().expect("row error");
        assert!(error.contains(&name), "{row}");
        let retry = |row: &serde_json::Value| {
            row["data"]["hints"].as_array().is_some_and(|hints| {
                hints.iter().any(|hint| {
                    hint.as_str()
                        .is_some_and(|hint| hint.contains("retry with the exact listed"))
                })
            })
        };
        assert_eq!(retry(row), at == 0, "hint once, on the first row: {row}");
        let related = row["data"]["related"].as_array().expect("row related");
        assert!(
            related
                .iter()
                .any(|lead| lead["input"]["queries"][0]["operation"] == "refs"),
            "{row}"
        );
    }
    runtime.close().await;
}
