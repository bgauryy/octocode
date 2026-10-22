//! Happy-path anchors: each operation's row carries the values the next hop
//! reads (`number`, `headSha`/`baseSha`/`mergeCommitSha`, `commitSha`,
//! changed files as path+stat, patch lines numbered on their own side).
use super::{HistoryItemRequest, execute};
use crate::id::ToolId;
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::test_support::{fixture_context, mock_provider};
use crate::tools::test_support::{checks, published};
use octocode_contract_types::ErrorCode;
use octocode_github::RetryPolicy;
use octocode_github::mock_http::mount_json;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BASE: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";
const MERGE_BASE: &str = "3333333333333333333333333333333333333333";

struct Passthrough;
impl ContentScan for Passthrough {
    fn sanitize(
        &self,
        text: &str,
        _path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((text.to_owned(), Vec::new()))
    }
}

async fn read(server: &MockServer, row: Value) -> Value {
    let provider = mock_provider(server, RetryPolicy::default());
    let context = fixture_context(Duration::from_secs(10), 4 << 20);
    let request = HistoryItemRequest::from_row(row).expect("valid row");
    execute(&provider.transport, &request, &context, &Passthrough)
        .await
        .expect("history item")
        .data
}

fn changed_file(name: &str) -> Value {
    json!({"filename":name,"status":"modified","additions":1,"deletions":1,
        "patch":"@@ -10,2 +10,2 @@\n keep\n-old\n+new"})
}

fn commit_raw(sha: &str, message: &str) -> Value {
    json!({"sha":sha,"commit":{"message":message,
        "author":{"name":"Ada","email":"a@x","date":"2024-01-01T00:00:00Z"}},
        "author":{"login":"ada"}})
}

#[tokio::test]
async fn commit_patch_continuations_preserve_match_context_and_exact_stream() {
    let server = MockServer::builder().start().await;
    let patch = std::iter::once("@@ -1,47 +1,50 @@".to_owned())
        .chain((1..=50).map(|line| {
            if [10, 25, 40].contains(&line) {
                format!("+needle{line:02}")
            } else {
                format!(" context{line:02}")
            }
        }))
        .collect::<Vec<_>>()
        .join("\n");
    let mut raw = commit_raw(HEAD, "matching patches");
    raw["files"] = json!([{
        "filename":"sample.rs","status":"modified","additions":3,"deletions":0,
        "patch":patch
    }]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    for context in [0, 5] {
        let mut query = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,
            "sections":["patches"],"matchString":"needle","contextLines":context,"length":15000});
        let whole = read(&server, query.clone()).await;
        let expected = whole["files"][0]["patch"].as_str().expect("whole patch");
        query["length"] = json!(32);
        let mut actual = String::new();
        let mut pages = 0;
        loop {
            pages += 1;
            assert!(pages < 100, "patch walk must terminate");
            let data = read(&server, query.clone()).await;
            assert_history_page_contract(&query, &data);
            actual.push_str(data["files"][0]["patch"].as_str().expect("patch window"));
            let Some(next) = data.pointer("/next/continuePatch/query") else {
                break;
            };
            let normalized = crate::contracts::validate("ghGetHistoryItem", next.clone())
                .expect("executable copied patch continuation");
            query = normalized["queries"][0].clone();
            assert_eq!(query["contextLines"], context, "{query}");
        }
        assert!(pages > 1, "the fixture must exercise continuation");
        assert_eq!(actual, expected, "contextLines={context}");
    }
}

#[tokio::test]
async fn commit_file_batches_resolve_a_movable_ref_once_before_paging() {
    let server = MockServer::builder().start().await;
    let base_url = server.uri();
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/api/v3/repos/o/r/commits/"))
        .respond_with(move |request: &wiremock::Request| {
            let reference = request.url.path().rsplit('/').next().expect("ref");
            if request.headers.get("accept").is_some_and(|accept| {
                accept == "application/vnd.github.sha"
            }) {
                return ResponseTemplate::new(200).set_body_string(HEAD);
            }
            let page = request
                .url
                .query_pairs()
                .find(|(key, _)| key == "page")
                .map(|(_, value)| value.into_owned())
                .unwrap_or_else(|| "1".to_owned());
            let moved = reference == "main" && page != "1";
            let mut raw = commit_raw(if moved { MERGE_BASE } else { HEAD }, "moving branch");
            raw["files"] = if page == "1" {
                json!((0..100).map(|i| changed_file(&format!("old-{i:03}.rs"))).collect::<Vec<_>>())
            } else {
                json!([changed_file(if moved { "new-only.rs" } else { "old-last.rs" })])
            };
            let mut response = ResponseTemplate::new(200).set_body_json(raw);
            if page == "1" {
                response = response.insert_header(
                    "Link",
                    format!("<{base_url}/api/v3/repos/o/r/commits/{reference}?page=2&per_page=100>; rel=\"next\""),
                );
            }
            response
        })
        .mount(&server)
        .await;
    let query = json!({"operation":"commit","owner":"o","repo":"r","ref":"main",
        "sections":["files"],"include":["old-last.rs"]});
    let data = read(&server, query.clone()).await;
    assert_history_page_contract(&query, &data);
    assert_eq!(data["commitSha"], HEAD, "{data}");
    assert_eq!(data["files"], json!(["M +1 -1 old-last.rs"]), "{data}");
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 3, "one resolution and two pinned batches");
    assert!(requests[0].url.path().ends_with("/main"));
    assert!(
        requests[1..]
            .iter()
            .all(|request| request.url.path().ends_with(HEAD))
    );
}

