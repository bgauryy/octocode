//! Tree and languages listings: file sizes, paging back to the first page,
//! renamed repositories, and the fields each operation takes.
use super::*;

/// Tree-only fields are rejected on refs and languages (ref also on languages).
#[tokio::test]
async fn refs_and_languages_reject_tree_fields() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", "http://127.0.0.1:9/api/v3".into())]);
    for (operation, field) in [
        ("refs", json!({"path": "src"})),
        ("refs", json!({"materialize": true})),
        ("languages", json!({"ref": "main"})),
    ] {
        let mut query = json!({"owner": "a", "repo": "b", "operation": operation});
        for (key, value) in field.as_object().expect("field") {
            query[key] = value.clone();
        }
        let error = call(&runtime, "ghStructure", query)
            .await
            .expect_err("unsupported operation field");
        assert_eq!(error.code, ErrorCode::InvalidInput);
        let payload = error
            .payload
            .as_deref()
            .expect("actionable validation payload");
        let message = payload["error"].as_str().expect("repair guidance");
        let key = field
            .as_object()
            .expect("field")
            .keys()
            .next()
            .expect("field name");
        assert!(message.contains(key), "{message}");
        assert!(
            crate::support::checks::leads(payload).is_empty(),
            "no guessed repair: {payload}"
        );
    }
    runtime.close().await;
}

/// Languages: bytes of code per language, largest first.
#[tokio::test]
async fn languages_list_bytes_per_language_largest_first() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/languages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"Shell":40,"Rust":9000,"TypeScript":700}"#)
                .insert_header("content-type", "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "operation": "languages"}),
    )
    .await
    .expect("languages");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        serde_json::to_string(&data["languages"]).expect("json"),
        r#"{"Rust":9000,"TypeScript":700,"Shell":40}"#,
        "{data}"
    );
    runtime.close().await;
}

/// A listing states each file's size, in the structureSearch entry
/// form `"<name> (<bytes>)"`; a folder is `"<name>/"` and dates key bare names.
#[tokio::test]
async fn a_listing_states_each_file_size() {
    let server = MockServer::builder().start().await;
    mount_head_at_sha(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name": "small.rs", "path": "small.rs", "type": "file", "size": 10},
            {"name": "big (1).bin", "path": "big (1).bin", "type": "file", "size": 400000},
            {"name": "src", "path": "src", "type": "dir", "size": 0}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b"}),
    )
    .await
    .expect("listing");
    let data = row_data(&outcome);
    assert_eq!(
        undated(&data["files"][0], "files"),
        json!(["big (1).bin (400000)", "small.rs (10)", "src/"]),
        "{data}"
    );
    // The size comes first, then the day: each name is written once.
    let dates = updated(&data["files"][0]);
    assert!(
        dates.contains_key("small.rs")
            && dates.contains_key("big (1).bin")
            && dates.contains_key("src"),
        "{data}"
    );
    assert!(data["files"][0].get("updated").is_none(), "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn caller_supplied_progress_is_rejected_before_provider_dispatch() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    for (operation, field) in [
        ("tree", "page"),
        ("tree", "pageSize"),
        ("tree", "dateOffset"),
        ("tree", "materializeOffset"),
        ("refs", "page"),
    ] {
        let mut query = json!({"operation":operation,"owner":"a","repo":"b"});
        query[field] = json!(2);
        let error = call(&runtime, "ghStructure", query)
            .await
            .expect_err("private progress is issued only through cursors");
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
    runtime.close().await;
}

/// A listing of a renamed repository at a ref (GitHub answers 301 to
/// `/repositories/<id>`) lists the canonical repository after one metadata
/// read, warns once, and leads under the canonical name.
#[tokio::test]
async fn a_renamed_repository_is_listed_and_continued_under_its_canonical_name() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/api/v3/repositories/1/commits/main", server.uri()),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"c/d"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/c/d/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"lib.rs","path":"lib.rs","type":"file","size":10}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b", "ref": "main"}),
    )
    .await
    .expect("listing");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    assert_eq!(
        data["warnings"]
            .to_string()
            .matches("renamed to c/d")
            .count(),
        1,
        "{data}"
    );
    let read = &crate::support::checks::related_call(data, "ghGetFileContent", |_| true)["input"]["queries"]
        [0];
    assert_eq!(
        (read["owner"].as_str(), read["repo"].as_str()),
        (Some("c"), Some("d")),
        "{data}"
    );
    let dates = graphql_requests(&server).await;
    assert_eq!(dates[0]["variables"]["owner"], "c", "{dates:?}");
    runtime.close().await;
}

/// Without a ref, the default-branch lookup already names the
/// canonical repository: no extra metadata request.
#[tokio::test]
async fn a_renamed_repository_without_a_ref_costs_no_extra_metadata_read() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"c/d"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/c/d/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"lib.rs","path":"lib.rs","type":"file","size":10}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b"}),
    )
    .await
    .expect("listing");
    let data = row_data(&outcome);
    assert!(
        data["warnings"].to_string().contains("renamed to c/d"),
        "{data}"
    );
    assert_eq!(
        crate::support::checks::related_call(data, "ghGetFileContent", |_| true)["input"]["queries"]
            [0]["owner"],
        "c",
        "{data}"
    );
    runtime.close().await;
}
