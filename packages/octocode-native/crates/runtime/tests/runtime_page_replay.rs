use crate::support::{self, Run as _};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;
use serde_json::json;
use support::Workspace;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn lines(word: &str) -> String {
    (0..200)
        .map(|index| format!("needle {word} {index}\n"))
        .collect()
}

async fn saved_fixture(
    workspace: &Workspace,
    runtime: &octocode_native::runtime::ToolRuntime,
) -> (Value, Value) {
    let file = workspace.write("saved-partitions.txt", lines("captured"));
    let query = json!({"path":file,"matchString":"needle","pageSize":50});
    let first = runtime
        .run(
            "save".into(),
            "localSearch".into(),
            json!({"queries":[query],"responseLength":800,"responseScope":"rows"}),
        )
        .await
        .expect("save");
    let next = first.structured_content["responsePagination"]["next"]["query"].clone();
    assert!(next.is_object(), "{}", first.structured_content);
    assert!(
        next["responseSnapshot"]
            .as_str()
            .is_some_and(|snapshot| snapshot.len() <= 128),
        "saved continuation must contain a snapshot string of at most 128 bytes: {next}"
    );
    octocode_native::contracts::validate("localSearch", next.clone())
        .expect("canonical continuation");
    (query, next)
}

#[tokio::test]
async fn saved_pages_retain_canonical_input_scope_across_persistent_replay() {
    for (scope, output_scope) in [
        ("text", "content.text"),
        ("structured", "structuredContent"),
        ("rows", "rows"),
    ] {
        let workspace = Workspace::new();
        let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
        let runtime = workspace.runtime(&env);
        let files = (0..3)
            .map(|index| workspace.write(&format!("page-{index}.txt"), lines("captured")))
            .collect::<Vec<_>>();
        let queries = files
            .iter()
            .map(|file| json!({"path":file,"fullContent":true}))
            .collect::<Vec<_>>();
        let first = runtime
            .run(
                "capture".into(),
                "localFetch".into(),
                json!({"queries":queries,"responseLength":96,"responseScope":scope}),
            )
            .await
            .expect("initial page");
        assert_eq!(
            first.structured_content["responsePagination"]["scope"],
            output_scope
        );
        let next = first.structured_content["responsePagination"]["next"]["query"].clone();
        assert!(next.is_object(), "{}", first.structured_content);
        octocode_native::contracts::validate("localFetch", next.clone())
            .expect("canonical continuation");
        runtime.close().await;
        for file in files {
            std::fs::remove_file(file).expect("remove live source");
        }
        let runtime = workspace.runtime(&env);
        for omit_scope in [false, true] {
            let mut request = next.clone();
            if omit_scope {
                request
                    .as_object_mut()
                    .expect("compact continuation object")
                    .remove("responseScope");
            }
            let page = runtime
                .run("replay".into(), "localFetch".into(), request)
                .await
                .expect("saved page uses canonical scope");
            let pagination = &page.structured_content["responsePagination"];
            assert_eq!(pagination["scope"], output_scope);
            assert_eq!(pagination["replayed"], true);
            if let Some(next) = pagination["next"].get("query") {
                assert_eq!(next["responseScope"], scope);
                octocode_native::contracts::validate("localFetch", next.clone())
                    .expect("canonical saved continuation");
            }
        }
        runtime.close().await;
    }
}

#[tokio::test]
async fn saved_pages_reject_mixed_input_tool_credential_config_and_workspace_changes() {
    use octocode_native::runtime::{Channel, ToolRuntime};
    let workspace = Workspace::new();
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let runtime = workspace.runtime(&env);
    let (query, next) = saved_fixture(&workspace, &runtime).await;
    for changed in [
        query.clone(),
        json!({"path":"/changed","matchString":"new question"}),
    ] {
        let mut mixed = next.clone();
        mixed["queries"] = json!([changed]);
        let error = runtime
            .run("mixed".into(), "localSearch".into(), mixed)
            .await
            .expect_err("mixed input");
        assert_eq!(error.code, "invalidInput");
    }
    let error = runtime
        .run("tool".into(), "localFetch".into(), next.clone())
        .await
        .expect_err("wrong tool");
    assert_eq!(error.code, "responseSnapshotExpired");
    let admission = runtime
        .admit_with(
            "credential".into(),
            Some(json!({"githubToken":"different-request-token"})),
        )
        .expect("admit");
    let error = runtime
        .execute(
            admission,
            "localSearch".into(),
            next.clone(),
            Channel::Structured,
        )
        .await
        .expect_err("credential boundary");
    assert_eq!(error.code, "responseSnapshotExpired");
    let admission = runtime.admit("surface".into()).expect("admit");
    let error = runtime
        .execute(admission, "localSearch".into(), next.clone(), Channel::Mcp)
        .await
        .expect_err("surface boundary");
    assert_eq!(error.code, "responseSnapshotExpired");
    for changed in ["cwd", "config"] {
        let mut input = workspace.config(&env);
        if changed == "cwd" {
            input.cwd = workspace.workspace.join("subdirectory");
            std::fs::create_dir_all(&input.cwd).expect("cwd");
        } else {
            input
                .env
                .insert("OCTOCODE_ENABLE_STATS".into(), "true".into());
        }
        let other = ToolRuntime::new(input).expect("other runtime");
        let error = other
            .run(changed.into(), "localSearch".into(), next.clone())
            .await
            .expect_err("partition");
        assert_eq!(error.code, "responseSnapshotExpired");
        other.close().await;
    }
    runtime.close().await;
}

