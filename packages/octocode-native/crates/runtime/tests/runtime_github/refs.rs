//! Refs and repository identity: pinned and bad refs, missing SHAs, renamed and standing repositories.

use crate::support::{self};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use support::{Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Through the whole response stage, a code search pinned to a non-default
/// ref warns with the index commit and keeps its ghStructure lead to the ref
/// first among the leads.
#[tokio::test]
async fn pinned_ref_code_search_warns_and_leads_to_the_ref_listing() {
    let server = MockServer::builder().start().await;
    let at_ref = "0123456789abcdef0123456789abcdef01234567";
    let head = "fedcba9876543210fedcba9876543210fedcba98";
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"base.py","path":"pkg/base.py","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"class Base:","matches":[{"text":"class","indices":[0,5]}]}]}
            ]
        })))
        .mount(&server)
        .await;
    for (reference, sha) in [("dev", at_ref), ("HEAD", head)] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/commits/{reference}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(sha))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/pkg%2Fbase.py"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("class Base:\n    pass\n")
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let out = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"a","repo":"b","keywords":["class"],"path":"pkg","ref":"dev"}),
    )
    .await
    .expect("search");
    let data = row_data(&out);
    // The warning states the per-call facts only; the route is the lead.
    assert_eq!(
        data["warnings"],
        json!(["Files from the default-branch index at fedcba9, not dev; `lines` read at dev."]),
        "{data}"
    );
    let lead = &crate::support::checks::hint(data, "viewRepo");
    assert_eq!(lead["tool"], "ghStructure", "{data}");
    assert_eq!(lead["query"]["queries"][0]["ref"], at_ref, "{data}");
    assert_eq!(lead["query"]["queries"][0]["path"], "pkg", "{data}");
    assert_eq!(
        data["hints"][0]
            .as_str()
            .and_then(|text| text.split_once(": "))
            .map(|(name, _)| name),
        Some("viewRepo"),
        "{data}"
    );
    runtime.close().await;
}

/// A ref that does not resolve is `notFound` (not `invalidInput`) on
/// both GitHub read tools, with one runnable lead to the repository's refs.
#[tokio::test]
async fn gh_bad_ref_is_not_found_with_a_refs_lead() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/no-such-ref"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: no-such-ref"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (tool, query) in [
        (
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"README.md","ref":"no-such-ref","forceRefresh":true}),
        ),
        (
            "ghStructure",
            json!({"owner":"a","repo":"b","ref":"no-such-ref"}),
        ),
    ] {
        let outcome = call(&runtime, tool, query).await.expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{tool}: {data}");
        assert!(
            data["error"]
                .as_str()
                .unwrap_or_default()
                .contains("\"no-such-ref\""),
            "{tool}: {data}"
        );
        let lead = &crate::support::checks::hint(data, "viewRefs");
        assert_eq!(lead["tool"], "ghStructure", "{tool}: {data}");
        let row = &lead["query"]["queries"][0];
        for (field, value) in [("owner", "a"), ("repo", "b"), ("operation", "refs")] {
            assert_eq!(row[field], value, "{tool}: {data}");
        }
    }
    runtime.close().await;
}

/// A full 40-hex SHA that GitHub does not have skips ref resolution, so
/// the listing's tree read is a bare 404. ghStructure must still name the
/// ref and lead to the repository's refs, as ghGetFileContent does, instead
/// of the generic "Repository, resource, or path not found".
#[tokio::test]
async fn gh_missing_full_sha_is_named_with_a_refs_lead() {
    const MISSING: &str = "2bd066d87f5bafd315be9f40889d0a60b9e58e0b";
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{MISSING}")))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":format!("No commit found for SHA: {MISSING}")})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/contents"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"message":format!("No commit found for the ref {MISSING}")})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (tool, query) in [
        (
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"README.md","ref":MISSING,"forceRefresh":true}),
        ),
        ("ghStructure", json!({"owner":"a","repo":"b","ref":MISSING})),
        (
            "ghStructure",
            json!({"owner":"a","repo":"b","ref":MISSING,"path":"src"}),
        ),
    ] {
        let outcome = call(&runtime, tool, query).await.expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{tool}: {data}");
        assert!(
            data["error"].as_str().unwrap_or_default().contains(MISSING),
            "{tool}: {data}"
        );
        assert_eq!(
            crate::support::checks::hint(data, "viewRefs")["tool"],
            "ghStructure",
            "{tool}: {data}"
        );
    }
    runtime.close().await;
}

/// A read of a renamed repository (GitHub answers 301 to
/// `/repositories/<id>`) warns once and names the canonical repository; a
/// later process reading from the immutable cache (no request, so no
/// redirect) still knows the rename from the memo.
#[tokio::test]
async fn gh_file_read_of_a_renamed_repository_names_the_canonical_repository() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for (from, to) in [
        (
            "/api/v3/repos/a/b/commits/main",
            "/api/v3/repositories/1/commits/main",
        ),
        (
            "/api/v3/repos/a/b/contents/README.md",
            "/api/v3/repositories/1/contents/README.md",
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(from))
            .respond_with(
                ResponseTemplate::new(301)
                    .insert_header("location", format!("{}{to}?ref={sha}", server.uri())),
            )
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/contents/README.md"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("hello\n"),
            "size":6,"sha":"f".repeat(40),"path":"README.md"
        })))
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
        .and(path("/api/v3/repos/c/d/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let renamed = |data: &serde_json::Value| {
        let renames = data["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|warning| {
                warning
                    .as_str()
                    .is_some_and(|text| text.contains("renamed"))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            renames,
            [&json!(
                "Repository a/b was renamed to c/d; leads use the canonical name."
            )],
            "one rename warning: {data}"
        );
        assert_eq!(data["owner"], "c", "{data}");
        assert_eq!(data["repo"], "d", "{data}");
    };
    let first = workspace.runtime(&settings);
    let outcome = call(
        &first,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"README.md","ref":"main"}),
    )
    .await
    .expect("read");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    renamed(row_data(&outcome));
    first.close().await;

    let second = workspace.runtime(&settings);
    let outcome = call(
        &second,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"README.md","ref":sha}),
    )
    .await
    .expect("cached read");
    renamed(row_data(&outcome));
    second.close().await;
}

/// A repository GitHub never redirected costs no metadata request.
#[tokio::test]
async fn gh_file_read_of_a_standing_repository_reads_no_metadata() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/README.md"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("hello\n"),
            "size":6,"sha":"f".repeat(40),"path":"README.md"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"README.md","ref":sha}),
    )
    .await
    .expect("read");
    let data = row_data(&outcome);
    checks::assert_absent(data, "renamed");
    runtime.close().await;
}
