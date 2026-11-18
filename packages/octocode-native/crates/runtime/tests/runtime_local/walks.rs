//! Local walks: structureSearch dispatch and file discovery, the shared default prune, and `defaultExcludes: false`.

use crate::support::{Workspace, call, row_data, row_status};
use octocode_contract_types::ErrorCode;
use octocode_native::runtime::{HostOptions, ToolRuntime};
use octocode_runtime_base::config::RuntimeSurface;
use octocode_test_support::json_fixture;
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
        json!({"operation":"files","path":workspace.workspace,"include":["**/*.ts"],"entryType":"file"}),
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
    let exclude = json_fixture(r#"["extra", "extra/**"]"#);
    let workspace_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    let paths_of = |outcome: &octocode_runtime_base::response::outcome::ToolOutcome,
                    key: &str|
     -> Vec<String> {
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
        json!({"path":root,"matchString":["needle"],"includeHidden":true,"exclude":exclude}),
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
        json!({"operation":"files","path":root,"include":["**/*.rs"],"entryType":"file","includeHidden":true,"exclude":exclude}),
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
        json!({"operation":"match","path":root,"parser":"rust","rule":{"pattern":"pub fn needle() {}"},"includeHidden":true,"exclude":exclude}),
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

/// `graphBuild` reads what `localSearch` and `astSearch` read: the one default
/// prune and no list of its own, so `obj/` is in all three. What the prune
/// leaves out is a disclosed gap, not a silent one.
#[tokio::test]
async fn graph_build_reads_what_the_other_walks_read_and_discloses_the_prune() {
    let workspace = Workspace::new();
    let source = "pub fn needle() {}\n";
    for file in ["obj/a.rs", "src/b.rs", "node_modules/dep/c.rs"] {
        workspace.write(file, source);
    }
    let runtime = workspace.runtime(&[]);
    let root = workspace.workspace.clone();
    let rendered = |outcome: &octocode_runtime_base::response::outcome::ToolOutcome| {
        serde_json::to_string(&row_data(outcome)["files"]).expect("json")
    };
    let searched = call(
        &runtime,
        "localSearch",
        json!({"path":root,"matchString":["needle"]}),
    )
    .await
    .expect("localSearch");
    let matched = call(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":root,"parser":"rust","rule":{"pattern":"pub fn needle() {}"}}),
    )
    .await
    .expect("astSearch");
    for files in [rendered(&searched), rendered(&matched)] {
        assert!(files.contains("a.rs") && files.contains("b.rs"), "{files}");
        assert!(!files.contains("c.rs"), "{files}");
    }

    let built = call(&runtime, "graphBuild", json!({"path":root}))
        .await
        .expect("graphBuild");
    let receipt = row_data(&built);
    assert_eq!(receipt["scan"]["filesScanned"], 2, "{receipt}");
    assert_eq!(receipt["diagnostics"]["default-excluded"], 1, "{receipt}");

    let nodes = call(
        &runtime,
        "graphQuery",
        json!({"operation":"find","path":root,"target":"obj/a.rs"}),
    )
    .await
    .expect("graphQuery find");
    assert!(
        serde_json::to_string(row_data(&nodes))
            .expect("json")
            .contains("obj/a.rs"),
        "{:?}",
        nodes.structured_content
    );
    let gaps = call(
        &runtime,
        "graphQuery",
        json!({"operation":"diagnostics","path":root,"target":"default-excluded"}),
    )
    .await
    .expect("graphQuery diagnostics");
    let gaps = serde_json::to_string(row_data(&gaps)).expect("json");
    assert!(gaps.contains("node_modules"), "{gaps}");

    // The freshness scan walks what the build walked: nothing reads as added.
    let stale = call(
        &runtime,
        "graphQuery",
        json!({"operation":"stale","path":root}),
    )
    .await
    .expect("graphQuery stale");
    assert_eq!(
        row_data(&stale)["fresh"],
        true,
        "{:?}",
        stale.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn default_excludes_false_walks_dependency_directories() {
    let workspace = Workspace::new();
    workspace.write("src/app.rs", "fn needle() {}\n");
    workspace.write("node_modules/dep/index.js", "function needle() {}\n");
    let runtime = workspace.runtime(&[]);
    let names = |outcome: &octocode_runtime_base::response::outcome::ToolOutcome| {
        serde_json::to_string(&outcome.structured_content).unwrap_or_default()
    };
    let pruned = call(
        &runtime,
        "localSearch",
        json!({"path":workspace.workspace,"matchString":["needle"]}),
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
        json_fixture(
            r#"{
                "entries": [{
                    "path": "node_modules",
                    "cause": "default-excluded",
                    "reason": "node_modules/"
                }]
            }"#
        ),
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
        json!({"path":workspace.workspace,"matchString":["needle"],"defaultExcludes":false}),
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

/// Every `skippedPaths` entry is spelled from the workspace root, whatever
/// cause skipped it and whichever directory the walk listed: a directory
/// holding withheld entries reads like the hidden and pruned paths beside it.
#[tokio::test]
async fn skipped_paths_of_a_nested_walk_are_spelled_from_the_workspace_root() {
    let workspace = Workspace::new();
    workspace.write("pkg/src/app.rs", "fn needle() {}\n");
    workspace.write("pkg/deploy/.env", "A=1\n");
    workspace.write("pkg/deploy/notes.txt", "needle\n");
    workspace.write("pkg/.cache/kept.txt", "needle\n");
    workspace.write("pkg/node_modules/dep/index.js", "function needle() {}\n");
    let runtime = workspace.runtime(&[]);
    let scope = workspace.workspace.join("pkg");
    for (tool, query, expected) in [
        (
            "structureSearch",
            json!({"operation":"files","path":scope,"includeHidden":true,"maxDepth":5}),
            vec![("withheld", "pkg/deploy")],
        ),
        // Entries of one cause share a row: its first path and a count.
        (
            "structureSearch",
            json!({"operation":"tree","path":scope,"maxDepth":5}),
            vec![],
        ),
        (
            "localSearch",
            json!({"path":scope,"matchString":["needle"],"includeHidden":true}),
            vec![("withheld", "pkg/deploy")],
        ),
    ] {
        let outcome = call(&runtime, tool, query).await.expect("walk");
        let entries = row_data(&outcome)["skippedPaths"]["entries"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| panic!("{tool}: no skippedPaths: {:?}", row_data(&outcome)));
        assert!(!entries.is_empty(), "{tool}: {:?}", row_data(&outcome));
        for (cause, path) in expected {
            assert!(
                entries
                    .iter()
                    .any(|entry| entry["cause"] == cause && entry["path"] == path),
                "{tool}: no {cause} entry at {path}: {entries:?}"
            );
        }
        for entry in &entries {
            let path = entry["path"].as_str().expect("path");
            assert!(path == "pkg" || path.starts_with("pkg/"), "{tool}: {entry}");
        }
    }
    runtime.close().await;
}

/// One process, two calls, two environments: each call's walk applies the
/// global ignore file its own `GIT_CONFIG_GLOBAL` names, and neither reads
/// the other's (nor the process's).
#[tokio::test]
async fn two_calls_in_one_process_walk_under_their_own_global_ignore() {
    let workspace = Workspace::new();
    std::fs::create_dir_all(workspace.workspace.join(".git")).expect("work tree");
    for name in ["keep.txt", "a.secret", "b.scratch"] {
        workspace.write(name, "needle\n");
    }
    let env_naming = |tag: &str, rule: &str| {
        let ignore = workspace.write(&format!("../global-{tag}/ignore"), rule);
        let config = workspace.write(
            &format!("../global-{tag}/config"),
            format!("[core]\n\texcludesFile = {}\n", ignore.display()),
        );
        let missing = workspace.workspace.join("../no-system-config");
        [
            ("GIT_CONFIG_GLOBAL", config.to_string_lossy().into_owned()),
            ("GIT_CONFIG_SYSTEM", missing.to_string_lossy().into_owned()),
        ]
    };
    let root = workspace.workspace.clone();
    let found = |runtime: ToolRuntime| {
        let root = root.clone();
        async move {
            let output = call(
                &runtime,
                "localSearch",
                json!({"path":root,"matchString":["needle"]}),
            )
            .await
            .expect("search");
            let names = row_data(&output)["files"]
                .as_array()
                .unwrap_or_else(|| panic!("files: {}", output.structured_content))
                .iter()
                .map(|file| {
                    file["path"]
                        .as_str()
                        .expect("path")
                        .rsplit('/')
                        .next()
                        .unwrap()
                        .to_owned()
                })
                .collect::<std::collections::BTreeSet<_>>();
            runtime.close().await;
            names
        }
    };
    let secrets = workspace.runtime(&env_naming("one", "*.secret\n"));
    let caches = workspace.runtime(&env_naming("two", "*.scratch\n"));
    let names = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(found(secrets).await, names(&["b.scratch", "keep.txt"]));
    assert_eq!(found(caches).await, names(&["a.secret", "keep.txt"]));
}

/// A search root that is no work tree, two repositories below it, and a
/// global ignore file of an unanchored and an anchored pattern: git reads the
/// anchored one relative to the work tree each file is in, so it hides the
/// file at each repository's top and nowhere else, and both patterns reach
/// only files inside a repository. localSearch and structureSearch serve the
/// same files.
#[tokio::test]
async fn global_ignore_rules_apply_per_repository_below_a_plain_root() {
    let workspace = Workspace::new();
    for repo in ["one", "deep/two"] {
        std::fs::create_dir_all(workspace.workspace.join(repo).join(".git")).expect("work tree");
        for file in [
            "keep.txt",
            "a.secret",
            "anchored.txt",
            "sub/anchored.txt",
            "sub/b.secret",
        ] {
            workspace.write(&format!("{repo}/{file}"), "needle\n");
        }
    }
    for file in ["plain.secret", "anchored.txt", "loose/anchored.txt"] {
        workspace.write(file, "needle\n");
    }
    let ignore = workspace.write("../global-plain/ignore", "*.secret\n/anchored.txt\n");
    let config = workspace.write(
        "../global-plain/config",
        format!("[core]\n\texcludesFile = {}\n", ignore.display()),
    );
    let missing = workspace.workspace.join("../no-system-config");
    let runtime = workspace.runtime(&[
        ("GIT_CONFIG_GLOBAL", config.to_string_lossy().into_owned()),
        ("GIT_CONFIG_SYSTEM", missing.to_string_lossy().into_owned()),
    ]);
    let root = workspace.workspace.clone();
    let searched = call(
        &runtime,
        "localSearch",
        json!({"path":root,"matchString":["needle"]}),
    )
    .await
    .expect("search");
    let mut found = row_data(&searched)["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| {
            let path = file["path"].as_str().expect("path");
            path.strip_prefix(root.to_string_lossy().as_ref())
                .unwrap_or(path)
                .trim_start_matches('/')
                .to_owned()
        })
        .collect::<Vec<_>>();
    found.sort();
    assert_eq!(
        found,
        [
            "anchored.txt",
            "deep/two/keep.txt",
            "deep/two/sub/anchored.txt",
            "loose/anchored.txt",
            "one/keep.txt",
            "one/sub/anchored.txt",
            "plain.secret"
        ]
    );
    let listed = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":root,"entryType":"file"}),
    )
    .await
    .expect("files");
    let mut listed_files = row_data(&listed)["files"]
        .as_array()
        .expect("files")
        .iter()
        .flat_map(|row| {
            let dir = row["dir"].as_str().expect("dir");
            row["files"]
                .as_array()
                .expect("names")
                .iter()
                .map(move |name| format!("{dir}/{}", name.as_str().expect("name")))
        })
        .map(|path| path.trim_start_matches("./").to_owned())
        .collect::<Vec<_>>();
    listed_files.sort();
    assert_eq!(listed_files, found);
    runtime.close().await;
}

/// One process, two calls, two environments: localSearch, structureSearch,
/// astSearch and graphBuild each walk under the global ignore file their own
/// call's environment names, so the daemon's key holds no variable that
/// names it.
#[tokio::test]
async fn every_walking_tool_applies_its_own_calls_global_ignore() {
    let workspace = Workspace::new();
    std::fs::create_dir_all(workspace.workspace.join(".git")).expect("work tree");
    for name in ["keep.rs", "a_secret.rs", "b_scratch.rs"] {
        workspace.write(name, "pub fn needle() {}\n");
    }
    let env_naming = |tag: &str, rule: &str| {
        let ignore = workspace.write(&format!("../global-{tag}/ignore"), rule);
        let config = workspace.write(
            &format!("../global-{tag}/config"),
            format!("[core]\n\texcludesFile = {}\n", ignore.display()),
        );
        let missing = workspace.workspace.join("../no-system-config");
        [
            ("GIT_CONFIG_GLOBAL", config.to_string_lossy().into_owned()),
            ("GIT_CONFIG_SYSTEM", missing.to_string_lossy().into_owned()),
        ]
    };
    let root = workspace.workspace.clone();
    let seen = |runtime: ToolRuntime| {
        let root = root.clone();
        async move {
            let names_of = |outcome: &octocode_runtime_base::response::outcome::ToolOutcome| {
                let rendered = serde_json::to_string(&row_data(outcome)["files"]).expect("json");
                ["keep", "a_secret", "b_scratch"]
                    .into_iter()
                    .filter(|name| rendered.contains(name))
                    .collect::<Vec<_>>()
            };
            let searched = call(
                &runtime,
                "localSearch",
                json!({"path":root,"matchString":["needle"]}),
            )
            .await
            .expect("localSearch");
            let listed = call(
                &runtime,
                "structureSearch",
                json!({"operation":"files","path":root,"entryType":"file"}),
            )
            .await
            .expect("structureSearch");
            let matched = call(
                &runtime,
                "astSearch",
                json!({"operation":"match","path":root,"parser":"rust","rule":{"pattern":"pub fn needle() {}"}}),
            )
            .await
            .expect("astSearch");
            let built = call(&runtime, "graphBuild", json!({"path":root}))
                .await
                .expect("graphBuild");
            let scanned = row_data(&built)["scan"]["filesScanned"].clone();
            let found = (
                names_of(&searched),
                names_of(&listed),
                names_of(&matched),
                scanned,
            );
            runtime.close().await;
            found
        }
    };
    let secrets = workspace.runtime(&env_naming("one", "*_secret.rs\n"));
    let scratch = workspace.runtime(&env_naming("two", "*_scratch.rs\n"));
    let (one, two) = (seen(secrets).await, seen(scratch).await);
    assert_eq!(one.0, ["keep", "b_scratch"], "localSearch {one:?}");
    assert_eq!(two.0, ["keep", "a_secret"], "localSearch {two:?}");
    assert_eq!(one.1, ["keep", "b_scratch"], "structureSearch {one:?}");
    assert_eq!(two.1, ["keep", "a_secret"], "structureSearch {two:?}");
    assert_eq!(one.2, ["keep", "b_scratch"], "astSearch {one:?}");
    assert_eq!(two.2, ["keep", "a_secret"], "astSearch {two:?}");
    assert_eq!(one.3, 2, "graphBuild {one:?}");
    assert_eq!(two.3, 2, "graphBuild {two:?}");
}
