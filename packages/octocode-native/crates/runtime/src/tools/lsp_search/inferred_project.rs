//! TypeScript/JavaScript inferred-project coverage signal.
//!
//! Without a `tsconfig.json`/`jsconfig.json` above the anchor file, tsserver
//! answers from an *inferred* project holding only the opened documents and
//! what they import. Incoming-direction questions (who references / calls /
//! implements this?) then silently miss every file that was never opened.
//! Such rows are flagged partial with reason `inferredProject`, a hint, and
//! a lexical `localSearch` fallback over the workspace.

use super::LspSearchQuery;
use super::failure::push_reason;
use super::importers::{MAX_CANDIDATE_FILES, SCAN_CAPPED, SCAN_COMPLETE};
use serde_json::{Value, json};
use std::path::Path;

pub(super) const REASON: &str = "inferredProject";
/// More files mention the name than importer recovery verifies.
pub(super) const CAPPED_REASON: &str = "importerScanCapped";
const INFERRED_WARNING: &str = "No tsconfig.json or jsconfig.json covers this file, so the TypeScript server used an inferred project that sees only opened files and their imports; results from other files are missing. Add a tsconfig.json/jsconfig.json at the workspace root, or confirm with next.textSearch.";

pub(super) const TS_LANGUAGE_IDS: [&str; 4] = [
    "typescript",
    "typescriptreact",
    "javascript",
    "javascriptreact",
];

