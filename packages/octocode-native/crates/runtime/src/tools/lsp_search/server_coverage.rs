//! Coverage signals where the protocol or server cannot establish
//! project-wide completeness.
//!
//! A Python server resolves a call only through a receiver whose type it
//! infers, so calls through untyped or dynamic receivers are missing:
//! reason `dynamicDispatch`, when files that spell the name are missing
//! from the answer. In a package that `include!`s source rust-analyzer does
//! not load, uses through the types it defines are missing: reason
//! `buildScriptsOff` (build-script output, loaded by a rerun) or
//! `includeOutsidePackage` (a file outside the package), on the same
//! file-level evidence. rust-analyzer analyzes one feature set, so code
//! under disabled `cfg(feature)` gates is compiled out: reason
//! `cfgGatedFiles` when files the answer did not cover use the name in code
//! and carry such gates, with an `expandFeatures` rerun page. An all-features run compiles out
//! `cfg(not(feature …))` code instead: reason `cfgNegatedFeatures`. Every
//! such row gets the lexical `findText` lead. Gate scans cover the server's
//! Cargo workspace: code outside it is not compiled, so no gate there hides
//! a use.

use super::failure::{
    FILE_LEVEL_REASONS, TEXT_CHECK, flag_partial, flagged_with, runs_build_scripts,
};
use super::inferred_project::is_incoming;
use super::scope::Scope;
use super::{LspSearchQuery, Op};
use crate::id::ToolId;
use crate::response::result::Confidence;
use crate::tools::lsp_search::lsp_search_query;
use serde_json::{Value, json};
use std::path::Path;

pub(super) const DYNAMIC_REASON: &str = "dynamicDispatch";
pub(super) const CFG_REASON: &str = "cfgGatedFiles";
pub(super) const CFG_NEGATED_REASON: &str = "cfgNegatedFeatures";
/// File-level: cleared when every file that uses the name is answered.
pub(super) const BUILD_SCRIPTS_REASON: &str = "buildScriptsOff";
/// File-level, like [`BUILD_SCRIPTS_REASON`]; no build context recovers it.
pub(super) const OUTSIDE_INCLUDE_REASON: &str = "includeOutsidePackage";
/// File-level: the anchor file is a module of several crates.
pub(super) const SHARED_MODULE_REASON: &str = "sharedModuleFile";
/// The server's inference limit is stated once in the tool description.
const PYTHON_WARNING: &str = "Files that spell the name are missing from the answer.";
/// Bound of the Rust gate scan; a larger workspace counts what it reached.
const MAX_SCANNED_FILES: usize = 20_000;
/// Largest file the gate scan reads; generated sources (a 2.4 MB
/// `tool_types.rs`) fit, so they never make every answer partial.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const GATE_SCAN_BUDGET: std::time::Duration = std::time::Duration::from_millis(250);

/// The lexical `findText` check, named by its search.
fn text_check() -> String {
    TEXT_CHECK.to_owned()
}

pub(super) fn annotate(
    row: &mut Value,
    query: &LspSearchQuery,
    language_id: Option<&str>,
    server: Server<'_>,
    scope: &Scope,
) {
    if !is_incoming(lsp_search_query::operation(query))
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
        || flagged_with(row, &FILE_LEVEL_REASONS)
    {
        return;
    }
    if language_id == Some("rust") {
        record_text_coverage(row, query, scope);
    }
    match language_id {
        Some("python") => flag_noted(row, query, DYNAMIC_REASON, PYTHON_WARNING.into(), scope),
        Some("rust") => {
            // A workspace symbol is a declaration the server found: no use
            // of it is missing.
            if lsp_search_query::op(query) != Op::WorkspaceSymbol {
                annotate_includes(row, query, server.anchor, scope);
            }
            annotate_rust(row, query, server.root, scope);
        }
        _ => {}
    }
}

/// Where the server answered: the anchor file and the server's workspace
/// root (for Rust, the Cargo workspace: the only code rust-analyzer
/// compiles, so the only code a cfg gate can hide).
#[derive(Clone, Copy)]
pub(super) struct Server<'a> {
    pub(super) anchor: &'a str,
    pub(super) root: &'a str,
}

/// Workspace-symbol arrays have no total, cursor, or completeness marker.
/// Native pagination covers the returned array; it cannot establish that
/// the provider returned every matching declaration in its project.
/// `capped_at` is the server's own result limit when its answer reached it
/// (only tsgo has one), which is said outright.
pub(super) fn workspace_symbols(row: &mut Value, anchor: Option<&str>, capped_at: Option<usize>) {
    if row.get("status").and_then(Value::as_str) == Some("error") {
        return;
    }
    // The reason states the protocol's missing completeness marker; only a
    // server cap this answer reached needs words.
    let mut warnings = Vec::new();
    if let Some(limit) = capped_at {
        warnings.push(format!(
            "tsgo returns at most {limit} workspace symbols; this list is capped by the server: narrow symbolName, or route .ts to typescript-language-server in lsp-servers.json."
        ));
    }
    crate::response::paging::mark_partial(row, "workspaceSymbolCoverageUnknown", &warnings);
    if let Some(path) = anchor {
        // This is an optional lead, moved to hints by continuation shaping;
        // it is not a page that recovers an exhaustive workspace result.
        row["next"]["inspectDocumentSymbols"] = crate::response::result::Continuation::new(
            ToolId::LspSearch,
            json!({"operation": "documentSymbols", "path": path}),
        )
        .confidence(Confidence::Medium)
        .build();
    }
}

