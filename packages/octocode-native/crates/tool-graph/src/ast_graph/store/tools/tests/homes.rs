//! Snapshots live in the workspace's home, and the workspace follows the
//! process directory: a query from another workspace names the home it
//! searched and every other home holding a snapshot of that path.
use super::*;
use octocode_test_support::json_fixture;

/// A runtime started in `workspace` that may also read `allowed`.
fn policy(workspace: &Path, allowed: &Path) -> octocode_runtime_base::policy::path::PathPolicy {
    octocode_runtime_base::policy::path::PathPolicy::new(
        octocode_runtime_base::policy::path::PathPolicyConfig {
            workspace_root: Some(workspace.to_path_buf()),
            additional_roots: vec![allowed.to_path_buf()],
            ..Default::default()
        },
    )
    .expect("policy")
}

#[test]
fn a_query_from_another_workspace_names_both_homes() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    assert!(built.get("error").is_none(), "{built}");
    let app = dir.0.join("app").canonicalize().expect("app");
    let searched = app.join(".octocode").join("graph");
    let built_home = dir
        .0
        .canonicalize()
        .expect("root")
        .join(".octocode")
        .join("graph");
    for row in [
        json_fixture(r#"{"operation": "stats", "path": "."}"#),
        json!({"operation":"stats","path":".","graph":built["graph"]}),
    ] {
        let row = octocode_runtime_base::contracts::validate_built_query(
            ToolId::GraphQuery.as_str(),
            row,
        )
        .expect("input contract");
        let missing = execute_query(&row, &policy(&app, &dir.0), &context(), None, None)
            .expect("execute")
            .data;
        assert_eq!(missing["errorCode"], "notFound", "{missing}");
        let text = missing.to_string();
        // The searched home lies in this workspace, so it is named as a row
        // path is; the other workspace's home has no such name.
        let error = missing["error"].as_str().expect("error");
        assert!(error.ends_with("(searched .octocode/graph)"), "{missing}");
        assert!(!text.contains(&*searched.to_string_lossy()), "{missing}");
        assert!(text.contains(&*built_home.to_string_lossy()), "{missing}");
        // The searched home may be a configured one (`local.graphHome`): the
        // hint names the others as the default home of the workspace that
        // built them, never as "the snapshot home".
        assert!(text.contains("default home"), "{missing}");
        assert!(!text.contains("the snapshot home is"), "{missing}");
    }
}

/// A home configured outside the workspace has no workspace-relative name:
/// the error names it in full.
#[test]
fn a_configured_home_outside_the_workspace_is_named_in_full() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    let app = dir.0.join("app").canonicalize().expect("app");
    let home = dir.0.canonicalize().expect("root").join("graphs");
    let row = octocode_runtime_base::contracts::validate_built_query(
        ToolId::GraphQuery.as_str(),
        json_fixture(r#"{"operation": "stats", "path": "."}"#),
    )
    .expect("input contract");
    let missing = execute_query(&row, &policy(&app, &dir.0), &context(), None, Some(&home))
        .expect("execute")
        .data;
    assert_eq!(missing["errorCode"], "notFound", "{missing}");
    let error = missing["error"].as_str().expect("error");
    assert!(
        error.contains(&format!("(searched {}", home.display())),
        "{missing}"
    );
}

/// A home holding a snapshot of the path but outside every allowed root is
/// never read, so the error does not name it.
#[test]
fn a_home_outside_the_allowed_roots_is_never_named() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    assert!(built.get("error").is_none(), "{built}");
    let app = dir.0.join("app").canonicalize().expect("app");
    let outside = dir.0.canonicalize().expect("root").join(".octocode");
    assert!(outside.join("graph").is_dir(), "the build wrote its home");
    let row = octocode_runtime_base::contracts::validate_built_query(
        ToolId::GraphQuery.as_str(),
        json_fixture(r#"{"operation": "stats", "path": "."}"#),
    )
    .expect("input contract");
    let missing = execute_query(&row, &policy(&app, &app), &context(), None, None)
        .expect("execute")
        .data;
    assert_eq!(missing["errorCode"], "notFound", "{missing}");
    let text = missing.to_string();
    assert!(!text.contains(&*outside.to_string_lossy()), "{missing}");
    assert!(!text.contains("Snapshots of this path exist"), "{missing}");
}