#[tokio::test]
async fn expired_disk_snapshot_does_not_reexecute() {
    let workspace = Workspace::new();
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let runtime = workspace.runtime(&env);
    let (_, next) = saved_fixture(&workspace, &runtime).await;
    runtime.close().await;
    let dir = workspace.home.join("tmp/response-pages");
    // An entry file is its header line, then the value.
    for file in std::fs::read_dir(dir).expect("snapshot store").flatten() {
        if file.path().extension().is_some_and(|ext| ext == "entry") {
            let bytes = std::fs::read(file.path()).expect("entry");
            let end = bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .expect("header");
            let mut header: Value = serde_json::from_slice(&bytes[..end]).expect("header json");
            header["storedAtUnix"] = json!(1);
            let mut aged = header.to_string().into_bytes();
            aged.extend_from_slice(&bytes[end..]);
            std::fs::write(file.path(), aged).expect("age entry");
        }
    }
    let runtime = workspace.runtime(&env);
    let error = runtime
        .run("expired".into(), "localSearch".into(), next)
        .await
        .expect_err("expired snapshot");
    assert_eq!(error.code, "responseSnapshotExpired");
    runtime.close().await;
}

#[tokio::test]
async fn failed_snapshot_retention_returns_complete_result_without_display_next() {
    let workspace = Workspace::new();
    std::fs::create_dir_all(workspace.home.join("tmp")).expect("cache parent");
    std::fs::write(workspace.home.join("tmp/response-pages"), "block directory")
        .expect("blocked cache");
    let runtime = workspace.runtime(&[("OCTOCODE_STORAGE_MODE", "persistent".into())]);
    let file = workspace.write("saved.txt", lines("all-evidence"));
    let response = runtime
        .run(
            "fallback".into(),
            "localSearch".into(),
            json!({
                "queries":[{"path":file,"matchString":"needle","pageSize":50}],
                "responseLength":800,"responseScope":"rows"
            }),
        )
        .await
        .expect("complete fallback");
    let output = response.structured_content;
    assert!(output.get("responsePagination").is_none(), "{output}");
    assert!(
        output["warnings"].to_string().contains("complete result"),
        "{output}"
    );
    assert_eq!(
        output["results"][0]["data"]["files"][0]["matches"]
            .as_array()
            .expect("complete match array")
            .len(),
        200
    );
    runtime.close().await;
}

#[tokio::test]
async fn immutable_response_pages_survive_source_changes_and_persistent_runtime_restart() {
    for restart in [false, true] {
        let workspace = Workspace::new();
        let file = workspace.write("saved.txt", lines("captured"));
        let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
        let mut runtime = workspace.runtime(&env);
        let first = runtime
            .run(
                "capture".into(),
                "localSearch".into(),
                json!({
                    "queries":[{"path":file,"matchString":"needle","pageSize":50}],
                    "responseLength":800,"responseScope":"rows"
                }),
            )
            .await
            .expect("initial page");
        let next = first.structured_content["responsePagination"]["next"]["query"].clone();
        assert!(next.is_object(), "{}", first.structured_content);
        std::fs::remove_file(&file).expect("remove live source");
        if restart {
            runtime.close().await;
            runtime = workspace.runtime(&env);
        }
        let page = runtime
            .run("saved-page".into(), "localSearch".into(), next)
            .await
            .expect("saved result remains readable");
        assert!(
            page.structured_content.to_string().contains("captured"),
            "{}",
            page.structured_content
        );
        assert_ne!(
            page.structured_content["responsePagination"]["restart"],
            true
        );
        runtime.close().await;
    }
}

