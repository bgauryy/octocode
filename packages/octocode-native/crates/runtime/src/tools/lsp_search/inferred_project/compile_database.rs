//! C-family coverage: configuration files clangd reads, recorded apart from
//! an effective compilation command.

use super::super::analysis_context;
use super::super::failure::flag_partial;
use super::super::scope::Scope;
use super::{anchor_dir, is_incoming};
use crate::tools::lsp_search::lsp_search_query;
use octocode_contract_types::LspSearchQuery;
use octocode_engine::lsp::clangd_database::{clangd_config_sources, database_search};
use octocode_engine::lsp::types::LspLanguageServerConfig;
use serde_json::{Value, json};
use std::path::Path;

const COMPILE_DATABASE_HINT: &str = "Generate compile_commands.json for clangd (CMake: -DCMAKE_EXPORT_COMPILE_COMMANDS=ON), or use astSearch/localSearch.";
/// Coverage reason of a C/C++ row answered without a compilation database.
pub(in crate::tools::lsp_search) const NO_COMPILE_DATABASE_REASON:
    crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::NoCompileDatabase;
const NO_COMPILE_DATABASE_WARNING: &str = "No compile_commands.json, compile_flags.txt, or .clangd was found where clangd looks (its --compile-commands-dir or compilationDatabasePath directory when set, else the file's ancestors). The effective compilation command and indexed scope are unverified; confirm project setup and diagnostics before interpreting missing results.";

/// Record discovered configuration separately from the effective command.
/// File presence cannot establish command validity or indexed coverage.
/// The files checked follow clangd's own lookup for the server's `config`.
pub(in crate::tools::lsp_search) fn annotate_compile_database(
    row: &mut Value,
    query: &LspSearchQuery,
    config: &LspLanguageServerConfig,
    anchor_path: &str,
    scope: &Scope,
) {
    if !octocode_engine::lsp::family::is_c_family(config.language_id.as_deref())
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
    {
        return;
    }
    let search = database_search(config);
    let sources: Vec<String> = clangd_config_sources(anchor_dir(Path::new(anchor_path)), &search)
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let found = !sources.is_empty();
    analysis_context::context(row)["compilation"] = json!({
        "configuration": if found { "found" } else { "notFound" },
        "command": "unverified",
        "sources": sources,
    });
    if found {
        return;
    }
    let empty = row.get("status").and_then(Value::as_str) == Some("empty")
        || row.pointer("/status").and_then(Value::as_str) == Some("empty");
    if empty {
        row["hints"] = json!([COMPILE_DATABASE_HINT]);
    }
    if empty || is_incoming(lsp_search_query::operation(query)) {
        flag_partial(
            row,
            query,
            NO_COMPILE_DATABASE_REASON,
            Some(NO_COMPILE_DATABASE_WARNING),
            scope,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{refs_query, refs_row};
    use super::*;
    use octocode_engine::lsp::clangd_database::DatabaseSearch;

    /// clangd for `language` rooted at `root`, started with `args`.
    fn clangd(language: &str, root: &Path, args: &[&str]) -> LspLanguageServerConfig {
        LspLanguageServerConfig {
            command: "clangd".into(),
            args: Some(args.iter().map(|arg| (*arg).to_owned()).collect()),
            workspace_root: root.to_string_lossy().into_owned(),
            language_id: Some(language.into()),
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
        let mut row = json!({"status":"empty","hints":["generic"]});
        annotate_compile_database(&mut row, &query, &cpp, &path, &root);
        assert_eq!(row["hints"][0], COMPILE_DATABASE_HINT);
        // References found only in the opened file look complete: flag them.
        let mut found = refs_row();
        found["matches"] = json!([{"path":"a.hpp","line":1}]);
        annotate_compile_database(&mut found, &query, &cpp, &path, &root);
        assert_eq!(
            found["partialReasons"],
            json!([NO_COMPILE_DATABASE_REASON]),
            "{found}"
        );
        assert!(
            found["warnings"][0]
                .as_str()
                .is_some_and(|warning| warning.contains("compile_commands.json")),
            "{found}"
        );
        assert_eq!(
            crate::tools::test_support::checks::related_call(&found, "localSearch", |_| true)["tool"],
            "localSearch"
        );
        // clangd also answers CUDA, with the same unverified-context warning.
        let mut cuda = refs_row();
        cuda["matches"] = json!([{"path":"a.hpp","line":1}]);
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
        let mut row = json!({"status":"empty","hints":["generic"]});
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
        let mut rust = json!({"status":"empty","hints":["generic"]});
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
        let mut row = json!({"status":"empty","hints":["generic"]});
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
        let mut row = json!({"status":"empty","hints":["generic"]});
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
        let mut row = json!({"status":"empty","hints":["generic"]});
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
}
