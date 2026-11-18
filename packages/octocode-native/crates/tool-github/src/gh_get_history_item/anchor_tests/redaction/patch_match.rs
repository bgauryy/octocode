//! `matchString` runs on redacted text: patches and discussion bodies alike.
use super::*;
use octocode_test_support::json_fixture;

/// A `matchString` inside a secret finds no file and no comment: matching
/// runs on the redacted patches and bodies, so it never confirms a secret.
#[tokio::test]
async fn a_match_string_inside_a_secret_matches_nothing() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1",
        200,
        json!({"number":1,"title":"Fix","state":"open","user":{"login":"ada"},
            "head":{"sha":HEAD,"ref":"fix"},"base":{"sha":BASE,"ref":"main"},
            "changed_files":1,"additions":1,"deletions":0,
            "created_at":"2024-01-01T00:00:00Z","comments":2}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1/files",
        200,
        json!([{"filename":"src/a.rs","sha":HEAD,"status":"added",
            "additions":1,"deletions":0,
            "patch":format!("@@ -0,0 +1 @@\n+token {}", secret())}]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/1/comments",
        200,
        json!([
            {"id":1,"user":{"login":"a"},"body":format!("token {}", secret()),
                "created_at":"2024-01-01T00:00:00Z"},
            {"id":2,"user":{"login":"b"},"body":"plain note",
                "created_at":"2024-01-01T00:00:00Z"}
        ]),
    )
    .await;
    let inside = &secret()[8..20];
    let out = read_secure(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["patches","comments"],"matchString":[inside]}),
    )
    .await;
    let comments = out["pullRequests"][0]["comments"].as_array();
    assert_eq!(comments.map(Vec::len), Some(0), "{out}");
    let files = out["pullRequests"][0]["files"].as_array();
    assert!(files.is_none_or(Vec::is_empty), "{out}");
    assert!(!leaks(&out.to_string()), "{out}");
    // The redacted marker's neighbour still finds the file's safe patch.
    let out = read_secure(
        &server,
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "o",
                "repo": "r",
                "number": 1,
                "sections": ["patches", "comments"],
                "matchString": ["token"]
            }"#,
        ),
    )
    .await;
    assert_eq!(
        out["pullRequests"][0]["files"].as_array().map(Vec::len),
        Some(1),
        "{out}"
    );
    assert!(
        out["pullRequests"][0]["files"][0]["patch"]
            .as_str()
            .is_some_and(|patch| patch.contains("[REDACTED")),
        "{out}"
    );
    assert!(!leaks(&out.to_string()), "{out}");
    let shown = out["pullRequests"][0]["comments"].to_string();
    assert!(shown.contains("[REDACTED"), "{out}");
    assert!(!leaks(&shown), "{out}");
}
