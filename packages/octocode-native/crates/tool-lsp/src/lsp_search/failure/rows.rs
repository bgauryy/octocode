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
    if let Some(lead) = symbol_search_lead(query, symbol, &scope) {
        octocode_runtime_base::response::result::attach_related(value, lead.build());
    }
}

pub(in super::super) fn empty(
    query: &LspSearchQuery,
    category: &str,
    reason: &str,
    server_available: bool,
) -> Value {
    let mut row = json!({
        "status": "empty",
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

/// The row of a server whose load has not ended after the call `waited`
/// for it: no answer of its says what the project holds yet, so the row is
/// partial and offers the same query again, which the server, kept loading,
/// answers once its load ends.
pub(in super::super) fn still_indexing(
    query: &LspSearchQuery,
    waited: std::time::Duration,
) -> Value {
    let reason = format!(
        "The language server was still indexing the project when this call ended after {:.1} s; the server stays loaded, so the same query again continues from there.",
        waited.as_secs_f64()
    );
    let mut row = empty(query, "languageServerIndexing", &reason, true);
    let reason = octocode_runtime_base::response::paging::PartialReason::LanguageServerIndexing;
    partial_with_related_rerun(&mut row, query, reason, &[]);
    row
}

/// The empty row of a language server that answered nothing about the
/// query's file: `category` with `reason`, unless the file lies outside the
/// project its server needs ([`without_project`]). The server then analyzes
/// nothing, so the row is `noProject` and partial: the empty answer is no
/// absence.
pub(in super::super) fn no_answer(query: &LspSearchQuery, category: &str, reason: &str) -> Value {
    if !without_project(query) {
        return empty(query, category, reason, true);
    }
    let mut row = empty(query, &NO_PROJECT.to_string(), NO_PROJECT_REASON, true);
    octocode_runtime_base::response::paging::mark_partial(&mut row, NO_PROJECT, &[]);
    row
}

/// Category and partial reason of an empty answer about a file no project
/// holds ([`no_answer`]).
const NO_PROJECT: octocode_runtime_base::response::paging::PartialReason =
    octocode_runtime_base::response::paging::PartialReason::NoProject;
const NO_PROJECT_REASON: &str = "No Cargo.toml or rust-project.json is above this file, and rust-analyzer analyzes no file outside one; empty is not absence.";

/// Whether the query's file lies outside the project its language server
/// needs ([`octocode_engine::lsp::workspace::outside_required_project`]).
fn without_project(query: &LspSearchQuery) -> bool {
    lsp_search_query::path(query)
        .map(uri_to_path)
        .is_some_and(|path| {
            octocode_engine::lsp::workspace::outside_required_project(std::path::Path::new(&path))
        })
}

pub(in super::super) fn annotate_empty_includes(
    row: &mut Value,
    query: &LspSearchQuery,
    rust: super::super::receipt::RustAnalysis,
    stopped: &dyn Fn() -> bool,
) {
    if let Some(path) = lsp_search_query::path(query) {
        // Incoming rows get the evidence-based include disclosure instead
        // (see `server_coverage`).
        if row["category"] != "noLocations"
            || !path.ends_with(".rs")
            || super::super::inferred_project::is_incoming(lsp_search_query::operation(query))
        {
            return;
        }
        let anchor = uri_to_path(path);
        // The crate that defines the name decides: a name an outside
        // include! declares (in the anchor's crate or one it depends on)
        // never resolves, whatever build scripts the anchor's crate has.
        if let Some(name) =
            lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
            && let Some(found) =
                super::super::server_coverage::include_declaring(&anchor, name, stopped)
        {
            let reason = super::super::server_coverage::OUTSIDE_INCLUDE_REASON;
            octocode_runtime_base::response::paging::mark_partial(row, reason, &[]);
            let target = found.target.to_string_lossy().into_owned();
            if let Some(lead) =
                text_search_lead(name, &target, &[], Some(std::slice::from_ref(&target)))
            {
                octocode_runtime_base::response::result::attach_related(row, lead.build());
            }
            // `includeOutsidePackage` says why no build context resolves it;
            // the hint names this answer's own facts.
            octocode_runtime_base::response::result::add_hints(
                row,
                [format!(
                    "`{name}` is declared in a file crate {} include!s from outside its directory.",
                    found.package
                )],
            );
            return;
        }
        if let Some(includes) = super::super::server_coverage::package_includes(&anchor, stopped) {
            // Each cause is the row's partial reason, as on incoming rows
            // (`server_coverage`): empty is no absence there, and what each
            // reason means is core's, said once.
            if !includes.outside.is_empty() {
                let reason = super::super::server_coverage::OUTSIDE_INCLUDE_REASON;
                octocode_runtime_base::response::paging::mark_partial(row, reason, &[]);
            }
            // A build script generates the names no source declares: a name
            // the package declares is no generated name, whatever scripts
            // the package has.
            let generated = !includes.generated.is_empty()
                && !lsp_search_query::symbol_name(query)
                    .filter(|name| !name.trim().is_empty())
                    .is_some_and(|name| {
                        super::super::server_coverage::package_declares(&anchor, name, stopped)
                    });
            if generated && !rust.build_scripts {
                // With an outside include as well, the name may be defined
                // there: the ~2 GB rerun is no repair to lead with. Alone,
                // the executable rerun is the lead.
                let allowed = rust.build_scripts_allowed;
                let rerun = includes.outside.is_empty() && allowed;
                let reason = super::super::server_coverage::BUILD_SCRIPTS_REASON;
                octocode_runtime_base::response::paging::mark_partial(row, reason, &[]);
                if rerun && let Some(action) = next_action(query, reason) {
                    octocode_runtime_base::response::result::attach_related(row, action);
                }
            }
        }
    }
}

/// The one next action of each partial reason that offers one, as an
/// executable `lspSearch` lead from the walk's first page: `buildScriptsOff`
/// reruns with `rustContext.buildScripts: true` (it runs repository code:
/// the caller's consent), `cfgGatedFiles` with `rustContext.features:
/// "all"`, and every other reason (`languageServerIndexing`, a retryable
/// server failure, …) retries the same query. The rest of the Rust context
/// is kept.
pub(in super::super) fn next_action(
    query: &LspSearchQuery,
    reason: octocode_runtime_base::response::paging::PartialReason,
) -> Option<Value> {
    use super::super::server_coverage::{BUILD_SCRIPTS_REASON, CFG_REASON};
    // A related request is fresh: it drops the query's page and snapshot.
    let setting = match reason {
        BUILD_SCRIPTS_REASON => Some(("buildScripts", json!(true))),
        CFG_REASON => Some(("features", json!("all"))),
        _ => None,
    };
    let rerun = match setting {
        Some((field, value)) => {
            let mut context = lsp_search_query::rust_context(query)
                .map_or_else(|| json!({}), |context| json!(context));
            context[field] = value;
            lsp_search_query::edited(query, &json!({"rustContext": context})).ok()?
        }
        None => query.clone(),
    };
    Some(octocode_runtime_base::response::result::Continuation::related_query(rerun).build())
}

/// The query row of the fresh rerun lead of `query`, for tests that name the
/// retry a row offers.
#[cfg(test)]
pub(in super::super) fn same_query_rerun(query: &LspSearchQuery) -> Value {
    let call =
        octocode_runtime_base::response::result::Continuation::related_query(query.clone()).build();
    call["input"]["queries"][0].clone()
}

/// Name the row's coverage state where the row reports coverage details
/// (`importerScan`, `textOnlyFiles`, …): `error` for a failure row, `partial`
/// for a row flagged incomplete or carrying a coverage reason, else `complete`.
/// A row without coverage details states no coverage object, and neither does
/// one whose scan found nothing to state, whatever else made it partial.
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
    let Some(coverage) = row.get_mut("coverage").and_then(Value::as_object_mut) else {
        return;
    };
    // No complete scan is stated, on a complete row or on one another reason
    // made partial, bounded by the request's workspaceRoot or not: the
    // absence of coverage is that statement, whichever language answered.
    // What stays is what a reader acts on, such as a count above zero
    // (importer files the scan added, files it left out), a scan that did not
    // complete, or the root that bounded it. A count of zero says nothing a
    // complete answer does not; on a partial row only the importer count of
    // zero goes, as the others count what the row's reasons are about.
    if state != "error" {
        if coverage.get("importerScan").and_then(Value::as_str) == Some("complete") {
            coverage.remove("importerScan");
        }
        if state == "complete" {
            coverage.retain(|_, value| value.as_u64() != Some(0));
        } else if coverage
            .get("verifiedImporterFiles")
            .and_then(Value::as_u64)
            == Some(0)
        {
            coverage.remove("verifiedImporterFiles");
        }
        if coverage.is_empty() {
            row.as_object_mut().map(|fields| fields.remove("coverage"));
            return;
        }
    }
    coverage.entry("state").or_insert_with(|| json!(state));
}

/// An empty diagnostics row with no retained report by the end of its wait
/// (while the server worked, then `wait_ms` quiet): the document may still
/// have diagnostics, so the row is partial with a retry.
pub(in super::super) fn mark_diagnostics_unpublished(
    row: &mut Value,
    query: &LspSearchQuery,
    wait_ms: u32,
) {
    if row.pointer("/status").and_then(Value::as_str) != Some("empty") {
        return;
    }
    row["category"] = json!("diagnosticsNotPublished");
    row["reason"] = json!(format!(
        "No diagnostic report was available for this document: none came while the server worked or in the {wait_ms} ms after its last sign of work; the server may not have published one, or the shared memory budget could not retain it."
    ));
    partial_with_related_rerun(
        row,
        query,
        octocode_runtime_base::response::paging::PartialReason::DiagnosticsNotPublished,
        &[],
    );
}

/// Partial reason of an empty answer on a line where the server reports an
/// error: its analysis of that line may have dropped what was asked about
/// (clang without the CUDA SDK drops every `kernel<<<…>>>(…)` launch).
pub(in super::super) const ANCHOR_LINE_ERROR:
    octocode_runtime_base::response::paging::PartialReason =
    octocode_runtime_base::response::paging::PartialReason::AnchorLineError;

/// Mark an empty row partial when an error-severity diagnostic (published
/// shape, see `ops::diagnostic_items`) covers its one-based anchor `line`:
/// the warning names the first such error and counts the rest. The context
/// assessment attaches a diagnostic follow-up preserving the effective query.
pub(in super::super) fn mark_anchor_line_errors(row: &mut Value, diagnostics: &Value, line: u32) {
    if row.get("status").and_then(Value::as_str) != Some("empty") {
        return;
    }
    let line = u64::from(line);
    let errors = super::super::render::items(diagnostics)
        .iter()
        .filter(|item| item.get("severity").and_then(Value::as_str) == Some("error"))
        .filter(|item| {
            let start = item.get("line").and_then(Value::as_u64);
            let end = super::super::locations::place_last_line(item);
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
    let (message, source) = (
        octocode_runtime_base::security::quoted(message),
        octocode_runtime_base::security::quoted(&source),
    );
    let warning = format!(
        "The language server reports an error on line {line}: {message}{source}{more}. Its analysis of this line is incomplete, so empty is not absence."
    );
    octocode_runtime_base::response::paging::mark_partial(row, ANCHOR_LINE_ERROR, &[warning]);
}

/// Flag a row as incomplete with a machine-readable reason, human warnings,
/// and an optional fresh reattempt. Reattempting a server after it changes is
/// related work, never a mandatory continuation of this response.
pub(in super::super) fn partial_with_related_rerun(
    row: &mut Value,
    query: &LspSearchQuery,
    reason: octocode_runtime_base::response::paging::PartialReason,
    warnings: &[String],
) {
    if !row.is_object() {
        return;
    }
    octocode_runtime_base::response::paging::mark_partial(row, reason, warnings);
    if let Some(action) = next_action(query, reason) {
        octocode_runtime_base::response::result::attach_related(row, action);
    }
}