/// rust-analyzer loads a package's own directory, and its build-script
/// output (`OUT_DIR`) only when `rustContext` runs build scripts. A type
/// defined in source the package `include!`s from elsewhere is unknown, so
/// uses that reach a symbol through it (a method called through `Deref` to
/// one, a field of one) do not resolve, and the server answers without
/// them. The row is partial when files that use the name in code are
/// missing from the answer, the lexical lead names them, and the reason
/// says which load is missing: `includeOutsidePackage` (no build context
/// loads it) or `buildScriptsOff` (with its rerun). The prose is static: a
/// notice, said once ([`flag_noted`]).
fn annotate_includes(row: &mut Value, query: &LspSearchQuery, anchor: &str, scope: &Scope) {
    let Some(includes) = package_includes(anchor) else {
        return;
    };
    if let Some(outside) = includes.outside.first() {
        // Named as rows name files: relative to the search scope.
        let shown = outside
            .strip_prefix(&scope.root)
            .unwrap_or(outside)
            .to_string_lossy()
            .into_owned();
        let note = format!(
            "rust-analyzer never loads {shown}{} (include!d from outside this package): uses through its types are missing under any rustContext; confirm with {}.",
            if includes.outside.len() > 1 {
                format!(" and {} more", includes.outside.len() - 1)
            } else {
                String::new()
            },
            text_check()
        );
        flag_noted(row, query, OUTSIDE_INCLUDE_REASON, note, scope);
    }
    // Several files declare the anchor as a module (`#[path]` from a bench
    // and a test): each crate compiles its own copy, and rust-analyzer
    // resolves the anchor in one, so uses of the others' copies are not
    // references of this symbol.
    if includes.declared_by.len() > 1 {
        let shown = |path: &Path| {
            path.strip_prefix(&scope.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned()
        };
        let note = format!(
            "{} is declared as a module by {} files ({}), so each compiles its own copy: rust-analyzer answers for one copy, and uses through the others are missing; confirm with {}.",
            shown(Path::new(anchor)),
            includes.declared_by.len(),
            includes
                .declared_by
                .iter()
                .map(|path| shown(path))
                .collect::<Vec<_>>()
                .join(", "),
            text_check()
        );
        flag_noted(row, query, SHARED_MODULE_REASON, note, scope);
    }
    if includes.generated.is_empty() || runs_build_scripts(query) {
        return;
    }
    // Which load hides a missed file is not known: when the outside include
    // may hide it too, the rerun (~2 GB) can recover nothing, so the text
    // check comes first and the rerun is no lead.
    let outside = lists_reason(row, OUTSIDE_INCLUDE_REASON);
    let note = if outside {
        format!(
            "rust-analyzer also did not run this package's build scripts; confirm the missing uses with {} first: a rustContext {{buildScripts:true}} rerun (a separate rust-analyzer, ~2 GB) loads only the generated types, never the outside include.",
            text_check()
        )
    } else {
        format!(
            "rust-analyzer did not run this package's build scripts: uses through the types they generate are missing; rerun with rustContext {{buildScripts:true}} (a separate rust-analyzer, ~2 GB) or confirm with {}.",
            text_check()
        )
    };
    flag_noted(row, query, BUILD_SCRIPTS_REASON, note, scope);
    // The rerun repeats the whole walk: offered once, on its first page,
    // and only where the reason holds alone.
    if !continues_a_page(query) && !outside && lists_reason(row, BUILD_SCRIPTS_REASON) {
        row["next"]["includeBuildScripts"] = super::failure::build_scripts_rerun(query);
    }
}

/// State the text check on every Rust incoming row, whatever its crate:
/// `coverage.textOnlyFiles` (files that use the name in code but are not in
/// the answer), so a checked, complete row reads differently from an
/// unchecked one. A workspace-symbol row lists declarations, not uses.
fn record_text_coverage(row: &mut Value, query: &LspSearchQuery, scope: &Scope) {
    if lsp_search_query::op(query) == Op::WorkspaceSymbol {
        return;
    }
    let Some(files) = lsp_search_query::symbol_name(query)
        .filter(|name| !name.trim().is_empty())
        .and_then(|name| scope.text_only_files(name))
    else {
        return;
    };
    if let Some(payload) = row.as_object_mut() {
        payload.entry("coverage").or_insert_with(|| json!({}))["textOnlyFiles"] =
            json!(files.len());
    }
}

/// [`flag_partial`] with `note`, prose that reads the same on every page and
/// row of the answer: a static notice, said once, on the first page
/// ([`crate::response::paging::push_notice`]), and only when the row is flagged.
fn flag_noted(row: &mut Value, query: &LspSearchQuery, reason: &str, note: String, scope: &Scope) {
    flag_partial(row, query, reason, None, scope);
    if lists_reason(row, reason) {
        crate::response::paging::push_notice(row, note);
    }
}

/// Whether `query` continues an earlier page of its answer
/// ([`crate::response::paging::continues_a_page`]): a rerun lead is offered on
/// the first page only.
fn continues_a_page(query: &LspSearchQuery) -> bool {
    crate::response::paging::continues_a_page(&lsp_search_query::to_row(query))
}

/// Whether `row` is flagged with `reason`.
fn lists_reason(row: &Value, reason: &str) -> bool {
    row.pointer("/coverage/reason").and_then(Value::as_str) == Some(reason)
        || row
            .get("partialReasons")
            .and_then(Value::as_array)
            .is_some_and(|listed| listed.iter().any(|listed| listed == reason))
}

/// Bound of a package's include scan; a larger package reports what the
/// scanned files include.
const MAX_INCLUDE_SCAN_FILES: usize = 5_000;

/// The includes of the Cargo package `anchor` belongs to.
pub(super) fn package_includes(
    anchor: &str,
) -> Option<octocode_engine::lsp::workspace::PackageIncludes> {
    octocode_engine::lsp::workspace::cargo_package_includes(
        Path::new(anchor),
        MAX_INCLUDE_SCAN_FILES,
    )
}

fn annotate_rust(row: &mut Value, query: &LspSearchQuery, server_root: &str, scope: &Scope) {
    let context = lsp_search_query::rust_context(query).unwrap_or_else(|| json!({}));
    let Some(name) = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
    else {
        return;
    };
    if context["features"] == "all" {
        let negated = gated_files_mentioning(scope, server_root, name, Gate::NegatedFeature);
        if negated.matches > 0 || negated.incomplete() {
            let warning = format!(
                "{} Rust files that mention `{name}` carry cfg(not(feature …)) gates; an all-features build compiles that code out. {}Compare with the default-features run, or confirm with {}.",
                negated.matches,
                negated.incomplete_warning(),
                text_check()
            );
            flag_partial(row, query, CFG_NEGATED_REASON, Some(&warning), scope);
        }
        return;
    }
    let gated = gated_files_mentioning(scope, server_root, name, Gate::Feature);
    if gated.matches == 0 && !gated.incomplete() {
        return;
    }
    // Gated files found: the rerun page and the partial reason say it all.
    // Only an incomplete scan that found none needs the prose.
    if gated.matches > 0 {
        flag_partial(row, query, CFG_REASON, None, scope);
    } else {
        let warning = format!(
            "{}Uses under features rust-analyzer left off may be missing: {}.",
            gated.incomplete_warning(),
            text_check()
        );
        flag_partial(row, query, CFG_REASON, Some(&warning), scope);
        return;
    }
    // The rerun repeats the whole walk under every feature: offered once,
    // on the walk's first page.
    if continues_a_page(query) {
        return;
    }
    let mut rerun = lsp_search_query::to_row(query);
    if let Some(fields) = rerun.as_object_mut() {
        fields
            .retain(|key, value| !value.is_null() && !matches!(key.as_str(), "page" | "snapshot"));
    }
    let mut context = context;
    context["features"] = json!("all");
    rerun["rustContext"] = context;
    row["next"]["expandFeatures"] =
        crate::response::result::Continuation::new(ToolId::LspSearch, rerun)
            .confidence(Confidence::Medium)
            .build();
}

/// Which `cfg` gate a scan looks for.
#[derive(Clone, Copy, Debug)]
enum Gate {
    /// `cfg(… feature …)`: compiled out under the default features.
    Feature,
    /// `cfg(not(feature …))`: compiled out under all features.
    NegatedFeature,
}

impl Gate {
    fn carried_by(self, text: &str) -> bool {
        match self {
            Self::Feature => has_feature_gate(text),
            Self::NegatedFeature => has_negated_feature_gate(text),
        }
    }
}

/// Rust files under the scope that use a name in code (not only in a
/// comment or string) and carry a gate, and whether the scan was complete.
#[derive(Clone, Debug, Default)]
struct GateScan {
    /// Canonical paths.
    files: Vec<String>,
    has_more: bool,
    unreadable: usize,
}

/// Gate scans by `(root, name, gate, scope fingerprint)`: the fingerprint
/// covers every Rust file under the root, so an unchanged one proves the
/// same files. Later pages of a walk reuse their first page's scan.
static GATE_SCANS: octocode_io::memo::Memo<String, GateScan> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 256,
        max_weight: 256,
        keep_newest: false,
    });

