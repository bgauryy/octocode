//! Single-project coverage signals: TypeScript/JavaScript inferred projects
//! and clangd without a compilation database.
//!
//! Without a `tsconfig.json`/`jsconfig.json` above the anchor file, tsserver
//! answers from an *inferred* project holding only the opened documents and
//! what they import. Incoming-direction questions (who references / calls /
//! implements this?) then silently miss every file that was never opened.
//! Such rows are flagged partial with reason `inferredProject`, a hint, and
//! a lexical `localSearch` fallback over the workspace.
//!
//! clangd without a compilation database is the C/C++ analogue: it answers
//! from the opened file alone, so its incoming-direction rows get reason
//! `noCompileDatabase` and the same fallback.

use super::LspSearchQuery;
use super::failure::flag_partial;
use super::importers::{SCAN_CAPPED, SCAN_COMPLETE, SCAN_FAILED};
use super::render::TS_LANGUAGE_IDS;
use super::scope::Scope;
use serde_json::{Value, json};
use std::path::Path;

pub(super) const REASON: &str = "inferredProject";
/// The lexical importer scan failed, so no importer was verified.
pub(super) const FAILED_REASON: &str = "importerScanFailed";
const INFERRED_WARNING: &str = "No tsconfig.json or jsconfig.json covers this file, so the TypeScript server used an inferred project that sees only opened files and their imports; results from other files are missing. Add a tsconfig.json/jsconfig.json at the workspace root, or confirm with hints.textSearch.";

/// Operations whose answer depends on files the server has not opened.
pub(super) fn is_incoming(operation: &str) -> bool {
    matches!(
        operation,
        "references" | "callers" | "implementation" | "subtypes" | "workspaceSymbol"
    )
}

/// True when no `tsconfig.json`/`jsconfig.json` exists in `start` or any
/// ancestor (tsserver's own config lookup walks every ancestor).
pub(super) fn lacks_project_config(start: &Path) -> bool {
    let dir = if start.is_dir() {
        Some(start)
    } else {
        start.parent()
    };
    !dir.into_iter().flat_map(Path::ancestors).any(|dir| {
        ["tsconfig.json", "jsconfig.json"]
            .iter()
            .any(|name| dir.join(name).is_file())
    })
}

/// Flag an incoming-direction TS/JS row answered by an inferred project.
pub(super) fn annotate(
    row: &mut Value,
    query: &LspSearchQuery,
    language_id: Option<&str>,
    anchor_path: &str,
    scope: &Scope,
) {
    if !language_id.is_some_and(|id| TS_LANGUAGE_IDS.contains(&id))
        || !is_incoming(&query.operation())
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
    {
        return;
    }
    // Importer recovery opened and verified every file that mentions the
    // name, so an inferred project no longer hides importers. A window with
    // later windows left was flagged by `Importers::annotate`, which names
    // the window and its `next.nextImporterPage`.
    let (reason, warning) = match row
        .pointer("/payload/coverage/importerScan")
        .and_then(Value::as_str)
    {
        Some(SCAN_COMPLETE | SCAN_CAPPED) => return,
        Some(SCAN_FAILED) => (
            FAILED_REASON,
            "Importer recovery could not read, open, or resolve every candidate, or its scan failed. Some importers remain unverified; confirm with hints.textSearch.".to_owned(),
        ),
        _ if lacks_project_config(Path::new(anchor_path)) => (REASON, INFERRED_WARNING.to_owned()),
        _ => return,
    };
    flag_partial(row, query, reason, &warning, scope);
}

