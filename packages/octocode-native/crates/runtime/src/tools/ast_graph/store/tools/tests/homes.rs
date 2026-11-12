//! Snapshots live in the workspace's home, and the workspace follows the
//! process directory: a query from another workspace names the home it
//! searched and every other home holding a snapshot of that path.
use super::*;

/// A runtime started in `workspace` that may also read `allowed`.
fn policy(workspace: &Path, allowed: &Path) -> crate::policy::path::PathPolicy {
    crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(workspace.to_path_buf()),
        additional_roots: vec![allowed.to_path_buf()],
        ..Default::default()
    })
    .expect("policy")
}

#[test]
fn a_query_from_another_workspace_names_both_homes() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
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
        json!({"operation":"stats","path":"."}),
        json!({"operation":"stats","path":".","graph":built["id"]}),
    ] {
        let row = crate::contracts::validate_built_query(ToolId::GraphQuery.as_str(), row)
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
    let row = crate::contracts::validate_built_query(
        ToolId::GraphQuery.as_str(),
        json!({"operation":"stats","path":"."}),
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
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert!(built.get("error").is_none(), "{built}");
    let app = dir.0.join("app").canonicalize().expect("app");
    let outside = dir.0.canonicalize().expect("root").join(".octocode");
    assert!(outside.join("graph").is_dir(), "the build wrote its home");
    let row = crate::contracts::validate_built_query(
        ToolId::GraphQuery.as_str(),
        json!({"operation":"stats","path":"."}),
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
    let row = crate::contracts::validate_built_query(
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
    let row = crate::contracts::validate_built_query(
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
