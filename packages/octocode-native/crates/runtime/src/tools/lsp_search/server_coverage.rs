//! Coverage signals for servers whose incoming-direction answers are
//! structurally incomplete even inside a configured project.
//!
//! A Python server resolves a call only through a receiver whose type it
//! infers, so calls through untyped or dynamic receivers are missing:
//! reason `dynamicDispatch`. rust-analyzer analyzes one feature set, so code
//! under disabled `cfg(feature)` gates is compiled out: reason
//! `cfgGatedFiles` when files that mention the name carry such gates, with
//! a `next.allFeatures` rerun. Both rows get the lexical `textSearch` lead.

use super::LspSearchQuery;
use super::failure::flag_partial;
use super::inferred_project::is_incoming;
use crate::tools::id::ToolId;
use serde_json::{Value, json};
use std::path::Path;

pub(super) const DYNAMIC_REASON: &str = "dynamicDispatch";
pub(super) const CFG_REASON: &str = "cfgGatedFiles";
const PYTHON_WARNING: &str = "The Python language server resolves a use only through a receiver whose type it infers; uses through untyped or dynamic receivers (managers, mixins, getattr, duck typing) are missing. Confirm with hints.textSearch.";
/// Bound of the Rust gate scan; a larger workspace counts what it reached.
const MAX_SCANNED_FILES: usize = 20_000;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub(super) fn annotate(
    row: &mut Value,
    query: &LspSearchQuery,
    language_id: Option<&str>,
    workspace_root: &str,
) {
    if !is_incoming(&query.operation())
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
        || row.pointer("/payload/coverage/reason").is_some()
    {
        return;
    }
    match language_id {
        Some("python") => flag_partial(row, query, DYNAMIC_REASON, PYTHON_WARNING, workspace_root),
        Some("rust") => annotate_rust(row, query, workspace_root),
        _ => {}
    }
}

fn annotate_rust(row: &mut Value, query: &LspSearchQuery, workspace_root: &str) {
    let context = query.rust_context().unwrap_or_else(|| json!({}));
    if context["features"] == "all" {
        return;
    }
    let Some(name) = query.symbol_name().filter(|name| !name.trim().is_empty()) else {
        return;
    };
    let gated = gated_files_mentioning(Path::new(workspace_root), name);
    if gated == 0 {
        return;
    }
    let warning = format!(
        "{gated} Rust files that mention `{name}` carry cfg(feature …) gates; rust-analyzer analyzed one feature set, so code under disabled features is missing. Rerun with next.allFeatures (rustContext.features:\"all\") or confirm with hints.textSearch."
    );
    flag_partial(row, query, CFG_REASON, &warning, workspace_root);
    let mut rerun = query.to_row();
    if let Some(fields) = rerun.as_object_mut() {
        fields
            .retain(|key, value| !value.is_null() && !matches!(key.as_str(), "page" | "snapshot"));
    }
    let mut context = context;
    context["features"] = json!("all");
    rerun["rustContext"] = context;
    row["next"]["allFeatures"] = crate::tools::result::Continuation::new(ToolId::LspSearch, rerun)
        .why("Analyze every Cargo feature so feature-gated uses are compiled in.")
        .confidence("medium")
        .build();
}

/// Rust files under `root` (ignore-aware) that spell `name` as a word and
/// contain a `cfg(feature` gate.
fn gated_files_mentioning(root: &Path, name: &str) -> usize {
    let walker = ignore::WalkBuilder::new(root)
        .filter_entry(|entry| entry.file_name() != "target")
        .build();
    walker
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_some_and(|kind| kind.is_file())
                && entry.path().extension().is_some_and(|ext| ext == "rs")
                && entry
                    .metadata()
                    .is_ok_and(|meta| meta.len() <= MAX_FILE_BYTES)
        })
        .take(MAX_SCANNED_FILES)
        .filter(|entry| {
            std::fs::read_to_string(entry.path())
                .is_ok_and(|text| has_feature_gate(&text) && mentions_word(&text, name))
        })
        .count()
}

