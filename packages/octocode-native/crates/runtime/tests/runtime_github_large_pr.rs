//! Large pull-request review: changed-file inventory, patch availability,
//! provider file-list cap, continuation payload size, and scan latency.

use crate::support::{self, Run as _};

use serde_json::{Value, json};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use support::{Workspace, call, row_data};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// When each request reached the mock: wiremock asks a responder for its
/// template on arrival, before the template's delay runs.
#[derive(Clone, Default)]
struct Arrivals(Arc<Mutex<Vec<Instant>>>);

impl Arrivals {
    fn sorted(&self) -> Vec<Instant> {
        let mut times = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        times.sort();
        times
    }
}

struct Recording(ResponseTemplate, Arrivals);

impl Respond for Recording {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        (self.1)
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Instant::now());
        self.0.clone()
    }
}

/// No page and no lead: `next` is absent and `hints` holds at most prose.
fn no_continuation(value: &serde_json::Value) -> bool {
    value.get("next").is_none()
        && value
            .get("hints")
            .and_then(serde_json::Value::as_object)
            .is_none_or(|hints| hints.keys().all(|key| key == "text"))
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn pr(changed_files: usize) -> Value {
    json!({
        "number": 9, "title": "Large refactor", "state": "closed",
        "merged_at": "2024-01-03T00:00:00Z",
        "merge_commit_sha": "fedcba9876543210fedcba9876543210fedcba98",
        "draft": false, "body": "x".repeat(900),
        "user": {"login": "alice"}, "labels": [{"name": "refactor"}],
        "head": {"sha": SHA, "ref": "feat"}, "base": {"ref": "main"},
        "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
        "closed_at": "2024-01-03T00:00:00Z",
        "comments": 4, "review_comments": 2,
        "changed_files": changed_files, "additions": 5000, "deletions": 4000
    })
}

/// A numbered patch view without its gutter (`N\t`: new-side numbers on
/// kept and added lines, old-side on removed ones): the raw patch.
fn raw_patch(view: &str) -> String {
    view.split_inclusive('\n')
        .map(|line| match line.split_once('\t') {
            Some((gutter, text))
                if !line.starts_with("@@") && gutter.bytes().all(|b| b.is_ascii_digit()) =>
            {
                text
            }
            _ => line,
        })
        .collect()
}

fn rest_file(name: &str, patch: Option<&str>, additions: u64, deletions: u64) -> Value {
    let mut file = json!({
        "sha": "1111111111111111111111111111111111111111", "filename": name,
        "status": "modified", "additions": additions, "deletions": deletions,
        "changes": additions + deletions
    });
    if let Some(patch) = patch {
        file["patch"] = json!(patch);
    }
    file
}

async fn mount_pr(server: &MockServer, changed_files: usize) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pr(changed_files)))
        .mount(server)
        .await;
}

/// Mount `batches` provider file pages; every page but the last links `next`.
async fn mount_file_batches(
    server: &MockServer,
    batches: Vec<Vec<Value>>,
    delay: Duration,
) -> Arrivals {
    let arrivals = Arrivals::default();
    let count = batches.len();
    for (index, files) in batches.into_iter().enumerate() {
        let page = index + 1;
        let mut response = ResponseTemplate::new(200)
            .set_delay(delay)
            .set_body_json(Value::Array(files));
        if page < count {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/pulls/9/files?per_page=100&page={}>; rel=\"next\"",
                    server.uri(),
                    page + 1
                )
                .as_str(),
            );
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/pulls/9/files"))
            .and(query_param("page", page.to_string()))
            .respond_with(Recording(response, arrivals.clone()))
            .mount(server)
            .await;
    }
    arrivals
}

fn numbered(batch: usize, size: usize) -> Vec<Value> {
    (0..size)
        .map(|i| {
            rest_file(
                &format!("src/b{batch}/f{i}.rs"),
                Some("@@ -1 +1 @@\n-a\n+b"),
                1,
                1,
            )
        })
        .collect()
}

async fn run(server: &MockServer, query: Value) -> Value {
    let (status, data) = run_status(server, query).await;
    assert_eq!(status, "success", "{data}");
    data
}

async fn run_status(server: &MockServer, query: Value) -> (String, Value) {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut query = query;
    query["operation"] = json!("pullRequest");
    query["owner"] = json!("a");
    query["repo"] = json!("b");
    query["number"] = json!(9);
    // A query without `extras` reads the full view.
    let extras = match query.as_object_mut().and_then(|row| row.remove("extras")) {
        Some(kinds) => serde_json::from_value::<Vec<String>>(kinds).expect("extras"),
        None => support::ALL_EXTRAS.map(str::to_owned).to_vec(),
    };
    let extras = extras.iter().map(String::as_str).collect::<Vec<_>>();
    let row = support::run(&runtime, "ghGetHistoryItem", query, &extras).await;
    let (status, data) = (row.status().to_owned(), row.data().clone());
    runtime.close().await;
    (status, data)
}

#[tokio::test]
async fn large_pr_inventory_flags_patchless_files_and_keeps_rename_origin() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 5).await;
    let mut renamed = rest_file("src/new_name.rs", None, 0, 0);
    renamed["status"] = json!("renamed");
    renamed["previous_filename"] = json!("src/old_name.rs");
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/ok.rs", Some("@@ -1 +1 @@\n-a\n+b"), 1, 1),
            renamed,
            rest_file("src/checker.ts", None, 39_550, 39_342),
            rest_file("assets/logo.png", None, 0, 0),
            rest_file("src/omitted.ts", None, 0, 0),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["files"], "extras": []})).await;
    // Compact rows: every file of one directory shares its first group.
    assert_eq!(
        data["pullRequests"][0]["files"],
        json!([
            {"src/": [
                "M +1 -1 ok.rs",
                "R +0 -0 new_name.rs <- src/old_name.rs",
                "M +39550 -39342 !tooLarge checker.ts",
                "M !omitted omitted.ts"
            ]},
            "M +0 -0 !binary assets/logo.png"
        ]),
        "{data}"
    );
}

#[tokio::test]
async fn pr_inventory_carries_the_identity_header_and_its_own_next_steps_only() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["files"], "extras": ["hints"]})).await;
    let row = &data["pullRequests"][0];
    // A section read keeps what names the PR and the commits code is read at.
    for kept in [
        "number",
        "title",
        "author",
        "state",
        "headSha",
        "mergeCommitSha",
    ] {
        assert!(row.get(kept).is_some(), "{kept} missing: {row}");
    }
    // The first page of a file inventory states how many files it lists.
    assert_eq!(row["changedFilesCount"], 250, "{row}");
    // The rest of the header rides the summary read only.
    for dropped in [
        "additions",
        "deletions",
        "bodyPreview",
        "updatedAt",
        "headBranch",
        "closedAt",
        "mergedAt",
        "baseBranch",
        "labels",
    ] {
        assert!(row.get(dropped).is_none(), "{dropped} kept: {row}");
    }
    // The next steps are the response's leads, not a list nested in the row.
    assert!(row.get("hints").is_none(), "{row}");
    let menu = crate::support::checks::leads(&data);
    // 250 files: no every-patch read, no placeholder literal search (only
    // the caller knows the literal), and no merge-commit read beside the
    // row's mergeCommitSha.
    assert_eq!(
        menu.iter()
            .map(|lead| lead.name.as_str())
            .collect::<Vec<_>>(),
        ["readSelectedPatches"],
        "{row}"
    );
    // The review read names up to one patch page of source files.
    let review = &crate::support::checks::hint(&data, "readSelectedPatches")["query"]["queries"][0]
        ["include"];
    assert_eq!(review.as_array().map(Vec::len), Some(30), "{row}");
    // An omitted pageSize reads the whole 250-file inventory in one page.
    let page = row.get("contentPagination").cloned().unwrap_or_default();
    assert!(page.get("files").is_none(), "{page}");
    let groups = row["files"].as_array().expect("files");
    let rows = groups
        .iter()
        .map(|group| {
            group
                .as_object()
                .and_then(|g| g.values().next())
                .and_then(Value::as_array)
                .map_or(1, Vec::len)
        })
        .sum::<usize>();
    assert_eq!(rows, 250, "{row}");
}