#[test]
fn a_path_outside_the_workspace_names_a_route() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    let paths = policy(&dir.0.join("app"), &dir.0);
    let row = octocode_runtime_base::contracts::validate_built_query(
        ToolId::GraphBuild.as_str(),
        json!({"path": dir.0.to_string_lossy()}),
    )
    .expect("input contract");
    let query: GraphBuildQuery = serde_json::from_value(row).expect("typed query");
    let refused = execute_build(
        &query,
        &paths,
        &ContentSecurity::new(),
        &context(),
        None,
        &Store::default(),
    )
    .expect("execute");
    let text = refused.data.to_string();
    assert!(
        text.contains("astSearch") && text.contains("localSearch"),
        "{text}"
    );
}

/// A directory outside both the workspace and the allowed roots needs read
/// access and a graph home: the first refusal names both, so following it
/// once makes the call succeed.
#[test]
fn a_path_outside_the_allowed_roots_names_every_setting_a_build_needs() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    let elsewhere = Fixture::new();
    elsewhere.write("lib/b.ts", "export const b = 1;\n");
    let paths = policy(&dir.0.join("app"), &dir.0.join("app"));
    let row = octocode_runtime_base::contracts::validate_built_query(
        ToolId::GraphBuild.as_str(),
        json!({"path": elsewhere.0.join("lib").to_string_lossy()}),
    )
    .expect("input contract");
    let query: GraphBuildQuery = serde_json::from_value(row).expect("typed query");
    let refused = execute_build(
        &query,
        &paths,
        &ContentSecurity::new(),
        &context(),
        None,
        &Store::default(),
    )
    .expect("execute");
    let text = refused.data.to_string();
    assert!(
        text.contains("OCTOCODE_GRAPH_HOME") && text.contains("ALLOWED_PATHS"),
        "{text}"
    );
}

