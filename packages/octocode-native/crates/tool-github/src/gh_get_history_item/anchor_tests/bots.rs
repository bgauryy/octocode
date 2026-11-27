//! `bots:false` leaves out the discussion entries a bot or app wrote, says
//! how many, keeps a review thread a person wrote in whole, and puts the
//! conversation before the inline review comments.
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

fn reply(id: u64, login: &str, kind: &str, to: u64) -> Value {
    let mut comment = inline(id, login, kind);
    comment["in_reply_to_id"] = json!(to);
    comment
}

fn ids(row: &Value, section: &str) -> Vec<String> {
    row[section]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| entry["id"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// The entries `bots:false` must return of `listed` review comments: every
/// one a person wrote, and every bot's whose thread a person wrote in.
fn answered(listed: &[Value]) -> Vec<String> {
    let thread = |entry: &Value| {
        entry
            .get("in_reply_to_id")
            .unwrap_or(&entry["id"])
            .to_string()
    };
    let person = |entry: &Value| entry["user"]["type"] != "Bot";
    listed
        .iter()
        .filter(|entry| {
            person(entry)
                || listed
                    .iter()
                    .any(|other| person(other) && thread(other) == thread(entry))
        })
        .map(|entry| entry["id"].to_string())
        .collect()
}

/// A bot's review comment a person replied to stays with `bots:false`, and
/// so does a bot's reply in a thread a person opened: the thread reads
/// whole. A bot thread nobody answered is left out and counted.
#[tokio::test]
async fn bots_false_keeps_a_thread_a_person_wrote_in_whole() {
    let server = MockServer::builder().start().await;
    mount_pr(&server).await;
    let listed = vec![
        inline(3, "ci[bot]", "Bot"),
        reply(4, "rev", "User", 3),
        inline(5, "lint[bot]", "Bot"),
        inline(7, "rev", "User"),
        reply(8, "ci[bot]", "Bot", 7),
        reply(9, "lint[bot]", "Bot", 5),
    ];
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1/comments",
        200,
        json!(listed),
    )
    .await;
    let row = json_fixture(
        r#"{"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["reviewComments"],"bots":false}"#,
    );
    let out = read(&server, row.clone()).await;
    let pr = &out["pullRequests"][0];
    let kept = answered(&listed);
    assert_eq!(ids(pr, "reviewComments"), kept, "{out}");
    assert!(kept.len() < listed.len() && kept.len() > 2, "{kept:?}");
    let left_out = listed.len() - kept.len();
    let warnings = out["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().is_some_and(|w| w.contains(&format!(
                "bots:false left out {left_out} of {} review comments",
                listed.len()
            )))),
        "{out}"
    );
    // The default stays: every entry, bots included.
    let mut everyone = row;
    everyone.as_object_mut().expect("row").remove("bots");
    let out = read(&server, everyone).await;
    assert_eq!(
        ids(&out["pullRequests"][0], "reviewComments").len(),
        listed.len(),
        "{out}"
    );
}

/// A person's reply in a later provider batch keeps the bot comment it
/// answers on the page that comment belongs to: walking every page returns
/// each kept entry exactly once.
#[tokio::test]
async fn a_reply_in_a_later_batch_keeps_its_bot_thread_on_every_page() {
    let server = MockServer::builder().start().await;
    mount_pr(&server).await;
    let batch = octocode_github::MAX_PER_PAGE as u64;
    // Bot threads fill the first batch; a person answers the first of them
    // from the second batch.
    let mut listed = (1..=batch + 20)
        .map(|id| inline(id, "ci[bot]", "Bot"))
        .collect::<Vec<_>>();
    listed.push(reply(batch + 50, "rev", "User", 1));
    listed.push(inline(batch + 51, "amy", "User"));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls/1/comments"))
        .respond_with(paged_like_github(server.uri(), listed.clone(), |items| {
            json!(items)
        }))
        .mount(&server)
        .await;
    let kept = answered(&listed);
    assert_eq!(kept.len(), 3, "{kept:?}");
    let mut query = json_fixture(
        r#"{"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["reviewComments"],"bots":false,"pageSize":1}"#,
    );
    let mut walked = Vec::new();
    for _ in 0..=kept.len() {
        let out = read(&server, query.clone()).await;
        walked.extend(ids(&out["pullRequests"][0], "reviewComments"));
        let Some(next) = out.pointer("/pullRequests/0/next/reviewComments/input") else {
            break;
        };
        let saved =
            octocode_runtime_base::contracts::validate_saved("ghGetHistoryItem", next.clone())
                .expect("an executable page");
        query = saved["queries"][0].clone();
    }
    assert_eq!(walked, kept, "each kept entry once, in order");
}