fn has_feature_gate(text: &str) -> bool {
    text.match_indices("cfg").any(|(at, _)| {
        let rest = &text[at + 3..];
        rest.trim_start().starts_with('(')
            && rest
                .split(')')
                .next()
                .is_some_and(|gate| gate.contains("feature"))
    })
}

fn mentions_word(text: &str, name: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(is_ident)
            && !text[at + name.len()..].chars().next().is_some_and(is_ident)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(value: Value) -> LspSearchQuery {
        serde_json::from_value(value).expect("query")
    }

    #[test]
    fn python_incoming_rows_name_dynamic_dispatch_and_offer_text_search() {
        let mut row = json!({"payload":{"kind":"callers","matches":[{"path":"a.py"}]}});
        let q = query(
            json!({"path":"/w/a.py","operation":"callers","symbolName":"get_or_create","lineHint":3}),
        );
        annotate(&mut row, &q, Some("python"), "/w");
        assert_eq!(
            row["payload"]["coverage"]["reason"], DYNAMIC_REASON,
            "{row}"
        );
        assert_eq!(row["isPartial"], true, "{row}");
        assert_eq!(row["next"]["textSearch"]["tool"], "localSearch", "{row}");
        // Outgoing questions are not affected.
        let mut definition = json!({"payload":{"kind":"definition","matches":[]}});
        let q =
            query(json!({"path":"/w/a.py","operation":"definition","symbolName":"x","lineHint":3}));
        annotate(&mut definition, &q, Some("python"), "/w");
        assert!(definition.get("isPartial").is_none(), "{definition}");
    }

    #[test]
    fn rust_rows_flag_feature_gated_mentions_and_offer_an_all_features_rerun() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("lib.rs"), "pub fn spawn_blocking() {}\n").expect("lib");
        std::fs::write(
            dir.path().join("gated.rs"),
            "#![cfg(feature = \"full\")]\nfn t() { spawn_blocking(); }\n",
        )
        .expect("gated");
        std::fs::write(
            dir.path().join("other.rs"),
            "#[cfg(feature = \"x\")]\nfn spawn_blocking_extra() {}\n",
        )
        .expect("other");
        let root = dir.path().to_string_lossy().into_owned();
        let row_query = json!({"path":format!("{root}/lib.rs"),"operation":"references","symbolName":"spawn_blocking","lineHint":1});
        let mut row = json!({"payload":{"kind":"references","matches":[{"path":"lib.rs"}]}});
        annotate(&mut row, &query(row_query.clone()), Some("rust"), &root);
        assert_eq!(row["payload"]["coverage"]["reason"], CFG_REASON, "{row}");
        assert!(
            row["warnings"][0]
                .as_str()
                .is_some_and(|w| w.starts_with("1 Rust files")),
            "{row}"
        );
        let rerun = &row["next"]["allFeatures"]["query"]["queries"][0];
        assert_eq!(rerun["rustContext"]["features"], "all", "{row}");
        assert_eq!(rerun["symbolName"], "spawn_blocking", "{row}");
        // An all-features run is not flagged again.
        let mut all = json!({"payload":{"kind":"references","matches":[]}});
        let mut all_query = row_query;
        all_query["rustContext"] = json!({"features":"all"});
        annotate(&mut all, &query(all_query), Some("rust"), &root);
        assert!(all.get("isPartial").is_none(), "{all}");
    }

    #[test]
    fn gate_and_word_detection() {
        assert!(has_feature_gate("#[cfg(all(test, feature = \"x\"))]"));
        assert!(has_feature_gate("#![cfg(feature=\"full\")]"));
        assert!(!has_feature_gate("#[cfg(test)]\nlet feature = 1;"));
        assert!(mentions_word("a(spawn_blocking)", "spawn_blocking"));
        assert!(!mentions_word("spawn_blocking_extra", "spawn_blocking"));
    }
}
