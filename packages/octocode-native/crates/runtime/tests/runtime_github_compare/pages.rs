//! Comparison pages: one collection per page, pinned refs, file-page continuations and bad-ref errors.

use super::{BASE_SHA, HEAD_SHA, mount_capped_compare};
use crate::support::{self, Run as _};
use serde_json::{Value, json};
use support::{Workspace, call, inventory_len, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A comparison resolves `head` to the commit it read and pins its
/// continuations to it; its 300-file list is not a complete count; a file
/// page carries only files, and a commit page only commits.
#[tokio::test]
async fn compare_pages_carry_one_collection_each_and_pin_both_refs() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let first = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2","pageSize":100,"extras":["hints","stats"]}),
    )
    .await
    .expect("compare");
    let data = row_data(&first).clone();
    assert_eq!(data["headSha"], HEAD_SHA, "{data}");
    assert_eq!(data["baseSha"], BASE_SHA, "{data}");
    // The cap is one marker; the echoes and duplicate markers the response
    // would drop are never built.
    assert_eq!(
        data["partialReasons"],
        json!(["providerFileLimit"]),
        "{data}"
    );
    for absent in [
        "owner",
        "repo",
        "terminalLimit",
        "providerLimit",
        "changedFilesCountScope",
    ] {
        assert!(data.get(absent).is_none(), "{absent}: {data}");
    }
    assert_eq!(
        data["contentPagination"]["files"]["countScope"], "partial",
        "{data}"
    );
    // The files past the cap are reached through the range's commits: a
    // 100-commit page lists them in batch-sized pages.
    let list = &crate::support::checks::hint(&data, "listCommits")["query"]["queries"][0];
    assert_eq!(list["sections"], json!(["commits"]), "{data}");
    assert_eq!(list["head"], HEAD_SHA, "{data}");
    assert!(
        list["pageSize"].as_u64().is_some_and(|size| size <= 5),
        "{data}"
    );
    // It starts where this page ends: the 100 commits shown are never listed
    // again, and the commits after them are each reached once.
    assert_eq!(
        list["pageSize"].as_u64().unwrap_or(0) * (list["page"].as_u64().unwrap_or(1) - 1),
        100,
        "{data}"
    );
    // The commit list names each commit by its headline; the
    // first one with more message leads to its whole-message read.
    assert_eq!(data["commits"][0]["messageHeadline"], "c0", "{data}");
    assert!(data["commits"][0].get("message").is_none(), "{data}");
    assert_eq!(
        crate::support::checks::hint(&data, "readCommit")["query"]["queries"][0]["ref"],
        data["commits"][0]["commitSha"],
        "{data}"
    );
    // The commit list is count-cut with a known total (`totalCommits`):
    // its page states the total and the page count.
    assert_eq!(
        data["contentPagination"]["commits"]["totalItems"], 150,
        "{data}"
    );
    assert_eq!(
        data["contentPagination"]["commits"]["totalPages"], 2,
        "{data}"
    );
    // The comparison answers what changed first: its files lead its commits.
    let order = |key: &str| {
        data.as_object()
            .and_then(|row| row.keys().position(|k| k == key))
    };
    assert!(order("files") < order("commits"), "files lead: {data}");
    // One `page` cursor: each continuation names the one section it pages.
    let commit_page = data["next"]["nextCommitPage"]["query"]["queries"][0].clone();
    assert_eq!(commit_page["sections"], json!(["commits"]), "{commit_page}");
    assert_eq!(commit_page["page"], 2, "{commit_page}");
    assert_eq!(commit_page["head"], HEAD_SHA, "{commit_page}");
    let file_page = data["next"]["nextFilePage"]["query"]["queries"][0].clone();
    assert_eq!(file_page["head"], HEAD_SHA, "{file_page}");
    assert_eq!(file_page["sections"], json!(["files"]), "{file_page}");

    let files = call(&runtime, "ghGetHistoryItem", file_page)
        .await
        .expect("file page");
    let files = row_data(&files);
    assert!(
        files.get("commits").is_none(),
        "file page repeats commits: {files}"
    );
    // Patch-free files read as the inventory (rows grouped by directory).
    assert_eq!(inventory_len(&files["files"]), 100, "{files}");

    let commits = call(&runtime, "ghGetHistoryItem", commit_page)
        .await
        .expect("commit page");
    let commits = row_data(&commits);
    assert!(commits.get("files").is_none(), "{commits}");
    assert_eq!(
        commits["commits"].as_array().map(Vec::len),
        Some(50),
        "{commits}"
    );
    // Page 2 continues page 1: no commit repeats.
    let shas = |rows: &serde_json::Value| {
        rows.as_array()
            .into_iter()
            .flatten()
            .filter_map(|row| row["commitSha"].as_str().map(str::to_owned))
            .collect::<std::collections::BTreeSet<_>>()
    };
    let (first_shas, second_shas) = (shas(&data["commits"]), shas(&commits["commits"]));
    assert_eq!(first_shas.len(), 100, "{data}");
    assert!(first_shas.is_disjoint(&second_shas), "{commits}");
    // The last commit page keeps its page object.
    assert_eq!(
        commits["contentPagination"]["commits"],
        json!({"currentPage":2,"totalPages":2,"pageSize":100,"totalItems":150,"hasMore":false}),
        "{commits}"
    );
    // A commit page past the last is the shared out-of-range outcome, never
    // a bare `commits:[]` beside `aheadBy:150`.
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v3/repos/a/b/compare/{BASE_SHA}...{HEAD_SHA}"
        )))
        .and(query_param("page", "3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"ahead","ahead_by":150,"behind_by":0,"total_commits":150,
            "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":BASE_SHA},
            "commits":[],"files":[]
        })))
        .with_priority(1)
        .mount(&server)
        .await;
    let past = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":BASE_SHA,"head":HEAD_SHA,
            "pageSize":100,"page":3,"sections":["commits"]}),
    )
    .await
    .expect("commit page past the end");
    let past_row = &past.structured_content["results"][0];
    assert_eq!(row_status(&past), "outOfRange", "{past_row}");
    let past = row_data(&past);
    assert_eq!(past["reason"], "page 3 of 2", "{past}");
    assert_eq!(
        past["contentPagination"]["commits"]["outOfRange"], true,
        "{past}"
    );
    assert!(past["next"]["restart"].is_object(), "{past}");
    // The commits section alone lists the same first page as the summary.
    let section = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2",
            "pageSize":100,"sections":["commits"]}),
    )
    .await
    .expect("commits section");
    assert_eq!(shas(&row_data(&section)["commits"]), first_shas);
    runtime.close().await;
}