/// The review pick is every source file, most changed first (not the
/// changeset, not the tests), labelled as a ranking guess; a small PR keeps
/// the every-patch read beside it.
#[tokio::test]
async fn pr_inventory_picks_the_largest_source_patch_and_keeps_all_patches() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 5).await;
    let hunk = Some("@@ -1 +1 @@\n-a\n+b");
    mount_file_batches(
        &server,
        vec![vec![
            rest_file(".changeset/sse-keepalive.md", hunk, 8, 0),
            rest_file("src/server/sseKeepAlive.ts", hunk, 15, 0),
            rest_file("src/server/webStandardStreamableHttp.ts", hunk, 241, 94),
            rest_file("test/server/sseKeepAlive.test.ts", hunk, 29, 0),
            rest_file("test/server/streamableHttp.test.ts", hunk, 879, 3),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["files"], "extras": ["hints"]})).await;
    let menu = &data["hints"];
    let selected = &crate::support::checks::hint(&data, "readSelectedPatches");
    // Source files only. Files that fit one call's patch budget come first:
    // at the default 20,000-char page the 335-line patch does not fit, so the
    // 15-line one leads and the larger follows through `continuePatch`.
    assert_eq!(
        selected["query"]["queries"][0]["include"],
        json!([
            "src/server/sseKeepAlive.ts",
            "src/server/webStandardStreamableHttp.ts"
        ]),
        "{menu}"
    );
    assert_eq!(
        crate::support::checks::hint(&data, "readPatches")["query"]["queries"][0]["sections"],
        json!(["patches"]),
        "{menu}"
    );
    assert!(data["pullRequests"][0].get("hints").is_none(), "{data}");
}

#[tokio::test]
async fn pr_file_filter_narrows_the_inventory_and_its_counts() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    let mut batches = vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)];
    batches[1][7] = rest_file("docs/guide.md", Some("@@ -1 +1 @@\n-a\n+b"), 30, 2);
    batches[2][3]["status"] = json!("added");
    mount_file_batches(&server, batches, Duration::ZERO).await;
    let data = run(
        &server,
        json!({"sections":["files"],
               "include":["*.md","src/b3/"],"status":["added","modified"],"minChanges":3}),
    )
    .await;
    let row = &data["pullRequests"][0];
    assert_eq!(row["files"], json!(["M +30 -2 docs/guide.md"]), "{row}");
    assert_eq!(row["contentPagination"]["files"]["totalItems"], 1, "{row}");
    let data = run(
        &server,
        json!({"sections":["files"], "extras": [],
               "include":["src/b3/"],"status":["added"]}),
    )
    .await;
    assert_eq!(
        data["pullRequests"][0]["files"],
        json!(["A +1 -1 src/b3/f3.rs"]),
        "{data}"
    );
    assert!(
        no_continuation(&data["pullRequests"][0]),
        "filtered reads carry no menu: {data}"
    );
}

#[tokio::test]
async fn pure_rename_patch_is_empty_not_a_provider_omission() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    let mut renamed = rest_file("src/new_name.rs", None, 0, 0);
    renamed["status"] = json!("renamed");
    renamed["previous_filename"] = json!("src/old_name.rs");
    mount_file_batches(
        &server,
        vec![vec![renamed, rest_file("src/checker.ts", None, 10, 2)]],
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["patches"], "extras": []})).await;
    let files = &data["pullRequests"][0]["files"];
    assert_eq!(files[0]["path"], "src/new_name.rs", "{files}");
    assert_eq!(files[0]["previousPath"], "src/old_name.rs", "{files}");
    assert_eq!(files[0]["patch"], "", "{files}");
    assert!(files[0].get("patchUnavailable").is_none(), "{files}");
    assert_eq!(files[1]["patchUnavailable"], "tooLarge", "{files}");
}

#[tokio::test]
async fn pr_file_list_stopped_by_the_provider_cap_is_not_complete() {
    // GitHub lists at most 3000 files: the last listable page has no `next`
    // link although the PR reports more changed files.
    let server = MockServer::builder().start().await;
    mount_pr(&server, 3500).await;
    mount_file_batches(&server, vec![numbered(1, 100)], Duration::ZERO).await;
    let data = run(
        &server,
        json!({"sections":["files"], "pageSize": 100, "extras": []}),
    )
    .await;
    // The section cursors always show; the provider limit is the
    // row's evidence beside them.
    assert!(
        data["pullRequests"][0].get("contentPagination").is_some(),
        "{data}"
    );
    assert_eq!(data["terminalLimit"], true, "{data}");
    assert_eq!(
        data["providerLimit"]["reason"], "providerFileListLimit",
        "{data}"
    );
    assert_eq!(data["providerLimit"]["listed"], 100, "{data}");
    assert_eq!(data["providerLimit"]["changedFilesCount"], 3500, "{data}");
    assert!(
        data["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&json!("providerFileListLimit"))),
        "{data}"
    );
}

#[tokio::test]
async fn pr_inventory_page_reports_the_provider_total() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"sections":["files"], "pageSize": 100, "extras": ["stats"]}),
    )
    .await;
    let page = &data["pullRequests"][0]["contentPagination"]["files"];
    assert_eq!(page["totalItems"], 250, "{page}");
    assert_eq!(page["totalPages"], 3, "{page}");
    assert_eq!(page["hasMore"], true, "{page}");
    // The cursor rides `next`, never the page block.
    assert!(page.get("nextPage").is_none(), "{page}");
    let next = &data["next"]["nextFilePage"]["query"]["queries"][0];
    assert_eq!(next["page"], 2, "{data}");
    assert_eq!(next["sections"], json!(["files"]), "{data}");
}

#[tokio::test]
async fn pr_continuation_reads_carry_only_the_identity_header() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    // Page one carries the identity header and the menu; a later page
    // re-proves identity only (the summary read owns the rest of the header):
    // number, title, author, state and the commits code is read at.
    let query = json!({"sections":["files"], "pageSize": 100, "extras": ["hints"]});
    let follow = json!({"sections":["files"], "pageSize": 100, "page": 2, "extras": ["hints"]});
    let first = run(&server, query).await;
    let first_row = &first["pullRequests"][0];
    assert_eq!(first_row["headSha"], SHA, "{first_row}");
    assert_eq!(first_row["title"], "Large refactor", "{first_row}");
    assert!(first_row.get("bodyPreview").is_none(), "{first_row}");
    assert!(first_row.get("hints").is_none(), "{first_row}");
    assert!(first.get("hints").is_some(), "{first}");
    // A literal search of every patch needs the caller's literal: it is
    // never offered as an executable placeholder.
    assert!(
        crate::support::checks::hint(&first, "findInPatches").is_null(),
        "{first}"
    );

    let data = run(&server, follow).await;
    let row = &data["pullRequests"][0];
    for kept in [
        "number",
        "title",
        "author",
        "state",
        "headSha",
        "mergeCommitSha",
    ] {
        assert!(row.get(kept).is_some(), "{kept} missing: {row}");
    }
    for dropped in [
        "changedFilesCount",
        "mergedAt",
        "baseBranch",
        "closedAt",
        "labels",
        "headBranch",
        "updatedAt",
        "commentsCount",
        "additions",
        "deletions",
        "bodyPreview",
        "next",
    ] {
        assert!(row.get(dropped).is_none(), "{dropped} repeated: {row}");
    }
    // 100 files in one directory: one group of 100 compact rows.
    let group = &row["files"][0]["src/b2/"];
    assert_eq!(group.as_array().map(Vec::len), Some(100), "{row}");
    assert!(data["next"]["nextFilePage"].is_object(), "{data}");

    // The details extra keeps the full header.
    let details = json!({"sections":["files"], "pageSize": 100, "page": 2, "extras": ["details"]});
    let details = run(&server, details).await;
    assert_eq!(details["pullRequests"][0]["labels"], json!(["refactor"]));
}

#[tokio::test]
async fn patch_window_does_not_repeat_the_file_cursor_in_content_pagination() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    let big = format!("@@ -1,2000 +1,2000 @@\n{}", "+line\n".repeat(12_000));
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/big.rs", Some(&big), 4_000, 0),
            rest_file("src/small.rs", Some("@@ -1 +1 @@\n-a\n+b"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"sections":["patches"],"include":["src/big.rs"],
               "extras": []}),
    )
    .await;
    let row = &data["pullRequests"][0];
    let file = &row["files"][0];
    assert_eq!(file["patchPagination"]["hasMore"], true, "{row}");
    let pages = &row["contentPagination"];
    // The finished single-page file list and the cursor copy add nothing.
    assert!(pages.get("files").is_none(), "{pages}");
    assert!(pages["patches"].get("offset").is_none(), "{pages}");
    assert!(pages["patches"].get("totalChars").is_none(), "{pages}");
    assert!(data["next"]["continuePatch"].is_object(), "{data}");
}

