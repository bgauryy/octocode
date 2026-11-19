//! `bots:false` leaves out the discussion entries a bot or app wrote, says
//! how many, and puts the conversation before the inline review comments.
use super::*;
use octocode_test_support::json_fixture;

async fn mount_pr(server: &MockServer) {
    mount_json(
        server,
        "/api/v3/repos/o/r/pulls/1",
        200,
        json!({"number":1,"title":"Fix","state":"open","user":{"login":"ada"},
            "head":{"sha":HEAD,"ref":"fix"},"base":{"sha":BASE,"ref":"main"},
            "changed_files":0,"comments":2,"review_comments":2,
            "additions":1,"deletions":1,"created_at":"2024-01-01T00:00:00Z"}),
    )
    .await;
}

fn comment(id: u64, login: &str, kind: &str) -> Value {
    json!({"id":id,"user":{"login":login,"type":kind},"body":format!("from {login}"),
        "created_at":"2024-01-01T00:00:00Z"})
}

fn inline(id: u64, login: &str, kind: &str) -> Value {
    let mut comment = comment(id, login, kind);
    comment["path"] = json!("src/a.rs");
    comment["line"] = json!(3);
    comment["side"] = json!("RIGHT");
    comment["commit_id"] = json!(HEAD);
    comment
}

fn authors(row: &Value, section: &str) -> Vec<String> {
    row[section]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| entry["author"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[tokio::test]
async fn bots_false_leaves_bot_entries_out_and_counts_them() {
    let server = MockServer::builder().start().await;
    mount_pr(&server).await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/1/comments",
        200,
        json!([
            comment(1, "amy", "User"),
            comment(2, "codex-connector[bot]", "Bot")
        ]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1/comments",
        200,
        json!([inline(3, "ci", "Bot"), inline(4, "rev", "User")]),
    )
    .await;
    let row = json_fixture(
        r#"{"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["comments","reviewComments"]}"#,
    );
    let everyone = read(&server, row.clone()).await;
    let pr = &everyone["pullRequests"][0];
    assert_eq!(
        authors(pr, "comments"),
        ["amy", "codex-connector[bot]"],
        "{everyone}"
    );
    assert_eq!(authors(pr, "reviewComments"), ["ci", "rev"], "{everyone}");
    // The conversation comes before the inline review comments.
    let keys = pr.as_object().expect("row").keys().collect::<Vec<_>>();
    let at = |key: &str| keys.iter().position(|k| *k == key).expect(key);
    assert!(at("comments") < at("reviewComments"), "{keys:?}");

    let mut people = row;
    people["bots"] = json!(false);
    let out = read(&server, people).await;
    let pr = &out["pullRequests"][0];
    assert_eq!(authors(pr, "comments"), ["amy"], "{out}");
    assert_eq!(authors(pr, "reviewComments"), ["rev"], "{out}");
    let warnings = out["warnings"].as_array().cloned().unwrap_or_default();
    for noun in ["1 of 2 comments", "1 of 2 review comments"] {
        assert!(
            warnings.iter().any(|w| w
                .as_str()
                .is_some_and(|w| w.contains(&format!("bots:false left out {noun}")))),
            "{noun}: {out}"
        );
    }
}

#[tokio::test]
async fn bots_false_leaves_bot_comments_of_an_issue_out() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/7",
        200,
        json!({"number":7,"title":"Thread","state":"open","user":{"login":"bob"},
            "body":"repro","comments":3,"created_at":"2024-01-01T00:00:00Z",
            "updated_at":"2024-01-02T00:00:00Z"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/7/comments",
        200,
        json!([
            comment(1, "dependabot[bot]", "Bot"),
            comment(2, "amy", "User"),
            comment(3, "renovate", "Bot")
        ]),
    )
    .await;
    let out = read(
        &server,
        json_fixture(
            r#"{"operation":"issue","owner":"o","repo":"r","number":7,
                "sections":["comments"],"bots":false}"#,
        ),
    )
    .await;
    assert_eq!(authors(&out["issues"][0], "comments"), ["amy"], "{out}");
    assert!(
        out["warnings"][0]
            .as_str()
            .is_some_and(|w| w.contains("bots:false left out 2 of 3 comments")),
        "{out}"
    );
}
