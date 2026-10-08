//! astTopology response shaping: pagination, coverage, continuations and
//! the read lead.

use super::{analysis::*, types::*};
use crate::tools::id::ToolId;
use crate::tools::id::query_limits::ast_topology::{DIAGNOSTIC_PAGE_MAXIMUM, PAGE_MAXIMUM};
use crate::tools::result::Continuation;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) fn paginate(items: Vec<Value>, q: &AstTopologyQuery) -> (Vec<Value>, Value) {
    let size = q.page_size().clamp(1, super::topology_max("pageSize")) as usize;
    let (page, facts) = crate::response::pages::slice_page(&items, q.page() as usize, size);
    (page, facts.to_value())
}

/// The warning for a result page past the end, naming the valid range.
pub(super) fn out_of_range_warning(pagination: &Value) -> Option<String> {
    (pagination["outOfRange"] == true).then(|| {
        let pages = pagination["totalPages"].as_u64().unwrap_or(1);
        format!(
            "page:{} is out of range (only {pages} page(s), {} result(s)) — returned 0 results. Use page:1..{pages}.",
            pagination["currentPage"],
            pagination["totalItems"]
        )
    })
}

/// Mark a replayed page whose graph snapshot no longer matches; the runtime
/// writes the shared `error` text (`response::pages::restart_stale`).
pub(super) fn insert_snapshot_changed(base: &mut Map<String, Value>) {
    base.insert("status".into(), json!("error"));
    base.insert("errorCode".into(), json!("staleSnapshot"));
}

/// Restart both result and diagnostic pagination from the current graph.
pub(super) fn restart_continuation(q: &AstTopologyQuery) -> Value {
    let mut query = clean_query(q);
    query["page"] = json!(1);
    if let Some(query) = query.as_object_mut() {
        query.remove("diagnosticSnapshot");
        query.remove("diagnosticPage");
    }
    Continuation::new(ToolId::AstTopology, query)
        .why("Restart pagination from the current graph snapshot.")
        .confidence("exact")
        .build()
}

/// Coverage outcome of one response: whether the caller's diagnostic snapshot
/// is stale, and the snapshot id of diagnostic rows withheld by the
/// counts-only default (rows stay reachable through `next.readDiagnostics`).
pub(super) struct CoverageState {
    pub(super) changed: bool,
    pub(super) withheld: Option<String>,
}

/// Attach coverage and diagnostics. The snapshot id binds both the diagnostic
/// list and the full result list, so a replayed result or diagnostic page from
/// a changed graph is rejected instead of silently mixing graph versions.
/// Diagnostic rows are returned only for an explicit `diagnosticPage`; the
/// default carries `diagnosticCounts` and gap reasons, and rows with the same
/// code and message are grouped into one row listing every `path[:line]`.
pub(super) fn add_coverage(
    base: &mut Map<String, Value>,
    b: &BuiltGraph,
    q: &AstTopologyQuery,
    results_digest: &str,
) -> CoverageState {
    let tuples = b
        .diagnostics
        .iter()
        .map(|d| json!([d.file, d.line, d.code, d.message]))
        .collect::<Vec<_>>();
    let id = crate::digest::json_sha256(&json!([
        tuples,
        results_digest,
        q.page_size(),
        q.diagnostic_page_size()
    ]));
    // `coverage.imports.unresolvedInternal` already counts these rows.
    let mut counts = BTreeMap::<String, u32>::new();
    for d in b
        .diagnostics
        .iter()
        .filter(|d| d.code != "unresolved-internal")
    {
        *counts.entry(d.code.clone()).or_default() += 1
    }
    let languages=b.languages.iter().map(|(language,files,linking)|json!({"language":language,"files":files,"linking":linking})).collect::<Vec<_>>();
    let mut imports = json!({"resolved":b.imports[0],"external":b.imports[1],"unresolvedInternal":b.imports[2],"unsupported":b.imports[3]});
    if b.imports[4] > 0 {
        imports["nonCode"] = json!(b.imports[4]);
    }
    let mut coverage = json!({"basis":"syntactic","referenceBasis":"lexical-occurrence","languages":languages,"imports":imports});
    if !counts.is_empty() {
        coverage["diagnosticCounts"] = json!(counts);
    }
    if q.diagnostic_snapshot().as_ref().is_some_and(|x| x != &id) {
        insert_snapshot_changed(base);
        base.insert("results".into(), json!([]));
        base.insert("coverage".into(), coverage);
        return CoverageState {
            changed: true,
            withheld: None,
        };
    }
    if base["pagination"]["hasMore"] == true {
        base["pagination"]["resultId"] = json!(id);
    }
    if !q.diagnostic_rows_requested() {
        base.insert("coverage".into(), coverage);
        return CoverageState {
            changed: false,
            withheld: (!b.diagnostics.is_empty()).then_some(id),
        };
    }
    let groups = group_diagnostics(&b.diagnostics);
    let size = q
        .diagnostic_page_size()
        .clamp(1, super::topology_max("diagnosticPageSize")) as usize;
    let pages = usize::max(1, groups.len().div_ceil(size));
    let current = (q.diagnostic_page().max(1) as usize).min(pages);
    let more = current < pages;
    let ds = groups
        .into_iter()
        .skip((current - 1) * size)
        .take(size)
        .collect::<Vec<_>>();
    let total = b
        .diagnostics
        .iter()
        .map(|d| (&d.code, &d.message))
        .collect::<BTreeSet<_>>()
        .len();
    let past_end = q.diagnostic_page() as usize > pages;
    let mut diagnostics_pagination =
        crate::response::pages::PageFacts::counted(current, size, total)
            .out_of_range(past_end)
            .to_value();
    diagnostics_pagination["hasMore"] = json!(more);
    diagnostics_pagination["resultId"] = json!(id);
    if past_end {
        let warning = format!(
            "diagnosticPage:{} is out of range; returned diagnostic page {}.",
            q.diagnostic_page(),
            current
        );
        match base.get_mut("warnings") {
            Some(Value::Array(warnings)) => warnings.push(json!(warning)),
            _ => {
                base.insert("warnings".into(), json!([warning]));
            }
        }
    }
    if !ds.is_empty() {
        coverage["diagnostics"] = json!(ds);
    }
    // One diagnostic page needs no pagination envelope; emit it only when a
    // continuation or an out-of-range correction depends on it.
    if pages > 1 || diagnostics_pagination["outOfRange"] == true {
        coverage["diagnosticPagination"] = diagnostics_pagination;
    }
    base.insert("coverage".into(), coverage);
    CoverageState {
        changed: false,
        withheld: None,
    }
}

