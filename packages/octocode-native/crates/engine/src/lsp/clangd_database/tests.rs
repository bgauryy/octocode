use super::*;
use crate::test_support::TmpDir;

fn config(root: &Path, args: &[&str], init: Option<serde_json::Value>) -> LspLanguageServerConfig {
    LspLanguageServerConfig {
        command: "clangd".into(),
        args: Some(args.iter().map(|arg| (*arg).to_owned()).collect()),
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("cpp".into()),
        initialization_options: init,
        env: None,
        max_memory_mb: None,
        source: crate::lsp::types::ServerSource::Path,
    }
}

/// clangd's lookup order and flag spellings, as its sources define them.
#[test]
fn database_search_follows_clangd() {
    let root = TmpDir::new();
    let debug = root.join("build/debug");
    let other = root.join("other");
    std::fs::create_dir_all(&debug).expect("debug");
    std::fs::create_dir_all(&other).expect("other");
    let fixed = |dir: &Path| DatabaseSearch::Fixed(dir.to_path_buf());
    let absolute = debug.to_string_lossy().into_owned();
    let file = root.join("not-a-dir");
    std::fs::write(&file, "").expect("file");
    let cases: Vec<(&str, Vec<&str>, Option<serde_json::Value>, DatabaseSearch)> = vec![
        (
            "= form",
            vec!["--compile-commands-dir=build/debug"],
            None,
            fixed(&debug),
        ),
        (
            "separate value",
            vec!["--compile-commands-dir", "build/debug"],
            None,
            fixed(&debug),
        ),
        (
            "single dash",
            vec!["-compile-commands-dir=build/debug"],
            None,
            fixed(&debug),
        ),
        (
            "single dash separate value",
            vec!["-compile-commands-dir", "build/debug"],
            None,
            fixed(&debug),
        ),
        (
            "empty = value is unset",
            vec!["--compile-commands-dir="],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "empty separate value is unset",
            vec!["--compile-commands-dir", ""],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "a later empty value unsets an earlier one",
            vec![
                "--compile-commands-dir=build/debug",
                "--compile-commands-dir=",
            ],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "flag without a value",
            vec!["--compile-commands-dir"],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "a value naming an existing file is accepted",
            vec!["--compile-commands-dir=not-a-dir"],
            None,
            fixed(&file),
        ),
        (
            "absolute",
            vec!["--compile-commands-dir", &absolute],
            None,
            fixed(&debug),
        ),
        (
            "missing dir falls back",
            vec!["--compile-commands-dir=nope"],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "no flag",
            vec!["--background-index"],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "init option beats the flag",
            vec!["--compile-commands-dir=build/debug"],
            Some(serde_json::json!({"compilationDatabasePath": "other"})),
            fixed(&other),
        ),
    ];
    for (case, args, init, expected) in cases {
        assert_eq!(
            database_search(&config(&root, &args, init)),
            expected,
            "{case}"
        );
    }
}

#[test]
fn a_fixed_search_finds_its_databases_and_ancestor_clangd_files() {
    let root = TmpDir::new();
    let src = root.join("src");
    let fixed_dir = root.join("out");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::create_dir_all(fixed_dir.join("build")).expect("out");
    // Ancestor databases are not read under a fixed search.
    std::fs::write(root.join("compile_commands.json"), "[]").expect("root db");
    std::fs::write(root.join(".clangd"), "").expect(".clangd");
    std::fs::write(fixed_dir.join("build/compile_commands.json"), "[]").expect("build db");
    std::fs::write(fixed_dir.join("compile_flags.txt"), "-std=c++20").expect("flags");
    let search = DatabaseSearch::Fixed(fixed_dir.clone());
    assert_eq!(
        clangd_config_sources(&src, &search),
        vec![
            fixed_dir.join("build/compile_commands.json"),
            fixed_dir.join("compile_flags.txt"),
            root.join(".clangd"),
        ]
    );
    assert_eq!(
        clangd_config_sources(&src, &DatabaseSearch::Ancestors),
        vec![root.join("compile_commands.json"), root.join(".clangd")]
    );
}

/// Under the default search a database also counts in an ancestor's
/// `build/`, but `.clangd` and `compile_flags.txt` under `build/` configure
/// nothing.
#[test]
fn an_ancestor_search_reads_build_databases_only() {
    let root = TmpDir::new();
    let src = root.join("src");
    std::fs::create_dir_all(root.join("build")).expect("build");
    std::fs::create_dir_all(&src).expect("src");
    if !clangd_config_sources(&src, &DatabaseSearch::Ancestors).is_empty() {
        return; // an ancestor of the temp dir has one
    }
    std::fs::write(root.join("build/.clangd"), b"").expect("build .clangd");
    std::fs::write(root.join("build/compile_flags.txt"), b"").expect("build flags");
    assert!(clangd_config_sources(&src, &DatabaseSearch::Ancestors).is_empty());
    std::fs::write(root.join("build/compile_commands.json"), b"[]").expect("db");
    std::fs::write(src.join(".clangd"), b"").expect(".clangd");
    assert_eq!(
        clangd_config_sources(&src, &DatabaseSearch::Ancestors),
        [
            root.join("build/compile_commands.json"),
            src.join(".clangd")
        ]
    );
}