/// Operations whose answer depends on files the server has not opened.
fn is_incoming(operation: &str) -> bool {
    matches!(
        operation,
        "references"
            | "callers"
            | "callHierarchy"
            | "implementation"
            | "subtypes"
            | "workspaceSymbol"
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

/// A word-bounded literal regex for `name` in the default (`rust`) engine.
pub(super) fn word_pattern(name: &str) -> String {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut pattern = regex::escape(name);
    if word(name.chars().next()) {
        pattern.insert_str(0, "\\b");
    }
    if word(name.chars().last()) {
        pattern.push_str("\\b");
    }
    pattern
}

/// Flag an incoming-direction TS/JS row answered by an inferred project.
pub(super) fn annotate(
    row: &mut Value,
    query: &LspSearchQuery,
    language_id: Option<&str>,
    anchor_path: &str,
    workspace_root: &str,
) {
    if !language_id.is_some_and(|id| TS_LANGUAGE_IDS.contains(&id))
        || !is_incoming(&query.operation())
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
    {
        return;
    }
    // Importer recovery opened and verified every file that mentions the
    // name, so an inferred project no longer hides importers; a capped scan
    // leaves some unchecked even inside a configured project.
    let (reason, warning) = match row
        .pointer("/payload/coverage/importerScan")
        .and_then(Value::as_str)
    {
        Some(SCAN_COMPLETE) => return,
        Some(SCAN_CAPPED) => (
            CAPPED_REASON,
            format!(
                "More than {MAX_CANDIDATE_FILES} files mention this name; only the first {MAX_CANDIDATE_FILES} were opened and verified as importers. Confirm the rest with next.textSearch."
            ),
        ),
        _ if lacks_project_config(Path::new(anchor_path)) => (REASON, INFERRED_WARNING.to_owned()),
        _ => return,
    };
    if let Some(payload) = row.get_mut("payload").and_then(Value::as_object_mut) {
        let coverage = payload
            .entry("coverage")
            .or_insert_with(|| json!({"scope":"languageServer","exhaustive":false}));
        coverage["exhaustive"] = json!(false);
        coverage["reason"] = json!(reason);
    }
    // A warning, not a hint: the hint policy keeps hints for empty/error rows
    // only, and this caveat matters most when the row looks complete.
    let warning = warning.as_str();
    let name = query.symbol_name().filter(|name| !name.trim().is_empty());
    match name {
        // Partial only with an executable recovery.
        Some(name) => {
            push_reason(row, reason, &[warning.to_owned()]);
            row["next"]["textSearch"] = json!({
                "tool": "localSearch",
                "confidence": "medium",
                "query": {
                    "path": workspace_root,
                    "searchText": word_pattern(name),
                    "goal": query.goal(),
                    "reasoning": "Find textual uses the inferred TypeScript project cannot see."
                }
            });
        }
        // A position anchor has no name to search for: coverage reason and
        // warning only.
        None => match row.get_mut("warnings").and_then(Value::as_array_mut) {
            Some(warnings) => warnings.push(json!(warning)),
            None => row["warnings"] = json!([warning]),
        },
    }
}

const CLANGD_LANGUAGE_IDS: [&str; 4] = ["c", "cpp", "objective-c", "objective-cpp"];
const COMPILE_DATABASE_HINT: &str = "clangd found no compile_commands.json, compile_flags.txt, or .clangd for this file, so includes and cross-file symbols are unresolved; generate one (CMake: -DCMAKE_EXPORT_COMPILE_COMMANDS=ON) or use astSearch/localSearch.";

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

/// An empty C/C++ answer without a compilation database is almost always the
/// missing database, not a missing symbol: say so instead of the generic
/// "verify the anchor" advice.
pub(super) fn annotate_compile_database(
    row: &mut Value,
    language_id: Option<&str>,
    anchor_path: &str,
) {
    let empty = row.get("status").and_then(Value::as_str) == Some("empty")
        || row.pointer("/payload/kind").and_then(Value::as_str) == Some("empty");
    if empty
        && language_id.is_some_and(|id| CLANGD_LANGUAGE_IDS.contains(&id))
        && !has_compile_database(Path::new(anchor_path))
    {
        row["hints"] = json!([COMPILE_DATABASE_HINT]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs_query(uri: &str) -> LspSearchQuery {
        serde_json::from_value(json!({
            "operation":"references","goal": "test", "reasoning":"test","uri":uri,
            "symbolName":"greet","lineHint":1
        }))
        .expect("references query")
    }

    fn refs_row() -> Value {
        json!({"payload":{"kind":"references","locations":[],
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
            &root,
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
        assert_eq!(next["query"]["path"], root.as_str());
        assert_eq!(next["query"]["searchText"], "\\bgreet\\b");
        crate::contracts::validate_query("localSearch", next["query"].clone())
            .expect("fallback query is contract-valid");

        // With a tsconfig at the workspace root the server loads the project.
        std::fs::write(dir.path().join("tsconfig.json"), "{}").expect("tsconfig");
        let mut row = refs_row();
        annotate(
            &mut row,
            &query,
            Some("typescript"),
            &file.to_string_lossy(),
            &root,
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
            &root,
        );
        assert_eq!(row, refs_row());

        let definition: LspSearchQuery = serde_json::from_value(json!({
            "operation":"definition","goal": "test", "reasoning":"test",
            "uri":format!("file://{}", file.display()),"symbolName":"greet","lineHint":1
        }))
        .expect("definition query");
        let mut row = refs_row();
        annotate(
            &mut row,
            &definition,
            Some("typescript"),
            &file.to_string_lossy(),
            &root,
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
        let mut row = json!({"status":"empty","payload":{"kind":"empty"},"hints":["generic"]});
        annotate_compile_database(&mut row, Some("cpp"), &file.to_string_lossy());
        assert_eq!(row["hints"][0], COMPILE_DATABASE_HINT);
        std::fs::write(dir.path().join("compile_commands.json"), "[]").expect("db");
        let mut row = json!({"status":"empty","payload":{"kind":"empty"},"hints":["generic"]});
        annotate_compile_database(&mut row, Some("cpp"), &file.to_string_lossy());
        assert_eq!(row["hints"][0], "generic");
        let mut rust = json!({"status":"empty","payload":{"kind":"empty"},"hints":["generic"]});
        annotate_compile_database(&mut rust, Some("rust"), &file.to_string_lossy());
        assert_eq!(rust["hints"][0], "generic");
    }

    #[test]
    fn word_pattern_bounds_only_word_edges() {
        assert_eq!(word_pattern("greet"), "\\bgreet\\b");
        assert_eq!(word_pattern("$store"), "\\$store\\b");
    }
}