/// A read by branch whose page one batch holds reads the commit by that
/// name once: the answer names the commit, and every continuation is
/// pinned to it, so no separate resolution is sent.
#[tokio::test]
async fn a_one_batch_commit_read_by_branch_sends_no_separate_resolution() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = json!(
        (0..30)
            .map(|i| changed_file(&format!("f-{i:02}.rs")))
            .collect::<Vec<_>>()
    );
    mount_json(&server, "/api/v3/repos/o/r/commits/main", 200, raw).await;
    let query = json!({"operation":"commit","owner":"o","repo":"r","ref":"main",
        "sections":["files"],"pageSize":10});
    let data = read(&server, query.clone()).await;
    assert_history_page_contract(&query, &data);
    assert_eq!(data["commitSha"], HEAD, "{data}");
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1, "one read of the commit by its name");
    assert_eq!(
        data.pointer("/next/nextFilePage/query/queries/0/ref"),
        Some(&json!(HEAD)),
        "{data}"
    );
}

#[tokio::test]
async fn commit_row_carries_its_sha_and_path_stat_files() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["parents"] = json!([{"sha":BASE}]);
    raw["stats"] = json!({"additions":1,"deletions":1});
    raw["files"] = json!([changed_file("src/a.rs")]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let row = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD});
    let out = read(&server, row.clone()).await;
    assert_eq!(out["commitSha"], HEAD, "{out}");
    assert_eq!(out["files"], json!(["M +1 -1 src/a.rs"]), "{out}");
    // A caller's SHA is not restated as its ref; a page listing every file
    // restates no count.
    assert!(out.get("ref").is_none(), "{out}");
    assert!(out.get("changedFilesCount").is_none(), "{out}");
    // Patches: each line numbered on its own side.
    let mut patched = row;
    patched["sections"] = json!(["patches"]);
    let out = read(&server, patched).await;
    let file = &out["files"][0];
    assert_eq!(file["path"], "src/a.rs", "{out}");
    let patch = file["patch"].as_str().expect("patch");
    assert!(patch.contains("10\t keep"), "{patch}");
    assert!(patch.contains("11\t-old"), "{patch}");
    assert!(patch.contains("11\t+new"), "{patch}");
}

/// A commit's file inventory flags a file GitHub sent without a patch, as
/// a pull request's does: a binary file's `+0 -0` never reads as an empty
/// change (react v18.2.0...v19.0.0: favicon.ico, graphvizlib.wasm).
#[tokio::test]
async fn commit_inventory_flags_files_without_a_patch() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Add assets");
    raw["files"] = json!([
        {"filename":"public/favicon.ico","status":"added","additions":0,"deletions":0,
            "sha":"4444444444444444444444444444444444444444"},
        {"filename":"src/old.rs","status":"renamed","additions":0,"deletions":0,
            "previous_filename":"src/older.rs","sha":"5555555555555555555555555555555555555555"},
    ]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD}),
    )
    .await;
    assert_eq!(
        out["files"],
        json!([
            "A +0 -0 !binary public/favicon.ico",
            "R +0 -0 src/old.rs <- src/older.rs"
        ]),
        "{out}"
    );
}

/// `matchString` with no section named searches the patches (as on a pull
/// request): it is never silently ignored.
#[tokio::test]
async fn commit_match_string_without_sections_reads_its_patches() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = json!([changed_file("src/a.rs"), {
        "filename":"src/b.rs","status":"modified","additions":1,"deletions":1,
        "patch":"@@ -1 +1 @@\n-x\n+y"}]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":"new"}),
    )
    .await;
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert_eq!(files[0]["path"], "src/a.rs", "{out}");
    assert!(
        files[0]["patch"]
            .as_str()
            .is_some_and(|patch| patch.contains("+new"))
    );
    assert!(out["next"].get("readPatches").is_none(), "{out}");
}

