//! Single-project coverage signals: TypeScript/JavaScript inferred projects
//! and, in `compile_database`, clangd without a compilation database.
//!
//! Without a `tsconfig.json`/`jsconfig.json` above the anchor file, tsserver
//! answers from an *inferred* project holding only the opened documents and
//! what they import. Incoming-direction questions (who references / calls /
//! implements this?) then silently miss every file that was never opened.
//! Such rows are flagged partial with reason `inferredProject`, a hint, and
//! a lexical `localSearch` fallback over the workspace.
//!
//! C-family rows record configuration files separately from an effective
//! compilation command. Missing configuration is a coverage warning; finding
//! a file does not verify the command or the server's indexed scope.

use super::failure::flag_partial;
use super::importers::{SCAN_CAPPED, SCAN_COMPLETE, SCAN_FAILED};
use super::scope::Scope;
use crate::tools::lsp_search::lsp_search_query;
use octocode_contract_types::LspSearchQuery;
use octocode_engine::lsp::workspace::typescript_project_dir;
use serde_json::Value;
use std::path::Path;

pub(super) const REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::InferredProject;
/// The lexical importer scan failed, so no importer was verified.
pub(super) const FAILED_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::ImporterScanFailed;
const INFERRED_WARNING: &str = "No tsconfig.json or jsconfig.json covers this file, so the TypeScript server used an inferred project that sees only opened files and their imports; results from other files are missing. Add a tsconfig.json/jsconfig.json at the workspace root, or confirm with a localSearch of the name.";

/// Operations whose answer depends on files the server has not opened.
pub(super) fn is_incoming(operation: &str) -> bool {
    matches!(
        operation,
        "references" | "callers" | "implementation" | "subtypes" | "workspaceSymbol"
    )
}

/// The directory whose ancestors a server searches for the project of
/// `anchor`: the anchor itself when it is a directory, else its parent.
fn anchor_dir(anchor: &Path) -> &Path {
    if anchor.is_dir() {
        anchor
    } else {
        anchor.parent().unwrap_or(anchor)
    }
}

/// Flag an incoming-direction TS/JS row answered by an inferred project.
pub(super) fn annotate(
    row: &mut Value,
    query: &LspSearchQuery,
    language_id: Option<&str>,
    anchor_path: &str,
    scope: &Scope,
) {
    if !octocode_engine::lsp::family::is_typescript_family(language_id)
        || !is_incoming(lsp_search_query::operation(query))
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
    {
        return;
    }
    // Importer recovery opened and verified every file that mentions the
    // name, so an inferred project no longer hides importers. A window with
    // later windows left was flagged by `Importers::annotate`, which names
    // the window and its `next.nextPage`.
    let (reason, warning) = match row
        .pointer("/coverage/importerScan")
        .and_then(Value::as_str)
    {
        Some(SCAN_COMPLETE | SCAN_CAPPED) => return,
        Some(SCAN_FAILED) => (
            FAILED_REASON,
            "Importer recovery could not read, open, or resolve every candidate, or its scan failed. Some importers remain unverified; confirm with a localSearch of the name.".to_owned(),
        ),
        _ if typescript_project_dir(anchor_dir(Path::new(anchor_path))).is_none() => {
            (REASON, INFERRED_WARNING.to_owned())
        }
        _ => return,
    };
    flag_partial(row, query, reason, Some(&warning), scope);
}

pub(in crate::tools::lsp_search) mod compile_database;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(super) fn refs_query(uri: &str) -> LspSearchQuery {
        serde_json::from_value(json!({
            "operation":"references","path":uri,
            "symbolName":"greet","line":1
        }))
        .expect("references query")
    }

    pub(super) fn refs_row() -> Value {
        json!({"operation":"references","matches":[],"coverage":{}})
    }

    #[test]
    fn inferred_ts_project_marks_references_partial_with_text_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).expect("src");
        let file = src.join("util.ts");
        std::fs::write(&file, "export function greet() {}\n").expect("write");
        // A tsconfig above the temp dir would legitimately suppress the flag.
        if typescript_project_dir(dir.path()).is_some() {
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
        // The reason is stated once, in partialReasons.
        assert_eq!(row["partialReasons"], json!([REASON]));
        assert!(row.pointer("/coverage/reason").is_none(), "{row}");
        assert!(
            row["warnings"][0]
                .as_str()
                .is_some_and(|w| w.contains("tsconfig.json")),
            "{row}"
        );
        let next = row["related"]
            .as_array()
            .unwrap()
            .iter()
            .find(|call| call["tool"] == "localSearch")
            .expect("text fallback");
        assert_eq!(next["kind"], "related");
        assert_eq!(next["input"]["queries"][0]["path"], root.as_str());
        assert_eq!(
            next["input"]["queries"][0]["matchString"],
            json!([octocode_engine::text::word_pattern("greet")])
        );
        crate::contracts::validate("localSearch", next["input"].clone())
            .expect("complete fallback");

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
            "operation":"definition",
            "path":format!("file://{}", file.display()),"symbolName":"greet","line":1
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
}
