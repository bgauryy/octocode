//! Empty and partial rows: text-search fallbacks, build-script tips,
//! coverage stamps, diagnostic flags and partial retries.
use super::*;

/// With no language server, the lexical check of a named symbol: a
/// whole-word, case-sensitive localSearch over the workspace (else the
/// file's directory).
pub(super) fn attach_text_search(value: &mut Value, query: &LspSearchQuery) {
    let Some(symbol) = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
    else {
        return;
    };
    let path = lsp_search_query::path(query)
        .map(uri_to_path)
        .unwrap_or_default();
    let file = std::path::Path::new(&path);
    if file.is_dir() {
        return; // `next.findText` already searches the directory.
    }
    let Some(scope) = lsp_search_query::workspace_root(query)
        .map(uri_to_path)
        .or_else(|| file.parent().map(|dir| dir.to_string_lossy().into_owned()))
    else {
        return;
    };
    let lead = symbol_search_lead(query, symbol, &scope);
    crate::response::result::attach_related(value, lead.build());
}

pub(in super::super) fn empty(
    query: &LspSearchQuery,
    category: &str,
    reason: &str,
    server_available: bool,
) -> Value {
    let mut row = json!({
        "status": "empty",
        "kind": "empty",
        "category": category,
        "reason": reason,
        "lsp": { "serverAvailable": server_available },
    });
    // A workspaceRoot-only query has no file: the row names none.
    if let Some(path) = lsp_search_query::path(query) {
        row["path"] = json!(path);
    }
    row
}

pub(in super::super) fn annotate_empty_includes(
    row: &mut Value,
    query: &LspSearchQuery,
    build_scripts: bool,
) {
    if let Some(path) = lsp_search_query::path(query) {
        // Incoming rows get the evidence-based include disclosure instead
        // (see `server_coverage`).
        if row["category"] == "noLocations"
            && path.ends_with(".rs")
            && !super::super::inferred_project::is_incoming(lsp_search_query::operation(query))
            && let Some(includes) =
                super::super::server_coverage::package_includes(&uri_to_path(path))
        {
            let mut tips = Vec::new();
            if !includes.outside.is_empty() {
                tips.push(OUTSIDE_INCLUDE_TIP.to_owned());
            }
            if !includes.generated.is_empty() && !build_scripts {
                // With an outside include as well, the name may be defined
                // there: the ~2 GB rerun is no repair to lead with, so the
                // tip names it instead. Alone, the executable rerun is the
                // lead, and the tip states what running it costs.
                let rerun = includes.outside.is_empty();
                if rerun {
                    crate::response::result::attach_related(row, build_scripts_rerun(query));
                }
                tips.push(build_script_tip(rerun));
            }
            if !tips.is_empty() {
                let hints = row
                    .as_object_mut()
                    .map(|object| object.entry("hints").or_insert_with(|| json!([])))
                    .and_then(Value::as_array_mut);
                if let Some(hints) = hints {
                    hints.extend(tips.into_iter().map(Value::from));
                }
            }
        }
    }
}

/// What running a package's build scripts costs and risks: the one
/// statement every build-script note ends with.
pub(in super::super) const BUILD_SCRIPTS_COST: &str =
    "executes this workspace's build scripts (repository code) in a separate rust-analyzer (~2 GB)";

/// rust-analyzer runs without build scripts by default, so in a package
/// with one, a name the script generates (`include!(concat!(env!("OUT_DIR"),
/// …))`) has no location until they run. With an executable rerun offered,
/// the note names its setting and cost; without it, the
/// note names the flag.
pub(super) fn build_script_tip(rerun_offered: bool) -> String {
    if rerun_offered {
        format!(
            "A name a build script generates (include! of OUT_DIR) has no location until build scripts run; the rerun with rustContext.buildScripts enabled {BUILD_SCRIPTS_COST}."
        )
    } else {
        format!(
            "A name a build script generates (include! of OUT_DIR) resolves only with rustContext {{buildScripts:true}}, which {BUILD_SCRIPTS_COST}."
        )
    }
}

/// rust-analyzer loads only the package's own directory: what a file
/// `include!`d from outside it defines never resolves.
pub(super) const OUTSIDE_INCLUDE_TIP: &str = "A name defined in source this package include!s from outside its directory never resolves in rust-analyzer: read it with localSearch.";

/// The same query with rust-analyzer running the crate's build scripts
/// (`rustContext.buildScripts: true`, the rest of its Rust context kept),
/// from the walk's first page.
pub(in super::super) fn build_scripts_rerun(query: &LspSearchQuery) -> Value {
    let mut rerun = same_query_rerun(query);
    let mut context =
        lsp_search_query::rust_context(query).map_or_else(|| json!({}), |context| json!(context));
    context["buildScripts"] = json!(true);
    rerun["rustContext"] = context;
    crate::response::result::Continuation::related(crate::id::ToolId::LspSearch, rerun).build()
}