#[tokio::test]
async fn immutable_response_missing_memory_session_expires_without_dispatch() {
    let workspace = Workspace::new();
    let file = workspace.write("saved.txt", lines("captured"));
    let env = [("OCTOCODE_STORAGE_MODE", "memory".into())];
    let runtime = workspace.runtime(&env);
    let first = runtime
        .run(
            "capture".into(),
            "localSearch".into(),
            json!({
                "queries":[{"path":file,"matchString":"needle","pageSize":50}],
                "responseLength":800,"responseScope":"rows"
            }),
        )
        .await
        .expect("initial page");
    let next = first.structured_content["responsePagination"]["next"]["query"].clone();
    assert!(next.is_object());
    runtime.close().await;
    let runtime = workspace.runtime(&env);
    let error = runtime
        .run("lost-page".into(), "localSearch".into(), next)
        .await
        .expect_err("a missing saved result never re-executes");
    assert_eq!(error.code, "responseSnapshotExpired");
    runtime.close().await;
}

#[tokio::test]
async fn symbol_row_fragments_preserve_every_alias_and_member_once() {
    let workspace = Workspace::new();
    let aliases = (0..80)
        .map(|index| format!("Alias{index:03}_{}", "exported_name_".repeat(10)))
        .collect::<Vec<_>>();
    let methods = (0..5)
        .map(|index| format!("  method{index}(): string {{ return \"member {index}\"; }}\n"))
        .collect::<String>();
    let exports = aliases
        .iter()
        .map(|name| format!("  Container as {name},\n"))
        .collect::<String>();
    let file = workspace.write(
        "class.ts",
        format!("export class Container {{\n{methods}}}\nexport {{\n{exports}}};\n"),
    );
    let runtime = workspace.runtime(&[]);
    let mut pending = Some(json!({
        "queries":[{"operation":"symbols","path":file,"pageSize":1000}],
        "responseScope":"rows","responseLength":1000
    }));
    let mut seen_aliases = Vec::new();
    let mut seen_members = Vec::new();
    let mut pages = 0;
    while let Some(query) = pending.take() {
        assert!(pages < 100, "symbol row continuation did not terminate");
        let outcome = runtime
            .run(
                format!("symbol-fragment-{pages}"),
                "astSearch".into(),
                query,
            )
            .await
            .expect("every symbol fragment satisfies the output contract");
        let output = &outcome.structured_content;
        for row in output["results"].as_array().expect("result rows") {
            for symbol in row["data"]["symbols"].as_array().expect("symbols") {
                assert_eq!(symbol["symbolName"], "Container");
                for alias in symbol["exportedAs"].as_array().into_iter().flatten() {
                    seen_aliases.push(alias.as_str().expect("export alias").to_owned());
                }
                for member in symbol["members"].as_array().into_iter().flatten() {
                    seen_members.push((
                        member["symbolName"]
                            .as_str()
                            .expect("member name")
                            .to_owned(),
                        member["line"].as_u64().expect("member source line"),
                    ));
                }
            }
        }
        let page = &output["responsePagination"];
        if page["hasMore"] == true {
            assert_eq!(page["next"]["tool"], "astSearch");
            pending = Some(page["next"]["query"].clone());
        }
        pages += 1;
    }
    assert!(pages > 2, "the fixture must exercise later row fragments");
    let mut public_names = aliases;
    public_names.push("Container".to_owned());
    assert_eq!(seen_aliases, public_names);
    assert_eq!(
        seen_members,
        (0..5)
            .map(|index| (format!("method{index}"), index + 2))
            .collect::<Vec<_>>()
    );
    runtime.close().await;
}

