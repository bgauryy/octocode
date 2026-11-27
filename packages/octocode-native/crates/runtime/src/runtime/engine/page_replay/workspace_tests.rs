//! Saved work and the directory a runtime starts in: a GitHub tool answers
//! from the API endpoint and home alone, so its cursors and saved pages
//! resume from any directory under the same configuration and access
//! identity; every other tool's stay bound to the workspace that issued them.
use super::*;
use crate::runtime::engine::ToolRuntime;
use crate::runtime::engine::cursors::SavedWork;
use octocode_io::cancellation::Cancellation;
use octocode_runtime_base::config::{FileInput, RuntimeSurface};
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::security::Sanitized;
use octocode_test_support::json_fixture;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// A persistent runtime of the Octocode home `home`, started in `cwd`.
fn runtime_in(home: &Path, cwd: &Path) -> ToolRuntime {
    std::fs::create_dir_all(cwd).expect("cwd");
    let env = BTreeMap::from([
        (
            "OCTOCODE_HOME".to_owned(),
            home.to_string_lossy().into_owned(),
        ),
        ("OCTOCODE_STORAGE_MODE".to_owned(), "persistent".to_owned()),
    ]);
    ToolRuntime::new(octocode_runtime_base::config::ConfigInput {
        env,
        cwd: cwd.to_path_buf(),
        os_home: home.to_path_buf(),
        global_env: FileInput::Missing {
            path: home.join(".env"),
        },
        project_env: FileInput::Missing {
            path: cwd.join(".octocode/.env"),
        },
        config_file: FileInput::Missing {
            path: home.join(".octocoderc"),
        },
        project_config_file: FileInput::Missing {
            path: cwd.join(".octocode/.octocoderc"),
        },
        runtime_surface: RuntimeSurface::Mcp,
    })
    .expect("runtime")
}

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: Cancellation::new(),
        deadline: Some(Instant::now() + Duration::from_secs(30)),
        response_window: None,
        github_credential: None,
    }
}

/// The handle `runtime` retains for `tool` resuming `input`.
fn issue(runtime: &ToolRuntime, tool: ToolId, input: &str) -> String {
    let mut pending = runtime
        .cursors
        .pending(page_replay_key("", &[], &[], None), None);
    let token = pending.insert(
        tool,
        json_fixture(input),
        SavedWork::ToolResume { index: Some(0) },
    );
    runtime
        .cursors
        .retain(pending, &context())
        .expect("retained");
    runtime.cursors.clear_memory();
    token
}

const GITHUB: &str = r#"{"queries": [{"operation": "pullRequest", "owner": "o", "repo": "r"}]}"#;
const LOCAL: &str = r#"{"queries": [{"path": "must-not-be-read"}]}"#;

/// A GitHub cursor resumes from another directory; a local tool's is
/// refused there and still resumes in its own; a handle sent to a tool of
/// the other kind is named as another tool's.
#[test]
fn only_a_github_cursor_resumes_from_another_directory() {
    let home = tempfile::tempdir().expect("home");
    let first = runtime_in(home.path(), &home.path().join("a"));
    let github = issue(&first, ToolId::GhSearchHistory, GITHUB);
    let local = issue(&first, ToolId::LocalFetch, LOCAL);
    let access = page_replay_key("", &[], &[], None);

    let elsewhere = runtime_in(home.path(), &home.path().join("b"));
    let resumed = elsewhere
        .cursors
        .open(ToolId::GhSearchHistory, &github, access)
        .unwrap_or_else(|refusal| panic!("GitHub cursor refused: {refusal:?}"));
    assert_eq!(resumed.member().input, json_fixture(GITHUB));
    assert_eq!(
        elsewhere
            .cursors
            .open(ToolId::LocalFetch, &local, access)
            .err(),
        Some(Refusal::NotRetained),
        "a local cursor is bound to its workspace"
    );
    let again = runtime_in(home.path(), &home.path().join("a"));
    assert!(
        again
            .cursors
            .open(ToolId::LocalFetch, &local, access)
            .is_ok()
    );

    for (runtime, tool, token) in [
        (&elsewhere, ToolId::LocalFetch, &github),
        (&again, ToolId::GhSearchHistory, &local),
    ] {
        assert_eq!(
            runtime.cursors.open(tool, token, access).err(),
            Some(Refusal::OtherTool),
            "{tool}"
        );
    }
    // Another access identity is refused wherever it is sent.
    assert_eq!(
        elsewhere
            .cursors
            .open(ToolId::GhSearchHistory, &github, [7; 32])
            .err(),
        Some(Refusal::OtherAccess)
    );
}

/// A saved response page of a GitHub tool is found from another directory;
/// one of a local tool is not.
#[test]
fn only_a_github_saved_page_is_found_from_another_directory() {
    let home = tempfile::tempdir().expect("home");
    let first = runtime_in(home.path(), &home.path().join("a"));
    let elsewhere = runtime_in(home.path(), &home.path().join("b"));
    let deadline = Instant::now() + Duration::from_secs(30);
    for (tool, found) in [
        (ToolId::GhSearchHistory, true),
        (ToolId::LocalSearch, false),
    ] {
        let cursor = Cursor {
            key: page_replay_key(tool.as_str(), &[], &[], None),
            digest: [3; 32],
            length: 600,
            offset: 1,
        };
        let replay = PageReplay {
            access: page_replay_key("", &[], &[], None),
            github_identity: None,
            structured: Sanitized::scrubbed(json!({"results": []})),
            response_query: json!({"queries": []}),
            failure: None,
            pages: SavedPages::default(),
        };
        let (_, retained) = first
            .page_replays
            .insert(tool.as_str(), &cursor, replay, deadline);
        assert!(retained, "{tool}");
        first.page_replays.store.clear_memory();
        assert!(first.page_replays.get(&cursor).is_some(), "{tool}");
        assert_eq!(
            elsewhere.page_replays.get(&cursor).is_some(),
            found,
            "{tool}"
        );
    }
}