/// A missing path leads, as on every local tool, to the tree of its nearest
/// existing parent, on both graph tools.
#[test]
fn a_missing_path_leads_to_its_nearest_existing_parent() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    for (tool, row) in [
        (ToolId::GraphBuild, json_fixture(r#"{"path": "app/nope"}"#)),
        (
            ToolId::GraphQuery,
            json_fixture(r#"{"operation": "stats", "path": "app/nope"}"#),
        ),
    ] {
        let mut missing = call(&dir.0, tool, row);
        octocode_runtime_base::response::paths::shown_paths(
            true,
            &mut missing,
            &workspace_policy(&dir.0),
            &Value::Null,
        );
        assert_eq!(missing["errorCode"], "notFound", "{missing}");
        let lead = &missing["related"][0];
        assert_eq!(lead["tool"], "structureSearch", "{missing}");
        let tree = &lead["input"]["queries"][0];
        assert_eq!(tree["operation"], "tree", "{missing}");
        assert_eq!(tree["path"], "app", "{missing}");
        assert!(dir.0.join("app").is_dir(), "{missing}");
    }
}

/// Runs one validated `tool` row as the runtime does, with `home` as the
/// configured snapshot home, naming paths as responses do.
fn run_at(dir: &Path, home: Option<&Path>, tool: ToolId, row: Value) -> Value {
    let paths = policy(dir, dir);
    let row = octocode_runtime_base::contracts::validate_built_query(tool.as_str(), row)
        .expect("input contract");
    let result = if tool == ToolId::GraphBuild {
        let query: GraphBuildQuery = serde_json::from_value(row).expect("typed query");
        let store = Store {
            home,
            ..Store::default()
        };
        execute_build(
            &query,
            &paths,
            &ContentSecurity::new(),
            &context(),
            None,
            &store,
        )
    } else {
        execute_query(&row, &paths, &context(), None, home)
    };
    let mut data = result.expect("execute").data;
    octocode_runtime_base::response::paths::shown_paths(true, &mut data, &paths, &Value::Null);
    data
}

/// The repair a file-path refusal offers, after checking the refusal names
/// `dir`, the directory it chose.
fn repair_of(refused: &Value, dir: &str) -> Value {
    assert_eq!(refused["errorCode"], "invalidInput", "{refused}");
    let error = refused["error"].as_str().expect("error");
    assert!(
        error.contains("not a file") && error.ends_with(&format!(" {dir}")),
        "{refused}"
    );
    refused["related"][0].clone()
}

/// Graphs scan directories: a file path is refused with a directory named
/// and, as `related`, a call there that runs as written and answers: a
/// build of the file's directory; a query of the pinned graph's own scope,
/// else of the nearest enclosing scope with a snapshot, else a build of the
/// file's directory. Each holds under the workspace's home and a configured
/// one, and each repair runs alone.
#[test]
fn a_file_path_offers_a_call_that_answers() {
    for configured in [false, true] {
        let dir = Fixture::new();
        dir.write("ws/app/a.ts", "export const a = 1;\n");
        let root = dir.0.canonicalize().expect("root");
        let home = configured.then(|| root.join("graphs"));
        let run = |tool, row| run_at(&root, home.as_deref(), tool, row);
        let answers = |lead: &Value| {
            let answer = run(
                ToolId::from_name(lead["tool"].as_str().expect("tool")).expect("tool id"),
                lead["input"]["queries"][0].clone(),
            );
            assert!(answer.get("error").is_none(), "{lead} -> {answer}");
        };
        let file = || json_fixture(r#"{"operation": "stats", "path": "ws/app/a.ts"}"#);

        // No snapshot: both tools lead to a build of the file's directory.
        let lead = repair_of(
            &run(
                ToolId::GraphBuild,
                json_fixture(r#"{"path": "ws/app/a.ts"}"#),
            ),
            "ws/app",
        );
        assert_eq!(lead["tool"], "graphBuild", "{lead}");
        assert_eq!(lead["input"]["queries"][0]["path"], "ws/app", "{lead}");
        let lead = repair_of(&run(ToolId::GraphQuery, file()), "ws/app");
        assert_eq!(lead["tool"], "graphBuild", "{lead}");
        answers(&lead);

        // A snapshot of the enclosing `ws/app`: the query moves there, unpinned.
        let lead = repair_of(&run(ToolId::GraphQuery, file()), "ws/app");
        assert_eq!(lead["tool"], "graphQuery", "{lead}");
        answers(&lead);

        // A pinned graph of `ws` holds the file: its scope wins over the
        // nearer `ws/app`, and the pin stays.
        let whole = run(ToolId::GraphBuild, json_fixture(r#"{"path": "ws"}"#));
        let pinned = json!({"operation":"stats","path":"ws/app/a.ts","graph":whole["graph"]});
        let lead = repair_of(&run(ToolId::GraphQuery, pinned), "ws");
        assert_eq!(lead["input"]["queries"][0]["graph"], whole["graph"], "{lead}");
        answers(&lead);
    }
}

/// A configured home inside the file's directory rules that directory out:
/// the refusal names both, so the agent picks another directory.
#[test]
fn a_file_beside_the_configured_home_names_it() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    let root = dir.0.canonicalize().expect("root");
    let home = root.join("app").join("graphs");
    let refused = run_at(
        &root,
        Some(&home),
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app/a.ts"}"#),
    );
    assert_eq!(refused["errorCode"], "invalidInput", "{refused}");
    let error = refused["error"].as_str().expect("error");
    assert!(
        error.contains("app holds local.graphHome") && error.contains("graphs"),
        "{refused}"
    );
    assert!(refused.get("related").is_none(), "{refused}");
}

/// A graph id used with another path is refused by naming the scope the
/// snapshot was built for, so the caller can repeat the call there.
#[test]
fn a_snapshot_asked_at_another_path_names_its_own_scope() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    dir.write("other/b.ts", "export const b = 1;\n");
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    let id = built["graph"].as_str().expect("graph id");
    call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "other"}"#),
    );
    let refused = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"stats","path":"other","graph":id}),
    );
    let error = refused["error"].as_str().expect("refused");
    assert!(error.contains("\"app\""), "{refused}");
    assert!(!error.contains("original scope"), "{refused}");
}

/// A graph query that names a file inside a snapshot's scope is refused with
/// the same query at the scope, carrying the file as its scope-relative
/// `target`, so the lead answers the question that was asked.
#[test]
fn a_query_on_a_file_leads_to_the_scope_with_the_file_as_target() {
    let dir = Fixture::new();
    dir.write("app/src/a.ts", "export const a = 1;\n");
    call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    let refused = call(
        &dir.0,
        ToolId::GraphQuery,
        json_fixture(r#"{"operation": "symbols", "path": "app/src/a.ts"}"#),
    );
    let lead = &refused["related"][0]["input"]["queries"][0];
    assert!(
        lead["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("app")),
        "{refused}"
    );
    assert_eq!(lead["target"], "src/a.ts", "{refused}");
    assert_eq!(lead["operation"], "symbols", "{refused}");
}
