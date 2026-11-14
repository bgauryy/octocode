//! The root a `workspaceSymbol` of a directory runs from.

use super::*;

/// `relative/Cargo.toml` under `root` holding `manifest`.
fn manifest_at(root: &std::path::Path, relative: &str, manifest: &str) -> std::path::PathBuf {
    let directory = root.join(relative);
    std::fs::create_dir_all(directory.join("src")).expect("package");
    std::fs::write(directory.join("Cargo.toml"), manifest).expect("manifest");
    directory
}

/// The `workspaceRoot` a `workspaceSymbol` of `directory` runs from.
fn symbol_root(
    directory: &std::path::Path,
    extra: serde_json::Value,
    paths: &crate::policy::path::PathPolicy,
) -> String {
    let mut row = serde_json::json!({
        "operation": "workspaceSymbol", "symbolFuzzy": "f",
        "path": directory.to_string_lossy()
    });
    row.as_object_mut()
        .expect("row")
        .extend(extra.as_object().cloned().unwrap_or_default());
    let mut q = query(row);
    let resolved = resolve_paths(
        &mut q,
        paths,
        &octocode_engine::lsp::config::LspDiscoveryOptions::default(),
    )
    .expect("a directory path");
    assert_eq!(resolved, directory.to_string_lossy(), "the query keeps it");
    lsp_search_query::workspace_root(&q)
        .expect("a root")
        .to_owned()
}

const MEMBER: &str = "[package]\nname = 'member'\n";

/// One rust-analyzer analyzes a whole Cargo workspace: every directory of a
/// member crate runs from the root a Rust file in it runs from, so it reuses
/// that server. A caller's own root and a crate outside the workspace stay.
#[test]
fn workspace_symbol_in_a_member_crate_runs_from_its_cargo_workspace() {
    let (_dir, root, paths) = temp_workspace();
    manifest_at(
        &root,
        ".",
        "[workspace]\nmembers = ['crates/*']\nexclude = ['crates/excluded']\n",
    );
    let member = manifest_at(&root, "crates/member", MEMBER);
    let excluded = manifest_at(&root, "crates/excluded", MEMBER);
    let none = serde_json::json!({});
    let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
    assert_eq!(symbol_root(&root, none.clone(), &paths), text(&root));
    assert_eq!(symbol_root(&member, none.clone(), &paths), text(&root));
    assert_eq!(
        symbol_root(&member.join("src"), none.clone(), &paths),
        text(&root)
    );
    assert_eq!(
        octocode_engine::lsp::workspace::resolve_workspace_root_for_file(text(
            &member.join("src/lib.rs")
        ))
        .expect("root of a file"),
        text(&root),
        "a file anchor names the same root"
    );
    assert_eq!(symbol_root(&excluded, none, &paths), text(&excluded));
    assert_eq!(
        symbol_root(
            &member,
            serde_json::json!({"workspaceRoot": text(&member)}),
            &paths
        ),
        text(&member),
        "an explicit workspaceRoot is the caller's"
    );
    let inside = crate::tools::test_support::workspace_policy(&member);
    assert_eq!(
        symbol_root(&member, serde_json::json!({}), &inside),
        text(&member),
        "a workspace above the allowed roots leaves the member's own root"
    );
}

/// Only a Rust root widens: a directory whose project selects another language
/// keeps its nearest project root, inside a Cargo workspace or not, and a
/// workspace that would select another server does not take a member crate.
#[test]
fn workspace_symbol_outside_rust_keeps_its_nearest_project_root() {
    let (_dir, root, paths) = temp_workspace();
    manifest_at(&root, ".", "[workspace]\nmembers = ['crates/*']\n");
    let none = serde_json::json!({});
    let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
    let web = root.join("web");
    std::fs::create_dir_all(web.join("src")).expect("web");
    std::fs::write(web.join("package.json"), "{}").expect("package.json");
    assert_eq!(symbol_root(&web, none.clone(), &paths), text(&web));
    assert_eq!(
        symbol_root(&web.join("src"), none.clone(), &paths),
        text(&web)
    );
    let typed = manifest_at(&root, "crates/typed", MEMBER);
    std::fs::write(typed.join("tsconfig.json"), "{}").expect("tsconfig");
    assert_eq!(symbol_root(&typed, none.clone(), &paths), text(&typed));

    let (_other, mixed, paths) = temp_workspace();
    manifest_at(&mixed, ".", "[workspace]\nmembers = ['crates/*']\n");
    std::fs::write(mixed.join("tsconfig.json"), "{}").expect("tsconfig");
    std::fs::write(mixed.join("package.json"), r#"{"workspaces":["web"]}"#).expect("package");
    let member = manifest_at(&mixed, "crates/member", MEMBER);
    assert_eq!(symbol_root(&member, none, &paths), text(&member));
}

/// A directory its files route (a folder of Python scripts with no manifest
/// of its own, under a TypeScript package of a Python project) runs from the
/// root a call on one of its files runs from, so both reach one server.
#[test]
fn a_directory_routed_by_its_files_runs_from_its_files_root() {
    let (_dir, root, paths) = temp_workspace();
    let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
    std::fs::write(root.join("pyproject.toml"), "[project]\nname = 'p'\n").expect("pyproject");
    let tools = root.join("tools");
    let scripts = tools.join("scripts");
    std::fs::create_dir_all(&scripts).expect("scripts");
    std::fs::write(tools.join("package.json"), "{}").expect("package.json");
    for name in ["flows.py", "walk.py", "stub.mjs"] {
        std::fs::write(scripts.join(name), "").expect("script");
    }
    let file_root = octocode_engine::lsp::workspace::resolve_workspace_root_for_file(text(
        &scripts.join("flows.py"),
    ))
    .expect("root of a file");
    assert_eq!(file_root, text(&root), "the Python project");
    assert_eq!(
        symbol_root(&scripts, serde_json::json!({}), &paths),
        file_root
    );
    // A directory with markers of its own keeps its nearest project root.
    assert_eq!(
        symbol_root(&tools, serde_json::json!({}), &paths),
        text(&tools)
    );
}