/// Unreadable but otherwise unchanged (same size and modification time):
/// a re-execution could no longer read the file, a replay still serves it.
#[cfg(unix)]
fn unreadable(path: &std::path::Path, locked: bool) {
    use std::os::unix::fs::PermissionsExt;
    let mode = if locked { 0o000 } else { 0o644 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

#[cfg(unix)]
#[tokio::test]
async fn a_matching_snapshot_pages_the_stored_envelope_without_re_executing() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", lines("original"));
    settle(&workspace.workspace);
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);
    let first = runtime
        .run(
            "page-1".into(),
            "localSearch".into(),
            json!({
                "queries":[{"path":root,"matchString":"needle","pageSize":50,
                    "mainGoal":"Find needles.","reasoning":"Exercise page replay."}],
                "responseLength": 800, "responseScope": "text"
            }),
        )
        .await
        .expect("first page");
    let pagination = first.structured_content["responsePagination"].clone();
    assert_eq!(pagination["hasMore"], true, "{pagination}");
    let snapshot = pagination["snapshot"].clone();
    let next = pagination["next"]["query"].clone();
    assert_eq!(next["responseSnapshot"], snapshot, "{pagination}");

    // A re-execution could no longer read the file and would demand a
    // restart; the stored envelope keeps serving the same response.
    unreadable(&file, true);
    let second = runtime
        .run("page-2".into(), "localSearch".into(), next.clone())
        .await
        .expect("second page");
    unreadable(&file, false);
    let page = &second.structured_content["responsePagination"];
    assert_eq!(page["snapshot"], snapshot, "{page}");
    assert_ne!(page["restart"], true, "{page}");
    assert!(page["offset"].as_u64().unwrap_or(0) > 0, "{page}");
    assert!(text(&second).contains("original"), "page 2 re-executed");

    let mut stale: Value = next;
    stale["responseSnapshot"] = json!(format!("{}0", snapshot.as_str().unwrap()));
    let third = runtime
        .run("page-stale".into(), "localSearch".into(), stale)
        .await
        .expect_err("unknown snapshots do not execute");
    assert_eq!(third.code, "responseSnapshotExpired");
    runtime.close().await;
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Dispatch counter: the content endpoint is hit `content_hits` times.
async fn github_file_server(content_hits: u64) -> MockServer {
    let server = MockServer::builder().start().await;
    let sha = SHA;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    let body = lines("remote");
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Flib.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(&body),
            "size": body.len(),
            "sha": "f".repeat(40),
            "path": "src/lib.rs"
        })))
        .expect(content_hits)
        .mount(&server)
        .await;
    server
}

/// A matching snapshot replays the stored answer at a commit SHA and on a
/// branch alike. A mismatched snapshot fails without fetching content again.
#[tokio::test]
async fn a_matching_snapshot_replays_and_a_mismatch_fails_without_dispatch() {
    for reference in [SHA, "main"] {
        let content_hits = 1;
        let server = github_file_server(content_hits).await;
        pages_at(&server, reference).await;
        drop(server);
    }
}

async fn pages_at(server: &MockServer, reference: &str) {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let first = runtime
        .run(
            "gh-page-1".into(),
            "ghGetFileContent".into(),
            json!({
                "queries":[{"owner":"a","repo":"b","path":"src/lib.rs","ref":reference,
                    "forceRefresh":true,"offset":0,"length":6000,"unit":"bytes",
                    "mainGoal":"Read the file.","reasoning":"Exercise page replay."}],
                "responseLength": 800, "responseScope": "text"
            }),
        )
        .await
        .expect("first page");
    let pagination = first.structured_content["responsePagination"].clone();
    assert_eq!(pagination["hasMore"], true, "{pagination}");
    // Page 2 is not the last page, so it carries the snapshot it continues.
    assert!(pagination["totalPages"].as_u64() >= Some(3), "{pagination}");
    let next = pagination["next"]["query"].clone();
    assert!(next.get("queries").is_none(), "{next}");

    for page in ["gh-page-2", "gh-page-2-again"] {
        let replayed = runtime
            .run(page.into(), "ghGetFileContent".into(), next.clone())
            .await
            .expect("replayed page");
        let page = &replayed.structured_content["responsePagination"];
        assert_eq!(page["snapshot"], pagination["snapshot"], "{page}");
        assert_ne!(page["restart"], true, "{page}");
    }

    let mut stale = next;
    stale["responseSnapshot"] = json!("response-rows-v2:stale");
    let error = runtime
        .run("gh-page-stale".into(), "ghGetFileContent".into(), stale)
        .await
        .expect_err("a mismatched snapshot never executes");
    assert_eq!(error.code, "responseSnapshotExpired");
    runtime.close().await;
}

