//! Issue comment pages: response-sized pages, body hops and a complete comment walk.

use crate::support::{self, Run as _};
use octocode_test_support::json_fixture;
use serde_json::json;
use support::Workspace;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// An issue comment page fits one response page: the comments that
/// fit are shown, and the one comment cursor resumes exactly at the first
/// unshown comment (no second, response-level cursor).
#[tokio::test]
async fn issue_comment_pages_fit_the_response_with_one_cursor() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "number": 7,
                "title": "Big thread",
                "state": "open",
                "body": "repro",
                "user": {"login": "alice"},
                "labels": [],
                "comments": 30,
                "created_at": "2026-09-20T00:00:00Z",
                "updated_at": "2026-09-25T00:00:00Z"
            }"#,
        )))
        .mount(&server)
        .await;
    let comments = (0..30)
        .map(|n| json!({"id": n, "user": {"login": "bob"}, "body": format!("comment {n} {}", "x".repeat(900)),
                        "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"}))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(comments)))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let walked = support::walk(
        &runtime,
        "ghGetHistoryItem",
        json_fixture(
            r#"{
                "queries": [
                    {
                        "operation": "issue",
                        "owner": "a",
                        "repo": "b",
                        "number": 7,
                        "sections": ["comments"]
                    }
                ],
                "responseChars": 2500
            }"#,
        ),
        |output| {
            output["results"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|row| {
                    assert!(row["status"] != "error", "{output}");
                    row["data"]["issues"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .flat_map(|issue| issue["comments"].as_array().into_iter().flatten())
                        .map(|comment| {
                            let id = comment["id"].as_str().unwrap();
                            assert_eq!(
                                comment["body"],
                                format!("comment {id} {}", "x".repeat(900))
                            );
                            id.to_owned()
                        })
                })
                .collect()
        },
    )
    .await;
    assert_eq!(walked.rows.len(), 30);
    assert!(walked.queries.len() > 1);
    runtime.close().await;
}