/// One row per distinct code+message, in first-file order. A single
/// occurrence keeps `file`/`line`; repeated ones list `files` as `path[:line]`.
pub(super) fn group_diagnostics(diagnostics: &[Diagnostic]) -> Vec<Value> {
    let mut order = Vec::<(&str, &str)>::new();
    let mut members = BTreeMap::<(&str, &str), Vec<&Diagnostic>>::new();
    for d in diagnostics {
        let key = (d.code.as_str(), d.message.as_str());
        let list = members.entry(key).or_default();
        if list.is_empty() {
            order.push(key);
        }
        list.push(d);
    }
    order
        .into_iter()
        .map(|key| match members[&key].as_slice() {
            [single] => json!(single),
            many => json!({
                "code": key.0,
                "message": key.1,
                "files": many
                    .iter()
                    .map(|d| match d.line {
                        Some(line) => format!("{}:{line}", d.file),
                        None => d.file.clone(),
                    })
                    .collect::<Vec<_>>()
            }),
        })
        .collect()
}
/// What cut a graph's scope: the file scan (`maxFiles`), the edge cap, or
/// skipped files. A drift unions its two graphs' cuts.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScanCuts {
    pub(super) max_files: bool,
    pub(super) edge_cap: bool,
    pub(super) files_skipped: bool,
}

impl ScanCuts {
    pub(super) fn of(b: &BuiltGraph) -> Self {
        Self {
            max_files: b.truncated,
            edge_cap: b.edges_capped,
            files_skipped: b.files_skipped > 0,
        }
    }
    pub(super) fn union(self, other: Self) -> Self {
        Self {
            max_files: self.max_files || other.max_files,
            edge_cap: self.edge_cap || other.edge_cap,
            files_skipped: self.files_skipped || other.files_skipped,
        }
    }
    /// The graph itself is cut (skipped files are named in coverage).
    pub(super) fn graph_cut(self) -> bool {
        self.max_files || self.edge_cap
    }
    /// A wider file scan reaches more of the graph: more files cannot lift
    /// the edge cap.
    pub(super) fn widenable(self) -> bool {
        self.max_files && !self.edge_cap
    }
    pub(super) fn reasons(self) -> Vec<&'static str> {
        [
            (self.max_files, "maxFiles"),
            (self.edge_cap, "edgeCap"),
            (self.files_skipped, "filesSkipped"),
        ]
        .into_iter()
        .filter_map(|(cut, reason)| cut.then_some(reason))
        .collect()
    }
    pub(super) fn warnings(self, q: &AstTopologyQuery) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.max_files {
            warnings.push(format!(
                "scan stopped at maxFiles ({}) — graph results are partial",
                q.max_files().unwrap_or(20_000)
            ));
        }
        if self.edge_cap {
            warnings.push(format!(
                "edge collection stopped at the {}-edge cap — graph results are partial; more files cannot lift it, so narrow path or add exclude",
                super::graph::edge_cap()
            ));
        }
        warnings
    }
    /// No continuation reaches the rest: a skipped file, the edge cap, or a
    /// file scan cut at the `maxFiles` maximum.
    pub(super) fn terminal(self, q: &AstTopologyQuery) -> bool {
        self.files_skipped
            || self.edge_cap
            || self.max_files && q.max_files().unwrap_or(20_000) >= super::topology_max("maxFiles")
    }
}

