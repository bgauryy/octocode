//! A GraphQL connection is reusable only when it carries the selected evidence.
use super::*;

#[tokio::test]
async fn graphql_files_missing_rename_metadata_use_rest_without_rereading_other_sections() {
    for sections in [json!(["files"]), json!(["files", "comments"])] {
        let server = MockServer::builder().start().await;
        mount_graphql(
            &server,
            graphql_pr(json!({
                "changedFiles":1,
                "files":{"pageInfo":{"hasNextPage":false},"nodes":[{
                    "path":"src/new.rs","additions":1,"deletions":1,"changeType":"RENAMED"
                }]},
                "commentsConn":{"pageInfo":{"hasNextPage":false},"nodes":[{
                    "databaseId":11,"author":{"login":"amy"},"body":"Keep the old path.",
                    "createdAt":"2026-01-01T00:00:00Z","url":"https://x"
                }]}
            })),
        )
        .await;
        mount_rest_pr(&server, 0).await;
        let file = json!({
            "filename":"src/new.rs","previous_filename":"src/old.rs","status":"renamed",
            "additions":1,"deletions":1,"changes":2,
            "patch":"@@ -1 +1 @@\n-old\n+new"
        });
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/pulls/7/files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([file])))
            .expect(1)
            .mount(&server)
            .await;
        let out = read(
            &server,
            json!({
                "operation":"pullRequest","owner":"a","repo":"b","number":7,
                "sections":sections
            }),
        )
        .await;
        let pr = &out.data["pullRequests"][0];
        assert_eq!(
            pr["files"],
            json!(["R +1 -1 src/new.rs <- src/old.rs"]),
            "{pr}"
        );
        assert_eq!(pr["headCommitSha"], HEAD, "{pr}");
        if sections.as_array().unwrap().len() > 1 {
            assert_eq!(pr["comments"][0]["body"], "Keep the old path.", "{pr}");
        }
        let requests = server.received_requests().await.expect("requests");
        assert_eq!(requests.len(), 2, "{requests:?}");
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.url.path() == "/api/graphql")
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn renamed_patch_parent_read_uses_the_old_path() {
    let server = MockServer::builder().start().await;
    mount_rest_pr(&server, 1).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "filename":"src/new.rs","previous_filename":"src/old.rs","status":"renamed",
            "additions":1,"deletions":1,"changes":2,
            "patch":"@@ -1 +1 @@\n-old\n+new"
        }])))
        .mount(&server)
        .await;
    let out = read(
        &server,
        json!({
            "operation":"pullRequest","owner":"a","repo":"b","number":7,
            "sections":["patches"]
        }),
    )
    .await;
    let read =
        crate::tools::test_support::checks::related_call(&out.data, "ghGetFileContent", |input| {
            input["queries"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["ref"] == BASE))
        });
    let parent = read["input"]["queries"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["ref"] == BASE)
        .expect("the parent side");
    assert_eq!(parent["path"], "src/old.rs", "{}", out.data);
}
