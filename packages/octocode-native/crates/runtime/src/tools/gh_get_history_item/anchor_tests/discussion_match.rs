//! `matchString` over discussion bodies: an issue's comments and a pull
//! request's comments, reviews and review comments keep the entries whose
//! body holds it, scanned in concurrent waves, disclosed once, recoverable
//! through `withoutMatchString`.
use super::*;

const ISSUE_PATH: &str = "/api/v3/repos/o/r/issues/7";
const COMMENTS_PATH: &str = "/api/v3/repos/o/r/issues/7/comments";

fn comment(n: usize, body: &str) -> Value {
    json!({"id": n, "user": {"login": "amy"}, "body": body,
        "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-01T00:00:00Z"})
}

/// An issue of `comments`, served like GitHub pages them.
async fn mount_issue(server: &MockServer, comments: Vec<Value>) {
    mount_json(
        server,
        ISSUE_PATH,
        200,
        json!({"number":7,"title":"Thread","state":"open","user":{"login":"bob"},
            "body":"repro","comments":comments.len(),"created_at":"2024-01-01T00:00:00Z",
            "updated_at":"2024-01-02T00:00:00Z"}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path(COMMENTS_PATH))
        .respond_with(paged_like_github(server.uri(), comments, Value::Array))
        .mount(server)
        .await;
}

/// Requests the server saw on `route`.
async fn requests_to(server: &MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .expect("recorded requests")
        .iter()
        .filter(|request| request.url.path() == route)
        .count()
}

fn issue_query(extra: Value) -> Value {
    let mut row = json!({"operation":"issue","owner":"o","repo":"r","number":7});
    for (key, value) in extra.as_object().expect("fields") {
        row[key] = value.clone();
    }
    row
}

/// The lossless recovery: the read without its literal, which every lead of
/// the read lacks otherwise.
fn without_match_read(data: &Value) -> Option<&Value> {
    crate::tools::test_support::checks::find_related_call(data, "ghGetHistoryItem", |input| {
        input["queries"][0].get("matchString").is_none()
    })
    .map(|call| &call["input"]["queries"][0])
}

/// The recovery the warning promises survives the response stage: the served
/// row (the dropped-literal note names `related`) keeps the read without it.
fn assert_without_match_served(out: &Value) {
    crate::tools::test_support::assert_lead_served(ToolId::GhGetHistoryItem, out, |input| {
        input["queries"][0].get("matchString").is_none()
    });
}

fn ids(data: &Value) -> Vec<String> {
    data["issues"][0]["comments"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|comment| comment["id"].as_str().expect("id").to_owned())
        .collect()
}

/// 250 comments over three provider batches, the needle in #217: one comment,
/// the drop disclosed, the lossless recovery offered, and the scan is the
/// three comment batches plus the one issue read, never a page walk.
#[tokio::test]
async fn a_match_string_finds_one_comment_among_three_batches() {
    let server = MockServer::builder().start().await;
    let comments = (1..=250)
        .map(|n| {
            let body = if n == 217 {
                "the needle is here".to_owned()
            } else {
                format!("chatter {n}")
            };
            comment(n, &body)
        })
        .collect();
    mount_issue(&server, comments).await;
    let out = read(&server, issue_query(json!({"matchString":["needle"]}))).await;
    assert_eq!(ids(&out), ["217"], "{out}");
    assert_eq!(out["warnings"].as_array().map(Vec::len), Some(1), "{out}");
    assert!(
        out["warnings"][0]
            .as_str()
            .is_some_and(|w| w.contains("dropped 249 of 250 comments")),
        "{out}"
    );
    // The header keeps GitHub's count; the page counts the filtered list.
    assert_eq!(out["issues"][0]["commentsCount"], 250, "{out}");
    let page = &out["issues"][0]["sectionPagination"]["comments"];
    assert_eq!(page["total"], 1, "{out}");
    assert_eq!(page["hasMore"], false, "{out}");
    assert_without_match_served(&out);
    let lead = without_match_read(&out).unwrap_or_else(|| panic!("no recovery lead: {out}"));
    assert_eq!(lead["operation"], "issue", "{out}");
    assert_eq!(lead["sections"], json!(["comments"]), "{out}");
    for key in [
        "matchString",
        "caseMode",
        "regexEngine",
        "wholeWord",
        "page",
    ] {
        assert!(lead.get(key).is_none(), "{key}: {out}");
    }
    assert_eq!(requests_to(&server, COMMENTS_PATH).await, 3);
    assert_eq!(requests_to(&server, ISSUE_PATH).await, 1);
}

/// The case, regex and whole-word options of `matchString` apply to a
/// comment body as they do to a patch.
#[tokio::test]
async fn match_string_options_apply_to_comment_bodies() {
    let server = MockServer::builder().start().await;
    let bodies = ["Needle in caps", "needle lower", "needles plural", "no hit"];
    mount_issue(
        &server,
        bodies
            .iter()
            .enumerate()
            .map(|(n, body)| comment(n + 1, body))
            .collect(),
    )
    .await;
    for (options, expected) in [
        (
            json!({"matchString":["needle"],"caseMode":"sensitive"}),
            vec!["2", "3"],
        ),
        (
            json!({"matchString":["needle"],"caseMode":"insensitive"}),
            vec!["1", "2", "3"],
        ),
        (
            json!({"matchString":["needle"],"caseMode":"insensitive","wholeWord":true}),
            vec!["1", "2"],
        ),
        (
            json!({"matchString":["need.e\\b"],"caseMode":"insensitive","regexEngine":"rust"}),
            vec!["1", "2"],
        ),
        (
            json!({"matchString":["need.e"],"regexEngine":"literal"}),
            vec![],
        ),
    ] {
        let out = read(&server, issue_query(options.clone())).await;
        assert_eq!(ids(&out), expected, "{options}: {out}");
    }
}

/// 30 matching comments on a response page that holds a few: the walk
/// follows the continuations, which keep the literal and its options, and
/// every match arrives once in order.
#[tokio::test]
async fn a_filtered_comment_walk_keeps_the_filter_and_repeats_nothing() {
    let server = MockServer::builder().start().await;
    let comments = (1..=90)
        .map(|n| {
            let body = if n % 3 == 0 {
                format!("hit {n} {}", "x".repeat(300))
            } else {
                format!("noise {n}")
            };
            comment(n, &body)
        })
        .collect();
    mount_issue(&server, comments).await;
    let mut query = issue_query(json!({"matchString":["HIT"],"caseMode":"insensitive"}));
    let (mut seen, mut pages) = (Vec::new(), 0);
    loop {
        pages += 1;
        assert!(pages <= 30, "the walk ends");
        let out = read_with_budget(&server, query.clone(), Some(4_000)).await;
        seen.extend(ids(&out));
        let Some(next) = out.pointer("/issues/0/next/comments/input") else {
            break;
        };
        let normalized = crate::contracts::validate_saved("ghGetHistoryItem", next.clone())
            .expect("executable comment page");
        query = normalized["queries"][0].clone();
        assert_eq!(query["matchString"], json!(["HIT"]), "{query}");
        assert_eq!(query["caseMode"], "insensitive", "{query}");
    }
    assert!(pages >= 2, "{pages} pages");
    let expected = (1..=30).map(|n| (n * 3).to_string()).collect::<Vec<_>>();
    assert_eq!(seen, expected);
}

/// No comment holds the literal: an empty, stated list, the warning and the
/// recovery lead, not an error.
#[tokio::test]
async fn a_match_string_that_hits_no_comment_is_an_empty_list_with_a_lead() {
    let server = MockServer::builder().start().await;
    mount_issue(&server, (1..=5).map(|n| comment(n, "chatter")).collect()).await;
    let out = read(&server, issue_query(json!({"matchString":["absent"]}))).await;
    assert_eq!(out["issues"][0]["comments"], json!([]), "{out}");
    assert!(
        out["warnings"][0]
            .as_str()
            .is_some_and(|w| w.contains("dropped 5 of 5 comments")),
        "{out}"
    );
    assert!(without_match_read(&out).is_some(), "{out}");
    assert!(out.get("status").is_none(), "{out}");
}

/// A regex scan that reaches the match limit is incomplete: a validation
/// error, never a list that silently lacks comments.
#[tokio::test]
async fn a_truncated_regex_scan_is_a_validation_error() {
    let budget = crate::tools::page_memo::test_budget!(4096);
    let _scope = crate::tools::source_view::extraction::SpanBudgetOverride::of(budget);
    let dense = "x\n".repeat(budget.limit() / std::mem::size_of::<(usize, usize)>() + 1);
    let server = MockServer::builder().start().await;
    mount_issue(&server, vec![comment(1, &dense)]).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = fixture_context(Duration::from_secs(10), 4 << 20);
    let request = HistoryItemRequest::from_row(issue_query(
        json!({"matchString":["x"],"regexEngine":"rust"}),
    ))
    .expect("valid row");
    let Err(error) = execute(&provider, &request, &context, &Passthrough).await else {
        panic!("a truncated regex scan fails");
    };
    assert_eq!(error.kind, octocode_github::ProviderErrorKind::Validation);
}

/// A pull request's comments, reviews and review comments each keep the
/// entries whose body holds the needle, with no files or patches read.
#[tokio::test]
async fn a_pull_request_match_string_filters_each_discussion_section() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1",
        200,
        json!({"number":1,"title":"pr","state":"open","user":{"login":"ada"},
            "head":{"sha":HEAD},"base":{"sha":BASE},"comments":3,"review_comments":3,
            "changed_files":2}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/1/comments",
        200,
        json!([
            comment(1, "plain"),
            comment(2, "has the needle"),
            comment(3, "plain")
        ]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1/reviews",
        200,
        json!([
            {"id":10,"user":{"login":"rev"},"state":"APPROVED","body":"","submitted_at":"2024-01-01T00:00:00Z"},
            {"id":11,"user":{"login":"rev"},"state":"COMMENTED","body":"needle review","submitted_at":"2024-01-01T00:00:00Z"}
        ]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1/comments",
        200,
        json!([
            {"id":20,"user":{"login":"rev"},"body":"x","path":"a.rs","line":3,"side":"RIGHT","created_at":"2024-01-01T00:00:00Z"},
            {"id":21,"user":{"login":"rev"},"body":"inline needle","path":"a.rs","line":4,"side":"RIGHT","created_at":"2024-01-01T00:00:00Z"},
            {"id":22,"user":{"login":"rev"},"body":"y","path":"a.rs","line":5,"side":"RIGHT","created_at":"2024-01-01T00:00:00Z"}
        ]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["comments","reviews","reviewComments"],"matchString":["needle"]}),
    )
    .await;
    let row = &out["pullRequests"][0];
    let kept = |key: &str| {
        row[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| entry["id"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(kept("comments"), ["2"], "{out}");
    assert_eq!(kept("reviews"), ["11"], "{out}");
    assert_eq!(kept("reviewComments"), ["21"], "{out}");
    let warnings = out["warnings"].to_string();
    for dropped in [
        "dropped 2 of 3 comments",
        "dropped 1 of 2 reviews",
        "dropped 2 of 3 review comments",
    ] {
        assert!(warnings.contains(dropped), "{dropped}: {out}");
    }
    assert!(without_match_read(&out).is_some(), "{out}");
    assert_without_match_served(&out);
    // The needle searches bodies: no file list or patch is read.
    assert_eq!(
        requests_to(&server, "/api/v3/repos/o/r/pulls/1/files").await,
        0
    );
    assert!(
        row.get("files").is_none() && row.get("patches").is_none(),
        "{out}"
    );
}

/// Without a `matchString` an issue's comment page reads what it always did:
/// the first batch beside the issue, nothing dropped, no warning, no lead.
#[tokio::test]
async fn without_a_match_string_the_issue_comment_read_is_unchanged() {
    let server = MockServer::builder().start().await;
    mount_issue(&server, (1..=5).map(|n| comment(n, "chatter")).collect()).await;
    let out = read(&server, issue_query(json!({"sections":["comments"]}))).await;
    assert_eq!(ids(&out), ["1", "2", "3", "4", "5"], "{out}");
    assert!(out.get("warnings").is_none(), "{out}");
    assert!(without_match_read(&out).is_none(), "{out}");
    assert_eq!(requests_to(&server, COMMENTS_PATH).await, 1);
    assert_eq!(requests_to(&server, ISSUE_PATH).await, 1);
}

/// A pull request's comments that no body matches: the warning promises the
/// read without the literal, and the served row keeps it.
#[tokio::test]
async fn a_pull_request_match_string_with_no_hit_keeps_its_recovery() {
    // Real-looking names: a one-letter owner is a substring of any note.
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/acme/widgets/pulls/1",
        200,
        json!({"number":1,"title":"pr","state":"open","user":{"login":"ada"},
            "head":{"sha":HEAD},"base":{"sha":BASE},"comments":2,"changed_files":1}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/acme/widgets/issues/1/comments",
        200,
        json!([comment(1, "plain"), comment(2, "plain too")]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"acme","repo":"widgets","number":1,
            "sections":["comments"],"matchString":["needle"]}),
    )
    .await;
    assert!(
        out["warnings"]
            .to_string()
            .contains("dropped 2 of 2 comments"),
        "{out}"
    );
    assert_without_match_served(&out);
}
