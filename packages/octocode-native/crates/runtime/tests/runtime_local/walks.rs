//! Local walks: structureSearch dispatch and file discovery, the shared default prune, and `defaultExcludes: false`.

use crate::support::{Workspace, call, row_data, row_status};
use octocode_contract_types::ErrorCode;
use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};
use serde_json::json;
use std::collections::BTreeMap;

#[tokio::test]
async fn structure_search_dispatches_on_both_surfaces_and_owns_file_discovery() {
    let workspace = Workspace::new();
    workspace.write("src/main.ts", "export const a = 1;\n");
    workspace.write("docs/readme.md", "# docs\n");
    let mcp = ToolRuntime::from_host(HostOptions {
        cwd: Some(workspace.workspace.clone()),
        // The fixture home, so the user's ~/.octocode is never read or written.
        env: Some(BTreeMap::from([
            ("OCTOCODE_ENABLE_LOCAL".into(), "true".into()),
            (
                "OCTOCODE_HOME".into(),
                workspace.home.to_string_lossy().into_owned(),
            ),
        ])),
        surface: RuntimeSurface::Mcp,
        ..HostOptions::default()
    })
    .expect("mcp runtime");
    assert!(mcp.is_available("structureSearch"));
    mcp.close().await;

    let runtime = workspace.runtime(&[]);
    assert!(runtime.is_available("structureSearch"));
    let tree = call(
        &runtime,
        "structureSearch",
        json!({"operation":"tree","path":workspace.workspace,"maxDepth":1}),
    )
    .await
    .expect("tree");
    assert_eq!(
        row_status(&tree),
        "success",
        "{:?}",
        tree.structured_content
    );
    let rendered = serde_json::to_string(row_data(&tree)).expect("json");
    // A directory is a `{dir}` group, or a bare `name/` entry without one.
    let listed = |dir: &str| {
        rendered.contains(&format!("{dir}/")) || rendered.contains(&format!("\"dir\":\"{dir}\""))
    };
    assert!(listed("src") && listed("docs"), "{rendered}");

    let files = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":workspace.workspace,"include":["*.ts"],"entryType":"f"}),
    )
    .await
    .expect("files");
    assert_eq!(
        row_status(&files),
        "success",
        "{:?}",
        files.structured_content
    );
    let rendered = serde_json::to_string(row_data(&files)).expect("json");
    assert!(
        rendered.contains("main.ts") && !rendered.contains("readme.md"),
        "{rendered}"
    );

    let retired = call(
        &runtime,
        "astSearch",
        json!({"operation":"files","path":workspace.workspace}),
    )
    .await
    .expect_err("astSearch no longer discovers files");
    assert_eq!(retired.code, ErrorCode::InvalidInput);
    runtime.close().await;
}