const CLANGD_LANGUAGE_IDS: [&str; 4] = ["c", "cpp", "objective-c", "objective-cpp"];
const COMPILE_DATABASE_HINT: &str = "Generate compile_commands.json for clangd (CMake: -DCMAKE_EXPORT_COMPILE_COMMANDS=ON), or use astSearch/localSearch.";
/// Coverage reason of a C/C++ row answered without a compilation database.
pub(super) const NO_COMPILE_DATABASE_REASON: &str = "noCompileDatabase";
const NO_COMPILE_DATABASE_WARNING: &str = "clangd found no compile_commands.json, compile_flags.txt, or .clangd for this file, so it answered from the opened file alone: references in other files are missing. Generate a compilation database (CMake: -DCMAKE_EXPORT_COMPILE_COMMANDS=ON; Make: bear -- make), or confirm with hints.textSearch.";

/// Whether a clangd compilation database or flags file covers `start`
/// (clangd also looks in a `build/` subdirectory of each ancestor).
fn has_compile_database(start: &Path) -> bool {
    let dir = if start.is_dir() {
        Some(start)
    } else {
        start.parent()
    };
    dir.into_iter().flat_map(Path::ancestors).any(|dir| {
        ["compile_commands.json", "compile_flags.txt", ".clangd"]
            .iter()
            .any(|name| dir.join(name).exists() || dir.join("build").join(name).exists())
    })
}

