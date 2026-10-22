//! ghGetHistoryItem compare views through the runtime against a mock
//! GitHub: file pages, patch windows, include scopes and both refs pinned.

use crate::support::{self, Run as _};

use serde_json::{Value, json};
use support::{Workspace, call, inventory_len, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BASE_SHA: &str = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const HEAD_SHA: &str = "2222222bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// A 150-commit comparison at GitHub's 300-file cap: commit pages of 100.
async fn mount_capped_compare(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/v2"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HEAD_SHA))
        .mount(server)
        .await;
    let files = (0..300)
        .map(|n| json!({"filename":format!("src/f{n:03}.rs"),"status":"modified","additions":1,"deletions":1,"sha":"3".repeat(40)}))
        .collect::<Vec<_>>();
    for (page, count) in [(1, 100), (2, 50)] {
        let commits = (0..count)
            .map(|n| json!({"sha":format!("{:040x}", page * 1000 + n),"commit":{"message":format!("c{n}\n\nWhy: detail {n}"),"author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}))
            .collect::<Vec<_>>();
        Mock::given(method("GET"))
            .and(path_regex(format!(
                "^/api/v3/repos/a/b/compare/(v1|{BASE_SHA})\\.\\.\\.{HEAD_SHA}$"
            )))
            .and(query_param("page", page.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status":"ahead","ahead_by":150,"behind_by":0,"total_commits":150,
                "permalink_url":"https://github.com/a/b/compare/a:1111111...a:2222222",
                "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":BASE_SHA},
                "commits":commits,"files":files
            })))
            .mount(server)
            .await;
    }
}

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

/// A path scope past GitHub's 300-file compare list is not "unchanged": the
/// row warns and leads to the path's commits up to the pinned head.
#[tokio::test]
async fn capped_compare_with_a_path_leads_to_the_path_history() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let out = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2","include":["lib/late.rs"],"extras":[]}),
    )
    .await
    .expect("compare");
    let data = row_data(&out).clone();
    assert_eq!(data["files"], json!([]), "{data}");
    let lead = &crate::support::checks::hint(&data, "narrowScope");
    assert_eq!(lead["tool"], "ghSearchHistory", "{data}");
    let row = &lead["query"]["queries"][0];
    assert_eq!(row["operation"], "commit", "{data}");
    assert_eq!(row["path"], "lib/late.rs", "{data}");
    assert_eq!(row["ref"], HEAD_SHA, "{data}");
    assert!(
        data["warnings"].to_string().contains("lib/late.rs"),
        "{data}"
    );
    // FIX §0 #3: the range's commit list is not path-scoped, so it names
    // each commit by its headline only.
    let commit = &data["commits"][0];
    assert_eq!(commit["messageHeadline"], "c0", "{data}");
    assert!(commit.get("message").is_none(), "{data}");
    runtime.close().await;
}

/// A comparison's patch window past the first carries files only: the
/// commit list rode the first window with its own page cursor.
#[tokio::test]
async fn compare_patch_windows_do_not_resend_the_commit_list() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let hop = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2",
               "pageSize":100,"sections":["patches"],"offset":5,"extras":[]}),
    )
    .await
    .expect("compare hop");
    let data = row_data(&hop);
    assert!(data.get("commits").is_none(), "{data}");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
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

/// Past GitHub's 300-file compare list the two commits' trees name every
/// other changed path (react v18.2.0...v19.0.0: 300 of thousands): the
/// file list is complete, those rows carry no line counts, and no cap
/// marker or commit-walk lead remains.
#[tokio::test]
async fn capped_compare_reaches_every_changed_path_through_the_trees() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let entry =
        |path: &str, sha: &str| json!({"path":path,"type":"blob","mode":"100644","sha":sha});
    let listed = |sha: &str| {
        (0..300)
            .map(|n| entry(&format!("src/f{n:03}.rs"), sha))
            .collect::<Vec<_>>()
    };
    let mut base = listed(&"4".repeat(40));
    base.push(entry("lib/old.rs", &"5".repeat(40)));
    base.push(entry("lib/same.rs", &"6".repeat(40)));
    let mut head = listed(&"3".repeat(40));
    head.push(entry("lib/new.rs", &"7".repeat(40)));
    head.push(entry("lib/same.rs", &"6".repeat(40)));
    for (sha, tree) in [(BASE_SHA, base), (HEAD_SHA, head)] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/git/trees/{sha}")))
            .and(query_param("recursive", "1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"sha":sha,"truncated":false,"tree":tree})),
            )
            .expect(2)
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let read = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2",
               "sections":["files"],"include":["lib/**"],"extras":["hints","stats"]}),
    )
    .await
    .expect("compare past the cap");
    let data = row_data(&read).clone();
    assert_eq!(row_status(&read), "success", "{data}");
    assert_eq!(
        data["files"],
        json!([{"lib/": ["A !omitted new.rs", "D !omitted old.rs"]}]),
        "{data}"
    );
    assert!(data.get("partialReasons").is_none(), "{data}");
    assert!(
        crate::support::checks::hint(&data, "listCommits").is_null(),
        "{data}"
    );
    assert!(data["warnings"].to_string().contains("trees"), "{data}");
    // The tree source is stated once: a later page lists its rows without
    // repeating it.
    let later = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":BASE_SHA,"head":HEAD_SHA,
               "sections":["files"],"include":["lib/**"],"pageSize":1,"page":2,"extras":[]}),
    )
    .await
    .expect("later page past the cap");
    let later = row_data(&later).clone();
    assert_eq!(later["files"], json!(["D !omitted lib/old.rs"]), "{later}");
    assert!(later.get("warnings").is_none(), "{later}");
    runtime.close().await;
}

