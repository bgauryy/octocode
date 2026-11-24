//! A GraphQL connection is reusable only when it carries the selected evidence.
use super::*;
use octocode_test_support::json_fixture;

#[tokio::test]
async fn graphql_files_missing_rename_metadata_use_rest_without_rereading_other_sections() {
    for sections in [
        json_fixture(r#"["files"]"#),
        json_fixture(r#"["files", "comments"]"#),
    ] {
        let server = MockServer::builder().start().await;
        mount_graphql(
            &server,
            graphql_pr(json_fixture(
                r#"{
                    "changedFiles": 1,
                    "files": {
                        "pageInfo": {"hasNextPage": false},
                        "nodes": [
                            {
                                "path": "src/new.rs",
                                "additions": 1,
                                "deletions": 1,
                                "changeType": "RENAMED"
                            }
                        ]
                    },
                    "commentsConn": {
                        "pageInfo": {"hasNextPage": false},
                        "nodes": [
                            {
                                "databaseId": 11,
                                "author": {"login": "amy"},
                                "body": "Keep the old path.",
                                "createdAt": "2026-01-01T00:00:00Z",
                                "url": "https://x"
                            }
                        ]
                    }
                }"#,
            )),
        )
        .await;
        mount_rest_pr(&server, 0).await;
        let file = json_fixture(
            r#"{
                "filename": "src/new.rs",
                "previous_filename": "src/old.rs",
                "status": "renamed",
                "additions": 1,
                "deletions": 1,
                "changes": 2,
                "patch": "@@ -1 +1 @@\n-old\n+new"
            }"#,
        );
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
            json_fixture(r#"["R +1 -1 src/new.rs <- src/old.rs"]"#),
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

/// A renamed file GitHub sent without a patch reads its parent side under
/// the path it had there.
#[tokio::test]
async fn renamed_patch_parent_read_uses_the_old_path() {
    let server = MockServer::builder().start().await;
    mount_rest_pr(&server, 1).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"[
                {
                    "filename": "src/new.rs",
                    "previous_filename": "src/old.rs",
                    "status": "renamed",
                    "additions": 9000,
                    "deletions": 1,
                    "changes": 9001,
                    "sha": "4444444444444444444444444444444444444444"
                }
            ]"#,
        )))
        .mount(&server)
        .await;
    let out = read(
        &server,
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 7,
                "sections": ["patches"]
            }"#,
        ),
    )
    .await;
    let read = &out.data["next"]["readParent"];
    assert_eq!(read["tool"], "ghGetFileContent", "{}", out.data);
    let parent = read["input"]["queries"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["ref"] == BASE)
        .expect("the parent side");
    assert_eq!(parent["path"], "src/old.rs", "{}", out.data);
}