#[tokio::test]
async fn selected_patch_scan_reads_file_batches_concurrently() {
    // A selected late file must not cost one sequential round trip per
    // provider batch before it: the six batches are read in parallel. Each
    // reply is held `delay`, so a sequential scan sends no request within
    // `delay` of the one before; overlapping requests prove the parallel
    // reads whatever the machine's load (a wall-clock bound does not).
    let delay = Duration::from_millis(300);
    let server = MockServer::builder().start().await;
    mount_pr(&server, 600).await;
    let mut batches = (1..=6).map(|b| numbered(b, 100)).collect::<Vec<_>>();
    batches[5][99] = rest_file("src/late.rs", Some("@@ -1 +1 @@\n-a\n+late"), 1, 1);
    let arrivals = mount_file_batches(&server, batches, delay).await;
    let data = run(
        &server,
        json!({"sections":["patches"],"include":["src/late.rs"],
               "extras": []}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    assert_eq!(file["path"], "src/late.rs", "{data}");
    assert_eq!(file["patch"], "@@ -1 +1 @@\n1\t-a\n1\t+late");
    let times = arrivals.sorted();
    assert_eq!(times.len(), 6, "every batch is read once");
    let overlapping = times
        .windows(2)
        .filter(|pair| pair[1].duration_since(pair[0]) < delay)
        .count();
    assert!(overlapping > 0, "sequential scan: {times:?}");
}

#[tokio::test]
async fn match_string_returns_matching_hunks_and_offers_the_whole_patch() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    let body = (1..=200)
        .map(|n| format!(" line {n}\n"))
        .collect::<String>();
    let big = format!("@@ -1,401 +1,401 @@\n{body}-old esbuild\n+new esbuild\n{body}");
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/big.rs", Some(&big), 1, 1),
            rest_file("src/other.rs", Some("@@ -1 +1 @@\n-a\n+b"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": "esbuild",
               "extras": []}),
    )
    .await;
    let files = &data["pullRequests"][0]["files"];
    assert_eq!(files.as_array().map(Vec::len), Some(1), "{data}");
    let patch = files[0]["patch"].as_str().expect("patch");
    // The contract's default context lines around each hit
    // (`limits.defaultReadContextLines`, 3), the rest re-read whole.
    assert!(
        patch.starts_with("@@ -198,7 +198,7 @@\n198\t line 198\n"),
        "{patch}"
    );
    assert!(
        patch.contains("\n201\t-old esbuild\n201\t+new esbuild\n"),
        "{patch}"
    );
    assert!(patch.ends_with("204\t line 3\n"), "{patch}");
    assert!(patch.len() < 600, "{patch}");
    // The narrowed-view marker selects the continuation; rows omit it.
    assert!(files[0].get("fullPatchChars").is_none(), "{data}");
    // A patch read carries the identity it re-proves (number, title, author,
    // state, the head it read) and no follow-up menu; the summary read dates
    // the PR.
    let row = &data["pullRequests"][0];
    assert!(no_continuation(row), "{row}");
    for dropped in [
        "additions",
        "deletions",
        "createdAt",
        "mergedAt",
        "baseBranch",
    ] {
        assert!(row.get(dropped).is_none(), "{dropped} kept: {row}");
    }
    // Every view names the merge commit beside the head it read.
    for kept in [
        "number",
        "title",
        "author",
        "state",
        "headSha",
        "mergeCommitSha",
    ] {
        assert!(row.get(kept).is_some(), "{kept} dropped: {row}");
    }
    // Without a patch row the read carries the same slim identity header.
    let (status, none) = run_status(
        &server,
        json!({"sections":["patches"], "matchString": "absent-term",
               "extras": []}),
    )
    .await;
    let none_row = &none["pullRequests"][0];
    assert_eq!(none_row["headSha"], SHA, "{none}");
    assert_eq!(none_row["state"], "merged", "{none}");
    assert_eq!(none_row["title"], "Large refactor", "{none}");
    assert!(none_row.get("bodyPreview").is_none(), "{none}");
    // A matchString that hits no patch line is an empty read, not a summary
    // that looks like an ignored filter; its leads run the fix: the same
    // read without the literal, then the file inventory.
    assert_eq!(status, "empty", "{none}");
    assert!(
        crate::support::checks::hint(&none, "text").is_null(),
        "{none}"
    );
    let unfiltered =
        &crate::support::checks::hint(&none, "withoutMatchString")["query"]["queries"][0];
    assert!(unfiltered.get("matchString").is_none(), "{none}");
    assert_eq!(unfiltered["sections"], json!(["patches"]), "{none}");
    assert!(unfiltered.get("minify").is_none(), "{none}");
    assert_eq!(
        crate::support::checks::hint(&none, "readFiles")["query"]["queries"][0]["sections"],
        json!(["files"]),
        "{none}"
    );
    support::checks::assert_leads_valid(&none);
    let only_hits = run(
        &server,
        json!({"sections":["patches"], "matchString": "esbuild",
               "contextLines": 0, "extras": []}),
    )
    .await;
    assert_eq!(
        only_hits["pullRequests"][0]["files"][0]["patch"],
        "@@ -201,1 +201,1 @@\n201\t-old esbuild\n201\t+new esbuild\n"
    );
    // The grep shows what was asked: the whole patch is not a page (no
    // `next`, the read is complete), it is an opt-in lead.
    assert!(
        only_hits["next"].get("readFullPatches").is_none(),
        "{only_hits}"
    );
    assert!(data.get("isPartial").is_none(), "{data}");
    let hinted = run(
        &server,
        json!({"sections":["patches"], "matchString": "esbuild",
               "extras": ["hints"]}),
    )
    .await;
    // Distinct optional reads remain available, even for one file.
    assert!(
        crate::support::checks::hint(&hinted, "readAtCommit").is_object(),
        "{hinted}"
    );
    let whole = crate::support::checks::hint(&hinted, "readFullPatches");
    assert_eq!(whole["tool"], "ghGetHistoryItem", "{hinted}");
    assert_eq!(whole["query"]["queries"][0]["sections"], json!(["patches"]));
    assert!(whole["query"]["queries"][0].get("matchString").is_none());
    // The whole patch is the same read without the grep, numbered.
    let full = run(
        &server,
        json!({"sections":["patches"], "include": ["src/big.rs"], "extras": []}),
    )
    .await;
    let whole = full["pullRequests"][0]["files"][0]["patch"]
        .as_str()
        .unwrap_or("");
    assert_eq!(raw_patch(whole), big, "{full}");
    assert!(whole.contains("\n201\t+new esbuild\n"), "{full}");
}

/// The default PR view is GitHub's verbatim text: a body's HTML comment
/// is evidence and stays, with no raw re-read to chase; code hunks are
/// never minified either: every context line arrives, numbered on the new
/// side.
#[tokio::test]
async fn pr_views_are_verbatim_by_default() {
    let server = MockServer::builder().start().await;
    let raw_body = "Fixes the cache.\n<!-- reviewer checklist: perf tested -->\nDetails here.";
    let mut meta = pr(2);
    meta["body"] = json!(raw_body);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(meta))
        .mount(&server)
        .await;
    let context = (1..=60).map(|n| format!(" ctx {n}\n")).collect::<String>();
    let big = format!("@@ -1,121 +1,121 @@ fn cache()\n{context}-old\n+new\n{context}");
    let small = "@@ -1 +1 @@\n-a\n+b";
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/big.rs", Some(&big), 1, 1),
            rest_file("src/small.rs", Some(small), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"sections":["body","patches"], "extras": ["hints"]}),
    )
    .await;
    let row = &data["pullRequests"][0];
    assert_eq!(row["body"], raw_body, "{row}");
    assert!(row.get("bodyView").is_none(), "{row}");
    let files = row["files"].as_array().expect("files");
    let whole = files
        .iter()
        .find(|f| f["path"] == "src/big.rs")
        .and_then(|f| f["patch"].as_str())
        .expect("big");
    assert_eq!(raw_patch(whole), big, "{whole}");
    assert!(
        whole.starts_with("@@ -1,121 +1,121 @@ fn cache()\n1\t ctx 1\n"),
        "{whole}"
    );
    assert!(
        whole.contains("\n61\t-old\n61\t+new\n62\t ctx 1\n"),
        "{whole}"
    );
    assert!(
        crate::support::checks::hint(&data, "readUntrimmed").is_null(),
        "{data}"
    );
    assert!(
        crate::support::checks::hint(&data, "readRawBody").is_null(),
        "{data}"
    );
    assert!(
        !crate::support::checks::hint(&data, "readAtMerge").is_null(),
        "{data}"
    );
}