/// What a gate check counts: gated files the server's answer did not
/// cover (an answered file was analyzed), and whether the scan finished.
struct GateCount {
    matches: usize,
    scan: GateScan,
}

impl GateCount {
    fn incomplete(&self) -> bool {
        self.scan.has_more || self.scan.unreadable > 0
    }

    fn incomplete_warning(&self) -> String {
        if self.incomplete() {
            format!(
                "The gate scan was incomplete (unvisited files: {}, unreadable or oversized files: {}); the count is a lower bound. ",
                if self.scan.has_more {
                    "at least one"
                } else {
                    "none"
                },
                self.scan.unreadable
            )
        } else {
            String::new()
        }
    }
}

/// Gated files under `root` (the server's workspace) that use `name`. The
/// scope fingerprint covers every Rust file under the scope, which holds
/// `root`, so it keys the memo.
fn gated_files_mentioning(scope: &Scope, root: &str, name: &str, gate: Gate) -> GateCount {
    let key = scope.fingerprint().map(|fingerprint| {
        octocode_engine::digest::sha256(
            format!("{root}\u{0}{name}\u{0}{gate:?}\u{0}{fingerprint}").as_bytes(),
        )
    });
    let scan = match key.as_ref().and_then(|key| GATE_SCANS.get(key)) {
        Some(hit) => (*hit.value).clone(),
        None => {
            let scan = scan_gates(Path::new(root), name, gate, MAX_SCANNED_FILES);
            if let Some(key) = key {
                GATE_SCANS.insert(key, scan.clone(), 1, None);
            }
            scan
        }
    };
    GateCount {
        matches: scan
            .files
            .iter()
            .filter(|file| !scope.is_answered(file))
            .count(),
        scan,
    }
}