#[cfg(unix)]
#[tokio::test]
async fn rows_scope_pages_replay_the_stored_envelope() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", lines("original"));
    settle(&workspace.workspace);
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);
    let row = |word: &str| json!({"path":root,"matchString":word,"mainGoal":"Find needles.","reasoning":"Exercise page replay."});
    let first = runtime
        .run(
            "rows-1".into(),
            "localSearch".into(),
            json!({"queries":[row("needle"), row("original")],
                "responseScope":"rows","responseLength": 1200}),
        )
        .await
        .expect("first rows page");
    let pagination = first.structured_content["responsePagination"].clone();
    assert_eq!(pagination["hasMore"], true, "{pagination}");
    let snapshot = pagination["snapshot"].as_str().unwrap().to_owned();
    assert!(snapshot.starts_with("saved-v1:"), "{snapshot}");
    let next = pagination["next"]["query"].clone();
    assert_eq!(next["responseSnapshot"], snapshot.as_str(), "{pagination}");

    let mut changed_window = next.clone();
    changed_window["responseLength"] = json!(2400);
    let restart = runtime
        .run(
            "rows-changed-window".into(),
            "localSearch".into(),
            changed_window,
        )
        .await
        .expect_err("changed row plan cannot replay the saved page");
    assert_eq!(restart.code, "responsePageMismatch");

    unreadable(&file, true);
    let second = runtime
        .run("rows-2".into(), "localSearch".into(), next)
        .await
        .expect("second rows page");
    unreadable(&file, false);
    let page = &second.structured_content["responsePagination"];
    assert_eq!(page["snapshot"], snapshot.as_str(), "{page}");
    assert_ne!(page["restart"], true, "{page}");
    runtime.close().await;
}

/// A continuation replays its origin even when the caller spelled a default
/// field in a different key position than the validated continuation does.
#[cfg(unix)]
#[tokio::test]
async fn a_continuation_replays_regardless_of_the_callers_key_order() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", lines("original"));
    settle(&workspace.workspace);
    let runtime = workspace.runtime(&[]);
    let first = runtime
        .run(
            "order-1".into(),
            "localFetch".into(),
            json!({
                "queries":[{"reasoning":"Exercise page replay.",
                    "path":file.to_string_lossy(),"offset":0,"length":6000,"unit":"bytes"}],
                "responseLength": 800, "responseScope": "text"
            }),
        )
        .await
        .expect("first page");
    let pagination = first.structured_content["responsePagination"].clone();
    assert_eq!(pagination["hasMore"], true, "{pagination}");
    // Page 2 is not the last page, so it carries the snapshot it continues.
    assert!(pagination["totalPages"].as_u64() >= Some(3), "{pagination}");
    unreadable(&file, true);
    let second = runtime
        .run(
            "order-2".into(),
            "localFetch".into(),
            pagination["next"]["query"].clone(),
        )
        .await
        .expect("second page");
    unreadable(&file, false);
    let page = &second.structured_content["responsePagination"];
    assert_eq!(page["snapshot"], pagination["snapshot"], "{page}");
    assert_ne!(page["restart"], true, "{page}");
    runtime.close().await;
}

/// Every file and directory under `root`, ten seconds older: settled sources
/// whose later edits a replay can tell apart.
fn settle(root: &std::path::Path) {
    let then = std::time::SystemTime::now() - std::time::Duration::from_secs(10);
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            stack.extend(
                std::fs::read_dir(&path)
                    .expect("dir")
                    .map(|e| e.expect("entry").path()),
            );
        }
        std::fs::File::open(&path)
            .expect("open")
            .set_modified(then)
            .expect("backdate");
    }
}

fn text(outcome: &octocode_native::response::outcome::ToolOutcome) -> String {
    outcome
        .content
        .iter()
        .map(|content| content.text.as_str())
        .collect()
}

