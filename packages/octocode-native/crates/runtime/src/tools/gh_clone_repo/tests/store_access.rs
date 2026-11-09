//! The shared store is no read root: the local tools read the worktrees under
//! `clones/`, and a path in `clone-store/` is denied by the path policy.

use super::*;
use crate::config::{ConfigInput, FileInput, RuntimeSurface};
use crate::execution::RuntimeLimits;
use crate::runtime::{Channel, ToolRuntime};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// A runtime whose home is `home`, run from `cwd`.
fn runtime(home: &Path, cwd: &Path) -> ToolRuntime {
    let env = BTreeMap::from([(
        crate::config::environment::OCTOCODE_HOME.to_owned(),
        home.to_string_lossy().into_owned(),
    )]);
    ToolRuntime::with_limits(
        ConfigInput {
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
        },
        RuntimeLimits::DEFAULT,
    )
    .expect("runtime")
}

async fn call(runtime: &ToolRuntime, tool: &str, row: Value) -> Value {
    let admission = runtime.admit("store".into()).expect("admitted");
    let outcome = runtime
        .execute(
            admission,
            tool.into(),
            json!({"queries":[row]}),
            Channel::Mcp,
        )
        .await
        .expect("call");
    outcome.structured_content["results"][0]["data"].clone()
}

/// A file in a checkout under `clones/` reads and lists; the same read and
/// listing of a file in `clone-store/` (the objects the checkout was made
/// from) are denied by policy, through localFetch and structureSearch.
#[tokio::test]
async fn the_shared_store_is_denied_to_the_local_tools() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let home = home.path().canonicalize().expect("canonical home");
    let checkout = storage::clones_root(&home).join("o/r/abc__host_x");
    let store = storage::store_root(&home).join("o/r__host_x__g1.git");
    for (dir, name) in [(&checkout, "README.md"), (&store, "config")] {
        fs::create_dir_all(dir).expect("dir");
        fs::write(dir.join(name), "readable?\n").expect("file");
    }
    let runtime = runtime(&home, cwd.path());

    let read = call(
        &runtime,
        "localFetch",
        json!({"path": checkout.join("README.md")}),
    )
    .await;
    assert!(
        read["content"]
            .as_str()
            .is_some_and(|text| text.contains("readable?")),
        "{read}"
    );
    let listing = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path": checkout}),
    )
    .await;
    assert!(listing.to_string().contains("README.md"), "{listing}");

    for (tool, row) in [
        ("localFetch", json!({"path": store.join("config")})),
        (
            "structureSearch",
            json!({"operation":"files","path": store}),
        ),
        (
            "structureSearch",
            json!({"operation":"tree","path": storage::store_root(&home)}),
        ),
    ] {
        let denied = call(&runtime, tool, row.clone()).await;
        assert_eq!(
            denied["errorCode"], "outsideAllowedRoots",
            "{tool} {row}: {denied}"
        );
        assert!(
            !denied.to_string().contains("readable?"),
            "{tool}: {denied}"
        );
    }
}

/// A home inside the workspace (the workspace root allows it whole) keeps its
/// store closed: the store is neither a clone nor a workspace file, whatever
/// the root that contains it.
#[tokio::test]
async fn a_store_in_a_home_inside_the_workspace_is_still_denied() {
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let home = workspace.join("home");
    let store = storage::store_root(&home).join("o/r__host_x__g1.git");
    fs::create_dir_all(store.join("worktrees")).expect("store");
    fs::write(store.join("config"), "readable?\n").expect("file");
    let runtime = runtime(&home, &workspace);

    for (tool, row) in [
        ("localFetch", json!({"path": store.join("config")})),
        (
            "structureSearch",
            json!({"operation":"files","path": store}),
        ),
        (
            "localSearch",
            json!({"path": storage::store_root(&home), "matchString": ["readable"]}),
        ),
    ] {
        let denied = call(&runtime, tool, row.clone()).await;
        assert!(denied.get("errorCode").is_some(), "{tool} {row}: {denied}");
        assert!(
            !denied.to_string().contains("readable?"),
            "{tool}: {denied}"
        );
    }
}

/// A checkout is a linked worktree: its `.git` is a pointer file. A text
/// search of it is no different from one of a standalone clone: it counts
/// no withheld entry for that file.
#[tokio::test]
async fn a_text_search_of_a_worktree_clone_withholds_nothing() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let home = home.path().canonicalize().expect("canonical home");
    let store = storage::store_root(&home).join("o/r__host_x__g1.git");
    fs::create_dir_all(&store).expect("store");
    git(&store, &["init", "--initial-branch=main", "."]);
    fs::write(store.join("README.md"), "needle\n").expect("file");
    git(&store, &["add", "."]);
    git(&store, &["commit", "-m", "one"]);
    let checkout = storage::clones_root(&home).join("o/r/abc__host_x");
    git(
        &store,
        &[
            "worktree",
            "add",
            "--detach",
            checkout.to_str().expect("utf8"),
        ],
    );
    assert!(
        checkout.join(".git").is_file(),
        "a worktree's .git is a file"
    );
    let runtime = runtime(&home, cwd.path());

    let row = call(
        &runtime,
        "localSearch",
        json!({"path": checkout, "matchString": ["needle"]}),
    )
    .await;
    assert_eq!(row["matchCount"], 1, "{row}");
    assert!(!row.to_string().contains("withheld"), "{row}");
    assert!(!row.to_string().contains(".git"), "{row}");
}
