//! Issue comment pages: response-sized pages, body hops and a complete comment walk.

use crate::support::{self, Run as _};
use serde_json::json;
use support::{Workspace, call, checks, row_data};
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
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Big thread", "state": "open", "body": "repro",
            "user": {"login": "alice"}, "labels": [], "comments": 30,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
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
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"queries":[{"operation":"issue","owner":"a","repo":"b","number":7,
               "sections":["comments"],"pageSize":30,"extras":[]}],"responseLength":12000}),
    )
    .await
    .expect("issue comments");
    let structured = &outcome.structured_content;
    assert!(
        structured
            .get("responsePagination")
            .is_none_or(|page| page["hasMore"] != true),
        "{structured}"
    );
    let data = row_data(&outcome);
    let shown = data["issues"][0]["comments"].as_array().map_or(0, Vec::len);
    assert!((1..30).contains(&shown), "{shown}: {data}");
    let next = &data["next"]["nextCommentPage"]["query"]["queries"][0];
    let (page, size) = (
        next["page"].as_u64().expect("page"),
        next["pageSize"].as_u64().expect("size"),
    );
    assert_eq!((page - 1) * size, shown as u64, "{data}");
    // Each comment's whole body restates no window.
    checks::assert_absent(data, "bodyPagination");
    runtime.close().await;
}

/// A comment-body continuation hop lists only the
/// bodies it continues, and pages exactly the comments its first window
/// showed. The first window reads the issue body beside the discussion, so
/// its comments get less of the page than a comments-only hop would: the
/// hop must still stop where the first window stopped, or it shows comments
/// first at a later offset (their opening text never read) and its
/// `nextCommentPage` skips them. Every lead of every response is followed;
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
    let first = json!({"queries":[{"operation":"issue","owner":"a","repo":"b","number":7,
        "sections":["body","comments"],"length":6000,"extras":["stats"]}]});
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
        assert!(calls <= 60, "walk did not finish");
        let outcome = runtime
            .run(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                input.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("call {calls} failed: {error:?}\n{input}"));
        let content = &outcome.structured_content;
        let data = &content["results"][0]["data"];
        let issue = &data["issues"][0];
        if let Some(text) = issue["body"].as_str() {
            let offset = issue["contentPagination"]["body"]["offset"]
                .as_u64()
                .unwrap_or(0);
            assert_eq!(
                offset as usize,
                issue_text.chars().count(),
                "call {calls}: issue body"
            );
            issue_text.push_str(text);
        }
        for comment in issue["comments"].as_array().into_iter().flatten() {
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
            assert_eq!(
                offset as usize,
                so_far.chars().count(),
                "call {calls}: comment {id} {comment}"
            );
            so_far.push_str(text);
        }
        if let Some(next) = content["responsePagination"]["next"]["query"].as_object() {
            queue.push_back(serde_json::Value::Object(next.clone()));
        }
        for (_, lead) in data["next"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(name, _)| {
                matches!(
                    name.as_str(),
                    "continueBody" | "continueCommentBody" | "nextCommentPage"
                )
            })
        {
            queue.push_back(lead["query"].clone());
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
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Big thread", "state": "open", "body": "repro",
            "user": {"login": "alice"}, "labels": [], "comments": 105,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
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
    let mut query = json!({"operation":"issue","owner":"a","repo":"b","number":7,"sections":["comments"],"extras":[]});
    let mut ids = Vec::new();
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 20, "walk did not finish");
        let outcome = call(
            &runtime,
            "ghGetHistoryItem",
            json!({"queries":[query.clone()]}),
        )
        .await
        .expect("comment page");
        let structured = &outcome.structured_content;
        assert!(
            structured.get("responsePagination").is_none(),
            "{structured}"
        );
        let data = row_data(&outcome).clone();
        let issue = &data["issues"][0];
        assert!(issue["comments"].is_array(), "call {calls}: {data}");
        for comment in issue["comments"].as_array().into_iter().flatten() {
            assert!(comment.get("commentType").is_none(), "{comment}");
            assert!(comment.get("updatedAt").is_none(), "{comment}");
            ids.push(comment["id"].as_str().expect("id").to_owned());
        }
        let consumed = ids.len() as u64;
        match data["next"]["nextCommentPage"]["query"]["queries"][0].as_object() {
            Some(next) => {
                // The page position is the visible comment pagination, not a
                // warning line.
                assert_eq!(
                    issue["contentPagination"]["comments"]["hasMore"],
                    json!(true),
                    "consumed {consumed}: {data}"
                );
                assert!(
                    !data["warnings"].to_string().contains("more comments"),
                    "consumed {consumed}: {data}"
                );
                query = serde_json::Value::Object(next.clone());
            }
            None => break,
        }
    }
    let unique = ids.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!((ids.len(), unique.len()), (105, 105));
    // ~73 KB of comments at a 20k page: budget-filled pages. The resume
    // grid follows the contract's default pageSize (10 since 2026-10-09),
    // so page cuts land one call later than at 30.
    assert!(calls <= 6, "{calls} calls");
    // A page past the discussion's last states it on the comment page
    // block: the page, the page count on its grid, and `outOfRange`.
    let past = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"issue","owner":"a","repo":"b","number":7,"sections":["comments"],
            "page":40,"pageSize":10,"extras":[]}),
    )
    .await
    .expect("page past the end");
    let data = row_data(&past).clone();
    let block = &data["issues"][0]["contentPagination"]["comments"];
    assert_eq!(block["outOfRange"], true, "{data}");
    assert_eq!(
        (block["currentPage"].clone(), block["totalPages"].clone()),
        (json!(40), json!(11)),
        "{data}"
    );
    assert_eq!(block["hasMore"], false, "{data}");
    assert!(data["issues"][0].get("comments").is_none(), "{data}");
    runtime.close().await;
}
