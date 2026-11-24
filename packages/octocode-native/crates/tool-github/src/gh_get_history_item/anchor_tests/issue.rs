//! Issue rows: number, state, author and discussion size; a pull-request number is not an issue; comment pages keep the caller's page size.
use super::*;
use octocode_test_support::json_fixture;

#[tokio::test]
async fn issue_row_carries_number_state_author_and_discussion_size() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/7",
        200,
        json_fixture(
            r#"{
                "number": 7,
                "title": "Crash",
                "state": "open",
                "user": {"login": "bob"},
                "body": "It crashes.",
                "comments": 3,
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-02T00:00:00Z"
            }"#,
        ),
    )
    .await;
    let out = read(
        &server,
        json_fixture(
            r#"{
                "operation": "issue",
                "owner": "o",
                "repo": "r",
                "number": 7
            }"#,
        ),
    )
    .await;
    let issue = &out["issues"][0];
    assert_eq!(issue["number"], 7, "{out}");
    assert_eq!(issue["state"], "open", "{out}");
    assert_eq!(issue["author"], "bob", "{out}");
    assert_eq!(issue["commentsCount"], 3, "{out}");
    // A summary previews the body, as a pull request's does; the whole body
    // and the discussion are `sections` values named in the tool description,
    // so no lead restates the query for them.
    assert_eq!(issue["bodyPreview"], "It crashes.", "{out}");
    assert!(issue.get("body").is_none(), "{out}");
    assert!(out.get("related").is_none(), "{out}");
    assert!(out.get("next").is_none(), "{out}");
}

/// The wrong operation for an existing number is one failure class both
/// ways: not found as the kind asked for, typed by its reason.
#[tokio::test]
async fn an_issue_read_of_a_pull_request_number_is_not_found() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/9",
        200,
        json_fixture(
            r#"{
                "number": 9,
                "title": "PR",
                "state": "open",
                "user": {"login": "bob"},
                "pull_request": {"url": "https://x"},
                "comments": 0
            }"#,
        ),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = fixture_context(Duration::from_secs(10), 4 << 20);
    let request = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "number": 9
        }"#,
    ))
    .expect("valid row");
    let Err(error) = execute(&provider, &request, &context, &Passthrough).await else {
        panic!("an issue read of a pull request number fails");
    };
    assert_eq!(error.kind, octocode_github::ProviderErrorKind::NotFound);
    assert_eq!(
        error.reason,
        Some(octocode_github::ProviderErrorReason::IssueIsPullRequest)
    );
    assert_eq!(&*error.message, "#9 is a pull request, not an issue");
}

/// A saved `pageSize` (the cursor's grid; no published field sets one) only
/// places a comment page: page 2 of a 4-comment grid starts at comment 4, and
/// the page then fills the response budget instead of stopping at 4, so a
/// walk resumed on a small grid never shrinks to it. Every comment from there
/// arrives once, in order, and the block states no resume size.
#[tokio::test]
async fn a_saved_grid_places_issue_comment_pages_without_capping_them() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/7",
        200,
        json_fixture(
            r#"{
                "number": 7,
                "title": "Thread",
                "state": "open",
                "user": {"login": "bob"},
                "body": "repro",
                "comments": 11,
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-02T00:00:00Z"
            }"#,
        ),
    )
    .await;
    let comments = (0..11)
        .map(|n| {
            json!({"id": n, "user": {"login": "amy"}, "body": format!("comment {n}"),
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-01T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/issues/7/comments"))
        .respond_with(paged_like_github(server.uri(), comments, Value::Array))
        .mount(&server)
        .await;
    let data = read(
        &server,
        json_fixture(
            r#"{
                "operation": "issue",
                "owner": "o",
                "repo": "r",
                "number": 7,
                "sections": ["comments"],
                "page": 2,
                "pageSize": 4
            }"#,
        ),
    )
    .await;
    let issue = &data["issues"][0];
    let ids = issue["comments"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|comment| comment["id"].to_string())
        .collect::<Vec<_>>();
    let expected = (4..11).map(|n| json!(n.to_string()).to_string());
    assert_eq!(ids, expected.collect::<Vec<_>>(), "{data}");
    let block = &issue["sectionPagination"]["comments"];
    assert!(block.get("pageSize").is_none(), "{data}");
    assert_eq!(block["hasMore"], false, "{data}");
}

/// A summary's preview is the body's opening, whitespace collapsed and cut
/// with `…`, never the whole body; a section read of the body holds it
/// verbatim.
#[tokio::test]
async fn an_issue_summary_previews_a_long_body_and_the_body_read_holds_it_whole() {
    let server = MockServer::builder().start().await;
    let body = format!("First line.\n\n{}", "word ".repeat(400));
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/7",
        200,
        json!({"number":7,"title":"Long","state":"open","user":{"login":"bob"},
            "body":body,"comments":0,"created_at":"2024-01-01T00:00:00Z",
            "updated_at":"2024-01-02T00:00:00Z"}),
    )
    .await;
    let summary = read(
        &server,
        json_fixture(
            r#"{
                "operation": "issue",
                "owner": "o",
                "repo": "r",
                "number": 7
            }"#,
        ),
    )
    .await;
    let preview = summary["issues"][0]["bodyPreview"]
        .as_str()
        .expect("preview");
    assert!(preview.starts_with("First line. word"), "{summary}");
    assert!(preview.ends_with('…'), "{summary}");
    assert!(preview.chars().count() < body.chars().count(), "{summary}");
    let whole = read(
        &server,
        json_fixture(
            r#"{
                "operation": "issue",
                "owner": "o",
                "repo": "r",
                "number": 7,
                "sections": ["body"]
            }"#,
        ),
    )
    .await;
    assert_eq!(whole["issues"][0]["body"], json!(body), "{whole}");
    assert!(whole["issues"][0].get("bodyPreview").is_none(), "{whole}");
}

