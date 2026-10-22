//! Hard-cutover shapes, through each tool's real runtime path: one
//! workspace-relative path form and one name per purpose; every tool output
//! these tests produce is checked against the output contract.

use crate::support::{self, Run as _, Workspace};
use octocode_native::response::outcome::ToolOutcome;
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};

fn beta_runtime(workspace: &Workspace) -> ToolRuntime {
    workspace.runtime(&[("OCTOCODE_BETA", "true".into())])
}

// ---------- one workspace-relative path form ----------

#[tokio::test]
async fn ast_search_error_rows_name_workspace_relative_paths() {
    let workspace = Workspace::new();
    workspace.write("sub/dir/Cargo.toml", "[package]\nname = \"x\"\n");
    let runtime = workspace.runtime(&[]);
    let outcome = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":"sub/dir/Cargo.toml"}),
        &[],
    )
    .await;
    let row = &outcome.structured_content["results"][0];
    assert_eq!(row["status"], "error", "{row}");
    // A basename (`Cargo.toml`) cannot be pasted into localFetch.
    assert_eq!(row["data"]["path"], "sub/dir/Cargo.toml", "{row}");
    runtime.close().await;
}

#[tokio::test]
async fn ast_search_scan_diagnostics_name_workspace_relative_paths() {
    let workspace = Workspace::new();
    workspace.write("sub/dir/a.rs", "fn a() { old(1); }\n");
    workspace.write("sub/dir/b.rs", "fn b() { old(2); }\n");
    let runtime = workspace.runtime(&[]);
    let outcome = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"sub/dir","language":"rust","pattern":"old($A)","maxFiles":1}),
        &[],
    )
    .await;
    let data = outcome.data();
    let diagnostics = data["diagnostics"].as_array().expect("diagnostics");
    let truncated = diagnostics
        .iter()
        .find(|diagnostic| diagnostic["code"] == "structural.scan.truncated")
        .unwrap_or_else(|| panic!("truncated diagnostic: {data}"));
    assert_eq!(truncated["path"], "sub/dir", "{data}");
    for file in data["files"].as_array().expect("files") {
        let path = file["path"].as_str().expect("file path");
        assert!(path.starts_with("sub/dir/"), "{path}: {data}");
    }
    runtime.close().await;
}

// ---------- one name per purpose ----------

#[tokio::test]
async fn artifact_search_takes_ecosystem_and_registry_url() {
    use wiremock::matchers::path_regex;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::builder().start().await;
    Mock::given(path_regex(".*"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"error":"Not found"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".into())]);
    // The new names reach the registry the row names.
    let outcome = support::run(
        &runtime,
        "artifactSearch",
        json!({"ecosystem":"npm","packageName":"zzqq-missing","registryUrl":server.uri()}),
        &[],
    )
    .await;
    let requests = server.received_requests().await.expect("recorded");
    assert!(!requests.is_empty(), "{}", outcome.structured_content);
    // The retired names fail closed, naming the replacement.
    for (row, replacement) in [
        (json!({"type":"npm","packageName":"zod"}), "ecosystem"),
        (
            json!({"ecosystem":"npm","packageName":"zod","registry":server.uri()}),
            "registryUrl",
        ),
    ] {
        let mut row = row;
        row["mainGoal"] = json!("cutover shapes.");
        let error = runtime
            .run(
                "d1-artifact-retired".into(),
                "artifactSearch".into(),
                json!({"queries":[row]}),
            )
            .await
            .expect_err("a retired input field is rejected");
        let text = format!("{error:?}");
        assert!(text.contains(replacement), "{replacement}: {text}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn history_reviews_and_issue_comments_name_author_and_commit_sha() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    const HEAD: &str = "1111111111111111111111111111111111111111";
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"pullRequest": {
                "number": 7, "title": "Fix parser", "url": "https://x", "state": "OPEN",
                "body": "Fixes the parser.", "isDraft": false, "author": {"login": "bob"},
                "labels": {"pageInfo": {"hasNextPage": false}, "nodes": []},
                "baseRefName": "main", "baseRefOid": "2222222222222222222222222222222222222222",
                "headRefName": "feat/parser", "headRefOid": HEAD,
                "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
                "closedAt": null, "mergedAt": null, "mergeCommit": null,
                "comments": {"totalCount": 0}, "changedFiles": 1, "additions": 1, "deletions": 0,
                "commitsCount": {"totalCount": 1}, "reviewThreads": {"totalCount": 0},
                "files": {"pageInfo": {"hasNextPage": false}, "nodes": [
                    {"path": "src/a.rs", "additions": 1, "deletions": 0, "changeType": "MODIFIED"}
                ]},
                "reviews": {"pageInfo": {"hasNextPage": false}, "nodes": [
                    {"databaseId": 11, "author": {"login": "ann"}, "state": "APPROVED",
                     "body": "ok", "submittedAt": "2026-01-03T00:00:00Z", "commit": {"oid": HEAD}}
                ]}
            }}
        }})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 9, "title": "Bug", "state": "open", "body": "repro",
            "user": {"login": "alice"}, "labels": [], "comments": 1,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/9/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "user": {"login": "bob"}, "body": "same here",
             "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"}
        ])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let reviews = support::run(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,"sections":["body","reviews"]}),
        &[],
    )
    .await;
    let review = &reviews.data()["pullRequests"][0]["reviews"][0];
    let seen = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| format!("{} {}", request.method, request.url.path()))
        .collect::<Vec<_>>();
    assert_eq!(
        review["author"], "ann",
        "{} {seen:?}",
        reviews.structured_content
    );
    assert_eq!(review["commitSha"], HEAD, "{}", reviews.structured_content);
    let comments = support::run(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"issue","owner":"a","repo":"b","number":9,"sections":["comments"]}),
        &[],
    )
    .await;
    let comment = &comments.data()["issues"][0]["comments"][0];
    assert_eq!(comment["author"], "bob", "{}", comments.structured_content);
    runtime.close().await;
}

