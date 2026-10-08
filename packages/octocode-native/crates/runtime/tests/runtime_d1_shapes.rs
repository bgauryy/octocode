//! D1 hard-cutover shapes, through each tool's real runtime path: one
//! workspace-relative path form (D4), one name per purpose (X4b, X5, AR3,
//! CL4, X6), and the retired-name deny-list
//! (`skills-dev/octocode-dev/scripts/retired-names.json`), checked against
//! every tool output these tests produce.
#![allow(clippy::panic, clippy::unwrap_used)]

use crate::support::Workspace;
use octocode_native::runtime::{ToolOutcome, ToolRuntime};
use serde_json::{Value, json};

/// Run one non-debug row through the full runtime (validation, response
/// stages, path shaping), as an agent sees it.
async fn run(runtime: &ToolRuntime, tool: &str, mut row: Value) -> ToolOutcome {
    row["mainGoal"] = json!("D1 cutover shapes.");
    let outcome = runtime
        .execute(format!("d1-{tool}"), tool.into(), json!({"queries":[row]}))
        .await
        .unwrap_or_else(|error| panic!("{tool}: {error:?}"));
    assert_no_retired_names(tool, &outcome.structured_content);
    outcome
}

fn data(outcome: &ToolOutcome) -> &Value {
    &outcome.structured_content["results"][0]["data"]
}

/// The deny-list entries `tool`'s output must not carry: scope `field` (an
/// output key), `lead` (a `next`/`hints` key), or `code` (an `errorCode`
/// value), limited to the entry's `tools` when it lists them.
fn retired(tool: &str) -> Vec<(String, String)> {
    crate::support::retired_names(&["field", "lead", "code"], Some(tool))
        .into_iter()
        .map(|entry| {
            (
                entry["name"].as_str().expect("name").to_owned(),
                entry["scope"].as_str().expect("scope").to_owned(),
            )
        })
        .collect()
}