/// After sources change, a persistent runtime and a fresh runtime both
/// serve the saved evidence without consulting the changed files.
#[tokio::test]
async fn an_edit_between_pages_answers_like_a_fresh_runtime() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", lines("original"));
    workspace.write("src/b.txt", lines("second"));
    settle(&workspace.workspace);
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let persistent = workspace.runtime(&[("OCTOCODE_STORAGE_MODE", "persistent".into())]);
    let first = persistent
        .run(
            "fresh-1".into(),
            "localSearch".into(),
            json!({
                "queries":[{"path":root,"matchString":"needle","pageSize":50,
                    "mainGoal":"Find needles.","reasoning":"Exercise page replay."}],
                "responseLength": 800, "responseScope": "text"
            }),
        )
        .await
        .expect("first page");
    let pagination = first.structured_content["responsePagination"].clone();
    assert_eq!(pagination["hasMore"], true, "{pagination}");
    let next = pagination["next"]["query"].clone();

    workspace.write("src/a.txt", lines("rewritten"));
    workspace.write("src/b.txt", lines("changed"));
    let replayed = persistent
        .run("fresh-2".into(), "localSearch".into(), next.clone())
        .await
        .expect("persistent page");
    let fresh_runtime = workspace.runtime(&[("OCTOCODE_STORAGE_MODE", "persistent".into())]);
    let fresh = fresh_runtime
        .run("fresh-2".into(), "localSearch".into(), next)
        .await
        .expect("fresh page");
    let page = &replayed.structured_content["responsePagination"];
    assert_ne!(page["restart"], true, "{page}");
    assert_eq!(
        replayed.structured_content, fresh.structured_content,
        "persistent and fresh runtimes disagree"
    );
    assert_eq!(text(&replayed), text(&fresh));
    assert!(
        text(&replayed).contains("original"),
        "saved evidence was replaced"
    );
    persistent.close().await;
    fresh_runtime.close().await;
}

/// An offset or snapshot without `responseLength` is no explicit window: the
/// response keeps the automatic page and read share, exactly as the same call
/// without it, and is never served unpaged past the page.
#[tokio::test]
async fn an_offset_or_snapshot_alone_keeps_the_automatic_page() {
    let workspace = Workspace::new();
    let rows: Vec<_> = (0..5)
        .map(|index| {
            let file = workspace.write(&format!("src/f{index}.txt"), lines(&"w".repeat(80)));
            json!({"path": file.to_string_lossy(), "fullContent": true,
                "mainGoal": "Read the files.", "reasoning": "Exercise automatic paging."})
        })
        .collect();
    let runtime = workspace.runtime(&[]);
    let call = |id: &str, extra: serde_json::Value| {
        let mut input = json!({"queries": rows});
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        runtime.run(id.into(), "localFetch".into(), input)
    };
    let default = call("auto", json!({})).await.expect("default");
    let size = |outcome: &octocode_native::response::outcome::ToolOutcome| {
        outcome.structured_content.to_string().len()
    };
    for (id, extra) in [("offset", json!({"responseOffset": 0}))] {
        let outcome = call(id, extra).await.expect(id);
        assert!(
            size(&outcome) <= size(&default) + 64,
            "{id}: {} vs automatic {}",
            size(&outcome),
            size(&default)
        );
    }
    assert!(
        size(&default) < 40_000,
        "automatic page: {}",
        size(&default)
    );
    runtime.close().await;
}