/// `matchString` searches patches, so a pull-request read with a
/// literal and no content selection searches every patch (within
/// `include`) instead of silently returning only the summary.
#[tokio::test]
async fn match_string_without_content_searches_the_filtered_patches() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 3).await;
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/mod/a.ts", Some("@@ -1 +1 @@\n-x\n+import y"), 1, 1),
            rest_file("src/mod/b.ts", Some("@@ -1 +1 @@\n-x\n+z"), 1, 1),
            rest_file("lib/c.ts", Some("@@ -1 +1 @@\n-x\n+import w"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"matchString": "import", "contextLines": 0,
               "include":["src/mod/**"], "extras": []}),
    )
    .await;
    let files = data["pullRequests"][0]["files"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let paths = files
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec!["src/mod/a.ts"], "{data}");
    assert!(
        files[0]["patch"]
            .as_str()
            .unwrap_or("")
            .contains("+import y"),
        "{data}"
    );
}

/// A patch search never implies absence for files GitHub sent without a
/// patch: it lists them, marks the row partial, and offers a source read at
/// the PR head.
#[tokio::test]
async fn match_string_lists_the_patchless_files_it_could_not_search() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 4).await;
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/hit.ts", Some("@@ -1 +1 @@\n-x\n+import y"), 1, 1),
            rest_file("src/module.ts", None, 900, 12),
            rest_file("src/system.ts", None, 0, 0),
            rest_file("assets/logo.png", None, 0, 0),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": "import",
               "extras": []}),
    )
    .await;
    assert_eq!(data["isPartial"], true, "{data}");
    let reasons = data["partialReasons"].to_string();
    assert!(reasons.contains("patchUnavailable"), "{data}");
    let unsearched = data["pullRequests"][0]["unsearchedFiles"].to_string();
    assert!(unsearched.contains("src/module.ts"), "{data}");
    assert!(unsearched.contains("src/system.ts"), "{data}");
    assert!(
        !unsearched.contains("logo.png"),
        "binary is not text: {data}"
    );
    let read = &data["next"]["searchUnpatchedFile"];
    assert_eq!(read["tool"], "ghGetFileContent", "{data}");
    assert_eq!(read["query"]["queries"][0]["ref"], SHA, "{data}");
    assert_eq!(
        read["query"]["queries"][0]["matchString"], "import",
        "{data}"
    );
    assert_eq!(
        read["query"]["queries"][0]["path"], "src/module.ts",
        "{data}"
    );
}

/// A merged PR's merge state rides every PR row, whatever the content
/// selection: summary, body + inventory, inventory, patch read and a later
/// file page all carry `state:"merged"` and the `mergeCommitSha`; a read that
/// names no section (the summary, a bare `matchString`) also carries
/// `mergedAt`, `baseBranch` and the labels (never `closedAt`).
#[tokio::test]
async fn merged_pr_rows_keep_merge_state_on_every_read() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    let mut batches = vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)];
    batches[0][0] = rest_file("src/hit.rs", Some("@@ -1 +1 @@\n-a\n+needle"), 1, 1);
    mount_file_batches(&server, batches, Duration::ZERO).await;
    for (label, query, full_header) in [
        ("summary", json!({"extras": []}), true),
        (
            "body+inventory",
            json!({"sections":["body","files"], "extras": []}),
            false,
        ),
        (
            "inventory",
            json!({"sections":["files"], "extras": []}),
            false,
        ),
        (
            "patches",
            json!({"sections":["patches"],"include":["src/hit.rs"],
                   "extras": []}),
            false,
        ),
        (
            "matchString",
            json!({"matchString": "needle", "extras": []}),
            true,
        ),
        (
            "later page",
            json!({"sections":["files"], "pageSize": 100, "page": 2,
                   "extras": []}),
            false,
        ),
    ] {
        let data = run(&server, query).await;
        let row = &data["pullRequests"][0];
        assert_eq!(row["state"], "merged", "{label}: {row}");
        assert_eq!(
            row["mergeCommitSha"], "fedcba9876543210fedcba9876543210fedcba98",
            "{label}: {row}"
        );
        // The merge time is the close time; it is not stated twice.
        assert!(row.get("closedAt").is_none(), "{label}: {row}");
        if full_header {
            assert_eq!(row["mergedAt"], "2024-01-03T00:00:00Z", "{label}: {row}");
            assert_eq!(row["baseBranch"], "main", "{label}: {row}");
            assert_eq!(row["labels"], json!(["refactor"]), "{label}: {row}");
        } else {
            for dropped in ["mergedAt", "baseBranch", "labels"] {
                assert!(row.get(dropped).is_none(), "{label}: {dropped}: {row}");
            }
        }
    }
}

/// A file page pins the head it read (`headSha` on `next.nextFilePage`);
/// replayed after the head moved, the page is a stale snapshot whose
/// restart is the first page without the pin, never a mix of two heads.
#[tokio::test]
async fn pull_request_pages_pin_the_head_and_a_moved_head_restarts() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    let first = run(
        &server,
        json!({"sections":["files"], "pageSize": 100, "extras": []}),
    )
    .await;
    let page = &first["next"]["nextFilePage"]["query"]["queries"][0];
    assert_eq!(page["headSha"], SHA, "{first}");
    let mut moved = page.clone();
    moved["headSha"] = json!("abcdef0123456789abcdef0123456789abcdef01");
    for field in ["operation", "owner", "repo", "number"] {
        moved.as_object_mut().expect("row").remove(field);
    }
    moved["extras"] = json!([]);
    let (status, stale) = run_status(&server, moved).await;
    assert_eq!(status, "error", "{stale}");
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
    let restart = &stale["next"]["restart"]["query"]["queries"][0];
    assert!(restart.get("headSha").is_none(), "{stale}");
    assert!(restart.get("page").is_none(), "{stale}");
    // An abbreviated pin of the same head reads the page, not a false
    // stale snapshot.
    let mut short = page.clone();
    short["headSha"] = json!(&SHA[..7]);
    for field in ["operation", "owner", "repo", "number"] {
        short.as_object_mut().expect("row").remove(field);
    }
    short["extras"] = json!([]);
    let (status, read) = run_status(&server, short).await;
    assert_ne!(status, "error", "{read}");
    assert!(read.get("errorCode").is_none(), "{read}");
}

/// Rows of one call share one patch budget, so two large patch
/// reads fit one response page instead of spilling into response
/// pagination; each row keeps its own exact `continuePatch`.
#[tokio::test]
async fn patch_rows_in_one_call_share_one_budget() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    let big = |tag: &str| {
        format!(
            "@@ -1,9000 +1,9000 @@\n{}",
            format!("+{tag} line\n").repeat(3_000)
        )
    };
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("pyproject.toml", Some(&big("a")), 3_000, 0),
            rest_file("src/_runtime.py", Some(&big("b")), 3_000, 0),
        ]],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let row = |file: &str| {
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 9,
               "mainGoal": "g", "reasoning": "r",
               "sections": ["patches"], "include": [file]})
    };
    let outcome = runtime
        .run(
            "test-1".into(),
            "ghGetHistoryItem".into(),
            json!({"queries": [row("pyproject.toml"), row("src/_runtime.py")]}),
        )
        .await
        .expect("two patch rows");
    let content = &outcome.structured_content;
    assert!(
        content.get("responsePagination").is_none(),
        "two rows overflowed the page: {}",
        content
            .get("responsePagination")
            .cloned()
            .unwrap_or_default()
    );
    for index in 0..2 {
        let data = &content["results"][index]["data"];
        let file = &data["pullRequests"][0]["files"][0];
        let taken = file["patchPagination"]["length"].as_u64().unwrap_or(0);
        // Half the default window each (one budget), not a whole window each.
        assert!((4_000..=20_000).contains(&taken), "row {index}: {taken}");
        assert!(
            data["next"]["continuePatch"].is_object(),
            "row {index}: {data}"
        );
    }
    runtime.close().await;
}