// ---------- one count vocabulary ----------

#[tokio::test]
async fn local_search_counts_are_match_line_and_file_counts() {
    let workspace = Workspace::new();
    for name in ["a.txt", "b.txt", "c.txt"] {
        workspace.write(name, "needle\nx\nneedle\n");
    }
    let runtime = workspace.runtime(&[]);
    let paged = support::run(
        &runtime,
        "localSearch",
        // The page counters are a stats extra.
        json!({"path":".","matchString":"needle","pageSize":1}),
        &["stats"],
    )
    .await;
    // The totals are flat on the row (no `stats` object, no `fileCount`: the
    // file count is `pagination.totalItems`).
    let totals = paged.data();
    assert_eq!(totals["matchCount"], 6, "{}", paged.structured_content);
    assert_eq!(
        totals["matchedLineCount"], 6,
        "{}",
        paged.structured_content
    );
    assert!(
        totals.get("stats").is_none(),
        "{}",
        paged.structured_content
    );
    // `pagination.totalItems` counts the paged unit: files.
    assert_eq!(
        paged.data()["pagination"]["totalItems"],
        3,
        "{}",
        paged.structured_content
    );
    // A merged context window lists its matched lines as `matchedLines`.
    let merged = support::run(
        &runtime,
        "localSearch",
        json!({"path":"a.txt","matchString":"needle","contextLines":2}),
        &[],
    )
    .await;
    let row = &merged.data()["files"][0]["matches"][0];
    assert_eq!(
        row["matchedLines"],
        json!([1, 3]),
        "{}",
        merged.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn ast_tools_count_matches_as_match_count() {
    let workspace = Workspace::new();
    workspace.write("src/a.ts", "export const f = (x) => old(x) + old(1);\n");
    let runtime = beta_runtime(&workspace);
    let search = support::run(
        &runtime,
        "astSearch",
        // A complete page drops its stats; the stats extra keeps them.
        json!({"operation":"match","path":"src","language":"typescript","pattern":"old($A)"}),
        &["stats"],
    )
    .await;
    assert_eq!(
        search.data()["matchCount"],
        2,
        "{}",
        search.structured_content
    );
    runtime.close().await;
}

// ---------- one 1-based column base ----------

/// Hit columns are a detail (`extras: ["details"]`); every one shown is
/// 1-based.
#[tokio::test]
async fn every_column_is_one_based() {
    let workspace = Workspace::new();
    // "a()" starts at the 4th character of line 1 and the 10th of line 2.
    workspace.write("x.rs", "fn a() {}\nfn b() { a(); }\n");
    let runtime = beta_runtime(&workspace);
    let spans = support::run(
        &runtime,
        "localSearch",
        json!({"path":"x.rs","matchString":"a()","resultView":"matchOnly"}),
        &["details"],
    )
    .await;
    let rows = &spans.data()["files"][0]["matches"];
    assert_eq!(rows[0]["column"], 4, "{}", spans.structured_content);
    assert_eq!(rows[1]["column"], 10, "{}", spans.structured_content);
    let matched = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"x.rs","pattern":"a()","captureText":true}),
        &["details"],
    )
    .await;
    let row = &matched.data()["files"][0]["matches"][0];
    assert_eq!(row["line"], 2, "{}", matched.structured_content);
    assert_eq!(row["column"], 10, "{}", matched.structured_content);
    let tree = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"syntaxTree","path":"x.rs","pageSize":2}),
        &[],
    )
    .await;
    // `function_item` spans line 1 from character 1 to the end (exclusive 10).
    assert_eq!(
        tree.data()["nodes"][1],
        "1 function_item 1:1-1:10 ^0",
        "{}",
        tree.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn lsp_position_and_call_hierarchy_inputs_fail_closed() {
    let workspace = Workspace::new();
    workspace.write("a.ts", "export function f() {}\n");
    let runtime = workspace.runtime(&[]);
    for (row, names) in [
        (
            json!({"operation":"definition","path":"a.ts","position":{"line":0,"character":16}}),
            ["position", "symbolName"],
        ),
        (
            json!({"operation":"callHierarchy","path":"a.ts","symbolName":"f","line":1}),
            ["callHierarchy", "callers"],
        ),
    ] {
        let mut row = row;
        row["mainGoal"] = json!("cutover shapes.");
        let error = runtime
            .run(
                "d1-lsp-retired".into(),
                "lspSearch".into(),
                json!({"queries":[row]}),
            )
            .await
            .expect_err("a retired lspSearch input is rejected");
        let text = format!("{error:?}");
        for name in names {
            assert!(text.contains(name), "{name}: {text}");
        }
    }
    runtime.close().await;
}

// ---------- location rows are objects named like tool inputs ----------

#[tokio::test]
async fn location_rows_are_objects_named_like_tool_inputs() {
    let workspace = Workspace::new();
    workspace.write(
        "pkg/src/lib.rs",
        "pub fn run() {\n    let a = 1;\n    let b = a + 1;\n}\n",
    );
    workspace.write("pkg/a.txt", "x\n");
    let runtime = workspace.runtime(&[]);
    // The enclosing declaration is an object an lspSearch/localFetch
    // call takes as is.
    let hits = support::run(
        &runtime,
        "localSearch",
        json!({"path":"pkg","matchString":"let"}),
        &[],
    )
    .await;
    let first = &hits.data()["files"][0]["matches"][0];
    assert_eq!(
        first["enclosing"],
        json!({"symbolName":"run","kind":"function","line":1,"endLine":4}),
        "{}",
        hits.structured_content
    );
    assert!(first.get("in").is_none(), "{first}");
    // A declaration without members is an object too.
    let outline = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":"pkg"}),
        &[],
    )
    .await;
    assert_eq!(
        outline.data()["files"][0]["symbols"][0],
        json!({"symbolName":"run","kind":"function","line":1,"endLine":4,"exported":true}),
        "{}",
        outline.structured_content
    );
    // Match rows are objects with their 1-based column: two matches can
    // start on one line.
    let matched = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"pkg","language":"rust","pattern":"let $A = $B;"}),
        &[],
    )
    .await;
    assert_eq!(
        matched.data()["files"][0]["matches"][0],
        json!({"line":2,"column":5,"value":"let a = 1;"}),
        "{}",
        matched.structured_content
    );
    // One listing shape, workspace-relative groups, key `files`.
    let tree = support::run(&runtime, "structureSearch", json!({"path":"pkg"}), &[]).await;
    assert!(
        tree.data().get("entries").is_none(),
        "{}",
        tree.structured_content
    );
    assert_eq!(
        tree.data()["files"],
        json!([{"dir":"pkg","files":["a.txt (2)","src/"]}]),
        "{}",
        tree.structured_content
    );
    let files = support::run(
        &runtime,
        "structureSearch",
        json!({"path":"pkg","operation":"files"}),
        &[],
    )
    .await;
    assert_eq!(
        files.data()["files"],
        json!([{"dir":"pkg","files":["a.txt (2)"]},{"dir":"pkg/src","files":["lib.rs (51)"]}]),
        "{}",
        files.structured_content
    );
    runtime.close().await;
}

