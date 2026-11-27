use super::super::tests::{refs_query, refs_row};
use super::*;
use octocode_engine::lsp::clangd_database::DatabaseSearch;
use octocode_test_support::json_fixture;

/// clangd for `language` rooted at `root`, started with `args`.
fn clangd(language: &str, root: &Path, args: &[&str]) -> LspLanguageServerConfig {
    LspLanguageServerConfig {
        command: "clangd".into(),
        args: Some(args.iter().map(|arg| (*arg).to_owned()).collect()),
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some(language.into()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    }
}

fn nothing_above(dir: &Path) -> bool {
    clangd_config_sources(dir, &DatabaseSearch::Ancestors).is_empty()
}

#[test]
fn empty_cpp_rows_without_a_compile_database_say_so() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("a.hpp");
    std::fs::write(&file, "int f();\n").expect("write");
    if !nothing_above(dir.path()) {
        return; // an ancestor of the temp dir has one
    }
    let query = refs_query(&format!("file://{}", file.display()));
    let root = Scope::new(dir.path().to_string_lossy().into_owned(), Vec::new());
    let path = file.to_string_lossy();
    let cpp = clangd("cpp", dir.path(), &[]);
    let mut row = json_fixture(r#"{"status": "empty", "hints": ["generic"]}"#);
    annotate_compile_database(&mut row, &query, &cpp, &path, &root);
    // The row's own hint stays and no static repair joins it: the
    // reason's meaning names the database repair once.
    assert_eq!(row["hints"], json_fixture(r#"["generic"]"#));
    assert_eq!(
        row["lsp"]["analysisContext"]["compilation"]["configuration"], "notFound",
        "{row}"
    );
    octocode_runtime_base::test_support::checks::assert_no_reason_meaning(&row);
    // References found only in the opened file look complete: flag them.
    let mut found = refs_row();
    found["matches"] = json_fixture(r#"[{"path": "a.hpp", "line": 1}]"#);
    annotate_compile_database(&mut found, &query, &cpp, &path, &root);
    assert_eq!(
        found["partialReasons"],
        json!([NO_COMPILE_DATABASE_REASON]),
        "{found}"
    );
    assert_eq!(
        octocode_runtime_base::test_support::checks::related_call(&found, "localSearch", |_| true)
            ["tool"],
        "localSearch"
    );
    // clangd also answers CUDA, with the same unverified-context warning.
    let mut cuda = refs_row();
    cuda["matches"] = json_fixture(r#"[{"path": "a.hpp", "line": 1}]"#);
    annotate_compile_database(
        &mut cuda,
        &query,
        &clangd("cuda", dir.path(), &[]),
        &path,
        &root,
    );
    assert_eq!(
        cuda["partialReasons"],
        json!([NO_COMPILE_DATABASE_REASON]),
        "{cuda}"
    );
    std::fs::write(dir.path().join("compile_commands.json"), "[]").expect("db");
    let mut row = json_fixture(r#"{"status": "empty", "hints": ["generic"]}"#);
    annotate_compile_database(&mut row, &query, &cpp, &path, &root);
    assert_eq!(row["hints"][0], "generic");
    assert_eq!(
        row["lsp"]["analysisContext"]["compilation"]["configuration"],
        "found"
    );
    assert_eq!(
        row["lsp"]["analysisContext"]["compilation"]["command"],
        "unverified"
    );
    let mut rust = json_fixture(r#"{"status": "empty", "hints": ["generic"]}"#);
    annotate_compile_database(
        &mut rust,
        &query,
        &clangd("rust", dir.path(), &[]),
        &path,
        &root,
    );
    assert_eq!(rust["hints"][0], "generic");
}

/// Issue #482: clangd started with `--compile-commands-dir=build/debug`
/// reads that database, so a row there is not flagged as unconfigured;
/// and an ancestor database clangd does not read under that flag is no
/// configuration.
#[test]
fn a_configured_compile_commands_dir_is_the_database_clangd_reads() {
    let dir = tempfile::tempdir().expect("tempdir");
    if !nothing_above(dir.path()) {
        return; // an ancestor of the temp dir has one
    }
    let src = dir.path().join("src");
    let debug = dir.path().join("build/debug");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::create_dir_all(&debug).expect("debug");
    let file = src.join("a.cpp");
    std::fs::write(&file, "int f();").expect("source");
    std::fs::write(debug.join("compile_commands.json"), "[]").expect("db");
    let query = refs_query(&format!("file://{}", file.display()));
    let scope = Scope::new(dir.path().to_string_lossy().into_owned(), Vec::new());
    let path = file.to_string_lossy();
    let flagged = clangd("cpp", dir.path(), &["--compile-commands-dir=build/debug"]);
    let mut row = json_fixture(r#"{"status": "empty", "hints": ["generic"]}"#);
    annotate_compile_database(&mut row, &query, &flagged, &path, &scope);
    let assessment = &row["lsp"]["analysisContext"]["compilation"];
    assert_eq!(assessment["configuration"], "found", "{row}");
    assert_eq!(
        assessment["sources"],
        json!([debug.join("compile_commands.json").to_string_lossy()])
    );
    assert!(row.get("partialReasons").is_none(), "{row}");
    assert_eq!(row["hints"][0], "generic");
    // The same tree without the flag: clangd does not look in build/debug.
    let mut row = json_fixture(r#"{"status": "empty", "hints": ["generic"]}"#);
    annotate_compile_database(
        &mut row,
        &query,
        &clangd("cpp", dir.path(), &[]),
        &path,
        &scope,
    );
    assert_eq!(
        row["partialReasons"],
        json!([NO_COMPILE_DATABASE_REASON]),
        "{row}"
    );
    // A flag directory with no database while the root has one: clangd
    // reads only the flag directory.
    std::fs::remove_file(debug.join("compile_commands.json")).expect("rm");
    std::fs::write(dir.path().join("compile_commands.json"), "[]").expect("root db");
    let mut row = json_fixture(r#"{"status": "empty", "hints": ["generic"]}"#);
    annotate_compile_database(&mut row, &query, &flagged, &path, &scope);
    assert_eq!(
        row["partialReasons"],
        json!([NO_COMPILE_DATABASE_REASON]),
        "{row}"
    );
}

#[test]
fn config_presence_is_not_a_verified_compilation_command() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("a.cpp");
    std::fs::write(&file, "int f();").expect("source");
    let config = dir.path().join(".clangd");
    std::fs::write(&config, "Diagnostics:\n  Suppress: unused-includes\n").expect("config");
    let query = refs_query(&format!("file://{}", file.display()));
    let scope = Scope::new(dir.path().to_string_lossy().into_owned(), Vec::new());
    let mut row = refs_row();
    annotate_compile_database(
        &mut row,
        &query,
        &clangd("cpp", dir.path(), &[]),
        &file.to_string_lossy(),
        &scope,
    );
    let assessment = &row["lsp"]["analysisContext"]["compilation"];
    assert_eq!(assessment["configuration"], "found");
    assert_eq!(assessment["command"], "unverified");
    assert!(
        assessment["sources"]
            .as_array()
            .expect("sources")
            .contains(&json!(config.to_string_lossy()))
    );
    assert!(row.get("isPartial").is_none());
}

#[test]
fn build_dir_clangd_config_is_not_a_configuration() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("a.cpp");
    std::fs::write(&file, "int f();").expect("source");
    if !nothing_above(dir.path()) {
        return; // an ancestor of the temp dir has one
    }
    // clangd reads `.clangd` and `compile_flags.txt` only beside the file
    // and in its ancestors, never under `build/`.
    std::fs::create_dir(dir.path().join("build")).expect("build");
    std::fs::write(dir.path().join("build/.clangd"), "").expect("config");
    std::fs::write(dir.path().join("build/compile_flags.txt"), "").expect("flags");
    let query = refs_query(&format!("file://{}", file.display()));
    let scope = Scope::new(dir.path().to_string_lossy().into_owned(), Vec::new());
    let mut row = refs_row();
    annotate_compile_database(
        &mut row,
        &query,
        &clangd("cpp", dir.path(), &[]),
        &file.to_string_lossy(),
        &scope,
    );
    let assessment = &row["lsp"]["analysisContext"]["compilation"];
    assert_eq!(assessment["configuration"], "notFound", "{row}");
    assert_eq!(assessment["sources"], json!([]), "{row}");
}

/// Without a compile database clangd has no index: a definition is the
/// declaration the anchor's unit includes, never the body another unit
/// compiles, so a definition row is partial for that reason, as an
/// incoming one is; an outline is the file's own and is not.
#[test]
fn a_definition_without_a_compile_database_is_partial() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("pthread_unix.c");
    std::fs::write(&file, "fatalf(\"x\");\n").expect("source");
    if !nothing_above(dir.path()) {
        return; // an ancestor of the temp dir has one
    }
    let scope = Scope::new(dir.path().to_string_lossy().into_owned(), Vec::new());
    let path = file.to_string_lossy().into_owned();
    let row_for = |operation: &str| {
        let mut wire = json!({"operation": operation, "path": path});
        if operation == "definition" {
            wire["symbolName"] = json!("fatalf");
            wire["line"] = json!(1);
        }
        let query: LspSearchQuery = serde_json::from_value(wire).expect("query");
        let mut row = json_fixture(r#"{"matches": [{"path": "libcgo.h", "line": 86}]}"#);
        annotate_compile_database(
            &mut row,
            &query,
            &clangd("c", dir.path(), &[]),
            &path,
            &scope,
        );
        row
    };
    let definition = row_for("definition");
    assert_eq!(
        definition["partialReasons"],
        json!([NO_COMPILE_DATABASE_REASON]),
        "{definition}"
    );
    let outline = row_for("documentSymbols");
    assert!(outline.get("isPartial").is_none(), "{outline}");
}

/// The compilation sources are spelled as the read policy spells a path:
/// relative inside the workspace, never an absolute home path.
#[test]
fn compilation_sources_are_spelled_by_the_read_policy() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let paths = octocode_runtime_base::test_support::workspace_policy(&root);
    let database = root
        .join("compile_commands.json")
        .to_string_lossy()
        .into_owned();
    let mut row = json!({"lsp": {"analysisContext": {"compilation": {
            "configuration": "found", "command": "unverified", "sources": [database]}}}});
    redact_sources(&mut row, &paths);
    assert_eq!(
        row["lsp"]["analysisContext"]["compilation"]["sources"],
        json_fixture(r#"["compile_commands.json"]"#)
    );
}