/// A whole-PR patch walk that asks for a larger response page gets patch
/// windows sized to it, and the windows still tile every patch exactly (no
/// gap, no repeat). Every hop has the page budget of the first window and
/// runs unchanged.
#[tokio::test]
async fn explicit_response_page_sizes_patch_walk_windows() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    let patch = |tag: &str| {
        format!(
            "@@ -1,9000 +1,9000 @@\n{}",
            (0..6_000)
                .map(|i| format!("+{tag} line {i}\n"))
                .collect::<String>()
        )
    };
    let patches = [patch("a"), patch("b")];
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/a.rs", Some(&patches[0]), 6_000, 0),
            rest_file("src/b.rs", Some(&patches[1]), 6_000, 0),
        ]],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let walk = |page: Option<u64>, char_length: Option<u64>| {
        let runtime = &runtime;
        async move {
            let mut first = json!({"operation": "pullRequest", "owner": "a", "repo": "b",
                "number": 9, "mainGoal": "g", "reasoning": "r",
                "sections": ["patches"], "extras": ["hints"]});
            if let Some(length) = char_length {
                first["length"] = json!(length);
            }
            let with_page = |row: Value| {
                let mut envelope = json!({"queries": [row]});
                if let Some(page) = page {
                    envelope["responseLength"] = json!(page);
                }
                envelope
            };
            let mut hops = std::collections::VecDeque::from([with_page(first)]);
            let mut read = std::collections::BTreeMap::<String, String>::new();
            let mut calls = 0;
            while let Some(envelope) = hops.pop_front() {
                calls += 1;
                assert!(calls <= 60, "walk did not finish");
                let outcome = runtime
                    .run(format!("walk-{calls}"), "ghGetHistoryItem".into(), envelope)
                    .await
                    .expect("patch window");
                let content = &outcome.structured_content;
                // An explicit page reports its (single) response page.
                assert_ne!(
                    content["responsePagination"]["hasMore"], true,
                    "a patch window overflowed its page: {content}"
                );
                let data = &content["results"][0]["data"];
                // A length above the page is clamped and says so on the
                // call that asked; the windows below still tile every patch.
                let clamped = data["warnings"].as_array().is_some_and(|text| {
                    text.iter()
                        .any(|t| t.as_str().is_some_and(|t| t.starts_with("length")))
                });
                assert_eq!(
                    clamped,
                    char_length.is_some() && calls == 1,
                    "call {calls}: {data}"
                );
                for file in data["pullRequests"][0]["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let path = file["path"].as_str().expect("path").to_owned();
                    let text = read.entry(path).or_default();
                    let offset = file["patchPagination"]["offset"].as_u64().unwrap_or(0);
                    assert_eq!(offset as usize, text.chars().count(), "{file}");
                    text.push_str(file["patch"].as_str().unwrap_or(""));
                }
                // One check at the merge commit, on the first window only.
                assert_eq!(
                    !crate::support::checks::hint(data, "readAtMerge").is_null(),
                    calls == 1,
                    "call {calls}: {data}"
                );
                // Each hop is a complete one-row input: the same file page
                // at the stream cursor, with the page budget of the first
                // window.
                if let Some(next) = data["next"]["continuePatch"]["query"]["queries"][0].as_object()
                {
                    assert_eq!(
                        data["next"]["continuePatch"]["query"]["queries"]
                            .as_array()
                            .map(Vec::len),
                        Some(1),
                        "{data}"
                    );
                    assert_eq!(next.get("sections"), Some(&json!(["patches"])), "{data}");
                    assert!(next.get("include").is_none(), "{data}");
                    // A caller's oversized length rides the hop clamped to
                    // the window it got, so the clamp is said once.
                    match char_length {
                        None => assert!(next.get("length").is_none(), "{data}"),
                        Some(asked) => assert!(
                            next["length"].as_u64().is_some_and(|length| length < asked),
                            "{data}"
                        ),
                    }
                    hops.push_back(with_page(Value::Object(next.clone())));
                }
            }
            (calls, read)
        }
    };
    let (default_calls, default_read) = walk(None, None).await;
    let (explicit_calls, explicit_read) = walk(Some(50_000), None).await;
    let (_, oversized_read) = walk(None, Some(80_000)).await;
    for (path, patch) in ["src/a.rs", "src/b.rs"].iter().zip(&patches) {
        for read in [&default_read, &explicit_read, &oversized_read] {
            assert_eq!(
                read.get(*path).map(|view| raw_patch(view)).as_ref(),
                Some(patch),
                "{path}"
            );
        }
    }
    // A larger explicit response page carries larger windows: fewer hops.
    assert!(
        explicit_calls < default_calls,
        "explicit {explicit_calls} vs default {default_calls} calls"
    );
    runtime.close().await;
}

/// A literal search covers every changed file in one call (not 30
/// files a page) and returns hit lines only, so its bytes track the hits.
#[tokio::test]
async fn match_string_covers_every_file_page_in_one_call() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 136).await;
    let hit = |i: usize| {
        rest_file(
            &format!("tests/t{i}.rs"),
            Some(&format!(
                "@@ -1,5 +1,5 @@\n a\n b\n-#![cfg(not(miri))]\n+#![cfg(not(miri))] // {i}\n c\n d\n"
            )),
            1,
            1,
        )
    };
    let mut first = numbered(1, 100);
    for (slot, i) in (0..18).map(|i| (i * 5, i)) {
        first[slot] = hit(i);
    }
    let second = (18..36).map(hit).collect::<Vec<_>>();
    mount_file_batches(&server, vec![first, second], Duration::ZERO).await;
    let data = run(&server, json!({"matchString": "miri", "extras": []})).await;
    let files = data["pullRequests"][0]["files"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(files.len(), 36, "{data}");
    for file in &files {
        let patch = file["patch"].as_str().unwrap_or("");
        assert!(!patch.contains("\n a\n"), "context leaked: {patch}");
        assert!(
            file.get("status").is_none() && file["stat"] == "M +1 -1",
            "{file}"
        );
    }
    assert!(
        data.get("next")
            .and_then(|n| n.get("nextFilePage"))
            .is_none(),
        "{data}"
    );
}

/// An `include` scope narrows a body + patch read to the matching files
/// through the full runtime.
#[tokio::test]
async fn include_scopes_a_body_and_patch_read() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 3).await;
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/a.rs", Some("@@ -1 +1 @@\n-a\n+needle"), 1, 1),
            rest_file("docs/b.md", Some("@@ -1 +1 @@\n-a\n+b"), 1, 1),
            rest_file("src/c.rs", Some("@@ -1 +1 @@\n-a\n+c"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let flat = run(
        &server,
        json!({"sections":["body","patches"],
               "include":["src/**"], "extras": []}),
    )
    .await;
    assert!(flat["pullRequests"][0].get("body").is_some(), "{flat}");
    let paths = flat["pullRequests"][0]["files"]
        .as_array()
        .map(|files| {
            files
                .iter()
                .filter_map(|f| f["path"].as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(paths, ["src/a.rs", "src/c.rs"], "{flat}");
}

/// An `include` scope that matches no changed file is an empty row (a
/// wrong path once got a silent one) whose lead reads the whole file
/// inventory to copy a path from.
#[tokio::test]
async fn files_scope_matching_nothing_is_an_empty_row_with_an_inventory_lead() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 1).await;
    mount_file_batches(
        &server,
        vec![vec![rest_file(
            "fastapi/telemetry/_runtime.py",
            Some("@@ -1 +1 @@\n+a"),
            1,
            0,
        )]],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 9,
               "sections": ["patches"], "include": ["fastapi/_runtime.py"], "extras": []}),
    )
    .await
    .expect("read");
    let data = row_data(&outcome);
    assert!(
        data.get("errorCode").is_none(),
        "an empty row carries no errorCode: {data}"
    );
    assert!(
        crate::support::checks::hint(data, "text").is_null(),
        "{data}"
    );
    assert!(
        crate::support::checks::hint(data, "withoutMatchString").is_null(),
        "{data}"
    );
    let inventory = &crate::support::checks::hint(data, "readFiles")["query"]["queries"][0];
    assert_eq!(inventory["sections"], json!(["files"]), "{data}");
    assert!(inventory.get("include").is_none(), "{data}");
    support::checks::assert_leads_valid(data);
    runtime.close().await;
}