/// A localFetch row fills its share of the call's response page and pages by
/// its own continuation: an explicit `responseLength` returns the row itself,
/// never an emptied `results` with a re-cut text window.
#[tokio::test]
async fn local_fetch_rows_page_by_the_calls_response_window() {
    let workspace = Workspace::new();
    let wide: String = (0..2_000)
        .map(|line| format!("line {line:05} of wide text\n"))
        .collect();
    let file = workspace.write("src/wide.txt", &wide);
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .run(
            "fetch-window".into(),
            "localFetch".into(),
            json!({
                "queries":[{"path":file.to_string_lossy(),"fullContent":true,
                    "mainGoal":"Read the file.","reasoning":"Exercise row windows."}],
                "responseLength": 5000
            }),
        )
        .await
        .expect("page");
    let page = &outcome.structured_content;
    assert!(page.get("responseWindow").is_none(), "{page}");
    let row = &page["results"][0]["data"];
    let content = row["content"].as_str().expect("row content");
    assert!(
        !content.is_empty() && content.len() <= 5_000,
        "{}",
        content.len()
    );
    assert!(
        row["next"].is_object(),
        "the row continues by its own offset: {row}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn saved_github_pages_check_current_home_credentials_without_network_replay() {
    use octocode_native::providers::github::{CredentialStore, StoredCredentials};
    for tool in ["ghGetFileContent", "clasify"] {
        let server = github_file_server(1).await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(|request: &wiremock::Request| {
                let body: Value = serde_json::from_slice(&request.body).expect("provider request JSON");
                let probabilities: serde_json::Map<_, _> = (0..64)
                    .map(|index| (format!("label{index:02}"), json!(usize::from(index == 0))))
                    .collect();
                let answers: serde_json::Map<_, _> = body["questions"].as_object().expect("provider questions object").keys()
                    .map(|key| (key.clone(), json!({"type":"choice","choice":"label00","confidence":1,"probabilities":probabilities})))
                    .collect();
                ResponseTemplate::new(200).set_body_json(json!({"model":"jev-1.13.0","answers":answers,"usage":{"input_tokens":10,"output_tokens":2}}))
            })
            .expect(u64::from(tool == "clasify"))
            .mount(&server).await;
        let workspace = Workspace::new();
        let store = CredentialStore::new(&workspace.home);
        let save = |token: &str| {
            let credentials: StoredCredentials = serde_json::from_value(json!({
                "hostname":"127.0.0.1","username":"fixture","token":{"token":token,"tokenType":"oauth"},
                "gitProtocol":"https","createdAt":"","updatedAt":""
            })).expect("stored credential fixture");
            store.save(&credentials).expect("save home credential");
        };
        save("first-local-token");
        let env = [
            ("OCTOCODE_STORAGE_MODE", "persistent".into()),
            ("GITHUB_TOKEN", "".into()),
            ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
            ("OCTOCODE_BETA", "true".into()),
            ("OCTOCODE_CLASSIFICATION_API", "fixture".into()),
            ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ];
        let runtime = workspace.runtime(&env);
        let source = json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":SHA,
            "forceRefresh":true,"offset":0,"length":6000,"unit":"bytes"});
        let row = if tool == "clasify" {
            let criteria: serde_json::Map<_, _> = (0..64)
                .map(|index| (format!("label{index:02}"), json!("criterion")))
                .collect();
            let questions: serde_json::Map<_, _> = (0..12).map(|index| (format!("q{index}"),
                json!({"type":"choice","instructions":"Choose a label.","criteria":criteria}))).collect();
            json!({"source":{"tool":"ghGetFileContent","query":{"owner":"a","repo":"b","path":"src/lib.rs","ref":SHA,"fullContent":true}},"questions":questions})
        } else {
            source
        };
        let first = runtime
            .run(
                "auth-first".into(),
                tool.into(),
                json!({"queries":[row],"responseLength":800,"responseScope":"text"}),
            )
            .await
            .expect("first");
        let next = first.structured_content["responsePagination"]["next"]["query"].clone();
        assert!(next.is_object(), "{}", first.structured_content);
        assert!(
            next["responseSnapshot"]
                .as_str()
                .is_some_and(|snapshot| snapshot.len() <= 128),
            "saved continuation must contain a snapshot string of at most 128 bytes: {next}"
        );
        octocode_native::contracts::validate(tool, next.clone()).expect("public snapshot contract");
        runtime.close().await;
        let runtime = workspace.runtime(&env);
        let same = runtime
            .run("auth-same".into(), tool.into(), next.clone())
            .await
            .expect("same local identity");
        assert_eq!(
            same.structured_content["responsePagination"]["replayed"],
            true
        );
        save("switched-local-token");
        let changed = runtime
            .run("auth-switched".into(), tool.into(), next.clone())
            .await
            .expect_err("local identity changed");
        assert_eq!(changed.code, "responseSnapshotExpired");
        runtime.close().await;
        let runtime = workspace.runtime(&env);
        let changed = runtime
            .run("auth-switched-process".into(), tool.into(), next)
            .await
            .expect_err("new process identity changed");
        assert_eq!(changed.code, "responseSnapshotExpired");
        runtime.close().await;
        let requests = server
            .received_requests()
            .await
            .expect("recorded provider requests");
        assert_eq!(
            requests.len(),
            1 + usize::from(tool == "clasify"),
            "saved reads must not call either provider"
        );
    }
}

/// A 5,000-line file of short lines: its line-number gutters outweigh its
/// text, so a page sized without them overflows.
fn short_lines() -> String {
    (0..5_000).map(|line| format!("l{line}\n")).collect()
}

/// One staged MCP response to a whole-file read under an explicit
/// `responseLength`: both channels, measured as sent (numbered content, so
/// gutters included), fit the page; the row is returned whole with its own
/// `next.continueWindow`, never an emptied `results` beside a text
/// `responseWindow`.
fn assert_sized_source_page(result: &Value, length: usize) {
    let page = &result["structuredContent"];
    assert!(page.get("responseWindow").is_none(), "{length}: {page}");
    assert!(page.get("responsePagination").is_none(), "{length}: {page}");
    let row = &page["results"][0]["data"];
    assert!(
        row["content"]
            .as_str()
            .is_some_and(|content| !content.is_empty()),
        "{length}: {row}"
    );
    assert!(row["next"]["continueWindow"].is_object(), "{length}: {row}");
    let structured = page.to_string().encode_utf16().count();
    let text: usize = result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| item["text"].as_str().unwrap_or("").encode_utf16().count())
        .sum();
    assert!(structured <= length, "{length}: structured {structured}");
    assert!(text <= length, "{length}: text {text}");
}