/// No retired field or lead name appears as a key anywhere in `value`.
fn assert_no_retired_names(tool: &str, value: &Value) {
    let names = retired(tool);
    let mut stack = vec![value];
    while let Some(node) = stack.pop() {
        match node {
            Value::Object(map) => {
                for (key, child) in map {
                    if let Some((name, scope)) = names
                        .iter()
                        .find(|(name, scope)| scope != "code" && name == key)
                    {
                        panic!("{tool}: retired {scope} name `{name}` in output: {value}");
                    }
                    // A retired code is an `errorCode` value, not a key.
                    if key == "errorCode"
                        && let Some(code) = child.as_str()
                        && names
                            .iter()
                            .any(|(name, scope)| scope == "code" && name == code)
                    {
                        panic!("{tool}: retired errorCode `{code}` in output: {value}");
                    }
                    stack.push(child);
                }
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
}

fn beta_runtime(workspace: &Workspace) -> ToolRuntime {
    workspace.runtime(&[("OCTOCODE_BETA", "true".into())])
}

// ---------- S-a: D4, one workspace-relative path form ----------

#[tokio::test]
async fn ast_search_error_rows_name_workspace_relative_paths() {
    let workspace = Workspace::new();
    workspace.write("sub/dir/Cargo.toml", "[package]\nname = \"x\"\n");
    let runtime = workspace.runtime(&[]);
    let outcome = run(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":"sub/dir/Cargo.toml"}),
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
    let outcome = run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"sub/dir","language":"rust","pattern":"old($A)","maxFiles":1}),
    )
    .await;
    let data = data(&outcome);
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

#[tokio::test]
async fn ast_rewrite_paths_and_expected_hashes_are_workspace_relative() {
    let workspace = Workspace::new();
    let file = workspace.write("src/sub/a.ts", "export const f = (x) => old(x);\n");
    let runtime = beta_runtime(&workspace);
    let preview = run(
        &runtime,
        "astRewrite",
        json!({"path":"src/sub","language":"typescript","pattern":"old($A)","rewrite":"neu($A)"}),
    )
    .await;
    let body = &preview.structured_content;
    let shown = data(&preview);
    assert_eq!(shown["files"][0]["path"], "src/sub/a.ts", "{body}");
    assert_eq!(shown["matches"][0]["path"], "src/sub/a.ts", "{body}");
    let patch = shown["files"][0]["patch"].as_str().expect("patch");
    assert!(
        patch.starts_with("--- a/src/sub/a.ts\n+++ b/src/sub/a.ts\n"),
        "{patch}"
    );
    // The envelope `root` names the workspace; no second, absolute root.
    assert!(shown.get("root").is_none(), "{body}");
    assert_eq!(
        body["root"],
        workspace.workspace.to_string_lossy().as_ref(),
        "{body}"
    );
    let apply = shown["hints"]["apply"]["query"].clone();
    let row = &apply["queries"][0];
    assert_eq!(row["path"], "src/sub", "{apply}");
    let hashes = row["expectedHashes"].as_object().expect("expectedHashes");
    assert_eq!(
        hashes.keys().collect::<Vec<_>>(),
        ["src/sub/a.ts"],
        "{apply}"
    );
    // The lead runs verbatim and writes the previewed file.
    let applied = runtime
        .execute("d1-apply".into(), "astRewrite".into(), apply)
        .await
        .expect("apply");
    assert_no_retired_names("astRewrite", &applied.structured_content);
    let applied_data = data(&applied);
    assert_eq!(
        applied_data["transaction"]["committed"], true,
        "{}",
        applied.structured_content
    );
    assert_eq!(applied_data["files"][0]["path"], "src/sub/a.ts");
    assert_eq!(
        std::fs::read_to_string(file).expect("rewritten"),
        "export const f = (x) => neu(x);\n"
    );
    runtime.close().await;
}

#[tokio::test]
async fn boundary_relative_keys_from_old_preview_fail_closed_with_rerun_hint() {
    let workspace = Workspace::new();
    let file = workspace.write("src/sub/a.ts", "export const f = (x) => old(x);\n");
    let runtime = beta_runtime(&workspace);
    let preview = run(
        &runtime,
        "astRewrite",
        json!({"path":"src/sub","language":"typescript","pattern":"old($A)","rewrite":"neu($A)"}),
    )
    .await;
    let mut apply = data(&preview)["hints"]["apply"]["query"].clone();
    let row = &mut apply["queries"][0];
    let hash = row["expectedHashes"]["src/sub/a.ts"].clone();
    // A pre-D4 preview keyed its hashes relative to the scanned directory.
    row["expectedHashes"] = json!({"a.ts": hash});
    let rejected = run(&runtime, "astRewrite", row.clone()).await;
    let result = &rejected.structured_content["results"][0];
    assert_eq!(result["status"], "error", "{result}");
    let code = result["data"]["errorCode"].as_str().unwrap_or_default();
    assert!(code.starts_with("expectedHash"), "{result}");
    let hint = result["data"]["hints"].to_string();
    assert!(hint.contains("re-run the preview"), "{result}");
    assert_eq!(
        std::fs::read_to_string(&file).expect("unchanged"),
        "export const f = (x) => old(x);\n"
    );
    // Absolute keys are still accepted.
    let absolute = workspace.workspace.join("src/sub/a.ts");
    row["expectedHashes"] = json!({absolute.to_string_lossy(): hash});
    let applied = run(&runtime, "astRewrite", row.clone()).await;
    assert_eq!(
        data(&applied)["transaction"]["committed"],
        true,
        "{}",
        applied.structured_content
    );
    runtime.close().await;
}

// ---------- S-b: X4b, X5, AR3, CL4 names ----------

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
    let outcome = run(
        &runtime,
        "artifactSearch",
        json!({"ecosystem":"npm","packageName":"zzqq-missing","registryUrl":server.uri()}),
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
        row["mainGoal"] = json!("D1 cutover shapes.");
        let error = runtime
            .execute(
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
    let reviews = run(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,"sections":["body","reviews"]}),
    )
    .await;
    let review = &data(&reviews)["pullRequests"][0]["reviews"][0];
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
    let comments = run(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"issue","owner":"a","repo":"b","number":9,"sections":["comments"]}),
    )
    .await;
    let comment = &data(&comments)["issues"][0]["comments"][0];
    assert_eq!(comment["author"], "bob", "{}", comments.structured_content);
    runtime.close().await;
}

// ---------- S-c: X6 + B12, one count vocabulary ----------