/// A comment-body continuation hop lists only the
/// bodies it continues, and pages exactly the comments its first window
/// showed. The first window reads the issue body beside the discussion, so
/// its comments get less of the page than a comments-only hop would: the
/// hop must still stop where the first window stopped, or it shows comments
/// first at a later offset (their opening text never read) and its
/// comment page skips them. Every lead of every response is followed;
/// every body (the issue's and each comment's) arrives contiguous and
/// exactly once, and no finished comment is listed again.
#[tokio::test]
async fn issue_comment_body_hops_list_only_unfinished_bodies() {
    let server = MockServer::builder().start().await;
    let issue_body = format!("issue body {}", "b".repeat(9_000));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Long thread", "state": "open", "body": issue_body,
            "user": {"login": "alice"}, "labels": [], "comments": 40,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let body = |n: usize| {
        let size = if n % 2 == 1 { 8_000 } else { 300 };
        format!("comment {n} ") + &format!("{n:02}.").repeat(size / 3)
    };
    let comments = (0..40)
        .map(|n| {
            json!({"id": n, "user": {"login": "bob"}, "body": body(n),
                        "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(comments)))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let first = json_fixture(
        r#"{
            "queries": [
                {
                    "operation": "issue",
                    "owner": "a",
                    "repo": "b",
                    "number": 7,
                    "sections": ["body", "comments"]
                }
            ],
            "responseChars": 6000
        }"#,
    );
    let mut queue = std::collections::VecDeque::from([first]);
    let mut followed = std::collections::HashSet::<String>::new();
    let mut read = std::collections::BTreeMap::<u64, String>::new();
    let mut issue_text = String::new();
    let (mut calls, mut relisted) = (0, 0);
    // One lead offered by two responses runs once, whatever its key order.
    fn canonical(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Object(map) => {
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                let fields = keys
                    .into_iter()
                    .map(|key| format!("{key:?}:{}", canonical(&map[key])))
                    .collect::<Vec<_>>();
                format!("{{{}}}", fields.join(","))
            }
            serde_json::Value::Array(items) => {
                format!(
                    "[{}]",
                    items.iter().map(canonical).collect::<Vec<_>>().join(",")
                )
            }
            scalar => scalar.to_string(),
        }
    }
    while let Some(input) = queue.pop_front() {
        if !followed.insert(canonical(&input)) {
            continue;
        }
        calls += 1;
        let before = issue_text.len() + read.values().map(String::len).sum::<usize>();
        let outcome = runtime
            .run(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                input.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("call {calls} failed: {error:?}\n{input}"));
        let content = &outcome.structured_content;
        let row = &content["results"][0];
        let text_part = row["rowPart"]["text"].as_str();
        let data = &row["data"];
        let issue = &data["issues"][0];
        if let Some(text) = issue["body"].as_str() {
            if text_part != Some("/data/issues/0/body") {
                let offset = issue["sectionPagination"]["body"]["offset"]
                    .as_u64()
                    .unwrap_or(0);
                assert_eq!(
                    offset as usize,
                    issue_text.chars().count(),
                    "call {calls}: issue body"
                );
            }
            assert!(
                issue_body[issue_text.len()..].starts_with(text),
                "call {calls}: issue body must be the exact next text"
            );
            issue_text.push_str(text);
        }
        for (index, comment) in issue["comments"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let id = match &comment["id"] {
                serde_json::Value::String(id) => id.parse::<u64>().expect("numeric id"),
                id => id.as_u64().expect("id"),
            };
            let text = comment["body"].as_str().unwrap_or("");
            let offset = comment["bodyPagination"]["offset"].as_u64().unwrap_or(0);
            let so_far = read.entry(id).or_default();
            if text.is_empty() && so_far.chars().count() == body(id as usize).chars().count() {
                relisted += 1;
                continue;
            }
            let fragment = format!("/data/issues/0/comments/{index}/body");
            if text_part != Some(fragment.as_str()) {
                assert_eq!(
                    offset as usize,
                    so_far.chars().count(),
                    "call {calls}: comment {id} {comment}"
                );
            }
            assert!(
                body(id as usize)[so_far.len()..].starts_with(text),
                "call {calls}: comment {id} must be the exact next text"
            );
            so_far.push_str(text);
        }
        assert!(
            issue_text.len() + read.values().map(String::len).sum::<usize>() > before,
            "call {calls}: the body walk must make evidence progress"
        );
        if let Some(next) = content["responsePagination"]["next"]["input"].as_object() {
            queue.push_back(serde_json::Value::Object(next.clone()));
        }
        // A section's walk rides its row's last part, which need not
        // repeat the section's `sectionPagination`: follow every one shown.
        for lead in data["issues"][0]["next"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(_, lead)| lead)
        {
            if lead.is_object() {
                queue.push_back(lead["input"].clone());
            }
        }
    }
    assert_eq!(relisted, 0, "finished comments listed again");
    assert_eq!(issue_text, issue_body);
    assert_eq!(read.len(), 40, "{:?}", read.keys().collect::<Vec<_>>());
    for (id, text) in &read {
        assert_eq!(text, &body(*id as usize), "comment {id}");
    }
    runtime.close().await;
}

/// GitHub issue comments paged by `per_page`/`page` from one list.
struct CommentList(Vec<serde_json::Value>, String);

impl Respond for CommentList {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let param = |name: &str| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<usize>().ok())
        };
        let per = param("per_page").unwrap_or(30);
        let page = param("page").unwrap_or(1);
        let start = (page - 1) * per;
        let items = self
            .0
            .iter()
            .skip(start)
            .take(per)
            .cloned()
            .collect::<Vec<_>>();
        let mut response = ResponseTemplate::new(200).set_body_json(json!(items));
        if start + per < self.0.len() {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/issues/7/comments?per_page={per}&page={}>; rel=\"next\"",
                    self.1,
                    page + 1
                )
                .as_str(),
            );
        }
        response
    }
}