fn scan_gates(root: &Path, name: &str, gate: Gate, limit: usize) -> GateScan {
    let walker = crate::tools::pruned_walk(root, crate::tools::syntax_prune([])).build();
    let mut scan = GateScan::default();
    let mut visited = 0;
    let started = std::time::Instant::now();
    for entry in walker {
        if started.elapsed() >= GATE_SCAN_BUDGET {
            scan.has_more = true;
            break;
        }
        let Ok(entry) = entry else {
            scan.unreadable += 1;
            continue;
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file())
            || entry.path().extension().is_none_or(|ext| ext != "rs")
        {
            continue;
        }
        if visited >= limit {
            scan.has_more = true;
            break;
        }
        visited += 1;
        // Only a file past the read cap leaves the scan incomplete.
        if !entry
            .metadata()
            .is_ok_and(|meta| meta.len() <= MAX_FILE_BYTES)
        {
            scan.unreadable += 1;
            continue;
        }
        match crate::tools::source::read_bounded(entry.path(), MAX_FILE_BYTES as usize)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
        {
            Some(text)
                if gate.carried_by(&text)
                    && mentions_word(&text, name)
                    && uses_in_code(entry.path(), &text, name) =>
            {
                scan.files
                    .push(super::scope::canonical(&entry.path().to_string_lossy()));
            }
            Some(_) => {}
            None => scan.unreadable += 1,
        }
    }
    scan
}

