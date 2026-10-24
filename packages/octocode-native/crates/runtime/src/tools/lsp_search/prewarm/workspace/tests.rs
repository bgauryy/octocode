use super::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, text).expect("write");
}

/// A workspace whose `.ts` and `.py` files map to configured servers.
fn fixture() -> (tempfile::TempDir, PathBuf, LspExecutionConfig) {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    let config = root.join("lsp-servers.json");
    write(
        &config,
        &serde_json::json!({"languageServers": {
            ".ts": {"command": "fake-ts", "args": [], "languageId": "typescript"},
            ".py": {"command": "fake-py", "args": [], "languageId": "python"}
        }})
        .to_string(),
    );
    let execution = LspExecutionConfig {
        config_path: Some(config.to_string_lossy().into_owned()),
        ..LspExecutionConfig::default()
    };
    (dir, root, execution)
}

/// Per family, the server with the most files is warmed: the larger
/// package of two TypeScript packages, and the one Python project; files
/// no server answers start nothing.
#[test]
fn the_workspace_warms_its_largest_project_per_family() {
    let (_dir, root, execution) = fixture();
    let paths = crate::tools::test_support::workspace_policy(&root);
    write(&root.join("small/package.json"), "{}\n");
    write(&root.join("small/a.ts"), "export const a = 1;\n");
    write(&root.join("big/package.json"), "{}\n");
    for index in 0..3 {
        write(&root.join(format!("big/src/m{index}.ts")), "export {};\n");
    }
    write(
        &root.join("tool/pyproject.toml"),
        "[project]\nname = 'tool'\n",
    );
    write(&root.join("tool/main.py"), "x = 1\n");
    write(&root.join("README.md"), "docs\n");
    let plans = workspace_plans(&root, &paths, &execution);
    let files = plans
        .iter()
        .map(|planned| {
            Path::new(&planned.file)
                .strip_prefix(&root)
                .expect("under root")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(files, ["big/src/m0.ts", "tool/main.py"], "{files:?}");
}

/// A workspace without a file any server answers plans no start.
#[test]
fn a_workspace_without_supported_files_starts_nothing() {
    let (_dir, root, execution) = fixture();
    let paths = crate::tools::test_support::workspace_policy(&root);
    write(&root.join("README.md"), "docs\n");
    write(&root.join("data/rows.csv"), "a,b\n");
    assert!(workspace_plans(&root, &paths, &execution).is_empty());
}