/// A `matchString` that removes every changed file of the page says so
/// (how many files, what it searches) and leads to the same read without it;
/// it is not the silent `"files":[]` of a file the commit never touched. A
/// match inside a hunk header (the function context after `@@`) keeps the
/// file: the header is part of the diff that is shown.
#[tokio::test]
async fn commit_match_string_that_removes_every_file_says_so() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = json!([changed_file("src/a.rs"), {
        "filename":"src/b.rs","status":"modified","additions":1,"deletions":1,
        "patch":"@@ -40,2 +40,2 @@ def url_for(\n keep\n-x\n+y"}]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,
            "include":["src/"],"matchString":"zzqx"}),
    )
    .await;
    assert_eq!(out["files"], json!([]), "{out}");
    assert_eq!(out["status"], "empty", "{out}");
    let warnings = out["warnings"].to_string();
    assert!(
        warnings.contains("matchString matched 0 of 2 changed files")
            && warnings.contains("hunk-header"),
        "{out}"
    );
    assert!(warnings.contains("drop matchString"), "{out}");
    let lead = &out["next"]["withoutMatchString"];
    assert_eq!(lead["tool"], "ghGetHistoryItem", "{out}");
    let query = &lead["query"]["queries"][0];
    assert!(query.get("matchString").is_none(), "{query}");
    assert_eq!(query["include"], json!(["src/"]), "{query}");
    assert_eq!(query["ref"], HEAD, "{query}");
    assert_eq!(query["sections"], json!(["files"]), "{query}");

    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":"url_for"}),
    )
    .await;
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert_eq!(files[0]["path"], "src/b.rs", "{out}");
    // Only a filter that removes every file is an empty answer; one that
    // removes some counts them.
    assert!(out.get("status").is_none(), "{out}");
    assert_eq!(
        out["warnings"],
        json!(["matchString dropped 1 of 2 changed files: no patch line holds it."]),
        "{out}"
    );
    assert!(out["next"].get("withoutMatchString").is_none(), "{out}");
}

/// A comparison's `matchString` that hits none of its changed files is the
/// same empty answer a commit's is (react v19.0.0...v19.0.1, "zzqqxx"): a
/// warning, `status: "empty"`, and the pinned read without the literal,
/// never a bare `files: []` that reads as "nothing changed".
#[tokio::test]
async fn compare_match_string_that_removes_every_file_says_so() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/{BASE}...{HEAD}"),
        200,
        json!({"ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE},"merge_base_commit":{"sha":MERGE_BASE},
            "commits":[commit_raw(HEAD, "Add feature\n\nWhy: the detail")],
            "files":[changed_file("src/a.rs"), changed_file("src/b.rs")]}),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","base":BASE,"head":HEAD,
            "matchString":"zzqqxx_nohit"}),
    )
    .await;
    assert_eq!(out["files"], json!([]), "{out}");
    assert_eq!(out["status"], "empty", "{out}");
    assert!(
        out["warnings"]
            .to_string()
            .contains("matchString matched 0 of 2 changed files"),
        "{out}"
    );
    let query = &out["next"]["withoutMatchString"]["query"]["queries"][0];
    assert_eq!(query["operation"], "compare", "{out}");
    assert!(query.get("matchString").is_none(), "{query}");
    assert_eq!(
        (&query["base"], &query["head"]),
        (&json!(BASE), &json!(HEAD))
    );
    // The literal alone chose the patches: without it, the read lists the
    // files (as the warning says).
    assert_eq!(query["sections"], json!(["files"]), "{query}");
    // The filter removed every file: no lead reads one of them.
    let next = out["next"].as_object().expect("next");
    for kind in ["readAtCommit", "readParent", "readCommit"] {
        assert!(!next.contains_key(kind), "{kind}: {out}");
    }
}

#[tokio::test]
async fn compare_row_names_both_resolved_commits_and_each_commit_sha() {
    let server = MockServer::builder().start().await;
    // The `application/vnd.github.sha` read answers the bare SHA.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits/feature"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HEAD))
        .mount(&server)
        .await;
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/v1...{HEAD}"),
        200,
        json!({"ahead_by":1,"behind_by":0,"total_commits":1,
            "permalink_url":"https://github.com/o/r/compare/o:1111111...o:2222222",
            "base_commit":{"sha":BASE},"merge_base_commit":{"sha":MERGE_BASE},
            "commits":[commit_raw(HEAD, "Add feature")],
            "files":[changed_file("src/a.rs")]}),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","base":"v1","head":"feature"}),
    )
    .await;
    // `base`/`head` carry the resolved SHAs, never the query's ref names.
    assert_eq!(out["baseSha"], BASE, "{out}");
    assert_eq!(out["headSha"], HEAD, "{out}");
    assert!(out.get("commitsScope").is_none(), "{out}");
    assert_eq!(out["commits"][0]["commitSha"], HEAD, "{out}");
    assert_eq!(out["files"], json!(["M +1 -1 src/a.rs"]), "{out}");
}