/// Whether `text` uses `name` in code, not only in comments or strings; a
/// file the grammar cannot parse cleanly counts its word mentions.
fn uses_in_code(path: &Path, text: &str, name: &str) -> bool {
    octocode_engine::lsp::resolver::code_occurrence_lines(&path.to_string_lossy(), text, name)
        .is_none_or(|lines| !lines.is_empty())
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

/// A `cfg(not(feature …))` gate (also nested, `cfg(all(not(feature …)))`).
fn has_negated_feature_gate(text: &str) -> bool {
    text.match_indices("cfg").any(|(at, _)| {
        let rest = text[at + 3..].trim_start();
        rest.starts_with('(')
            && rest
                .split(']')
                .next()
                .is_some_and(|gate| gate.replace(' ', "").contains("not(feature"))
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

    #[test]
    fn workspace_coverage_preserves_errors_and_avoids_a_directory_document_hint() {
        let mut stale = json!({"status":"error","errorCode":"staleSnapshot"});
        let before = stale.clone();
        workspace_symbols(&mut stale, Some("/w/a.ts"), None);
        assert_eq!(stale, before);
        let mut empty = json!({"kind":"empty"});
        workspace_symbols(&mut empty, None, None);
        assert_eq!(empty["isPartial"], true);
        assert_eq!(
            empty["partialReasons"],
            json!(["workspaceSymbolCoverageUnknown"])
        );
        assert!(empty.get("next").is_none(), "a directory is not a document");
        assert!(empty.get("terminalLimit").is_none());
    }

    #[test]
    fn gate_scan_reports_unvisited_files_after_its_cap() {
        let root = tempfile::tempdir().expect("root");
        std::fs::write(root.path().join("a.rs"), "pub fn other() {}\n").expect("first");
        std::fs::write(
            root.path().join("z.rs"),
            "#[cfg(feature = \"extra\")] pub fn wanted() {}\n",
        )
        .expect("second");
        let scan = scan_gates(root.path(), "wanted", Gate::Feature, 1);
        assert!(scan.files.is_empty());
        assert!(scan.has_more);
    }

    /// A gated file counts only when it uses the name in code and the
    /// server's answer did not cover it: a doc link or a comment is no use,
    /// and an answered file was analyzed.
    #[test]
    fn gate_scan_ignores_comment_mentions_and_answered_files() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().canonicalize().expect("root");
        std::fs::write(
            root.join("doc.rs"),
            "//! See [`Continuation`].\n#[cfg(feature = \"x\")]\nfn other() {}\n",
        )
        .expect("doc");
        let used = root.join("used.rs");
        std::fs::write(&used, "#[cfg(feature = \"x\")]\nfn f(c: Continuation) {}\n").expect("used");
        let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.rs".into()]);
        let count = gated_files_mentioning(&scope, &scope.root, "Continuation", Gate::Feature);
        assert_eq!(count.scan.files, vec![used.to_string_lossy().into_owned()]);
        assert_eq!(count.matches, 1);
        scope.answer([used.to_string_lossy()]);
        assert_eq!(
            gated_files_mentioning(&scope, &scope.root, "Continuation", Gate::Feature).matches,
            0
        );
    }

    fn query(value: Value) -> LspSearchQuery {
        serde_json::from_value(value).expect("query")
    }

    #[test]
    fn python_incoming_rows_name_dynamic_dispatch_and_offer_text_search() {
        let mut row = json!({"kind":"callers","matches":[{"path":"a.py"}]});
        let q = query(
            json!({"path":"/w/a.py","operation":"callers","symbolName":"get_or_create","line":3}),
        );
        let scope = Scope::new("/w".into(), vec!["*.py".into()]);
        annotate(
            &mut row,
            &q,
            Some("python"),
            Server {
                anchor: "/w/a.py",
                root: "/w",
            },
            &scope,
        );
        // The reason is stated once, in partialReasons.
        assert_eq!(row["partialReasons"], json!([DYNAMIC_REASON]), "{row}");
        assert!(row["coverage"].get("reason").is_none(), "{row}");
        assert_eq!(row["isPartial"], true, "{row}");
        // The inference limit reads the same on every page: a notice.
        assert_eq!(row["notices"], json!([PYTHON_WARNING]), "{row}");
        assert!(
            !PYTHON_WARNING.contains("hints.findText"),
            "{PYTHON_WARNING}"
        );
        assert!(row.get("warnings").is_none(), "{row}");
        assert_eq!(row["next"]["findText"]["tool"], "localSearch", "{row}");
        // Outgoing questions are not affected.
        let mut definition = json!({"kind":"definition","matches":[]});
        let q = query(json!({"path":"/w/a.py","operation":"definition","symbolName":"x","line":3}));
        annotate(
            &mut definition,
            &q,
            Some("python"),
            Server {
                anchor: "/w/a.py",
                root: "/w",
            },
            &scope,
        );
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
        let scope = Scope::new(root.clone(), vec!["*.rs".into()]);
        let row_query = json!({"path":format!("{root}/lib.rs"),"operation":"references","symbolName":"spawn_blocking","line":1});
        let mut row = json!({"kind":"references","matches":[{"path":"lib.rs"}]});
        annotate(
            &mut row,
            &query(row_query.clone()),
            Some("rust"),
            Server {
                anchor: &format!("{root}/lib.rs"),
                root: &root,
            },
            &scope,
        );
        // Gated files found: the rerun page and one reason, no prose.
        assert_eq!(row["partialReasons"], json!([CFG_REASON]), "{row}");
        assert!(
            row.get("coverage")
                .is_none_or(|c| c.get("reason").is_none()),
            "{row}"
        );
        assert!(row.get("warnings").is_none(), "{row}");
        let rerun = &row["next"]["expandFeatures"]["query"]["queries"][0];
        assert_eq!(rerun["rustContext"]["features"], "all", "{row}");
        assert_eq!(rerun["symbolName"], "spawn_blocking", "{row}");
        // A later page of the same walk keeps the reason; the rerun lead
        // was offered once, on the first page.
        for later in [
            json!({"page": 2, "snapshot": "lsp-v3:abc"}),
            json!({"importerPage": 2, "snapshot": "lsp-v3:abc"}),
        ] {
            let mut later_query = row_query.clone();
            later_query
                .as_object_mut()
                .expect("query")
                .extend(later.as_object().expect("fields").clone());
            let mut page = json!({"kind":"references","matches":[{"path":"lib.rs"}]});
            annotate(
                &mut page,
                &query(later_query),
                Some("rust"),
                Server {
                    anchor: &format!("{root}/lib.rs"),
                    root: &root,
                },
                &scope,
            );
            assert_eq!(page["partialReasons"], json!([CFG_REASON]), "{page}");
            assert!(page.pointer("/next/expandFeatures").is_none(), "{page}");
        }
        // An all-features run is not flagged again.
        let mut all = json!({"kind":"references","matches":[]});
        let mut all_query = row_query;
        all_query["rustContext"] = json!({"features":"all"});
        annotate(
            &mut all,
            &query(all_query),
            Some("rust"),
            Server {
                anchor: &format!("{root}/lib.rs"),
                root: &root,
            },
            &scope,
        );
        assert!(all.get("isPartial").is_none(), "{all}");
    }

    /// A Rust row of `symbol` in a package at `root/pkg` whose `src/lib.rs`
    /// is `lib`, after the server answered `answered` (files under `pkg/src`).
    async fn rust_row(root: &Path, lib: &str, query_fields: Value, answered: &[&str]) -> Value {
        let package = root.join("pkg");
        std::fs::write(package.join("src/lib.rs"), lib).expect("lib");
        let lib_path = package.join("src/lib.rs").to_string_lossy().into_owned();
        let root_text = root.to_string_lossy().into_owned();
        let policy = crate::tools::test_support::workspace_policy(root);
        let scope = Scope::new(root_text.clone(), vec!["*.rs".into()]);
        scope
            .text_files(
                "include_diff",
                &policy,
                &crate::execution::cancel::NeverCancel,
            )
            .await
            .expect("scan");
        scope.answer(answered.iter().map(|file| {
            package
                .join("src")
                .join(file)
                .to_string_lossy()
                .into_owned()
        }));
        let mut q = json!({"path": lib_path, "operation": "references",
            "symbolName": "include_diff", "line": 1});
        for (key, value) in query_fields.as_object().into_iter().flatten() {
            q[key] = value.clone();
        }
        if q["operation"] == "workspaceSymbol" {
            q.as_object_mut().expect("query").remove("line");
        }
        let mut row = json!({"kind":"references","matches":[{"path":"pkg/src/lib.rs"}]});
        annotate(
            &mut row,
            &query(q),
            Some("rust"),
            Server {
                anchor: &lib_path,
                root: &root_text,
            },
            &scope,
        );
        row
    }

    /// A use the answer misses inside the anchor file itself (an answered
    /// file, so no file count shows it) is still read by the lexical lead;
    /// a fully answered anchor file is not.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_text_lead_reads_misses_inside_the_anchor_file() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().canonicalize().expect("root");
        let package = root.join("pkg");
        std::fs::create_dir_all(package.join("src")).expect("src");
        std::fs::write(package.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
        std::fs::write(
            package.join("src/user.rs"),
            "fn u(q: &R) { q.include_diff(); }\n",
        )
        .expect("user");
        let lib = package.join("src/lib.rs");
        std::fs::write(
            &lib,
            "include!(\"../../contract/types.rs\");\nimpl R { fn include_diff(&self) -> bool { true } }\nfn t(r: &R) -> bool { r.include_diff() }\n",
        )
        .expect("lib");
        let lib_path = lib.to_string_lossy().into_owned();
        let user_path = package.join("src/user.rs").to_string_lossy().into_owned();
        let policy = crate::tools::test_support::workspace_policy(&root);
        let root_text = root.to_string_lossy().into_owned();
        let lead_paths = |covered: Vec<u32>| {
            let scope = Scope::new(root_text.clone(), vec!["*.rs".into()]);
            let (lib_path, user_path, root_text, policy) = (
                lib_path.clone(),
                user_path.clone(),
                root_text.clone(),
                &policy,
            );
            async move {
                scope
                    .text_files(
                        "include_diff",
                        policy,
                        &crate::execution::cancel::NeverCancel,
                    )
                    .await
                    .expect("scan");
                scope.answer([lib_path.clone()]);
                scope.answer_anchor_lines(&lib_path, 1, covered);
                let mut row = json!({"kind":"references","matches":[{"path":"pkg/src/lib.rs"}]});
                annotate(
                    &mut row,
                    &query(json!({"path": lib_path, "operation": "references",
                        "symbolName": "include_diff", "line": 2})),
                    Some("rust"),
                    Server {
                        anchor: &lib_path,
                        root: &root_text,
                    },
                    &scope,
                );
                assert_eq!(row["coverage"]["textOnlyFiles"], 1, "{row}");
                let mut paths = row["next"]["findText"]["query"]["queries"]
                    .as_array()
                    .expect("lead rows")
                    .iter()
                    .map(|search| search["path"].as_str().expect("path").to_owned())
                    .collect::<Vec<_>>();
                paths.sort();
                assert!(paths.contains(&user_path), "{row}");
                paths
            }
        };
        // Line 3 (zero-based 2) uses the name; the answer lists only the
        // declaration.
        let missed = lead_paths(vec![1]).await;
        assert_eq!(missed.len(), 2, "{missed:?}");
        assert!(missed.contains(&lib_path), "{missed:?}");
        let covered = lead_paths(vec![1, 2]).await;
        assert_eq!(covered, std::slice::from_ref(&user_path));
    }

    /// A support file a bench and a test both declare by `#[path]` is
    /// compiled into each: rust-analyzer answers for one copy, so an answer
    /// that misses the other declaring file is partial, names both, and
    /// leads to the text check; an answer covering both is complete.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shared_module_files_flag_the_unanswered_copy() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().canonicalize().expect("root");
        let package = root.join("pkg");
        for sub in ["benches/support", "tests", "src"] {
            std::fs::create_dir_all(package.join(sub)).expect("dir");
        }
        std::fs::write(package.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
        std::fs::write(package.join("src/lib.rs"), "pub fn other() {}\n").expect("lib");
        let support = package.join("benches/support/frozen.rs");
        std::fs::write(&support, "pub fn frozen_graph() {}\n").expect("support");
        let bench = package.join("benches/graph.rs");
        std::fs::write(
            &bench,
            "#[path = \"support/frozen.rs\"]\nmod frozen;\nfn main() { frozen::frozen_graph(); }\n",
        )
        .expect("bench");
        let test = package.join("tests/graph.rs");
        std::fs::write(
            &test,
            "#[path = \"../benches/support/frozen.rs\"]\nmod frozen;\n#[test]\nfn t() { frozen::frozen_graph(); }\n",
        )
        .expect("test");
        let path = |file: &Path| file.to_string_lossy().into_owned();
        let (support_path, root_text) = (path(&support), path(&root));
        let policy = crate::tools::test_support::workspace_policy(&root);
        let row_for = |answered: Vec<String>| {
            let scope = Scope::new(root_text.clone(), vec!["*.rs".into()]);
            let (support_path, root_text, policy) =
                (support_path.clone(), root_text.clone(), &policy);
            async move {
                scope
                    .text_files(
                        "frozen_graph",
                        policy,
                        &crate::execution::cancel::NeverCancel,
                    )
                    .await
                    .expect("scan");
                scope.answer(answered);
                let mut row =
                    json!({"kind":"references","matches":[{"path":"pkg/tests/graph.rs"}]});
                annotate(
                    &mut row,
                    &query(json!({"path": support_path, "operation": "references",
                        "symbolName": "frozen_graph", "line": 1})),
                    Some("rust"),
                    Server {
                        anchor: &support_path,
                        root: &root_text,
                    },
                    &scope,
                );
                row
            }
        };
        let missed = row_for(vec![support_path.clone(), path(&test)]).await;
        assert_eq!(
            missed["partialReasons"],
            json!([SHARED_MODULE_REASON]),
            "{missed}"
        );
        assert_eq!(missed["coverage"]["textOnlyFiles"], 1, "{missed}");
        assert!(
            missed["notices"][0].as_str().is_some_and(|notice| {
                notice.contains("pkg/benches/graph.rs") && notice.contains("pkg/tests/graph.rs")
            }),
            "{missed}"
        );
        assert!(missed["next"].get("findText").is_some(), "{missed}");
        let complete = row_for(vec![support_path.clone(), path(&test), path(&bench)]).await;
        assert!(complete.get("isPartial").is_none(), "{complete}");
        assert_eq!(complete["coverage"]["textOnlyFiles"], 0, "{complete}");
    }

    /// A method reached through `Deref` to a type the package `include!`s
    /// does not resolve while rust-analyzer cannot load that source: the
    /// server answers with the declaration alone. Files that use the name in
    /// code and are missing from that answer make the row partial; the
    /// reason says which load is missing, the build-script rerun is offered
    /// only where it helps and only on a partial first page, and the static
    /// prose is stated once. A comment mention is no missed use.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unloaded_includes_flag_unanswered_files() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().canonicalize().expect("root");
        let package = root.join("pkg");
        std::fs::create_dir_all(package.join("src")).expect("src");
        std::fs::create_dir_all(root.join("contract")).expect("contract");
        std::fs::write(package.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
        std::fs::write(package.join("build.rs"), "fn main() {}\n").expect("build.rs");
        std::fs::write(
            package.join("src/user.rs"),
            "fn u(q: &R) { q.include_diff(); }\n",
        )
        .expect("user");
        std::fs::write(
            package.join("src/notes.rs"),
            "/// include_diff is documented here.\nfn n() { let _ = \"include_diff\"; }\n",
        )
        .expect("notes");
        let method = "impl R { fn include_diff(&self) -> bool { true } }\n";
        let generated = format!("include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\n{method}");
        let outside = format!("include!(\"../../contract/types.rs\");\n{method}");

        let first = rust_row(&root, &generated, json!({}), &["lib.rs"]).await;
        assert_eq!(
            first["partialReasons"],
            json!([BUILD_SCRIPTS_REASON]),
            "{first}"
        );
        assert!(
            first["notices"][0]
                .as_str()
                .is_some_and(|notice| notice.contains("buildScripts:true")),
            "{first}"
        );
        assert!(first.get("warnings").is_none(), "{first}");
        assert_eq!(
            first["coverage"]["textOnlyFiles"], 1,
            "comments and strings are no uses: {first}"
        );
        let rerun = &first["next"]["includeBuildScripts"]["query"]["queries"][0];
        assert_eq!(
            rerun["rustContext"],
            json!({"buildScripts": true}),
            "{first}"
        );
        assert_eq!(rerun["symbolName"], "include_diff", "{first}");

        let later_fields = json!({"page": 2, "pageSize": 10});
        let later = rust_row(&root, &generated, later_fields.clone(), &["lib.rs"]).await;
        assert_eq!(
            later["partialReasons"],
            json!([BUILD_SCRIPTS_REASON]),
            "{later}"
        );
        // The response stage says a notice on the first page only.
        let shaped = crate::response::rows::result_row(
            ToolId::LspSearch,
            0,
            &later_fields,
            later.clone(),
            None,
        );
        assert!(
            shaped["data"].get("notices").is_none() && shaped["data"].get("warnings").is_none(),
            "stated once, on page 1: {shaped}"
        );
        assert!(
            later["next"].get("includeBuildScripts").is_none(),
            "{later}"
        );

        let complete = rust_row(&root, &generated, json!({}), &["lib.rs", "user.rs"]).await;
        assert!(complete.get("isPartial").is_none(), "{complete}");
        assert!(
            complete.get("next").is_none(),
            "no rerun lead on a complete row: {complete}"
        );
        assert_eq!(complete["coverage"]["textOnlyFiles"], 0, "{complete}");

        let ran = rust_row(
            &root,
            &generated,
            json!({"rustContext": {"buildScripts": true}}),
            &["lib.rs"],
        )
        .await;
        assert!(ran.get("isPartial").is_none(), "{ran}");
        assert_eq!(ran["coverage"]["textOnlyFiles"], 1, "{ran}");

        // No build context loads a file outside the package: the rerun
        // stays partial, with the lexical lead and no rerun of its own.
        let outside_row = rust_row(
            &root,
            &outside,
            json!({"rustContext": {"buildScripts": true}}),
            &["lib.rs"],
        )
        .await;
        assert_eq!(
            outside_row["partialReasons"],
            json!([OUTSIDE_INCLUDE_REASON]),
            "{outside_row}"
        );
        assert!(
            outside_row["notices"][0]
                .as_str()
                .is_some_and(|notice| notice.contains("contract/types.rs")),
            "{outside_row}"
        );
        assert!(
            outside_row["next"].get("findText").is_some(),
            "{outside_row}"
        );
        assert!(
            outside_row["next"].get("includeBuildScripts").is_none(),
            "{outside_row}"
        );

        // Both loads missing: which one hides a file is unknown, so the
        // text check leads and the ~2 GB rerun is named, not offered.
        let both = format!(
            "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\ninclude!(\"../../contract/types.rs\");\n{method}"
        );
        let both_row = rust_row(&root, &both, json!({}), &["lib.rs"]).await;
        assert_eq!(
            both_row["partialReasons"],
            json!([OUTSIDE_INCLUDE_REASON, BUILD_SCRIPTS_REASON]),
            "{both_row}"
        );
        assert!(both_row["next"].get("findText").is_some(), "{both_row}");
        assert!(
            both_row["next"].get("includeBuildScripts").is_none(),
            "{both_row}"
        );
        assert!(
            both_row["notices"][1]
                .as_str()
                .is_some_and(|notice| notice.contains("first")),
            "{both_row}"
        );

        // A workspace symbol is a found declaration: no use is missing.
        let symbols = rust_row(
            &root,
            &outside,
            json!({"operation": "workspaceSymbol"}),
            &["lib.rs"],
        )
        .await;
        assert!(symbols.get("isPartial").is_none(), "{symbols}");

        // A package whose includes all load: checked, complete, and says so.
        let plain = rust_row(&root, method, json!({}), &["lib.rs"]).await;
        assert!(plain.get("isPartial").is_none(), "{plain}");
        assert_eq!(plain["coverage"]["textOnlyFiles"], 1, "{plain}");
    }

    #[test]
    fn all_features_run_flags_negated_feature_gates() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("lib.rs"), "pub fn spawn_blocking() {}\n").expect("lib");
        std::fs::write(
            dir.path().join("fallback.rs"),
            "#[cfg(not(feature = \"rt\"))]\nfn t() { spawn_blocking(); }\n",
        )
        .expect("negated");
        let root = dir.path().to_string_lossy().into_owned();
        let scope = Scope::new(root.clone(), vec!["*.rs".into()]);
        let mut row = json!({"kind":"references","matches":[]});
        let q = query(
            json!({"path":format!("{root}/lib.rs"),"operation":"references",
            "symbolName":"spawn_blocking","line":1,"rustContext":{"features":"all"}}),
        );
        annotate(
            &mut row,
            &q,
            Some("rust"),
            Server {
                anchor: &format!("{root}/lib.rs"),
                root: &root,
            },
            &scope,
        );
        assert_eq!(row["partialReasons"], json!([CFG_NEGATED_REASON]), "{row}");
        assert_eq!(row["next"]["findText"]["tool"], "localSearch", "{row}");
        assert!(row["next"].get("expandFeatures").is_none(), "{row}");
        assert!(has_negated_feature_gate(
            "#[cfg(all(unix, not( feature = \"x\")))]"
        ));
        assert!(!has_negated_feature_gate("#[cfg(feature = \"x\")]"));
    }

    #[test]
    fn text_search_lead_targets_repo_scope_with_language_include() {
        let mut row = json!({"kind":"references","matches":[{"path":"a.py"}]});
        let q = query(
            json!({"path":"/repo/pkg/a.py","operation":"references","symbolName":"get","line":3}),
        );
        let scope = Scope::new("/repo".into(), vec!["*.py".into(), "*.pyi".into()]);
        annotate(
            &mut row,
            &q,
            Some("python"),
            Server {
                anchor: "/w/a.py",
                root: "/w",
            },
            &scope,
        );
        let lead = &row["next"]["findText"]["query"]["queries"][0];
        assert_eq!(lead["path"], "/repo", "{row}");
        assert_eq!(lead["include"], json!(["*.py", "*.pyi"]), "{row}");
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