/// Every local walk prunes one default directory set, and a caller
/// `exclude` glob skips more rather than replacing it. Tool-config directories
/// such as `.github` are not on it: with `includeHidden`, text, structure and
/// AST walks all reach them.
#[tokio::test]
async fn local_walks_share_one_default_prune_and_exclude_adds() {
    let workspace = Workspace::new();
    let source = "pub fn needle() {}\n";
    workspace.write("src/lib.rs", source);
    let pruned = [
        "node_modules",
        "target",
        "dist",
        "coverage",
        ".venv",
        "__pycache__",
        "DerivedData",
        "extra",
    ];
    for dir in pruned {
        workspace.write(&format!("{dir}/lib.rs"), source);
    }
    workspace.write(".github/lib.rs", source);
    let runtime = workspace.runtime(&[]);
    let root = workspace.workspace.clone();
    let exclude = json!(["extra"]);
    let workspace_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    let paths_of =
        |outcome: &octocode_native::response::outcome::ToolOutcome, key: &str| -> Vec<String> {
            let data = row_data(outcome);
            let rows = data[key]
                .as_array()
                .unwrap_or_else(|| panic!("{key} rows: {}", outcome.structured_content));
            // structureSearch groups list entries by `dir`: `dir/<name> (<size>)`.
            let mut paths = rows
                .iter()
                .flat_map(|row| match (row["dir"].as_str(), row["files"].as_array()) {
                    (Some(dir), Some(entries)) => entries
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(|entry| format!("{dir}/{entry}"))
                        .collect::<Vec<_>>(),
                    _ => row
                        .as_str()
                        .or_else(|| row["path"].as_str())
                        .map(str::to_owned)
                        .into_iter()
                        .collect(),
                })
                .map(|path| {
                    let path = path.as_str();
                    path.split(' ').next().unwrap_or(path).replace('\\', "/")
                })
                .filter(|path| path.ends_with(".rs"))
                .map(|path| {
                    // Rows are relative to `base`, the workspace's parent.
                    path.strip_prefix(&format!("{}/", root.to_string_lossy()))
                        .or_else(|| path.strip_prefix(&format!("{workspace_name}/")))
                        .unwrap_or(&path)
                        .to_owned()
                })
                .collect::<Vec<_>>();
            paths.sort();
            paths
        };
    let syntax_visible = vec![".github/lib.rs".to_owned(), "src/lib.rs".to_owned()];

    let searched = call(
        &runtime,
        "localSearch",
        json!({"path":root,"matchString":"needle","includeHidden":true,"exclude":exclude}),
    )
    .await
    .expect("localSearch");
    assert_eq!(
        paths_of(&searched, "files"),
        syntax_visible,
        "{:?}",
        searched.structured_content
    );

    let files = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":root,"include":["*.rs"],"entryType":"f","includeHidden":true,"exclude":exclude}),
    )
    .await
    .expect("structureSearch files");
    assert_eq!(
        paths_of(&files, "files"),
        syntax_visible,
        "{:?}",
        files.structured_content
    );

    let tree = call(
        &runtime,
        "structureSearch",
        json!({"operation":"tree","path":root,"includeHidden":true,"exclude":exclude}),
    )
    .await
    .expect("structureSearch tree");
    // Listed entries only: a withheld notice names policy entries.
    let rendered = serde_json::to_string(&row_data(&tree)["files"]).expect("json");
    for dir in pruned {
        assert!(
            !rendered.contains(&format!("{dir}/")),
            "{dir} in {rendered}"
        );
    }
    let listed = |dir: &str| {
        rendered.contains(&format!("{dir}/")) || rendered.contains(&format!("\"dir\":\"{dir}\""))
    };
    assert!(listed(".github") && listed("src"), "{rendered}");

    let matched = call(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":root,"language":"rust","pattern":"pub fn needle() {}","includeHidden":true,"exclude":exclude}),
    )
    .await
    .expect("astSearch match");
    assert_eq!(
        paths_of(&matched, "files"),
        syntax_visible,
        "{:?}",
        matched.structured_content
    );

    let symbols = call(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":root,"exclude":exclude}),
    )
    .await
    .expect("astSearch symbols");
    let symbol_paths = paths_of(&symbols, "files");
    assert!(
        symbol_paths
            .iter()
            .all(|path| path == "src/lib.rs" || path == ".github/lib.rs")
            && symbol_paths.contains(&"src/lib.rs".to_owned()),
        "{symbol_paths:?}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn default_excludes_false_walks_dependency_directories() {
    let workspace = Workspace::new();
    workspace.write("src/app.rs", "fn needle() {}\n");
    workspace.write("node_modules/dep/index.js", "function needle() {}\n");
    let runtime = workspace.runtime(&[]);
    let names = |outcome: &octocode_native::response::outcome::ToolOutcome| {
        serde_json::to_string(&outcome.structured_content).unwrap_or_default()
    };
    let pruned = call(
        &runtime,
        "localSearch",
        json!({"path":workspace.workspace,"matchString":"needle"}),
    )
    .await
    .expect("default prune");
    // Not searched, and disclosed separately from matched files.
    assert!(
        !serde_json::to_string(&row_data(&pruned)["files"])
            .unwrap_or_default()
            .contains("node_modules"),
        "{}",
        names(&pruned)
    );
    assert_eq!(
        row_data(&pruned)["skippedPaths"],
        json!({"entries":[{"path":"node_modules","cause":"default-excluded"}],"sampled":false}),
        "{}",
        names(&pruned)
    );
    // `skippedPaths` is the one disclosure: no warning restates it.
    assert!(
        row_data(&pruned)["warnings"]
            .as_array()
            .is_none_or(|warnings| warnings
                .iter()
                .all(|warning| !warning.to_string().contains("Default excludes"))),
        "{}",
        names(&pruned)
    );
    let all = call(
        &runtime,
        "localSearch",
        json!({"path":workspace.workspace,"matchString":"needle","defaultExcludes":false}),
    )
    .await
    .expect("defaults off");
    let mut paths = row_data(&all)["files"]
        .as_array()
        .expect("matched files")
        .iter()
        .map(|file| file["path"].as_str().expect("match path"))
        .collect::<Vec<_>>();
    paths.sort_unstable();
    assert_eq!(paths, ["node_modules/dep/index.js", "src/app.rs"]);
    let tree = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":workspace.workspace,"defaultExcludes":false}),
    )
    .await
    .expect("structure defaults off");
    assert!(names(&tree).contains("index.js"), "{}", names(&tree));
    runtime.close().await;
}
