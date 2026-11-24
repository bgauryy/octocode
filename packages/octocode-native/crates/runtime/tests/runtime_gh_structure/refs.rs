//! Refs listings: branches and tags paged together, the default branch,
//! one resolved ref, and the leads of a refs page.
use super::*;
use octocode_test_support::json_fixture;

/// Branches and tags page together: each page
/// holds the same page number of both lists, and `next.nextPage` continues
/// while either has more, so every ref is listed exactly once. Tags are
/// names (each is a ref) from the repository's whole git tag-ref list,
/// paged in one order. The listing resolves no ref, so it states no
/// `commitSha`: each branch row carries its own head.
#[tokio::test]
async fn refs_list_every_branch_and_tag_once_with_the_default_branch() {
    let server = MockServer::builder().start().await;
    mount_default_branch(&server).await;
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
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "operation": "refs",
                "limit": 100
            }"#,
        ),
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
    assert!(data.get("commitSha").is_none(), "{data}");
    assert_eq!(data["branches"], json!(["dev", "main"]), "{data}");
    assert_eq!(data["tags"], json!(["v1.0.0"]), "{data}");
    let next = first.structured_content["next"]["input"].clone();
    support::assert_cursor_only(&next);
    let second = call(&runtime, "ghStructure", next)
        .await
        .expect("refs page 2");
    let data = row_data(&second);
    assert_eq!(data["branches"], json!(["release"]), "{data}");
    assert_eq!(data["tags"], json!([]), "{data}");
    assert!(data.get("next").is_none(), "{data}");
    assert!(second.structured_content.get("next").is_none());
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
/// secret gate in its `"name sha"` row (a glob listing: `"*"` is every ref),
/// with no redaction (the tester's bgauryy/octocode branch).
#[tokio::test]
async fn a_credential_worded_branch_keeps_its_commit_sha_in_its_name_sha_row() {
    let server = MockServer::builder().start().await;
    mount_default_branch(&server).await;
    let head = "df5e4353e6dbef3b8a6e70441e7322092dbe110b";
    let name = "execute-plan/f5008669-pr-3-featocto-github-device-login-and-token-refresh";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    // A glob reads its branches as git refs under its literal start.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/matching-refs/heads"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"ref": format!("refs/heads/{name}"), "object": {"sha": head, "type": "commit"}}
        ])))
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
        json_fixture(r#"{"owner": "a", "repo": "b", "operation": "refs", "ref": "*"}"#),
    )
    .await
    .expect("refs");
    let data = row_data(&out);
    assert_eq!(
        data["branches"],
        json!([format!("{name} {head}")]),
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

/// A branch whose name holds a value only the context detector finds
/// (`password=<value>`: no token shape) never shows the value in its
/// `"name sha"` row, on the first listing or on its saved-answer repeat.
#[tokio::test]
async fn a_context_secret_in_a_branch_name_is_never_served_in_its_name_sha_row() {
    use octocode_runtime_base::security::scan::ContentScan as _;
    const VALUE: &str = "Kx9mVb2Qp7Lr4Tz8Wn3";
    // A ref name holds no space: the first of these spellings the detector
    // fires on, by context, not by a token shape.
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let name = [
        format!("password=\"{VALUE}\""),
        format!("ops/password=\"{VALUE}\""),
        format!("db_password=\"{VALUE}\""),
        format!("password={VALUE}"),
    ]
    .into_iter()
    .find(|name| {
        security
            .sanitize(name, std::path::Path::new("branches"))
            .is_ok_and(|(bare, _)| !bare.contains(VALUE))
    })
    .expect("the context detector fires on a ref-name spelling");
    let server = MockServer::builder().start().await;
    mount_default_branch(&server).await;
    let head = "df5e4353e6dbef3b8a6e70441e7322092dbe110b";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/branches"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "private, max-age=60")
                .set_body_json(json!([{"name":name,"commit":{"sha":head}}])),
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
    for round in ["first", "repeat"] {
        let out = call(
            &runtime,
            "ghStructure",
            json_fixture(r#"{"owner": "a", "repo": "b", "operation": "refs"}"#),
        )
        .await
        .expect("refs");
        assert!(
            !out.structured_content.to_string().contains(VALUE),
            "{round}: the value is served: {}",
            out.structured_content
        );
    }
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
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "operation": "refs",
                "ref": "v1.0.0"
            }"#,
        ),
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
/// its own ref; the rows' refs-listing recoveries are one envelope `related`
/// call holding each row's query (hoist K-3), and no row keeps a copy. Each
/// error names the default branch and a recovery call reads it, so no row
/// (and not the envelope) adds the generic `notFound` hint to restate them.
#[tokio::test]
async fn the_not_found_rows_of_a_batch_share_one_recovery_call() {
    let server = MockServer::builder().start().await;
    mount_default_branch(&server).await;
    Mock::given(method("GET"))
        .and(path_regex(
            "/api/v3/repos/a/b/(commits|git/ref|git/trees)/.*nope-",
        ))
        .respond_with(ResponseTemplate::new(422).set_body_json(json_fixture(
            r#"{"message": "No commit found for SHA: nope"}"#,
        )))
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
        // The row's message names its own ref, where it looked and the
        // default branch; its related[] leads (the refs listing, the default
        // branch's tree) are the calls: that is the whole recovery.
        let name = format!("nope-{}", at + 1);
        let error = row["data"]["error"].as_str().expect("row error");
        assert!(error.contains(&name), "{row}");
        assert_eq!(row["data"].get("hints"), None, "no generic hint: {row}");
        let row_refs = row["data"]["related"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|lead| lead["input"]["queries"][0]["operation"] == "refs")
            .count();
        assert_eq!(row_refs, 0, "no row keeps a copy of the shared call: {row}");
    }
    assert_eq!(
        outcome.structured_content.get("hints"),
        None,
        "{}",
        outcome.structured_content
    );
    // The one envelope call holds the rows' recovery reads once each: the
    // repository's refs listing (each row's ref is no ref of it) beside the
    // default branch's tree.
    let shared = outcome.structured_content["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "ghStructure")
        .unwrap_or_else(|| panic!("an envelope refs call: {}", outcome.structured_content));
    let queries = shared["input"]["queries"].as_array().expect("queries");
    assert!(
        queries
            .iter()
            .all(|query| query["owner"] == "a" && query["repo"] == "b"),
        "{shared}"
    );
    assert_eq!(
        queries
            .iter()
            .filter(|query| query["operation"] == "refs")
            .count(),
        1,
        "the refs listing once: {shared}"
    );
    runtime.close().await;
}