/// A contract-valid fresh LSP query: current execution state (page, snapshot,
/// and other state fields) is not public input for a new related reattempt.
pub(in super::super) fn same_query_rerun(query: &LspSearchQuery) -> Value {
    let mut rerun = lsp_search_query::to_row(query);
    if let Some(fields) = rerun.as_object_mut() {
        fields.retain(|key, value| {
            !value.is_null() && !crate::contracts::state_fields(ToolId::LspSearch).contains(key)
        });
    }
    rerun
}

/// An exact, executable `lspSearch` continuation.
pub(in super::super) fn continuation(query: Value) -> Value {
    crate::response::result::Continuation::continued(ToolId::LspSearch, query).build()
}

/// Name the row's coverage state where the row reports coverage details
/// (`importerScan`, `textOnlyFiles`, …): `error` for a failure row, `partial`
/// for a row flagged incomplete or carrying a coverage reason, else `complete`.
/// A row without coverage details states no coverage object.
pub(in super::super) fn stamp_coverage(row: &mut Value) {
    let state = if row.get("status").and_then(Value::as_str) == Some("error") {
        "error"
    } else if row.get("isPartial").and_then(Value::as_bool) == Some(true)
        || row.pointer("/coverage/reason").is_some()
    {
        "partial"
    } else {
        "complete"
    };
    if let Some(coverage) = row.get_mut("coverage").and_then(Value::as_object_mut) {
        coverage.entry("state").or_insert_with(|| json!(state));
    }
}

/// An empty diagnostics row whose server pushed nothing within `wait_ms`: the
/// document may still have diagnostics, so the row is partial with a retry.
pub(in super::super) fn mark_diagnostics_unpublished(
    row: &mut Value,
    query: &LspSearchQuery,
    wait_ms: u32,
) {
    if row.pointer("/kind").and_then(Value::as_str) != Some("empty") {
        return;
    }
    row["category"] = json!("diagnosticsNotPublished");
    row["reason"] = json!(format!(
        "The language server published no diagnostics for this document within {wait_ms} ms."
    ));
    partial_with_related_rerun(row, query, "diagnosticsNotPublished", &[]);
}

/// Partial reason of an empty answer on a line where the server reports an
/// error: its analysis of that line may have dropped what was asked about
/// (clang without the CUDA SDK drops every `kernel<<<…>>>(…)` launch).
pub(in super::super) const ANCHOR_LINE_ERROR: &str = "anchorLineError";

/// Mark an empty row partial when an error-severity diagnostic (published
/// shape, see `ops::diagnostic_items`) covers its one-based anchor `line`:
/// the warning names the first such error and counts the rest. The context
/// assessment attaches a diagnostic follow-up preserving the effective query.
pub(in super::super) fn mark_anchor_line_errors(row: &mut Value, diagnostics: &Value, line: u32) {
    if row.get("kind").and_then(Value::as_str) != Some("empty") {
        return;
    }
    let line = u64::from(line);
    let errors = super::super::render::items(diagnostics)
        .iter()
        .filter(|item| item.get("severity").and_then(Value::as_str) == Some("error"))
        .filter(|item| {
            let start = item
                .pointer("/displayRange/startLine")
                .and_then(Value::as_u64);
            let end = item
                .pointer("/displayRange/endLine")
                .and_then(Value::as_u64);
            matches!((start, end), (Some(start), Some(end)) if start <= line && line <= end)
        })
        .collect::<Vec<_>>();
    let Some(first) = errors.first() else {
        return;
    };
    // A diagnostic's first line is its claim; later lines are notes and
    // stay in the full list behind `next.diagnostics`.
    let message = first
        .get("message")
        .and_then(Value::as_str)
        .and_then(|message| message.lines().next())
        .unwrap_or_default();
    let source = first
        .get("source")
        .and_then(Value::as_str)
        .map(|source| format!(" ({source})"))
        .unwrap_or_default();
    let more = match errors.len() - 1 {
        0 => String::new(),
        1 => ", plus 1 more error".to_owned(),
        n => format!(", plus {n} more errors"),
    };
    let warning = format!(
        "The language server reports an error on line {line}: {message}{source}{more}. Its analysis of this line is incomplete, so empty is not absence."
    );
    crate::response::paging::mark_partial(row, ANCHOR_LINE_ERROR, &[warning]);
}

/// Flag a row as incomplete with a machine-readable reason, human warnings,
/// and an optional fresh reattempt. Reattempting a server after it changes is
/// related work, never a mandatory continuation of this response.
pub(in super::super) fn partial_with_related_rerun(
    row: &mut Value,
    query: &LspSearchQuery,
    reason: &str,
    warnings: &[String],
) {
    if !row.is_object() {
        return;
    }
    crate::response::paging::mark_partial(row, reason, warnings);
    crate::response::result::attach_related(
        row,
        crate::response::result::Continuation::related(ToolId::LspSearch, same_query_rerun(query))
            .build(),
    );
}