/// clangd without a compilation database answers from the opened file only.
/// An empty answer is almost always the missing database, not a missing
/// symbol: say so instead of the generic "verify the anchor" advice. A
/// non-empty incoming-direction answer (references, callers) looks complete
/// but holds single-file results: flag it partial with the same cause.
pub(super) fn annotate_compile_database(
    row: &mut Value,
    query: &LspSearchQuery,
    language_id: Option<&str>,
    anchor_path: &str,
    scope: &Scope,
) {
    if !language_id.is_some_and(|id| CLANGD_LANGUAGE_IDS.contains(&id))
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
        || has_compile_database(Path::new(anchor_path))
    {
        return;
    }
    let empty = row.get("status").and_then(Value::as_str) == Some("empty")
        || row.pointer("/payload/kind").and_then(Value::as_str) == Some("empty");
    if empty {
        row["hints"] = json!([COMPILE_DATABASE_HINT]);
    } else if is_incoming(&query.operation()) {
        flag_partial(
            row,
            query,
            NO_COMPILE_DATABASE_REASON,
            NO_COMPILE_DATABASE_WARNING,
            scope,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs_query(uri: &str) -> LspSearchQuery {
        serde_json::from_value(json!({
            "operation":"references","mainGoal": "test", "reasoning":"test","path":uri,
            "symbolName":"greet","lineHint":1
        }))
        .expect("references query")
    }

    fn refs_row() -> Value {
        json!({"payload":{"kind":"references","matches":[],
            "coverage":{"scope":"languageServer","exhaustive":false}}})
    }

    #[test]
    fn inferred_ts_project_marks_references_partial_with_text_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).expect("src");
        let file = src.join("util.ts");
        std::fs::write(&file, "export function greet() {}\n").expect("write");
        // A tsconfig above the temp dir would legitimately suppress the flag.
        if !lacks_project_config(dir.path()) {
            return;
        }
        let query = refs_query(&format!("file://{}", file.display()));
        let mut row = refs_row();
        let root = dir.path().to_string_lossy().into_owned();
        annotate(
            &mut row,
            &query,
            Some("typescript"),
            &file.to_string_lossy(),
            &Scope::new(root.clone(), Vec::new()),
        );
        assert_eq!(row["isPartial"], true, "{row}");
        assert_eq!(row["partialReasons"], json!([REASON]));
        assert_eq!(row["payload"]["coverage"]["reason"], REASON);
        assert!(
            row["warnings"][0]
                .as_str()
                .is_some_and(|w| w.contains("tsconfig.json")),
            "{row}"
        );
        let next = &row["next"]["textSearch"];
        assert_eq!(next["tool"], "localSearch");
        assert_eq!(next["query"]["queries"][0]["path"], root.as_str());
        assert_eq!(next["query"]["queries"][0]["matchString"], "\\bgreet\\b");
        // The engine copies the input row's brief onto emitted continuations.
        let mut replay = next["query"]["queries"][0].clone();
        replay["mainGoal"] = serde_json::json!("Find greet's references.");
        replay["reasoning"] = serde_json::json!("Fall back to text search.");
        crate::contracts::validate_query("localSearch", replay)
            .expect("fallback query is contract-valid");

        // With a tsconfig at the workspace root the server loads the project.
        std::fs::write(dir.path().join("tsconfig.json"), "{}").expect("tsconfig");
        let mut row = refs_row();
        annotate(
            &mut row,
            &query,
            Some("typescript"),
            &file.to_string_lossy(),
            &Scope::new(root.clone(), Vec::new()),
        );
        assert_eq!(row, refs_row());
    }

    #[test]
    fn non_ts_languages_and_outgoing_operations_are_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("lib.rs");
        std::fs::write(&file, "fn greet() {}\n").expect("write");
        let root = dir.path().to_string_lossy().into_owned();
        let query = refs_query(&format!("file://{}", file.display()));
        let mut row = refs_row();
        annotate(
            &mut row,
            &query,
            Some("rust"),
            &file.to_string_lossy(),
            &Scope::new(root.clone(), Vec::new()),
        );
        assert_eq!(row, refs_row());

        let definition: LspSearchQuery = serde_json::from_value(json!({
            "operation":"definition","mainGoal": "test", "reasoning":"test",
            "path":format!("file://{}", file.display()),"symbolName":"greet","lineHint":1
        }))
        .expect("definition query");
        let mut row = refs_row();
        annotate(
            &mut row,
            &definition,
            Some("typescript"),
            &file.to_string_lossy(),
            &Scope::new(root.clone(), Vec::new()),
        );
        assert_eq!(row, refs_row());
    }

    #[test]
    fn empty_cpp_rows_without_a_compile_database_say_so() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("a.hpp");
        std::fs::write(&file, "int f();\n").expect("write");
        if has_compile_database(dir.path()) {
            return; // an ancestor of the temp dir has one
        }
        let query = refs_query(&format!("file://{}", file.display()));
        let root = Scope::new(dir.path().to_string_lossy().into_owned(), Vec::new());
        let path = file.to_string_lossy();
        let mut row = json!({"status":"empty","payload":{"kind":"empty"},"hints":["generic"]});
        annotate_compile_database(&mut row, &query, Some("cpp"), &path, &root);
        assert_eq!(row["hints"][0], COMPILE_DATABASE_HINT);
        // References found only in the opened file look complete: flag them.
        let mut found = refs_row();
        found["payload"]["matches"] = json!([{"path":"a.hpp","line":1}]);
        annotate_compile_database(&mut found, &query, Some("cpp"), &path, &root);
        assert_eq!(
            found["payload"]["coverage"]["reason"],
            NO_COMPILE_DATABASE_REASON
        );
        assert!(
            found["warnings"][0]
                .as_str()
                .is_some_and(|warning| warning.contains("compile_commands.json")),
            "{found}"
        );
        assert_eq!(found["next"]["textSearch"]["tool"], "localSearch");
        std::fs::write(dir.path().join("compile_commands.json"), "[]").expect("db");
        let mut row = json!({"status":"empty","payload":{"kind":"empty"},"hints":["generic"]});
        annotate_compile_database(&mut row, &query, Some("cpp"), &path, &root);
        assert_eq!(row["hints"][0], "generic");
        let mut rust = json!({"status":"empty","payload":{"kind":"empty"},"hints":["generic"]});
        annotate_compile_database(&mut rust, &query, Some("rust"), &path, &root);
        assert_eq!(rust["hints"][0], "generic");
    }

    #[test]
    fn word_pattern_bounds_only_word_edges() {
        use super::super::render::word_pattern;
        assert_eq!(word_pattern("greet"), "\\bgreet\\b");
        assert_eq!(word_pattern("$store"), "\\$store\\b");
    }
}
