//! GitHub reads keep no reusable result: a repeated ghGetFileContent,
//! ghStructure or ghGetHistoryItem call reaches GitHub again, unconditionally,
//! and answers the same evidence with no public cache telemetry.
use super::*;

/// Each read of pinned content asks GitHub for the body, beside the
/// contract's first-page last-change request, and answers identical
/// evidence without public cache telemetry. (The removed `refresh` field
/// has nothing left to bypass: every read is this fresh read.)
#[tokio::test]
async fn ghgetfilecontent_reads_github_on_every_call_without_public_telemetry() {
    let server = MockServer::builder().start().await;
    let sha = sha('a');
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/contents/file.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file", "encoding":"base64", "content":STANDARD.encode("hello cache\n")
        })))
        .expect(2)
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = support::github_api(&server);
    let query = json!({"owner":"o","repo":"r","path":"file.rs","ref":sha});

    let runtime = workspace.runtime(&settings);

    let first = call(&runtime, "ghGetFileContent", query.clone())
        .await
        .unwrap();
    assert_eq!(row_status(&first), "success");
    assert_no_cache_telemetry(&first);

    let second = call(&runtime, "ghGetFileContent", query).await.unwrap();
    assert_eq!(row_status(&second), "success");
    assert_no_cache_telemetry(&second);
    assert_eq!(row_data(&first), row_data(&second));
    server.verify().await;
    assert_eq!(row_data(&second)["content"], "1\thello cache\n");
    // One contract-mandated last-change request per first page.
    let requests = server.received_requests().await.unwrap_or_default();
    let dates = requests
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .count();
    assert_eq!(dates, 2);
    runtime.close().await;
}

/// A repeated `ghStructure` traversal with `maxDepth >= 2` reads the git
/// tree again and lists the same entries.
#[tokio::test]
async fn ghstructure_tree_reads_the_tree_on_every_call() {
    let server = MockServer::builder().start().await;
    let sha = sha('c');

    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/git/trees/{sha}")))
        .and(query_param("recursive", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "truncated": false,
            "tree": [
                {"path":"src","type":"tree","mode":"040000","sha":"aaa"},
                {"path":"src/lib.rs","type":"blob","mode":"100644","sha":"bbb"},
            ]
        })))
        .expect(2)
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = support::github_api(&server);
    let runtime = workspace.runtime(&settings);

    let query = json!({
        "operation": "tree",
        "owner": "o",
        "repo": "r",
        "ref": sha,
        "maxDepth": 2,
    });

    let first = call(&runtime, "ghStructure", query.clone()).await.unwrap();
    assert_eq!(row_status(&first), "success");

    let second = call(&runtime, "ghStructure", query).await.unwrap();
    assert_eq!(row_status(&second), "success");
    assert_no_cache_telemetry(&second);
    assert_eq!(row_data(&first)["files"], row_data(&second)["files"]);
    server.verify().await;
    runtime.close().await;
}

/// `ghGetHistoryItem` reads a mutable item afresh every time: the second
/// read of the same PR is a plain (unconditional) read, never an ETag
/// revalidation, and answers the same item.
#[tokio::test]
async fn ghgethistoryitem_reads_the_item_afresh_without_a_conditional_read() {
    let server = MockServer::builder().start().await;
    // Disable GraphQL so the tool falls back to the REST PR endpoint.
    let pr_body = json!({
        "number": 1,
        "title": "cache test PR",
        "state": "open",
        "html_url": "https://github.com/o/r/pull/1",
        "body": "",
        "draft": false,
        "merged": false,
        "user": {"login": "actor"},
        "base": {"ref": "main", "sha": "0".repeat(40), "repo": {"full_name": "o/r"}},
        "head": {"ref": "feat", "sha": "a".repeat(40)},
        "labels": [],
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "merged_at": null,
        "closed_at": null,
        "additions": 1,
        "deletions": 0,
        "changed_files": 1,
        "commits": 1,
        "comments": 0,
    });
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls/1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_json(pr_body),
        )
        .expect(2)
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = [
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_GITHUB_GRAPHQL", "false".into()),
    ];
    let runtime = workspace.runtime(&settings);

    let query = json!({"operation":"pullRequest","owner":"o","repo":"r","number":1});
    let mut rows = Vec::new();
    for _ in 0..2 {
        let outcome = call(&runtime, "ghGetHistoryItem", query.clone())
            .await
            .unwrap();
        rows.push(row_data(&outcome)["pullRequests"].clone());
        assert_no_cache_telemetry(&outcome);
    }
    assert_eq!(rows[0], rows[1], "the same item");
    let conditional = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.headers.contains_key("if-none-match"))
        .count();
    assert_eq!(conditional, 0, "no read is conditional");
    server.verify().await;
    runtime.close().await;
}
