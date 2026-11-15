//! Refs, languages and missing refs of a renamed repository.
use super::*;

/// A refs page of a renamed repository (GitHub answers 301 to
/// `/repositories/<id>`) warns once and leads under the canonical
/// name, as a listing does.
#[tokio::test]
async fn a_refs_page_of_a_renamed_repository_warns_and_leads_canonically() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/api/v3/repositories/1/commits/HEAD", server.uri()),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main","full_name":"c/d"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/git/matching-refs/tags",
        200,
        json!([{"ref":"refs/tags/v1","object":{"sha":SHA}}]),
    )
    .await;
    mount_json(&server, "/api/v3/repos/a/b/branches", 200, json!([])).await;
    let out = list(&server, json!({"owner":"a","repo":"b","operation":"refs"}))
        .await
        .expect("refs");
    assert_eq!(
        out.data["warnings"],
        json!([crate::tools::gh_shared::renamed_warning(
            "a",
            "b",
            &("c".into(), "d".into())
        )]),
        "{}",
        out.data
    );
    assert_eq!(tag_names(&out.data), ["v1"]);
    assert!(
        out.data.get("next").is_none(),
        "complete refs need no continuation"
    );
}

/// A renamed repository answers `/repos/a/b/...` with a 301 to
/// `/repositories/1/...`; its metadata names `c/d`.
async fn renamed_server(route: &str, status: u16, body: Value) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/{route}")))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/api/v3/repositories/1/{route}", server.uri()),
        ))
        .mount(&server)
        .await;
    mount_json(
        &server,
        format!("/api/v3/repositories/1/{route}"),
        status,
        body,
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main","full_name":"c/d"}),
    )
    .await;
    server
}

/// `operation:"languages"` on a renamed repository carries the
/// rename warning, as its tree and refs rows do.
#[tokio::test]
async fn languages_of_a_renamed_repository_warn_of_the_rename() {
    let server = renamed_server("languages", 200, json!({"Rust": 10})).await;
    let out = list(
        &server,
        json!({"owner":"a","repo":"b","operation":"languages"}),
    )
    .await
    .expect("languages");
    assert_eq!(out.data["languages"], json!({"Rust": 10}), "{}", out.data);
    assert_eq!(
        out.data["warnings"],
        json!([crate::tools::gh_shared::renamed_warning(
            "a",
            "b",
            &("c".into(), "d".into())
        )]),
        "{}",
        out.data
    );
}

/// A ref that does not resolve on a renamed repository names the
/// repository as it is now, in the message and the `viewRefs` lead.
#[tokio::test]
async fn a_missing_ref_of_a_renamed_repository_names_it_canonically() {
    let server = renamed_server(
        "commits/nope",
        422,
        json!({"message":"No commit found for SHA: nope"}),
    )
    .await;
    let home = tempfile::tempdir().expect("home");
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let failure = run(
        &provider,
        &query(json!({"owner":"a","repo":"b","operation":"refs","ref":"nope"})),
        Ok(&context),
        home.path(),
    )
    .await
    .expect_err("missing ref");
    assert!(failure.message.contains("c/d"), "{}", failure.message);
    let lead = &failure.calls.expect("viewRefs")["viewRefs"]["input"]["queries"][0];
    assert_eq!(
        (lead["owner"].as_str(), lead["repo"].as_str()),
        (Some("c"), Some("d")),
        "{lead}"
    );
}

/// Three rows of one call on a renamed repository: the rename is one fact
/// of the call, stated once for it, not once per row.
#[tokio::test]
async fn a_rename_every_row_shares_is_stated_once_for_the_call() {
    let server = renamed_server("languages", 200, json!({"Rust": 10})).await;
    let mut rows = Vec::new();
    for _ in 0..3 {
        let out = list(
            &server,
            json!({"owner":"a","repo":"b","operation":"languages"}),
        )
        .await
        .expect("languages");
        rows.push(out.data);
    }
    let response =
        crate::tools::test_support::staged_response(crate::id::ToolId::GhStructure, rows);
    let warning = crate::tools::gh_shared::renamed_warning("a", "b", &("c".into(), "d".into()));
    assert_eq!(response["warnings"], json!([warning]), "{response}");
    let rows = response["results"].as_array().expect("rows");
    assert_eq!(rows.len(), 3, "{response}");
    for row in rows {
        assert_eq!(row["data"]["languages"], json!({"Rust": 10}), "{response}");
        assert!(row["data"].get("warnings").is_none(), "{response}");
    }
}

/// A listing whose ref GitHub resolved through a rename redirect is read
/// under the canonical name and names the repository as it is now, on the
/// row and in every lead (the file reads), so each runs unchanged.
#[tokio::test]
async fn a_listing_of_a_renamed_repository_keeps_canonical_identity() {
    let server = MockServer::builder().start().await;
    crate::tools::gh_shared::test_support::mount_renamed(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/c/d/contents",
        200,
        json!([
            {"name":"lib.rs","path":"lib.rs","type":"file","size":3},
            {"name":"src","path":"src","type":"dir"}
        ]),
    )
    .await;
    let out = list(&server, json!({"operation":"tree","owner":"a","repo":"b"}))
        .await
        .expect("listing");
    let data = &out.data;
    assert_eq!(
        (data["owner"].as_str(), data["repo"].as_str()),
        (Some("c"), Some("d")),
        "{data}"
    );
    assert_eq!(
        data["warnings"],
        json!([crate::tools::gh_shared::renamed_warning(
            "a",
            "b",
            &("c".into(), "d".into())
        )]),
        "{data}"
    );
    assert!(data.get("related").is_none(), "{data}");
    let read =
        json!({"owner":data["owner"],"repo":data["repo"],"path":"lib.rs","ref":data["commitSha"]});
    crate::contracts::validate_query("ghGetFileContent", read)
        .expect("canonical identity is sufficient for a read");
}