/// A 105-comment issue thread (two bot comments) walked by its own
/// cursor: every comment, bots included, arrives once, each page's
/// remainder count is exact, and pages fill the response budget (few calls).
#[tokio::test]
async fn issue_comment_walk_reads_every_comment_and_counts_the_rest() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "number": 7,
                "title": "Big thread",
                "state": "open",
                "body": "repro",
                "user": {"login": "alice"},
                "labels": [],
                "comments": 105,
                "created_at": "2026-09-20T00:00:00Z",
                "updated_at": "2026-09-25T00:00:00Z"
            }"#,
        )))
        .mount(&server)
        .await;
    let comments = (0..105)
        .map(|n| {
            let login = if n == 40 || n == 90 { "github-actions[bot]" } else { "bob" };
            json!({"id": n, "user": {"login": login}, "body": format!("comment {n} {}", "x".repeat(700)),
                   "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .respond_with(CommentList(comments, server.uri()))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let walked = support::walk(
        &runtime,
        "ghGetHistoryItem",
        json_fixture(
            r#"{
                "queries": [
                    {
                        "operation": "issue",
                        "owner": "a",
                        "repo": "b",
                        "number": 7,
                        "sections": ["comments"]
                    }
                ],
                "responseChars": 4000
            }"#,
        ),
        |output| {
            output["results"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|row| {
                    assert!(row["status"] != "error", "{output}");
                    row["data"]["issues"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .flat_map(|issue| issue["comments"].as_array().into_iter().flatten())
                        .map(|comment| {
                            let id = comment["id"].as_str().unwrap();
                            assert_eq!(
                                comment["body"],
                                format!("comment {id} {}", "x".repeat(700))
                            );
                            if ["40", "90"].contains(&id) {
                                assert_eq!(comment["author"], "github-actions[bot]");
                            }
                            id.to_owned()
                        })
                })
                .collect()
        },
    )
    .await;
    assert_eq!(walked.rows.len(), 105);
    assert!(
        walked
            .queries
            .iter()
            .skip(1)
            .all(|input| input["cursor"].is_string())
    );
    runtime.close().await;
}

/// A long discussion of uneven comments walked by its own cursor: a page
/// after a short one still fills the response budget (the private grid that
/// places a page never caps it), so every page but the last holds most of a
/// response, and page numbers only climb.
#[tokio::test]
async fn issue_comment_pages_after_an_uneven_cut_still_fill_the_budget() {
    const TOTAL: usize = 365;
    const RESPONSE_CHARS: usize = 9_000;
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Long thread", "state": "closed", "body": "proposal",
            "user": {"login": "alice"}, "labels": [], "comments": TOTAL,
            "created_at": "2026-09-20T00:00:00Z", "closed_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    // Uneven bodies, as a real discussion has: cuts land on every residue.
    let body = |n: usize| format!("comment {n} {}", "y".repeat(150 + (n * 397) % 1_100));
    let comments = (0..TOTAL)
        .map(|n| {
            json!({"id": n, "user": {"login": "bob"}, "body": body(n),
                   "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .respond_with(CommentList(comments, server.uri()))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut input = Some(
        json!({"queries":[{"operation":"issue","owner":"a","repo":"b","number":7,
        "sections":["comments"]}],"responseChars":RESPONSE_CHARS}),
    );
    let (mut ids, mut pages, mut sizes) = (Vec::new(), Vec::new(), Vec::new());
    while let Some(query) = input.take() {
        assert!(sizes.len() < TOTAL, "walk did not finish");
        let output = runtime
            .run("walk".into(), "ghGetHistoryItem".into(), query.clone())
            .await
            .unwrap_or_else(|error| panic!("{query}: {error:?}"))
            .structured_content
            .into_value();
        let issue = &output["results"][0]["data"]["issues"][0];
        let shown = issue["comments"].as_array().cloned().unwrap_or_default();
        for comment in &shown {
            let id = comment["id"]
                .as_str()
                .expect("id")
                .parse::<usize>()
                .expect("numeric id");
            assert_eq!(comment["body"], body(id), "{output}");
            ids.push(id);
        }
        sizes.push(serde_json::to_string(&shown).expect("json").len());
        let page = &issue["sectionPagination"]["comments"];
        pages.push(page["offset"].as_u64().expect("page offset"));
        input = issue["next"]["comments"]["input"]
            .is_object()
            .then(|| issue["next"]["comments"]["input"].clone());
    }
    ids.sort_unstable();
    assert_eq!(ids, (0..TOTAL).collect::<Vec<_>>());
    // Every page but the last fills most of the response: the largest
    // comment is under 1.3 KB, so a full page leaves less than that unused.
    let thin = sizes[..sizes.len() - 1]
        .iter()
        .filter(|size| **size < RESPONSE_CHARS / 3)
        .count();
    assert_eq!(thin, 0, "comment chars per page: {sizes:?}");
    assert!(
        pages.windows(2).all(|pair| pair[0] < pair[1]),
        "page numbers: {pages:?}"
    );
    runtime.close().await;
}