#[tokio::test]
async fn local_search_counts_are_match_line_and_file_counts() {
    let workspace = Workspace::new();
    for name in ["a.txt", "b.txt", "c.txt"] {
        workspace.write(name, "needle\nx\nneedle\n");
    }
    let runtime = workspace.runtime(&[]);
    let paged = run(
        &runtime,
        "localSearch",
        json!({"path":".","matchString":"needle","pageSize":1}),
    )
    .await;
    let stats = &data(&paged)["stats"];
    assert_eq!(stats["matchCount"], 6, "{}", paged.structured_content);
    assert_eq!(stats["matchedLineCount"], 6, "{}", paged.structured_content);
    assert_eq!(stats["fileCount"], 3, "{}", paged.structured_content);
    // `pagination.totalItems` counts the paged unit: files.
    assert_eq!(
        data(&paged)["pagination"]["totalItems"],
        3,
        "{}",
        paged.structured_content
    );
    // A merged context window lists its matched lines as `matchedLines`.
    let merged = run(
        &runtime,
        "localSearch",
        json!({"path":"a.txt","matchString":"needle","contextLines":2}),
    )
    .await;
    let row = &data(&merged)["files"][0]["matches"][0];
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
    let search = run(
        &runtime,
        "astSearch",
        // A complete page drops its stats; debug keeps them.
        json!({"operation":"match","path":"src","language":"typescript","pattern":"old($A)","debug":true}),
    )
    .await;
    assert_eq!(
        data(&search)["stats"]["matchCount"],
        2,
        "{}",
        search.structured_content
    );
    let rewrite = run(
        &runtime,
        "astRewrite",
        json!({"path":"src","language":"typescript","pattern":"old($A)","rewrite":"neu($A)"}),
    )
    .await;
    assert_eq!(
        data(&rewrite)["matchCount"],
        2,
        "{}",
        rewrite.structured_content
    );
    runtime.close().await;
}

// ---------- S-d: D2/X3/LP1/LP3, one 1-based column base ----------

#[tokio::test]
async fn every_column_is_one_based() {
    let workspace = Workspace::new();
    // "a()" starts at the 4th character of line 1 and the 10th of line 2.
    workspace.write("x.rs", "fn a() {}\nfn b() { a(); }\n");
    let runtime = beta_runtime(&workspace);
    let spans = run(
        &runtime,
        "localSearch",
        json!({"path":"x.rs","matchString":"a()","resultView":"matchOnly"}),
    )
    .await;
    let rows = &data(&spans)["files"][0]["matches"];
    assert_eq!(rows[0]["column"], 4, "{}", spans.structured_content);
    assert_eq!(rows[1]["column"], 10, "{}", spans.structured_content);
    let matched = run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"x.rs","pattern":"a()","captureText":true}),
    )
    .await;
    let row = &data(&matched)["files"][0]["matches"][0];
    assert_eq!(row["line"], 2, "{}", matched.structured_content);
    assert_eq!(row["column"], 10, "{}", matched.structured_content);
    let tree = run(
        &runtime,
        "astSearch",
        json!({"operation":"syntaxTree","path":"x.rs","pageSize":2}),
    )
    .await;
    // `function_item` spans line 1 from character 1 to the end (exclusive 10).
    assert_eq!(
        data(&tree)["nodes"][1],
        "1 function_item 1:1-1:10 ^0",
        "{}",
        tree.structured_content
    );
    let rewrite = run(
        &runtime,
        "astRewrite",
        json!({"path":"x.rs","pattern":"a()","rewrite":"z()","debug":true}),
    )
    .await;
    let range = &data(&rewrite)["matches"][0]["range"];
    assert_eq!(
        range["start"]["column"], 10,
        "{}",
        rewrite.structured_content
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
            json!({"operation":"callHierarchy","path":"a.ts","symbolName":"f","lineHint":1}),
            ["callHierarchy", "callers"],
        ),
    ] {
        let mut row = row;
        row["mainGoal"] = json!("D1 cutover shapes.");
        let error = runtime
            .execute(
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

// ---------- S-e: X1/X2, location rows are objects named like tool inputs ----------

#[tokio::test]
async fn location_rows_are_objects_named_like_tool_inputs() {
    let workspace = Workspace::new();
    workspace.write(
        "pkg/src/lib.rs",
        "pub fn run() {\n    let a = 1;\n    let b = a + 1;\n}\n",
    );
    workspace.write("pkg/a.txt", "x\n");
    let runtime = workspace.runtime(&[]);
    // X2: the enclosing declaration is an object an lspSearch/localFetch
    // call takes as is.
    let hits = run(
        &runtime,
        "localSearch",
        json!({"path":"pkg","matchString":"let"}),
    )
    .await;
    let first = &data(&hits)["files"][0]["matches"][0];
    assert_eq!(
        first["enclosing"],
        json!({"symbolName":"run","kind":"function","line":1,"endLine":4}),
        "{}",
        hits.structured_content
    );
    assert!(first.get("in").is_none(), "{first}");
    // P1: a declaration without members is an entry string.
    let outline = run(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":"pkg"}),
    )
    .await;
    assert_eq!(
        data(&outline)["files"][0]["symbols"][0],
        json!("run (1-4, function, exported)"),
        "{}",
        outline.structured_content
    );
    // AS2: match rows are objects with 1-based columns.
    let matched = run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"pkg","language":"rust","pattern":"let $A = $B;"}),
    )
    .await;
    assert_eq!(
        data(&matched)["files"][0]["matches"][0],
        json!({"line":2,"column":5,"value":"let a = 1;"}),
        "{}",
        matched.structured_content
    );
    // SS2/SS4: one listing shape, workspace-relative groups, key `files`.
    let tree = run(&runtime, "structureSearch", json!({"path":"pkg"})).await;
    assert!(
        data(&tree).get("entries").is_none(),
        "{}",
        tree.structured_content
    );
    assert_eq!(
        data(&tree)["files"],
        json!([{"dir":"pkg","files":["a.txt (2)","src/"]}]),
        "{}",
        tree.structured_content
    );
    let files = run(
        &runtime,
        "structureSearch",
        json!({"path":"pkg","operation":"files"}),
    )
    .await;
    assert_eq!(
        data(&files)["files"],
        json!([{"dir":"pkg","files":["a.txt (2)"]},{"dir":"pkg/src","files":["lib.rs (51)"]}]),
        "{}",
        files.structured_content
    );
    runtime.close().await;
}