/// A compare of identical refs changes nothing: an empty answer (exit 1),
/// as a commit or PR whose scope holds nothing is.
#[tokio::test]
async fn compare_of_identical_refs_is_an_empty_answer() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/{HEAD}...{HEAD}"),
        200,
        json!({"ahead_by":0,"behind_by":0,"total_commits":0,
            "base_commit":{"sha":HEAD},"merge_base_commit":{"sha":HEAD},
            "commits":[],"files":[]}),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","base":HEAD,"head":HEAD}),
    )
    .await;
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(out["aheadBy"], 0, "{out}");
}

#[tokio::test]
async fn issue_row_carries_number_state_author_and_discussion_size() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/7",
        200,
        json!({"number":7,"title":"Crash","state":"open","user":{"login":"bob"},
            "body":"It crashes.","comments":3,"created_at":"2024-01-01T00:00:00Z",
            "updated_at":"2024-01-02T00:00:00Z"}),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"issue","owner":"o","repo":"r","number":7}),
    )
    .await;
    let issue = &out["issues"][0];
    assert_eq!(issue["number"], 7, "{out}");
    assert_eq!(issue["state"], "open", "{out}");
    assert_eq!(issue["author"], "bob", "{out}");
    assert_eq!(issue["commentsCount"], 3, "{out}");
    assert_eq!(issue["body"], "It crashes.", "{out}");
}

async fn mount_pr(server: &MockServer, files: Value) {
    mount_json(
        server,
        "/api/v3/repos/o/r/pulls/1",
        200,
        json!({"number":1,"title":"Fix","state":"open","user":{"login":"ada"},
            "head":{"sha":HEAD,"ref":"fix"},"base":{"sha":BASE,"ref":"main"},
            "changed_files":files.as_array().map_or(0, Vec::len),
            "additions":1,"deletions":1,"created_at":"2024-01-01T00:00:00Z"}),
    )
    .await;
    mount_json(server, "/api/v3/repos/o/r/pulls/1/files", 200, files).await;
}

/// `matchString` matches patch lines only: a file whose path alone holds
/// it is not kept (its whole patch would answer nothing), and the files the
/// filter dropped are counted on the call.
#[tokio::test]
async fn pr_match_string_keeps_patch_hits_only_and_counts_dropped_files() {
    let server = MockServer::builder().start().await;
    mount_pr(
        &server,
        json!([
            {"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,"sha":"1",
                "patch":"@@ -1,2 +1,2 @@\n keep\n-old\n+new Needle"},
            {"filename":"src/needle.rs","status":"modified","additions":1,"deletions":1,"sha":"2",
                "patch":"@@ -1 +1 @@\n-x\n+y"},
            {"filename":"src/b.rs","status":"modified","additions":1,"deletions":1,"sha":"3",
                "patch":"@@ -1 +1 @@\n-p\n+q"}
        ]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["patches"],"matchString":"needle"}),
    )
    .await;
    let paths = out["pullRequests"][0]["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| file["path"].clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, [json!("src/a.rs")], "{out}");
    let warnings = out["warnings"].to_string();
    assert!(
        warnings.contains("matchString dropped 2 of 3 changed files"),
        "{out}"
    );
}

/// A patchless file a `matchString` could not search is read at the side
/// that holds it: a removed file at the base, never at the head (a 404).
/// Its path holding the literal does not make it searched.
#[tokio::test]
async fn pr_unpatched_removed_file_is_searched_at_the_base() {
    let server = MockServer::builder().start().await;
    mount_pr(
        &server,
        json!([
            {"filename":"src/needle_gone.rs","status":"removed","additions":0,"deletions":900,
                "sha":"1"},
            {"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,"sha":"2",
                "patch":"@@ -1,2 +1,2 @@\n keep\n-old\n+new needle"}
        ]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["patches"],"matchString":"needle"}),
    )
    .await;
    assert_eq!(
        out["pullRequests"][0]["unsearchedFiles"],
        json!(["!tooLarge src/needle_gone.rs"]),
        "{out}"
    );
    let search = &out["next"]["searchUnpatchedFile"]["query"]["queries"][0];
    assert_eq!(search["path"], "src/needle_gone.rs", "{out}");
    assert_eq!(search["ref"], BASE, "{out}");
}