const EXPLICIT_LENGTHS: [usize; 4] = [5_000, 30_000, 40_000, 50_000];

#[tokio::test]
async fn explicit_lengths_return_local_source_rows_sized_to_the_page() {
    let workspace = Workspace::new();
    let file = workspace.write("src/short.txt", short_lines());
    let runtime = workspace.runtime(&[]);
    for length in EXPLICIT_LENGTHS {
        let result = runtime
            .run_mcp(
                format!("fetch-{length}"),
                "localFetch".into(),
                json!({"queries":[{"path":file.to_string_lossy(),"fullContent":true}],
                    "responseLength": length}),
            )
            .await
            .expect("page");
        assert_sized_source_page(&result, length);
    }
    runtime.close().await;
}

#[tokio::test]
async fn explicit_lengths_return_github_source_rows_sized_to_the_page() {
    let server = MockServer::builder().start().await;
    let body = short_lines();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fshort.rs"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(&body),
            "size": body.len(),
            "sha": "f".repeat(40),
            "path": "src/short.rs"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for length in EXPLICIT_LENGTHS {
        let result = runtime
            .run_mcp(
                format!("gh-{length}"),
                "ghGetFileContent".into(),
                json!({"queries":[{"owner":"a","repo":"b","path":"src/short.rs","ref":SHA,
                    "fullContent":true}],
                    "responseLength": length}),
            )
            .await
            .expect("page");
        assert_sized_source_page(&result, length);
    }
    runtime.close().await;
}

/// A wide search under an explicit `responseLength` pages whole rows: the
/// page carries its rows and a `responseScope:"rows"` continuation, never a
/// text `responseWindow`.
#[tokio::test]
async fn an_explicit_length_pages_a_wide_search_by_rows() {
    let workspace = Workspace::new();
    for index in 0..40 {
        workspace.write(&format!("src/f{index:02}.txt"), lines(&"w".repeat(60)));
    }
    let root = workspace
        .workspace
        .join("src")
        .to_string_lossy()
        .into_owned();
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .run(
            "wide".into(),
            "localSearch".into(),
            json!({"queries":[{"path":root,"matchString":"needle","pageSize":40}],
                "responseLength": 30_000}),
        )
        .await
        .expect("page");
    let page = &outcome.structured_content;
    assert!(page.get("responseWindow").is_none(), "{page}");
    assert!(
        page["results"][0]["data"].is_object(),
        "the page carries its rows: {page}"
    );
    let pagination = &page["responsePagination"];
    assert_eq!(pagination["scope"], "rows", "{pagination}");
    assert_eq!(pagination["next"]["query"]["responseScope"], "rows");
    assert_eq!(pagination["next"]["query"]["responseLength"], 30_000);
    runtime.close().await;
}

/// A saved read past the last page of an unchanged response is a restart
/// page that ends the walk: `hasMore` is false, so a caller following `next`
/// while `hasMore` stops, and its `next` still starts over at offset 0. A
/// changed page size is refused as a caller error (exit class invalidInput).
#[tokio::test]
async fn a_saved_read_past_the_last_page_ends_the_walk() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let (_, next) = saved_fixture(&workspace, &runtime).await;
    let mut past = next.clone();
    past["responseOffset"] = json!(9_999);
    let page = runtime
        .run("past".into(), "localSearch".into(), past)
        .await
        .expect("restart page");
    let pagination = &page.structured_content["responsePagination"];
    assert_eq!(pagination["restart"], true, "{pagination}");
    assert_eq!(pagination["hasMore"], false, "{pagination}");
    assert_eq!(
        pagination["next"]["query"]["responseOffset"], 0,
        "{pagination}"
    );
    octocode_native::contracts::validate_output("localSearch", &page.structured_content)
        .expect("restart page output contract");
    let mut resized = next;
    resized["responseLength"] = json!(900);
    let error = runtime
        .run("resized".into(), "localSearch".into(), resized)
        .await
        .expect_err("a changed page size is refused");
    assert_eq!(error.code, "responsePageMismatch");
    assert_eq!(
        error.exit_class(),
        octocode_native::response::exit::ExitClass::InvalidInput
    );
    runtime.close().await;
}