/// `include` scopes a comparison's files like a pull request's; past
/// GitHub's 300-file compare list a scoped path is disclosed as possibly
/// missing, with the path's commit history as the lead.
#[tokio::test]
async fn compare_include_scopes_files_and_capped_scope_leads_to_path_history() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let listed = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2",
               "include":["src/f001.rs","src/f2*.rs"],"extras":["stats"]}),
    )
    .await
    .expect("compare include");
    let data = row_data(&listed).clone();
    assert_eq!(row_status(&listed), "success", "{data}");
    let files = data["files"].to_string();
    assert!(
        files.contains("f001.rs") && files.contains("f200.rs"),
        "{data}"
    );
    assert!(!files.contains("f002.rs"), "{data}");
    // At GitHub's cap the comparison's count is only a floor: no count is
    // claimed. An omitted pageSize fills the response page, so the scope's
    // 101 files are one complete page: listed, with no files page block.
    assert!(data.get("changedFilesCount").is_none(), "{data}");
    assert!(data["contentPagination"].get("files").is_none(), "{data}");
    assert_eq!(files.matches(".rs").count(), 101, "{data}");
    let beyond = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2",
               "include":["lib/**"],"extras":[]}),
    )
    .await
    .expect("compare include beyond cap");
    let data = row_data(&beyond).clone();
    assert!(
        data["warnings"].to_string().contains("300 changed files"),
        "{data}"
    );
    let row = &crate::support::checks::hint(&data, "narrowScope")["query"]["queries"][0];
    assert_eq!(
        crate::support::checks::hint(&data, "narrowScope")["tool"],
        "ghSearchHistory",
        "{data}"
    );
    assert_eq!(row["path"], "lib/", "{data}");
    assert_eq!(row["ref"], HEAD_SHA, "{data}");
    runtime.close().await;
}

/// A comparison's patch view reads its top file at the head and at the
/// merge base (the old side of a three-dot diff). Two files: a complete
/// one-file answer offers one lead only.
#[tokio::test]
async fn compare_patch_view_reads_head_and_merge_base() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex(format!("^/api/v3/repos/a/b/compare/{BASE_SHA}\\.\\.\\.{HEAD_SHA}$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"ahead","ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":"3333333ccccccccccccccccccccccccccccccccc"},
            "commits":[{"sha":HEAD_SHA,"commit":{"message":"m","author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}],
            "files":[{"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,
                      "patch":"@@ -5,1 +7,1 @@\n-x\n+y"},
                     {"filename":"docs/b.md","status":"modified","additions":1,"deletions":0,
                      "patch":"@@ -1,0 +1,1 @@\n+z"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":BASE_SHA,"head":HEAD_SHA,
               "sections":["patches"],"extras":["hints"]}),
    )
    .await
    .expect("compare patches");
    let data = row_data(&outcome);
    assert_eq!(
        data["files"][0]["patch"], "@@ -5,1 +7,1 @@\n5\t-x\n7\t+y",
        "{data}"
    );
    assert_eq!(
        crate::support::checks::hint(data, "readAtCommit")["query"]["queries"][0]["ref"],
        HEAD_SHA,
        "{data}"
    );
    let parent = &crate::support::checks::hint(data, "readParent")["query"]["queries"][0];
    assert_eq!(
        parent["ref"], "3333333ccccccccccccccccccccccccccccccccc",
        "{data}"
    );
    assert_eq!(parent["ranges"], json!(["1-15"]), "{data}");
    runtime.close().await;
}

/// A compare patch grep keeps the files whose patch it hits, each narrowed
/// to its hits, like a pull-request grep; the internal narrowed-view marker
/// stays off the rows.
#[tokio::test]
async fn compare_match_string_keeps_only_hit_files() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex(format!("^/api/v3/repos/a/b/compare/{BASE_SHA}\\.\\.\\.{HEAD_SHA}$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"ahead","ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":BASE_SHA},
            "commits":[{"sha":HEAD_SHA,"commit":{"message":"m","author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}],
            "files":[
                {"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,
                 "patch":"@@ -1,3 +1,3 @@\n a\n-old needle\n+new needle\n b"},
                {"filename":"docs/b.md","status":"modified","additions":1,"deletions":0,
                 "patch":"@@ -1,0 +1,1 @@\n+unrelated"}
            ]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":BASE_SHA,"head":HEAD_SHA,
               "sections":["patches"],"matchString":"needle","contextLines":0,"extras":[]}),
    )
    .await
    .expect("compare grep");
    let data = row_data(&outcome);
    let files = data["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{data}");
    assert_eq!(files[0]["path"], "src/a.rs", "{data}");
    assert!(files[0].get("fullPatchChars").is_none(), "{data}");
    let patch = files[0]["patch"].as_str().expect("patch");
    assert!(
        patch.contains("needle") && !patch.contains("\n a\n"),
        "{patch}"
    );
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