/// A compare with an unresolvable ref says "ref" (a branch, tag, or SHA),
/// never GitHub's "No commit found for SHA", and names both refs asked for.
#[tokio::test]
async fn bad_compare_ref_names_refs_not_a_sha() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BASE_SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/no-such-tag"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"message":"No commit found for SHA: no-such-tag"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"no-such-tag","extras":[]}),
    )
    .await
    .expect("compare row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    let message = data["error"].as_str().expect("message");
    assert!(!message.contains("No commit found"), "{data}");
    assert!(
        message.contains("v1") && message.contains("no-such-tag"),
        "{data}"
    );
    assert!(message.contains("branch, tag, or SHA"), "{data}");
    runtime.close().await;
}

/// A comparison's budget-sized file page pins its grid: its
/// `next.nextFilePage`, copied unchanged, continues the same file list alone
/// or batched with another row, so every walk lists each file exactly once.
#[tokio::test]
async fn compare_file_page_continuations_continue_the_same_grid_in_any_batch() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/v2"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HEAD_SHA))
        .mount(&server)
        .await;
    let files = (0..250)
        .map(|n| json!({"filename":format!("f{n:03}.rs"),"status":"modified","additions":1,"deletions":1,"sha":"3".repeat(40)}))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/api/v3/repos/a/b/compare/(v1|{BASE_SHA})\\.\\.\\.{HEAD_SHA}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"ahead","ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":BASE_SHA},
            "commits":[{"sha":"9".repeat(40),"commit":{"message":"c","author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}],
            "files":files
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let row = json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2",
        "mainGoal":"g","reasoning":"r","sections":["files"]});
    let other = json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2",
        "mainGoal":"g","reasoning":"r","sections":["commits"]});
    let mut calls = 0;
    for first_batched in [true, false] {
        let mut seen = Vec::new();
        let mut next = Some(row.clone());
        let mut batched = first_batched;
        while let Some(query) = next {
            let queries = if batched {
                json!([query, other.clone()])
            } else {
                json!([query])
            };
            calls += 1;
            let outcome = runtime
                .run(
                    format!("grid-{calls}"),
                    "ghGetHistoryItem".into(),
                    json!({ "queries": queries }),
                )
                .await
                .expect("compare file page");
            let content = outcome.structured_content;
            assert!(content.get("responsePagination").is_none(), "{content}");
            let data = &content["results"][0]["data"];
            let page = data["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(|row| row.rsplit(' ').next())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert!(!page.is_empty(), "an empty page mid-walk: {data}");
            seen.extend(page);
            next = data.pointer("/next/nextFilePage/query/queries/0").cloned();
            batched = !batched;
        }
        let unique = seen.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), seen.len(), "a file repeated");
        assert_eq!(seen.len(), 250, "{seen:?}");
    }
    runtime.close().await;
}

/// A pinned comparison's files-only continuation reads the comparison page
/// the first read cached: it spends no second compare request.
#[tokio::test]
async fn a_compare_file_page_continuation_reuses_the_first_read() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let first = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":BASE_SHA,"head":HEAD_SHA,
               "pageSize":3}),
    )
    .await
    .expect("compare");
    let data = row_data(&first).clone();
    assert_eq!(row_status(&first), "success", "{data}");
    let next = data["next"]["nextFilePage"]["query"].clone();
    assert!(next.is_object(), "{data}");
    let second = call(&runtime, "ghGetHistoryItem", next)
        .await
        .expect("file page");
    assert_eq!(row_status(&second), "success", "{}", row_data(&second));
    let compares = server
        .received_requests()
        .await
        .expect("recorded requests")
        .iter()
        .filter(|request| request.url.path().contains("/compare/"))
        .map(|request| request.url.to_string())
        .collect::<Vec<_>>();
    assert_eq!(compares.len(), 1, "{compares:#?}");
    runtime.close().await;
}