fn first_query<'a>(out: &'a Value, lead: &str) -> &'a Value {
    &out["next"][lead]["query"]["queries"][0]
}

/// A PR `matchString` that hit no patch line is an empty row whose leads run
/// the fix, with no prose naming them: the same read without the literal (its
/// scope kept, its `contextLines` dropped), then the whole file inventory. The
/// published row keeps both ahead of the section menu.
#[tokio::test]
async fn pr_match_string_miss_offers_the_unfiltered_read() {
    let server = MockServer::builder().start().await;
    mount_pr(
        &server,
        json!([{"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,
            "sha":"1","patch":"@@ -1,2 +1,2 @@\n keep\n-old\n+new"}]),
    )
    .await;
    let query = json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
        "include":["src/"],"matchString":"zzqx","contextLines":2});
    let out = read(&server, query.clone()).await;
    assert_eq!(out["status"], "empty", "{out}");
    assert!(out.get("hints").is_none(), "{out}");
    let lead = &out["next"]["withoutMatchString"];
    assert_eq!(lead["tool"], "ghGetHistoryItem", "{out}");
    assert_eq!(lead["confidence"], "exact", "{out}");
    let unfiltered = first_query(&out, "withoutMatchString");
    for absent in ["matchString", "contextLines"] {
        assert!(unfiltered.get(absent).is_none(), "{absent}: {unfiltered}");
    }
    assert_eq!(unfiltered["number"], 1, "{unfiltered}");
    assert_eq!(unfiltered["include"], json!(["src/"]), "{unfiltered}");
    assert_eq!(unfiltered["sections"], json!(["patches"]), "{unfiltered}");
    let inventory = first_query(&out, "readFiles");
    assert_eq!(inventory["number"], 1, "{inventory}");
    assert_eq!(inventory["sections"], json!(["files"]), "{inventory}");
    for absent in ["include", "matchString", "contextLines"] {
        assert!(inventory.get(absent).is_none(), "{absent}: {inventory}");
    }
    checks::assert_leads_valid(&out);
    let published = published(ToolId::GhGetHistoryItem, &query, &out);
    let names = checks::leads(&published)
        .into_iter()
        .map(|lead| lead.name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["withoutMatchString", "readFiles", "readDiscussion"],
        "{published}"
    );
}

/// A PR `include` that matched no changed file is an empty row whose lead
/// reads the whole file inventory to copy a path from; there is no literal
/// to drop.
#[tokio::test]
async fn pr_include_miss_offers_the_file_inventory() {
    let server = MockServer::builder().start().await;
    mount_pr(
        &server,
        json!([{"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,
            "sha":"1","patch":"@@ -1,2 +1,2 @@\n keep\n-old\n+new"}]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["files"],"include":["docs/"]}),
    )
    .await;
    assert_eq!(out["status"], "empty", "{out}");
    assert!(out.get("hints").is_none(), "{out}");
    assert!(out["next"].get("withoutMatchString").is_none(), "{out}");
    let inventory = first_query(&out, "readFiles");
    assert_eq!(inventory["number"], 1, "{inventory}");
    assert_eq!(inventory["sections"], json!(["files"]), "{inventory}");
    assert!(inventory.get("include").is_none(), "{inventory}");
    checks::assert_leads_valid(&out);
}

/// A commit's `matchString` keeps the files whose patch holds it (not a
/// path-only hit) and counts the files it dropped.
#[tokio::test]
async fn commit_match_string_drops_path_only_hits_and_counts_them() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = json!([changed_file("src/a.rs"), {
        "filename":"src/new_thing.rs","status":"modified","additions":1,"deletions":1,
        "patch":"@@ -1 +1 @@\n-x\n+y"}]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":"NEW"}),
    )
    .await;
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert_eq!(files[0]["path"], "src/a.rs", "{out}");
    assert!(
        out["warnings"]
            .to_string()
            .contains("matchString dropped 1 of 2 changed files"),
        "{out}"
    );
}