/// cli/cli#13541: the first patch window holds only docs, a test, and a
/// test too large for a patch, so its code file sits in a later window.
/// `readAtMerge` still names the most-changed code file of the whole PR
/// file list, not only of the window shown.
#[tokio::test]
async fn read_at_merge_picks_the_code_file_of_the_whole_pr_not_the_first_window() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 5).await;
    let test_patch = format!(
        "@@ -0,0 +1,3000 @@\n{}",
        (1..=3000)
            .map(|line| format!("+\tassert(t, line{line}, expected{line})"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let added = |name: &str, patch: Option<&str>, additions: u64| {
        let mut file = rest_file(name, patch, additions, 0);
        file["status"] = json!("added");
        file
    };
    let files = vec![
        added(
            "acceptance/README.md",
            Some("@@ -0,0 +1 @@\n+Run the suite"),
            1,
        ),
        added("acceptance/acceptance_test.go", Some(&test_patch), 3000),
        added("pkg/client/client_test.go", None, 3842),
        added(
            "pkg/client/client.go",
            Some("@@ -0,0 +1,3 @@\n+package client\n+func Alpha() {}\n+func Beta() {}"),
            3,
        ),
        added("pkg/client/types.go", Some("@@ -0,0 +1 @@\n+type T int"), 1),
    ];
    mount_file_batches(&server, vec![files], Duration::ZERO).await;
    let data = run(
        &server,
        json!({"sections":["patches"], "extras": ["hints"]}),
    )
    .await;
    let shown = data["pullRequests"][0]["files"]
        .as_array()
        .expect("patch rows")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect::<Vec<_>>();
    assert!(!shown.contains(&"pkg/client/client.go"), "{data}");
    let read = &crate::support::checks::hint(&data, "readAtMerge")["query"]["queries"][0];
    for (field, value) in [
        ("ref", json!("fedcba9876543210fedcba9876543210fedcba98")),
        ("path", json!("pkg/client/client.go")),
        // The merge commit is searched for the added code, not read
        // at `headSha` line numbers.
        ("matchString", json!(["func Alpha() {}"])),
    ] {
        assert_eq!(read[field], value, "{field}: {data}");
    }
}

/// A `matchString` that hits no patch line says so: alone it is an empty
/// row whose leads read without it and read the file inventory; beside a
/// body read it is a warning on the row, never a silent summary.
#[tokio::test]
async fn match_string_with_zero_hits_says_so() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/a.ts", Some("@@ -1 +1 @@\n-x\n+y"), 1, 1),
            rest_file("src/b.ts", Some("@@ -1 +1 @@\n-x\n+z"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let (status, alone) = run_status(
        &server,
        json!({"matchString": "absentNeedle", "extras": []}),
    )
    .await;
    assert_eq!(status, "empty", "{alone}");
    assert!(
        crate::support::checks::hint(&alone, "text").is_null(),
        "{alone}"
    );
    let unfiltered =
        &crate::support::checks::hint(&alone, "withoutMatchString")["query"]["queries"][0];
    assert!(unfiltered.get("matchString").is_none(), "{alone}");
    assert_eq!(unfiltered["number"], 9, "{alone}");
    // The literal alone searched every patch; the read without it reads them.
    assert_eq!(unfiltered["sections"], json!(["patches"]), "{alone}");
    assert_eq!(
        crate::support::checks::hint(&alone, "readFiles")["query"]["queries"][0]["sections"],
        json!(["files"]),
        "{alone}"
    );
    support::checks::assert_leads_valid(&alone);
    let with_body = run(
        &server,
        json!({"sections": ["body", "patches"], "matchString": "absentNeedle", "extras": []}),
    )
    .await;
    assert!(
        with_body["pullRequests"][0].get("body").is_some(),
        "{with_body}"
    );
    let warnings = with_body["warnings"].to_string();
    assert!(
        warnings.contains("matchString") && warnings.contains("absentNeedle"),
        "{with_body}"
    );
}

/// A later comment page of a large PR does not re-scan every provider batch
/// one round trip at a time: with the PR's comment count in hand, the
/// batches load at once, and the page holds exactly its comments.
#[tokio::test]
async fn later_review_comment_pages_load_provider_batches_concurrently() {
    let server = MockServer::builder().start().await;
    let mut raw = pr(1);
    raw["review_comments"] = json!(600);
    // The clock starts at the first provider request: building the HTTP
    // client (system proxy lookup) is a one-time cost, not a batch scan.
    let first_request = std::sync::Arc::new(std::sync::OnceLock::<Instant>::new());
    let pr_template = ResponseTemplate::new(200).set_body_json(raw);
    let arrival = first_request.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(move |_: &wiremock::Request| {
            arrival.get_or_init(Instant::now);
            pr_template.clone()
        })
        .mount(&server)
        .await;
    for page in 1..=6usize {
        let comments = (0..100)
            .map(|i| {
                let n = (page - 1) * 100 + i;
                json!({"id": n, "user": {"login": "reviewer"}, "body": format!("note {n}"),
                       "created_at": "2024-01-01T00:00:00Z", "path": "a.rs", "line": 1})
            })
            .collect::<Vec<_>>();
        let mut response = ResponseTemplate::new(200)
            .set_delay(Duration::from_millis(500))
            .set_body_json(Value::Array(comments));
        if page < 6 {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/pulls/9/comments?per_page=100&page={}>; rel=\"next\"",
                    server.uri(),
                    page + 1
                )
                .as_str(),
            );
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/pulls/9/comments"))
            .and(query_param("page", page.to_string()))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let data = run(
        &server,
        json!({"sections":["reviewComments"], "page": 15, "pageSize": 30, "extras": []}),
    )
    .await;
    let elapsed = first_request.get().expect("the PR was read").elapsed();
    let comments = data["pullRequests"][0]["reviewComments"]
        .as_array()
        .expect("review comments");
    assert_eq!(comments.len(), 30, "{data}");
    assert_eq!(comments[0]["body"], "note 420", "{data}");
    assert!(
        // A sequential re-scan of the five batches before the page takes 2.5 s.
        elapsed < Duration::from_millis(1_800),
        "sequential re-scan: {elapsed:?}"
    );
}

/// A hunk with GitHub's 3 context lines around a hit at line 53 of a
/// 100-line file, and that file at the head.
fn hunk_at_53() -> (String, String) {
    let patch = "@@ -50,7 +50,7 @@ fn f\n line 50\n line 51\n line 52\n-old 53\n+new needle 53\n line 54\n line 55\n line 56\n".to_owned();
    let head = (1..=100)
        .map(|n| {
            if n == 53 {
                "new needle 53\n".to_owned()
            } else {
                format!("line {n}\n")
            }
        })
        .collect();
    (patch, head)
}

/// contextLines past GitHub's hunk context: the read widens the run from
/// the file at the PR head (`headSha`), so the default 10 lines are not
/// silently cut to the hunk's 3.
#[tokio::test]
async fn match_context_past_the_hunk_reads_the_head_text() {
    use base64::Engine as _;
    let server = MockServer::builder().start().await;
    mount_pr(&server, 1).await;
    let (patch, head) = hunk_at_53();
    mount_file_batches(
        &server,
        vec![vec![rest_file("src/a.rs", Some(&patch), 1, 1)]],
        Duration::ZERO,
    )
    .await;
    Mock::given(method("GET"))
        // The shared file read names the path as one segment, as
        // ghGetFileContent does.
        .and(path("/api/v3/repos/a/b/contents/src%2Fa.rs"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","path":"src/a.rs",
            "content": base64::engine::general_purpose::STANDARD.encode(&head)
        })))
        .mount(&server)
        .await;
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": "needle", "contextLines": 10, "extras": []}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    let text = file["patch"].as_str().expect("patch");
    assert!(
        text.starts_with("@@ -44,20 +44,20 @@ fn f\n44\t line 44\n"),
        "{text}"
    );
    assert!(text.ends_with("63\t line 63\n"), "{text}");
    assert!(file.get("contextClipped").is_none(), "{data}");
    assert!(
        data.get("next")
            .is_none_or(|next| next.get("expandContext").is_none()),
        "{data}"
    );
}

/// A head file past the Contents API's 1 MB body (`encoding:"none"`, no
/// content) is read through its blob, as ghGetFileContent reads it: the
/// widened context is not silently lost to an empty body.
#[tokio::test]
async fn a_head_file_past_one_megabyte_is_read_through_its_blob() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 1).await;
    let (patch, head) = hunk_at_53();
    mount_file_batches(
        &server,
        vec![vec![rest_file("src/a.rs", Some(&patch), 1, 1)]],
        Duration::ZERO,
    )
    .await;
    let blob = "b".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fa.rs"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"none","content":"","path":"src/a.rs",
            "size": 2_000_000, "sha": blob
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/blobs/{blob}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/vnd.github.raw")
                .set_body_string(head),
        )
        .expect(1)
        .mount(&server)
        .await;
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": "needle", "contextLines": 10, "extras": []}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    let text = file["patch"].as_str().expect("patch");
    assert!(
        text.starts_with("@@ -44,20 +44,20 @@ fn f\n44\t line 44\n"),
        "{text}"
    );
    assert!(file.get("contextClipped").is_none(), "{data}");
    server.verify().await;
}

/// A later file page widens its own hits: the head reads name the files
/// this page shows, never the first page's again.
#[tokio::test]
async fn a_later_match_page_reads_the_head_text_of_its_own_files() {
    use base64::Engine as _;
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    let (patch, head) = hunk_at_53();
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/a.rs", Some(&patch), 1, 1),
            rest_file("src/b.rs", Some(&patch), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    for (file, reads) in [("a", 0), ("b", 1)] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{file}.rs")))
            .and(query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","path":format!("src/{file}.rs"),
                "content": base64::engine::general_purpose::STANDARD.encode(&head)
            })))
            .expect(reads)
            .mount(&server)
            .await;
    }
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": "needle", "contextLines": 10,
            "pageSize": 1, "page": 2, "extras": []}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    assert_eq!(file["path"], "src/b.rs", "{data}");
    assert!(file.get("contextClipped").is_none(), "{data}");
    server.verify().await;
}