/// A summary previews what GitHub shows of the body: its template's HTML
/// comments (`<!-- … -->`, hidden when rendered) spend none of the preview,
/// and the body read keeps them verbatim. A body of only comments still
/// previews its text: an absent preview means an empty body.
#[tokio::test]
async fn an_issue_preview_skips_the_html_comments_github_hides() {
    let server = MockServer::builder().start().await;
    let template = "<!-- Please do not ignore this template. ".repeat(20) + "-->";
    let body = format!(
        "{template}\n### Describe the bug\n<!-- A clear description. -->\nIt crashes.\n<!-- unclosed"
    );
    let comments_only = "<!-- only the template -->";
    for (number, text) in [(7, body.as_str()), (8, comments_only)] {
        mount_json(
            &server,
            &format!("/api/v3/repos/o/r/issues/{number}"),
            200,
            json!({"number":number,"title":"Crash","state":"open","user":{"login":"bob"},
                "body":text,"comments":0,"created_at":"2024-01-01T00:00:00Z",
                "updated_at":"2024-01-02T00:00:00Z"}),
        )
        .await;
    }
    let summary = |number: u64, sections: Value| {
        let server = &server;
        async move {
            let mut query = json!({"operation":"issue","owner":"o","repo":"r","number":number});
            if !sections.is_null() {
                query["sections"] = sections;
            }
            read(server, query).await
        }
    };
    let out = summary(7, Value::Null).await;
    assert_eq!(
        out["issues"][0]["bodyPreview"], "### Describe the bug It crashes. <!-- unclosed",
        "{out}"
    );
    let whole = summary(7, json!(["body"])).await;
    assert_eq!(whole["issues"][0]["body"], json!(body), "{whole}");
    let only = summary(8, Value::Null).await;
    assert_eq!(only["issues"][0]["bodyPreview"], comments_only, "{only}");
}

/// A discussion the caller asked for is stated even when it holds no
/// comment: an empty list says zero, where an absent one reads as not
/// fetched.
#[tokio::test]
async fn a_requested_discussion_with_no_comment_is_an_empty_list() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/8",
        200,
        json_fixture(
            r#"{
                "number": 8,
                "title": "Quiet",
                "state": "closed",
                "user": {"login": "bob"},
                "body": "b",
                "comments": 0,
                "created_at": "2024-01-01T00:00:00Z",
                "closed_at": "2024-01-03T00:00:00Z"
            }"#,
        ),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/8/comments",
        200,
        json!([]),
    )
    .await;
    let out = read(
        &server,
        json_fixture(
            r#"{
                "operation": "issue",
                "owner": "o",
                "repo": "r",
                "number": 8,
                "sections": ["comments"]
            }"#,
        ),
    )
    .await;
    assert_eq!(out["issues"][0]["comments"], json!([]), "{out}");
}

/// Every `continue` lead of a page, as the row it runs.
fn continue_leads(data: &Value) -> Vec<Value> {
    let mut leads = Vec::new();
    let mut stack = vec![data];
    while let Some(node) = stack.pop() {
        match node {
            Value::Object(map) => {
                if map.get("kind") == Some(&json!("continue")) {
                    leads.push(node["input"]["queries"][0].clone());
                    continue;
                }
                stack.extend(map.values());
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    leads
}

/// A comment longer than one body window continues through its body before
/// the next comment page: walking every lead reads each comment once, in
/// order, whole, and no page offers a lead an earlier page offered.
#[tokio::test]
async fn a_long_comment_walk_offers_each_lead_once() {
    let server = MockServer::builder().start().await;
    let long = "x".repeat(30_000);
    let short = |n: usize| format!("{n}{}", "y".repeat(999));
    let bodies = std::iter::once(long.clone())
        .chain((2..=4).map(short))
        .collect::<Vec<_>>();
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/7",
        200,
        json!({"number": 7, "title": "Thread", "state": "open", "user": {"login": "bob"},
            "body": "repro", "comments": bodies.len(),
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z"}),
    )
    .await;
    let comments = bodies
        .iter()
        .enumerate()
        .map(|(n, body)| {
            json!({"id": n + 1, "user": {"login": "amy"}, "body": body,
                "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-01T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/issues/7/comments"))
        .respond_with(paged_like_github(server.uri(), comments, Value::Array))
        .mount(&server)
        .await;
    let mut pending = vec![json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "number": 7,
            "sections": ["comments"]
        }"#,
    )];
    let mut offered: Vec<Value> = Vec::new();
    let mut shown: Vec<(String, String)> = Vec::new();
    while let Some(row) = pending.pop() {
        let data = read_with_budget(&server, row, Some(15_000)).await;
        for comment in data["issues"][0]["comments"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let id = comment["id"].as_str().expect("id").to_owned();
            let text = comment["body"].as_str().unwrap_or_default();
            match shown.iter_mut().find(|(seen, _)| *seen == id) {
                Some((_, body)) => body.push_str(text),
                None => shown.push((id, text.to_owned())),
            }
        }
        for lead in continue_leads(&data) {
            assert!(!offered.contains(&lead), "offered again: {lead}\n{data}");
            offered.push(lead.clone());
            pending.push(lead);
        }
    }
    let expected = bodies
        .iter()
        .enumerate()
        .map(|(n, body)| ((n + 1).to_string(), body.clone()))
        .collect::<Vec<_>>();
    assert_eq!(shown, expected);
}