/// A `matchString` filters a commit's and a comparison's changed files
/// before they are paged, as on a pull request: page N is the N-th page of
/// files whose patch holds the literal, never a page of all files with the
/// misses cut out (which reads as "matched 0" while later pages hit).
#[tokio::test]
async fn commit_and_compare_match_pages_are_pages_of_hits() {
    let server = MockServer::builder().start().await;
    let miss = |name: &str| {
        json!({"filename":name,"status":"modified","additions":1,"deletions":1,
            "patch":"@@ -1 +1 @@\n-x\n+y"})
    };
    let files = json!([
        miss("src/a.rs"),
        changed_file("src/b.rs"),
        miss("src/c.rs"),
        changed_file("src/d.rs")
    ]);
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = files.clone();
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/{BASE}...{HEAD}"),
        200,
        json!({"ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE},"merge_base_commit":{"sha":MERGE_BASE},
            "commits":[commit_raw(HEAD, "Fix parser")],"files":files}),
    )
    .await;
    for row in [
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD}),
        json!({"operation":"compare","owner":"o","repo":"r","base":BASE,"head":HEAD,
            "sections":["patches"]}),
    ] {
        let mut paths = Vec::new();
        for page in [1, 2] {
            let mut query = row.clone();
            query["matchString"] = json!("new");
            query["pageSize"] = json!(1);
            query["page"] = json!(page);
            let out = read(&server, query).await;
            assert!(out.get("status").is_none(), "page {page}: {out}");
            let files = out["files"].as_array().cloned().unwrap_or_default();
            paths.extend(files.iter().map(|file| file["path"].clone()));
            let pages = &out["contentPagination"]["files"];
            assert_eq!(pages["totalItems"], 2, "page {page}: {out}");
            assert_eq!(pages["hasMore"], page == 1, "page {page}: {out}");
        }
        assert_eq!(paths, [json!("src/b.rs"), json!("src/d.rs")], "{row}");
    }
}

/// A changed file GitHub sent without a patch (too large to diff) was never
/// searched by `matchString`: it is listed as unsearched, never counted as
/// a file no patch line holds the literal in, and the row is partial with
/// the read that searches its source at the side that holds it.
#[tokio::test]
async fn commit_match_string_lists_patchless_files_as_unsearched() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["parents"] = json!([{"sha":BASE}]);
    raw["files"] = json!([changed_file("src/a.rs"), {
        "filename":"src/b.rs","status":"modified","additions":1,"deletions":1,
        "patch":"@@ -1 +1 @@\n-x\n+y"}, {
        "filename":"src/big.rs","status":"modified","additions":900,"deletions":3,"sha":"9"}]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":"new"}),
    )
    .await;
    let paths = out["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| file["path"].clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, [json!("src/a.rs")], "{out}");
    assert_eq!(
        out["warnings"],
        json!(["matchString dropped 1 of 3 changed files: no patch line holds it."]),
        "{out}"
    );
    assert_eq!(
        out["unsearchedFiles"],
        json!(["!tooLarge src/big.rs"]),
        "{out}"
    );
    assert_eq!(out["isPartial"], true, "{out}");
    assert_eq!(out["partialReasons"], json!(["patchUnavailable"]), "{out}");
    let search = &out["next"]["searchUnpatchedFile"]["query"]["queries"][0];
    assert_eq!(search["path"], "src/big.rs", "{out}");
    assert_eq!(search["ref"], HEAD, "{out}");
    assert_eq!(search["matchString"], "new", "{out}");

    // A literal no searched patch holds, beside an unsearched file, is no
    // empty answer: the unsearched file may hold it.
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":"zzqx"}),
    )
    .await;
    assert_eq!(out["files"], json!([]), "{out}");
    assert!(out.get("status").is_none(), "{out}");
    assert_eq!(
        out["warnings"],
        json!(["matchString dropped 2 of 3 changed files: no patch line holds it."]),
        "{out}"
    );
    assert_eq!(
        out["unsearchedFiles"],
        json!(["!tooLarge src/big.rs"]),
        "{out}"
    );
    assert_eq!(out["partialReasons"], json!(["patchUnavailable"]), "{out}");
}

/// A comparison's `matchString` lists patchless files as unsearched too; a
/// removed one is searched at the merge base, where its text is.
#[tokio::test]
async fn compare_match_string_lists_patchless_files_as_unsearched() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/{BASE}...{HEAD}"),
        200,
        json!({"ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE},"merge_base_commit":{"sha":MERGE_BASE},
            "commits":[commit_raw(HEAD, "Add feature")],
            "files":[changed_file("src/a.rs"), changed_file("src/c.rs"), {
                "filename":"src/gone.rs","status":"removed","additions":0,"deletions":700,
                "sha":"9"}]}),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","base":BASE,"head":HEAD,
            "matchString":"new"}),
    )
    .await;
    assert!(out.get("warnings").is_none(), "{out}");
    assert_eq!(
        out["unsearchedFiles"],
        json!(["!tooLarge src/gone.rs"]),
        "{out}"
    );
    assert_eq!(out["isPartial"], true, "{out}");
    assert_eq!(out["partialReasons"], json!(["patchUnavailable"]), "{out}");
    let search = &out["next"]["searchUnpatchedFile"]["query"]["queries"][0];
    assert_eq!(search["path"], "src/gone.rs", "{out}");
    assert_eq!(search["ref"], MERGE_BASE, "{out}");
}