/// Without the head text (the read failed), the clip is explicit: the file
/// is flagged and `next.expandContext` reads the wanted lines at headSha.
#[tokio::test]
async fn match_context_clipped_by_the_hunk_is_flagged_with_a_head_read() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 1).await;
    let (patch, _) = hunk_at_53();
    mount_file_batches(
        &server,
        vec![vec![rest_file("src/a.rs", Some(&patch), 1, 1)]],
        Duration::ZERO,
    )
    .await;
    let (_, data) = run_status(
        &server,
        json!({"sections":["patches"], "matchString": "needle", "contextLines": 10, "extras": []}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    assert_eq!(file["contextClipped"], true, "{data}");
    let read = &data["next"]["expandContext"];
    assert_eq!(read["tool"], "ghGetFileContent", "{data}");
    let row = &read["query"]["queries"][0];
    assert_eq!(row["ref"], SHA, "{data}");
    assert_eq!(row["path"], "src/a.rs", "{data}");
    assert_eq!(row["ranges"], json!(["43-63"]), "{data}");
    assert!(
        data["partialReasons"]
            .to_string()
            .contains("contextClipped"),
        "{data}"
    );
}

/// A 115-file pull request (one 100-file provider batch, then 15 files)
/// read with the default `sections:["patches"]`: following only the
/// returned continuations (`responsePagination.next` first, then
/// `next.continuePatch` / `next.nextFilePage`) every call is a
/// contract-valid success and the walk delivers every patch exactly once.
/// The first window of a file page that does not fit one response also
/// lists the page's `fileSummary`: that list must ride inside the page
/// budget, or the row splits into a part with an empty `files` array.
#[tokio::test]
async fn default_large_pr_patch_walk_reaches_every_patch_once() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 115).await;
    let file = |index: usize| {
        let dir = [
            "packages/react-devtools-timeline/src/content-views/utils",
            "packages/react-devtools-shared/src/devtools/views/Profiler",
            "packages/react-devtools-shell/src/app/InspectableElements",
        ][index % 3];
        let name = format!("{dir}/ComponentMeasuresView{index}.js");
        let lines = 6 + (index * 37) % 140;
        let body = (0..lines)
            .map(|line| {
                format!("-  const value{line} = \"timeline\" + computeLane({index}, {line});\n")
            })
            .collect::<String>();
        let patch = format!(
            "@@ -1,{lines} +0,0 @@ module.exports = {{\n{}",
            body.trim_end()
        );
        rest_file(&name, Some(&patch), 0, lines as u64)
    };
    let files = (0..115).map(file).collect::<Vec<_>>();
    let expected = files
        .iter()
        .map(|file| {
            (
                file["filename"].as_str().expect("name").to_owned(),
                file["patch"].as_str().expect("patch").to_owned(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    mount_file_batches(
        &server,
        vec![files[..100].to_vec(), files[100..].to_vec()],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut envelope = json!({"queries": [{"operation": "pullRequest", "owner": "a",
        "repo": "b", "number": 9, "sections": ["patches"]}]});
    let mut read = std::collections::BTreeMap::<String, String>::new();
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 80, "walk did not finish");
        let outcome = runtime
            .run(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                envelope.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("call {calls} failed: {error:?}\n{envelope}"));
        let content = &outcome.structured_content;
        assert_eq!(
            content["results"][0]["status"]
                .as_str()
                .unwrap_or("success"),
            "success",
            "{content}"
        );
        // A default patch window fits its response page: the row never
        // splits into response parts.
        assert!(
            content.get("responsePagination").is_none(),
            "call {calls}: {}",
            content["responsePagination"]
        );
        let data = &content["results"][0]["data"];
        for file in data["pullRequests"][0]["files"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let path = file["path"].as_str().expect("path").to_owned();
            let text = read.entry(path).or_default();
            let offset = file["patchPagination"]["offset"].as_u64().unwrap_or(0);
            assert_eq!(
                offset as usize,
                text.chars().count(),
                "call {calls}: {file}"
            );
            text.push_str(file["patch"].as_str().unwrap_or(""));
        }
        let next = content["responsePagination"]["next"]["query"]
            .as_object()
            .or_else(|| data["next"]["continuePatch"]["query"].as_object())
            .or_else(|| data["next"]["nextFilePage"]["query"].as_object());
        match next {
            Some(next) => envelope = Value::Object(next.clone()),
            None => break,
        }
    }
    assert_eq!(read.len(), 115, "patches reached: {}", read.len());
    for (path, patch) in &expected {
        assert_eq!(
            read.get(path).map(|view| raw_patch(view)).as_ref(),
            Some(patch),
            "{path}"
        );
    }
    runtime.close().await;
}

/// A merged pull request's `matchString` read offers the hit
/// file at the PR head (`readAtCommit` at `headSha`, where the gutter
/// numbers hold) and no `readAtMerge` (a targeted answer already).
#[tokio::test]
async fn history_b_merged_match_string_read_offers_the_source_side_only() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    mount_file_batches(
        &server,
        vec![vec![
            rest_file(
                "src/hit.rs",
                Some("@@ -1,3 +1,3 @@ fn f\n a\n-old needle\n+new needle\n b"),
                1,
                1,
            ),
            rest_file("src/other.rs", Some("@@ -1 +1 @@\n-a\n+b"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"matchString": "needle", "contextLines": 1, "extras": ["hints"]}),
    )
    .await;
    let lead = |name: &str| {
        let hint = crate::support::checks::hint(&data, name);
        data.get("next")
            .and_then(|next| next.get(name))
            .cloned()
            .or_else(|| (!hint.is_null()).then_some(hint))
    };
    let at_commit = lead("readAtCommit").expect("readAtCommit lead");
    let row = &at_commit["query"]["queries"][0];
    assert_eq!(row["ref"], SHA, "{data}");
    assert_eq!(row["path"], "src/hit.rs", "{data}");
    assert!(lead("readAtMerge").is_none(), "{data}");
    // The hit row counts the hits it shows.
    assert_eq!(
        data["pullRequests"][0]["files"][0]["matchCount"], 2,
        "{data}"
    );
}

/// A 115-file, ~560k-char pull request walked by following its leads
/// only (`continuePatch`, then `nextFilePage`) at a 20k configured page reads
/// every patch byte-identical (after the gutters) with no gap and no
/// repeat, and never overflows its page. Every hop runs at the first call's
/// page (no doubled `responseLength`, 2026-10-09): 21 → 44 calls, each the
/// size of the first (the numbered view is ~10% larger than the raw
/// patches).
#[tokio::test]
async fn history_b_large_pr_lead_walk_is_lossless_at_the_first_page_size() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 115).await;
    // A few large patches and a long tail of small ones (~560k chars).
    let size = |i: usize| match i {
        0 => 70_000,
        7 => 48_000,
        19 => 33_000,
        40 => 26_000,
        77 => 21_000,
        _ => 1_200 + (i * 1_327) % 4_200,
    };
    let patches = (0..115)
        .map(|i| {
            let mut patch = format!("@@ -1,{0} +1,{0} @@ fn f{i}\n", size(i) / 40);
            let mut line = 0;
            while patch.len() < size(i) {
                line += 1;
                patch.push_str(&format!(
                    "+let value_{i}_{line} = compute({line}); // pad\n"
                ));
            }
            patch
        })
        .collect::<Vec<_>>();
    let total: usize = patches.iter().map(String::len).sum();
    assert!((540_000..600_000).contains(&total), "{total}");
    let files = patches
        .iter()
        .enumerate()
        .map(|(i, patch)| rest_file(&format!("src/m{i:03}.rs"), Some(patch), 9, 0))
        .collect::<Vec<_>>();
    mount_file_batches(
        &server,
        vec![files[..100].to_vec(), files[100..].to_vec()],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    // The constant 20k page: 15k patch windows on every call.
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut envelope = json!({"queries": [{"operation": "pullRequest", "owner": "a",
        "repo": "b", "number": 9, "sections": ["patches"]}]});
    let mut read = std::collections::BTreeMap::<String, String>::new();
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 60, "walk did not finish");
        let outcome = runtime
            .run(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                envelope.clone(),
            )
            .await
            .expect("patch window");
        let content = &outcome.structured_content;
        assert_ne!(
            content["responsePagination"]["hasMore"], true,
            "call {calls}: a patch window overflowed the page it asked for"
        );
        let data = &content["results"][0]["data"];
        for file in data["pullRequests"][0]["files"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let Some(patch) = file["patch"].as_str() else {
                continue;
            };
            let path = file["path"].as_str().expect("path").to_owned();
            let text = read.entry(path).or_default();
            let offset = file["patchPagination"]["offset"].as_u64().unwrap_or(0);
            assert_eq!(
                offset as usize,
                text.chars().count(),
                "gap or repeat: {file}"
            );
            text.push_str(patch);
        }
        let next = &data["next"];
        let Some(lead) = ["continuePatch", "nextFilePage"]
            .iter()
            .find_map(|name| next.get(*name))
        else {
            break;
        };
        envelope = lead["query"].clone();
    }
    for (i, patch) in patches.iter().enumerate() {
        let path = format!("src/m{i:03}.rs");
        assert_eq!(
            read.get(&path).map(|view| raw_patch(view)).as_ref(),
            Some(patch),
            "{path}"
        );
    }
    assert!(calls <= 45, "{calls} calls");
    runtime.close().await;
}

