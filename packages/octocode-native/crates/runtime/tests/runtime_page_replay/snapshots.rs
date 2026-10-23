//! Immutable snapshots: stored envelopes served across source edits, restarts and key order, without re-executing.

use super::lines;
#[cfg(unix)]
use crate::support;
use crate::support::{Run as _, Workspace};
use octocode_contract_types::ErrorCode;
#[cfg(unix)]
use serde_json::Value;
use serde_json::json;

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
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
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
                "responseLength": 800, "responseScope": "structured"
            }),
        )
        .await
        .expect("first page");
    let pagination = first.structured_content["responsePagination"].clone();
    assert_eq!(pagination["hasMore"], true, "{pagination}");
    let response = support::cursor_response(&pagination);
    let next = pagination["next"]["query"].clone();
    support::assert_cursor_only(&next);

    // A re-execution could no longer read the file and would demand a
    // restart; the stored envelope keeps serving the same response.
    unreadable(&file, true);
    let second = runtime
        .run("page-2".into(), "localSearch".into(), next.clone())
        .await
        .expect("second page");
    unreadable(&file, false);
    let page = &second.structured_content["responsePagination"];
    assert_eq!(support::cursor_response(page), response, "{page}");
    assert_ne!(page["restart"], true, "{page}");
    assert!(page["offset"].as_u64().unwrap_or(0) > 0, "{page}");
    assert!(text(&second).contains("original"), "page 2 re-executed");

    let mut stale: Value = next.clone();
    stale["responseSnapshot"] = json!(format!("{}0", next["responseSnapshot"].as_str().unwrap()));
    let third = runtime
        .run("page-stale".into(), "localSearch".into(), stale)
        .await
        .expect_err("unknown snapshots do not execute");
    assert_eq!(third.code, ErrorCode::ResponseSnapshotExpired);
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
    let response = support::cursor_response(&pagination);
    let next = pagination["next"]["query"].clone();
    support::assert_cursor_only(&next);

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
    assert_eq!(restart.code, ErrorCode::ResponsePageMismatch);

    unreadable(&file, true);
    let second = runtime
        .run("rows-2".into(), "localSearch".into(), next)
        .await
        .expect("second rows page");
    unreadable(&file, false);
    let page = &second.structured_content["responsePagination"];
    assert_eq!(support::cursor_response(page), response, "{page}");
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
                "responseLength": 800, "responseScope": "structured"
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
    assert_eq!(
        support::cursor_response(page),
        support::cursor_response(&pagination),
        "{page}"
    );
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

/// The JSON text both channels print for `outcome`.
fn text(outcome: &octocode_native::response::outcome::ToolOutcome) -> String {
    outcome.structured_content.to_string()
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
                "responseLength": 800, "responseScope": "structured"
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