/// Comment text is GitHub's verbatim text by default: an HTML comment in a
/// discussion or inline review comment stays, and no raw re-read is needed.
#[tokio::test]
async fn pr_comment_bodies_are_verbatim_by_default() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, json!([])).await;
    let comment = |id: u64| {
        json!({"id":id,"user":{"login":"rev"},"body":"See <!-- hidden note --> this \u{1f6a8}",
            "created_at":"2024-01-01T00:00:00Z"})
    };
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/1/comments",
        200,
        json!([comment(1)]),
    )
    .await;
    let mut inline = comment(2);
    inline["path"] = json!("src/a.rs");
    inline["line"] = json!(3);
    inline["side"] = json!("RIGHT");
    inline["commit_id"] = json!(HEAD);
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1/comments",
        200,
        json!([inline]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["comments","reviewComments"]}),
    )
    .await;
    let row = &out["pullRequests"][0];
    for section in ["comments", "reviewComments"] {
        assert_eq!(
            row[section][0]["body"], "See <!-- hidden note --> this \u{1f6a8}",
            "{section}: {out}"
        );
    }
    assert!(row.get("bodyView").is_none(), "{out}");
    assert!(out.pointer("/next/readRawBody").is_none(), "{out}");
}

fn assert_history_page_contract(query: &Value, data: &Value) {
    let shaped = crate::response::rows::result_row(
        crate::id::ToolId::GhGetHistoryItem,
        0,
        query,
        data.clone(),
        None,
    );
    crate::contracts::validate_output("ghGetHistoryItem", &json!({"results": [shaped]}))
        .expect("actual normalized history row satisfies the canonical output contract");
}

#[tokio::test]
async fn commit_path_scope_keeps_the_unfiltered_observed_count_separate() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "scoped files");
    raw["files"] = json!([
        changed_file("src/a.rs"),
        changed_file("docs/b.md"),
        changed_file("tests/c.rs")
    ]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let query = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,
        "sections":["files"],"include":["src/**"]});
    let data = read(&server, query.clone()).await;
    assert_history_page_contract(&query, &data);
    assert_eq!(data["files"], json!(["M +1 -1 src/a.rs"]), "{data}");
    assert_eq!(data["changedFilesCount"], 3, "unfiltered observed files");
    assert!(data.get("changedFilesCountScope").is_none(), "{data}");
    assert_eq!(
        data["contentPagination"]["files"]["totalItems"], 1,
        "scoped files"
    );
    assert_eq!(
        data["contentPagination"]["files"]["countScope"], "complete",
        "{data}"
    );
}

#[tokio::test]
async fn pr_commit_last_short_batch_discloses_the_unreachable_reported_total() {
    for (total, extras) in [
        (250, json!([])),
        (260, json!([])),
        (260, json!(["details"])),
    ] {
        let server = MockServer::builder().start().await;
        mount_json(
            &server,
            "/api/v3/repos/o/r/pulls/1",
            200,
            json!({
                "number":1,"title":"commits","state":"open","user":{"login":"ada"},
                "head":{"sha":HEAD},"base":{"sha":BASE},"commits":total,"changed_files":0
            }),
        )
        .await;
        for page in 1..=3 {
            let start = (page - 1) * 100;
            let rows = (start..(start + 100).min(250))
                .map(|i| commit_raw(&format!("{i:040x}"), &format!("commit {i}")))
                .collect::<Vec<_>>();
            let mut response = ResponseTemplate::new(200).set_body_json(rows);
            if page < 3 {
                response = response.insert_header(
                    "Link",
                    format!(
                        "<{}/api/v3/repos/o/r/pulls/1/commits?page={}&per_page=100>; rel=\"next\"",
                        server.uri(),
                        page + 1,
                    ),
                );
            }
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/o/r/pulls/1/commits"))
                .and(wiremock::matchers::query_param("page", page.to_string()))
                .respond_with(response)
                .mount(&server)
                .await;
        }
        // The details view keeps page detail: the row's limits still lift
        // off its pages (the page schema declares none of them).
        let mut query = json!({"operation":"pullRequest","owner":"o","repo":"r",
            "number":1,"sections":["commits"],"pageSize":100,"extras":extras});
        let mut all = Vec::new();
        let mut pages = 0;
        let last = loop {
            pages += 1;
            assert!(pages <= 3);
            let data = read(&server, query.clone()).await;
            assert_history_page_contract(&query, &data);
            all.extend(
                data["pullRequests"][0]["commits"]
                    .as_array()
                    .expect("commits")
                    .iter()
                    .map(|c| c["commitSha"].as_str().expect("sha").to_owned()),
            );
            let Some(next) = data.pointer("/next/nextCommitPage/query") else {
                break data;
            };
            let normalized = crate::contracts::validate("ghGetHistoryItem", next.clone())
                .expect("executable copied commit page");
            query = normalized["queries"][0].clone();
        };
        assert_eq!(
            all,
            (0..250).map(|i| format!("{i:040x}")).collect::<Vec<_>>()
        );
        assert_eq!(
            all.iter().collect::<std::collections::HashSet<_>>().len(),
            250
        );
        let page = &last["pullRequests"][0]["contentPagination"]["commits"];
        assert_eq!(page["hasMore"], false);
        if total > 250 {
            assert_eq!(last["terminalLimit"], true, "{last}");
            assert_eq!(last["isPartial"], true, "{last}");
            assert_eq!(last["pullRequests"][0]["commitsCount"], total, "{last}");
            assert_eq!(page["countScope"], "partial", "{last}");
            assert_eq!(
                last["providerLimit"]["reason"], "providerCommitListLimit",
                "{last}"
            );
            assert!(last["next"].get("nextCommitPage").is_none(), "{last}");
        } else {
            assert_ne!(last["terminalLimit"], true, "{last}");
            assert_eq!(page["countScope"], "complete", "{last}");
        }
    }
}