// ---------- S-f: X10 lead names, X12 runnable recoveries ----------

/// The lead `name` under `next` or `hints`, whichever channel holds it.
fn lead<'a>(outcome: &'a ToolOutcome, name: &str) -> &'a Value {
    let data = data(outcome);
    [&data["next"][name], &data["hints"][name]]
        .into_iter()
        .find(|lead| lead.is_object())
        .unwrap_or_else(|| panic!("no `{name}` lead: {}", outcome.structured_content))
}

#[tokio::test]
async fn a_case_sensitive_miss_leads_to_a_runnable_ignore_case_read() {
    let workspace = Workspace::new();
    workspace.write("pkg/a.txt", "alpha\nHello World\n");
    let runtime = workspace.runtime(&[]);
    let miss = run(
        &runtime,
        "localFetch",
        json!({"path":"pkg/a.txt","matchString":"hello World"}),
    )
    .await;
    let hint = data(&miss)["hints"]
        .as_array()
        .and_then(|hints| hints.iter().find_map(Value::as_str))
        .or_else(|| data(&miss)["hints"]["text"][0].as_str())
        .unwrap_or_default()
        .to_owned();
    assert!(!hint.contains("caseMode"), "{hint}");
    let ignore = lead(&miss, "ignoreCase");
    assert_eq!(ignore["tool"], "localFetch", "{ignore}");
    let row = ignore["query"]["queries"][0].clone();
    assert_eq!(row["caseMode"], "insensitive", "{row}");
    let hit = run(&runtime, "localFetch", row).await;
    assert!(
        data(&hit)["content"]
            .as_str()
            .is_some_and(|text| text.contains("Hello World")),
        "{}",
        hit.structured_content
    );
    // A miss that already ignored case offers no ignoreCase lead.
    let insensitive = run(
        &runtime,
        "localFetch",
        json!({"path":"pkg/a.txt","matchString":"absent","caseMode":"insensitive"}),
    )
    .await;
    assert!(
        data(&insensitive)["next"].get("ignoreCase").is_none()
            && data(&insensitive)["hints"].get("ignoreCase").is_none(),
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
    let missing = run(&runtime, "localFetch", json!({"path":"pkg/server.ts"})).await;
    let find = lead(&missing, "findFile");
    assert_eq!(find["tool"], "structureSearch", "{find}");
    runtime.close().await;
}

// ---------- S-g: X13b one camelCase error-code set ----------

#[tokio::test]
async fn error_codes_are_declared_camel_case_names() {
    let workspace = Workspace::new();
    workspace.write("pkg/a.rs", "fn a() {}\n");
    let runtime = workspace.runtime(&[]);
    let unsupported = run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"pkg/a.rs","language":"cobol","pattern":"fn $A() {}"}),
    )
    .await;
    assert_eq!(
        data(&unsupported)["errorCode"],
        "languageUnsupported",
        "{}",
        unsupported.structured_content
    );
    let missing = run(&runtime, "localFetch", json!({"path":"pkg/none.rs"})).await;
    assert_eq!(data(&missing)["errorCode"], "pathNotFound");
    for outcome in [&unsupported, &missing] {
        let code = data(outcome)["errorCode"].as_str().expect("code");
        assert!(
            code.chars().all(|c| c.is_ascii_alphanumeric()),
            "{code} is not camelCase"
        );
        assert!(
            octocode_native::tools::id::error_codes::class(code).is_some(),
            "{code} is not declared"
        );
    }
    runtime.close().await;
}