/// The `supersedes` field and warning of page 1 of a widened scan.
pub(super) fn insert_supersedes(
    base: &mut Map<String, Value>,
    q: &AstTopologyQuery,
    warnings: &mut Vec<String>,
) {
    if let Some(narrower) = q.supersedes() {
        base.insert("supersedes".into(), json!(narrower));
        warnings.push(format!(
            "widened scan: these results replace every row of the maxFiles:{narrower} scan; discard those rows"
        ));
    }
}

/// The same query over a doubled file bound whose results replace this
/// scan's rows (`supersedes`); `None` at the `maxFiles` maximum.
pub(super) fn expand_scan(q: &AstTopologyQuery) -> Option<Value> {
    let max_files = super::topology_max("maxFiles");
    let cur = q.max_files().unwrap_or(20_000);
    if cur >= max_files {
        return None;
    }
    let mut widen = continuation(
        q,
        Some(1),
        Some((cur * 2).max(cur + 1).min(max_files)),
        None,
        "Rebuild the graph with more files; its results replace every row of this scan.",
    );
    if let Some(row) = widen
        .pointer_mut("/query/queries/0")
        .and_then(Value::as_object_mut)
    {
        row.insert("supersedes".into(), json!(cur));
    }
    Some(widen)
}

pub(super) fn add_next(
    base: &mut Map<String, Value>,
    q: &AstTopologyQuery,
    root: &Path,
    widenable: bool,
    withheld_diagnostics: Option<&str>,
) {
    let mut next = Map::new();
    let more_results = base
        .get("pagination")
        .is_some_and(|pagination| pagination["hasMore"] == true);
    if more_results && (q.page() as usize) < PAGE_MAXIMUM {
        let why = match q.analysis() {
            GraphAnalysis::Dependencies => "Continue dependencies.",
            GraphAnalysis::Dependents => "Continue dependents.",
            GraphAnalysis::Path => "Continue path results.",
            GraphAnalysis::Cycles => "Continue cycle components.",
            GraphAnalysis::Reachability => "Continue reachability classifications.",
            GraphAnalysis::DeadCode => "Continue dead-code candidates.",
            GraphAnalysis::Drift => "Continue topology drift results.",
        };
        // Bind the next result page to this graph snapshot; result pages
        // never re-send diagnostic rows.
        let snapshot = base["pagination"].get("resultId").cloned();
        let mut page = continuation(q, Some(q.page() + 1), None, snapshot, why);
        if let Some(row) = page
            .pointer_mut("/query/queries/0")
            .and_then(Value::as_object_mut)
        {
            row.shift_remove("diagnosticPage");
        }
        next.insert("nextPage".into(), page);
    }
    if let Some(snapshot) = withheld_diagnostics {
        let mut value = clean_query(q);
        value["diagnosticPage"] = json!(1);
        value["diagnosticPageSize"] = json!(q.diagnostic_page_size());
        value["diagnosticSnapshot"] = json!(snapshot);
        if let Some(row) = value.as_object_mut() {
            row.shift_remove("page");
        }
        next.insert(
            "readDiagnostics".into(),
            Continuation::new(ToolId::AstTopology, value)
                .why("Coverage diagnostic rows behind the coverage counts.")
                .confidence("exact")
                .build(),
        );
    }
    if base["coverage"]["diagnosticPagination"]["hasMore"] == true
        && (q.diagnostic_page() as usize) < DIAGNOSTIC_PAGE_MAXIMUM
    {
        let mut value = clean_query(q);
        value["diagnosticPage"] = json!(q.diagnostic_page() + 1);
        value["diagnosticPageSize"] = json!(q.diagnostic_page_size());
        value["diagnosticSnapshot"] = base["coverage"]["diagnosticPagination"]["resultId"].clone();
        next.insert(
            "nextDiagnosticPage".into(),
            Continuation::new(ToolId::AstTopology, value)
                .why("Continue coverage diagnostics from the same diagnostic snapshot.")
                .confidence("exact")
                .build(),
        );
    }
    if base["coverage"]["diagnosticPagination"]["outOfRange"] == true {
        let mut value = clean_query(q);
        value["diagnosticPage"] = json!(1);
        if let Some(query) = value.as_object_mut() {
            query.remove("diagnosticSnapshot");
        }
        next.insert(
            "restartDiagnostics".into(),
            Continuation::new(ToolId::AstTopology, value)
                .why("Restart diagnostic pagination from the current diagnostic snapshot.")
                .confidence("exact")
                .build(),
        );
    }
    // A widened scan rebuilds the whole graph, and its rows replace this
    // scan's (`supersedes`): offered once, on the last reachable result page,
    // so a walk reads every row of this graph before the one replacing it.
    if widenable
        && !next.contains_key("nextPage")
        && let Some(widen) = expand_scan(q)
    {
        next.insert("expandScan".into(), widen);
    }
    if q.analysis() == GraphAnalysis::DeadCode
        && let Some(c) = base["results"].as_array().and_then(|x| x.first())
        && let (Some(file), Some(name), Some(line)) =
            (c["file"].as_str(), c["name"].as_str(), c["line"].as_u64())
    {
        // The path anchors on the canonical graph root (inferred when `path`
        // is omitted). No lead when no language server would answer it.
        let path = root.join(file).to_string_lossy().into_owned();
        if let Some(mut row) = crate::tools::lsp_search::verify_query(
            &path,
            name,
            line,
            crate::tools::lsp_search::Verify::References,
        ) {
            row["includeDeclaration"] = json!(false);
            row["groupByFile"] = json!(true);
            next.insert(
                crate::tools::lsp_search::lead_name(&row),
                Continuation::new(ToolId::LspSearch, row)
                    .why(format!("Verify candidate \"{name}\" before deletion; repeat for each result, prioritizing viaHeuristic:\"reexport-chain\"."))
                    .confidence("high")
                    .build(),
            );
        }
    }
    if !next.is_empty() {
        base.insert("next".into(), Value::Object(next));
    }
}
/// The natural next step after a graph answer: read the import line that
/// links the top result (dependents: in the importer; dependencies: in the
/// file that imports it; path and cycles: the first edge).
pub(super) fn read_lead(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
    base: &Map<String, Value>,
) -> Option<Value> {
    let top = base.get("results")?.as_array()?.first()?;
    let (importer, line) = match q.analysis() {
        GraphAnalysis::Dependents => (top["file"].as_str()?, top["importLine"].as_u64()?),
        GraphAnalysis::Dependencies => (top["via"].as_str()?, top["importLine"].as_u64()?),
        GraphAnalysis::Path => {
            let edge = top["edges"].as_array()?.first()?;
            (edge["from"].as_str()?, edge["importLine"].as_u64()?)
        }
        GraphAnalysis::Cycles => {
            let edge = top["cycleEdges"].as_array()?.first()?;
            let from = edge["from"].as_str()?;
            let line = first_import_line(b, from, edge["to"].as_str()?)?;
            (from, u64::from(line))
        }
        _ => return None,
    };
    let path = b.root.join(importer).to_string_lossy().into_owned();
    Some(
        Continuation::new(
            ToolId::LocalFetch,
            json!({"path": path, "ranges": [format!("{line}-{line}")]}),
        )
        .why("Read the import line behind the top result.")
        .confidence("high")
        .build(),
    )
}
pub(super) fn clean_query(q: &AstTopologyQuery) -> Value {
    let mut v = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    if let Some(m) = v.as_object_mut() {
        m.retain(|_, x| !x.is_null());
        // Only page 1 of a widened scan replaces the narrower scan's rows.
        m.shift_remove("supersedes");
        if m.get("diagnosticPage") == Some(&json!(1)) {
            m.remove("diagnosticPage");
        }
    }
    v
}
/// A re-run over a new graph restarts diagnostic paging: rows stay requested
/// only when this query requested them, and the old snapshot is dropped.
pub(super) fn restart_diagnostic_rows(query: &mut Value, q: &AstTopologyQuery) {
    if let Some(fields) = query.as_object_mut() {
        fields.remove("diagnosticSnapshot");
        if q.diagnostic_rows_requested() {
            fields.insert("diagnosticPage".into(), json!(1));
        } else {
            fields.remove("diagnosticPage");
        }
    }
}
pub(super) fn continuation(
    q: &AstTopologyQuery,
    page: Option<u32>,
    max: Option<u32>,
    snapshot: Option<Value>,
    why: &str,
) -> Value {
    let mut v = clean_query(q);
    if let Some(x) = page {
        v["page"] = json!(x)
    }
    if let Some(x) = max {
        v["maxFiles"] = json!(x);
        restart_diagnostic_rows(&mut v, q);
    }
    if let Some(snapshot) = snapshot {
        v["diagnosticSnapshot"] = snapshot;
    }
    Continuation::new(ToolId::AstTopology, v)
        .why(why)
        .confidence("exact")
        .build()
}