/// A patch-walk hop that outgrows its page pages its response by
/// whole rows (automatic `responseScope:"rows"`), never by text windows
/// that empty the structured `results` and bury the walk's `next.*`. File
/// page 2 lists 96 patch-less files with long names beside a few patches:
/// the first window's metadata rows and summary alone exceed the 20k page,
/// so the row splits into structured `rowPart`s. Following
/// `responsePagination.next` first, then `continuePatch`/`nextFilePage`,
/// every call keeps its rows, every patch arrives exactly once (a patch
/// identical to an earlier one of its file page names that row,
/// `samePatchAs`), and every patch-less file is listed exactly once: the
/// walk's cursor rides the last part, so no part is skipped.
#[tokio::test]
async fn overflowing_patch_hop_pages_by_rows_and_loses_nothing() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 200).await;
    let long = |i: usize| {
        format!(
            "src/generated/{}_{i:03}.ts",
            "very_long_generated_name".repeat(10)
        )
    };
    let mut second = (0..96)
        .map(|i| rest_file(&long(i), None, 0, 0))
        .collect::<Vec<_>>();
    second.extend((0..4).map(|i| {
        let patch = format!(
            "@@ -1,150 +1,150 @@ fn big{i}\n{}",
            (0..150)
                .map(|line| format!("+let changed_{i}_{line} = compute({line}); // padding\n"))
                .collect::<String>()
                .trim_end()
        );
        rest_file(&format!("src/big{i}.rs"), Some(&patch), 150, 0)
    }));
    let first = numbered(0, 100);
    let expected = first
        .iter()
        .chain(&second)
        .filter_map(|file| {
            Some((
                file["filename"].as_str()?.to_owned(),
                file["patch"].as_str()?.to_owned(),
            ))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    mount_file_batches(&server, vec![first, second], Duration::ZERO).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut envelope = json!({"queries": [{"operation": "pullRequest", "owner": "a",
        "repo": "b", "number": 9, "sections": ["patches"]}]});
    let mut read = std::collections::BTreeMap::<String, String>::new();
    let mut listed = std::collections::BTreeMap::<String, usize>::new();
    let mut same = std::collections::BTreeMap::<String, String>::new();
    let (mut calls, mut split_parts) = (0, 0);
    loop {
        calls += 1;
        assert!(calls <= 40, "walk did not finish");
        let outcome = runtime
            .run(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                envelope.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("call {calls} failed: {error:?}\n{envelope}"));
        let content = &outcome.structured_content;
        let pagination = &content["responsePagination"];
        if pagination.is_object() {
            assert_eq!(pagination["scope"], "rows", "call {calls}: {pagination}");
        }
        let row = &content["results"][0];
        if row.get("rowPart").is_some() {
            split_parts += 1;
        }
        let data = &row["data"];
        // A later part of a split row omits a list the split emptied there
        // (it pages another list); a whole row always shows its files.
        let empty = Vec::new();
        let files = match data["pullRequests"][0]["files"].as_array() {
            Some(files) => files,
            None if row.get("rowPart").is_some_and(|part| part["part"] != 1) => &empty,
            None => panic!("call {calls}: rows hidden from structuredContent: {content}"),
        };
        for file in files {
            let path = file["path"].as_str().expect("path").to_owned();
            if let Some(earlier) = file["samePatchAs"].as_str() {
                assert!(
                    same.insert(path, earlier.to_owned()).is_none(),
                    "call {calls}: {file}"
                );
                continue;
            }
            match file["patch"].as_str() {
                Some(patch) => {
                    let text = read.entry(path).or_default();
                    let offset = file["patchPagination"]["offset"].as_u64().unwrap_or(0);
                    assert_eq!(
                        offset as usize,
                        text.chars().count(),
                        "call {calls}: {file}"
                    );
                    text.push_str(patch);
                }
                None => *listed.entry(path).or_default() += 1,
            }
        }
        let next = pagination["next"]["query"]
            .as_object()
            .or_else(|| data["next"]["continuePatch"]["query"].as_object())
            .or_else(|| data["next"]["nextFilePage"]["query"].as_object());
        match next {
            Some(next) => envelope = Value::Object(next.clone()),
            None => break,
        }
    }
    assert!(split_parts >= 2, "the hop never overflowed its page");
    assert_eq!(
        read.len() + same.len(),
        expected.len(),
        "patches reached: {} + {} repeats",
        read.len(),
        same.len()
    );
    for (path, patch) in &expected {
        let holder = same.get(path).unwrap_or(path);
        assert_eq!(
            read.get(holder).map(|view| raw_patch(view)).as_ref(),
            Some(patch),
            "{path}"
        );
    }
    assert_eq!(listed.len(), 96, "{listed:?}");
    assert!(listed.values().all(|count| *count == 1), "{listed:?}");
    runtime.close().await;
}

/// The changed-file paths a compact inventory page lists, in order:
/// `M +1 -1 path` rows and `{"dir/": [rows]}` groups.
fn inventory_paths(files: &Value) -> Vec<String> {
    let name = |row: &Value| {
        row.as_str()
            .and_then(|row| row.rsplit(' ').next())
            .map(str::to_owned)
    };
    files
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| match row {
            Value::Object(group) => group
                .iter()
                .flat_map(|(dir, rows)| {
                    rows.as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(name)
                        .map(|file| format!("{dir}{file}"))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
            row => name(row).into_iter().collect(),
        })
        .collect()
}

/// A budget-sized file page pins its grid: `next.nextFilePage`, copied
/// unchanged, continues the same file list whether it runs alone or batched
/// with other rows (a different budget share), so every walk lists each
/// changed file exactly once.
#[tokio::test]
async fn file_page_continuations_continue_the_same_grid_in_any_batch() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 450).await;
    mount_file_batches(
        &server,
        (1..=5)
            .map(|batch| numbered(batch, if batch == 5 { 50 } else { 100 }))
            .collect(),
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let files_row = json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,
        "mainGoal":"g","reasoning":"r","sections":["files"]});
    let body_row = json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,
        "mainGoal":"g","reasoning":"r","sections":["body"]});
    // One call: the files row with or without a batched body row; returns
    // the files row's paths and its copied-unchanged next file page.
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let call = |row: Value, batched: bool| {
        let runtime = &runtime;
        let id = calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let body_row = body_row.clone();
        async move {
            let queries = if batched {
                json!([row, body_row])
            } else {
                json!([row])
            };
            let outcome = runtime
                .run(
                    format!("grid-{id}"),
                    "ghGetHistoryItem".into(),
                    json!({ "queries": queries }),
                )
                .await
                .expect("file page");
            let content = outcome.structured_content;
            assert!(content.get("responsePagination").is_none(), "{content}");
            let data = &content["results"][0]["data"];
            let paths = inventory_paths(&data["pullRequests"][0]["files"]);
            let next = data.pointer("/next/nextFilePage/query/queries/0").cloned();
            (paths, next)
        }
    };
    for first_batched in [true, false] {
        let mut seen = Vec::new();
        let (paths, mut next) = call(files_row.clone(), first_batched).await;
        let first_page = paths.len();
        seen.extend(paths);
        // Every later page runs in the other batch shape.
        while let Some(row) = next {
            let (paths, after) = call(row, !first_batched).await;
            assert!(!paths.is_empty(), "an empty page mid-walk");
            seen.extend(paths);
            next = after;
        }
        let unique = seen.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), seen.len(), "a file repeated: {first_page}");
        assert_eq!(seen.len(), 450, "first page {first_page}: {seen:?}");
    }
    runtime.close().await;
}