// ---------- lead names and runnable recoveries ----------

/// The lead `name` under `next` or `hints`, whichever channel holds it.
fn lead(outcome: &ToolOutcome, name: &str) -> Value {
    let data = support::row_data(outcome);
    [
        data["next"][name].clone(),
        support::checks::hint(data, name),
    ]
    .into_iter()
    .find(|lead| lead.is_object())
    .unwrap_or_else(|| panic!("no `{name}` lead: {}", outcome.structured_content))
}

#[tokio::test]
async fn a_case_sensitive_miss_leads_to_a_runnable_ignore_case_read() {
    let workspace = Workspace::new();
    workspace.write("pkg/a.txt", "alpha\nHello World\n");
    let runtime = workspace.runtime(&[]);
    let miss = support::run(
        &runtime,
        "localFetch",
        json!({"path":"pkg/a.txt","matchString":"hello World"}),
        &[],
    )
    .await;
    let hint = support::checks::hint(miss.data(), "text")[0]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!hint.contains("caseMode"), "{hint}");
    let ignore = lead(&miss, "ignoreCase");
    assert_eq!(ignore["tool"], "localFetch", "{ignore}");
    let row = ignore["query"]["queries"][0].clone();
    assert_eq!(row["caseMode"], "insensitive", "{row}");
    let hit = support::run(&runtime, "localFetch", row, &[]).await;
    assert!(
        hit.data()["content"]
            .as_str()
            .is_some_and(|text| text.contains("Hello World")),
        "{}",
        hit.structured_content
    );
    // A miss that already ignored case offers no ignoreCase lead.
    let insensitive = support::run(
        &runtime,
        "localFetch",
        json!({"path":"pkg/a.txt","matchString":"absent","caseMode":"insensitive"}),
        &[],
    )
    .await;
    assert!(
        insensitive.data()["next"].get("ignoreCase").is_none()
            && support::checks::hint(insensitive.data(), "ignoreCase").is_null(),
        "{}",
        insensitive.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_missing_file_leads_to_find_file() {
    let workspace = Workspace::new();
    workspace.write("pkg/server.rs", "fn main() {}\n");
    let runtime = workspace.runtime(&[]);
    let missing = support::run(&runtime, "localFetch", json!({"path":"pkg/server.ts"}), &[]).await;
    let find = lead(&missing, "findFile");
    assert_eq!(find["tool"], "structureSearch", "{find}");
    runtime.close().await;
}

// ---------- one camelCase error-code set ----------

#[tokio::test]
async fn error_codes_are_declared_camel_case_names() {
    let workspace = Workspace::new();
    workspace.write("pkg/a.rs", "fn a() {}\n");
    let runtime = workspace.runtime(&[]);
    let unsupported = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"pkg/a.rs","language":"cobol","pattern":"fn $A() {}"}),
        &[],
    )
    .await;
    assert_eq!(
        unsupported.data()["errorCode"],
        "languageUnsupported",
        "{}",
        unsupported.structured_content
    );
    let missing = support::run(&runtime, "localFetch", json!({"path":"pkg/none.rs"}), &[]).await;
    assert_eq!(missing.data()["errorCode"], "pathNotFound");
    for outcome in [&unsupported, &missing] {
        let code = support::row_data(outcome)["errorCode"]
            .as_str()
            .expect("code");
        assert!(
            code.chars().all(|c| c.is_ascii_alphanumeric()),
            "{code} is not camelCase"
        );
        assert!(
            octocode_native::id::error_codes::parse(code).is_some(),
            "{code} is not declared"
        );
    }
    runtime.close().await;
}