#[tokio::test]
async fn commit_file_boundary_is_a_lower_bound_with_or_without_a_final_link() {
    for (listed, final_link, bounded) in [
        (2999, false, false),
        (3000, false, true),
        (3000, true, true),
    ] {
        let server = MockServer::builder().start().await;
        for page in 1..=30 {
            let start = (page - 1) * 100;
            let files = (start..(start + 100).min(listed)).map(|i| {
                json!({"filename":format!("file{i:04}.ts"),"status":"added","additions":1,"deletions":0})
            }).collect::<Vec<_>>();
            let mut raw = commit_raw(HEAD, "files");
            raw["parents"] = json!([{"sha":BASE}]);
            raw["files"] = json!(files);
            let mut response = ResponseTemplate::new(200).set_body_json(raw);
            if page < 30 || final_link {
                response = response.insert_header(
                    "Link",
                    format!(
                        "<{}/api/v3/repos/o/r/commits/{HEAD}?page={}&per_page=100>; rel=\"next\"",
                        server.uri(),
                        page + 1,
                    ),
                );
            }
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/o/r/commits/{HEAD}")))
                .and(wiremock::matchers::query_param("page", page.to_string()))
                .respond_with(response)
                .mount(&server)
                .await;
        }
        let mut query = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,
            "sections":["files"],"pageSize":100});
        let mut all = Vec::new();
        let mut pages = 0;
        let last = loop {
            pages += 1;
            assert!(
                pages <= 30,
                "no phantom page past the hard provider boundary"
            );
            let data = read(&server, query.clone()).await;
            assert_history_page_contract(&query, &data);
            all.extend(data["files"].as_array().expect("files").iter().map(|file| {
                file.as_str()
                    .expect("root-file inventory row")
                    .split_whitespace()
                    .last()
                    .expect("path")
                    .to_owned()
            }));
            let Some(next) = data.pointer("/next/nextFilePage/query") else {
                break data;
            };
            let normalized = crate::contracts::validate("ghGetHistoryItem", next.clone())
                .expect("executable copied file page");
            query = normalized["queries"][0].clone();
        };
        assert_eq!(
            all,
            (0..listed)
                .map(|i| format!("file{i:04}.ts"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            all.iter().collect::<std::collections::HashSet<_>>().len(),
            listed
        );
        assert_eq!(
            last["contentPagination"]["files"]["hasMore"], false,
            "{last}"
        );
        if bounded {
            assert_eq!(last["terminalLimit"], true, "{last}");
            assert_eq!(last["isPartial"], true, "{last}");
            assert_eq!(
                last["contentPagination"]["files"]["countScope"], "partial",
                "{last}"
            );
            assert_eq!(last["changedFilesCountScope"], "partial", "{last}");
            assert_eq!(
                last["changedFilesCount"], 3000,
                "only observed files, never an invented total"
            );
        } else {
            assert_ne!(last["terminalLimit"], true, "{last}");
            assert_eq!(
                last["contentPagination"]["files"]["countScope"], "complete",
                "{last}"
            );
        }
        assert!(last["next"].get("nextFilePage").is_none(), "{last}");
    }
}
