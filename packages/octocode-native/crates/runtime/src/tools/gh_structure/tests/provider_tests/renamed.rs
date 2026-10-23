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
    let out = list(
        &server,
        json!({"owner":"a","repo":"b","operation":"refs","extras":[]}),
    )
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
    let lead = &out.data["next"]["viewRefs"]["query"]["queries"][0];
    assert_eq!(
        (lead["owner"].as_str(), lead["repo"].as_str()),
        (Some("c"), Some("d")),
        "{}",
        out.data
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
    let lead = &failure.next.expect("viewRefs")["viewRefs"]["query"]["queries"][0];
    assert_eq!(
        (lead["owner"].as_str(), lead["repo"].as_str()),
        (Some("c"), Some("d")),
        "{lead}"
    );
}
