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
        let missing = execute_query(&row, &policy(&app, &dir.0), &context(), None)
            .expect("execute")
            .data;
        assert_eq!(missing["errorCode"], "notFound", "{missing}");
        let text = missing.to_string();
        assert!(text.contains(&*searched.to_string_lossy()), "{missing}");
        assert!(text.contains(&*built_home.to_string_lossy()), "{missing}");
    }
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
    let missing = execute_query(&row, &policy(&app, &app), &context(), None)
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
        false,
    )
    .expect("execute");
    let text = refused.data.to_string();
    assert!(
        text.contains("astSearch") && text.contains("localSearch"),
        "{text}"
    );
}
