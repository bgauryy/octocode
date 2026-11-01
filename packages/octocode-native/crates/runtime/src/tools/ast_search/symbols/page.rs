use super::super::RowPaths;
use super::ast_search_query_symbols;
use super::left_out::Suppressed;
use super::rows::{group_by_file, row_sizes};
use super::select::SymbolSet;
use crate::id::ToolId;
use crate::response::symbol_outline::{declaration_rows, top_level_spans};
use crate::tools::leads::skipped;
use octocode_contract_types::AstSearchQuerySymbols;
use serde_json::{Value, json};

pub(crate) fn render_page(
    q: &AstSearchQuerySymbols,
    set: &SymbolSet,
    row_paths: RowPaths<'_>,
    response_window: Option<usize>,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Value, crate::response::result::ToolError> {
    // A page without a snapshot pins nothing: it pages the current outline,
    // as `match` and structureSearch do; only a snapshot that no longer
    // matches is stale.
    if ast_search_query_symbols::page(q) > 1
        && ast_search_query_symbols::snapshot(q).is_some_and(|given| given != set.snapshot)
    {
        return Ok(crate::response::result::stale_snapshot(
            crate::response::result::restart_query(
                ToolId::AstSearch,
                &crate::response::result::query_row(q, &[]),
            ),
        ));
    }
    let (skipped, truncated) = (set.skipped, set.truncated);
    let page = ast_search_query_symbols::page(q).max(1) as usize;
    let cut = SymbolPage::of(q, set, row_paths, response_window);
    let unreadable = set.walk_errors.0 > 0;
    let incomplete = truncated || skipped > 0 || unreadable;
    let rows = &set.declarations[cut.range.clone()];
    // Every scan cut continues ([`scan_window::expand_scan`]): a cut by the
    // scan time budget resumes after the files this window outlined
    // (`scanOffset`), so it is offered once the window's last page is reached.
    let expand_scan =
        crate::tools::ast_search::scan_window::expand_scan(truncated, set.scan_resume);
    let mut out = json!({"operation":"symbols","filesScanned":set.files_scanned,"filesSkipped":skipped,"diagnostics":outline_diagnostics(set),"isPartial":cut.more||incomplete||set.recovered});
    if let Some(excluded) = set.test_code_excluded.stated() {
        out["testCodeExcluded"] = json!(excluded);
    }
    // Each coverage cut names itself, so a partial final page (no `next`)
    // still says why it is partial.
    for (cut, reason) in [
        (truncated, crate::tools::ast_search::scan_window::CUT_REASON),
        (
            skipped > 0 || unreadable,
            crate::response::paging::PartialReason::FilesSkipped,
        ),
        (
            set.recovered,
            crate::response::paging::PartialReason::ParseRecovered,
        ),
    ] {
        if cut {
            crate::response::paging::mark_partial(&mut out, reason, &[]);
        }
    }
    // What the walk skipped is disclosed like localSearch does, once, on
    // page 1; an empty outline leads over it in `note_skipped`. What the
    // exact entries left out is a fact of the whole result, so page 1
    // states it once with its lead.
    if page == 1
        && let Some(shown) = set
            .skipped_paths
            .as_ref()
            .and_then(|paths| skipped::grouped(paths.entries.iter().cloned(), true, paths.sampled))
    {
        out["skippedPaths"] = serde_json::to_value(shown).unwrap_or_default();
    }
    crate::response::paging::push_warnings(
        &mut out,
        set.kinds_left_out
            .as_ref()
            .filter(|_| page == 1)
            .map(|left_out| left_out.notice(q))
            .into_iter()
            .chain(
                set.suppressed
                    .as_ref()
                    .filter(|_| page == 1)
                    .map(Suppressed::notice),
            )
            .chain(crate::policy::include::parent_glob_warning(
                &[],
                &ast_search_query_symbols::exclude(q).unwrap_or_default(),
            )),
    );
    // A directory outline groups rows under their file, like `match` results,
    // so each path is written once instead of on every declaration; a file
    // outline names its file once. Rows are outline rows (see
    // `response::symbol_outline`).
    if set.directory || rows.iter().any(|row| row.get("path").is_some()) {
        out["files"] = Value::Array(group_by_file(rows, row_paths));
    } else {
        out["path"] = json!(row_paths.delivered(&set.path));
        out["symbols"] = Value::Array(declaration_rows(rows));
    }
    // The snapshot only pins later pages to the same source; a single page
    // has nothing to pin.
    if cut.more || page > 1 {
        out["snapshot"] = json!(set.snapshot);
    }
    out["pagination"] = cut.pagination;
    if skipped > 0 || unreadable {
        crate::response::paging::terminal_limit(
            &mut out,
            crate::response::paging::PartialReason::FilesSkipped,
            &[],
        );
    }
    attach_symbol_leads(q, set, &mut out, cut.more, expand_scan);
    if set.declarations.is_empty() && !incomplete {
        out["status"] = json!("empty");
        out["hints"] = json!([empty_hint(q, set.files_scanned)]);
    }
    super::super::evidence::attach_page(&mut out, &set.source_descriptors, cancel)?;
    Ok(out)
}

/// The top-level rows of one symbols page.
struct SymbolPage {
    /// The page's rows (with their members) in the declarations.
    range: std::ops::Range<usize>,
    /// Top-level rows are left after the page.
    more: bool,
    /// Its pagination block.
    pagination: Value,
}

impl SymbolPage {
    /// Top-level rows fitting the response budget, at most `limit`; a page
    /// past the end is empty.
    fn of(
        q: &AstSearchQuerySymbols,
        set: &SymbolSet,
        row_paths: RowPaths<'_>,
        response_window: Option<usize>,
    ) -> Self {
        let declarations = &set.declarations;
        let page = ast_search_query_symbols::page(q).max(1) as usize;
        let budget = crate::response::page_windows::page_budget(response_window);
        let paging = crate::response::page_windows::Paging::of(
            q.limit.map(|limit| crate::num::usize_of(limit.get())),
            budget,
        );
        // Pages count top-level rows; each carries its members.
        let spans = top_level_spans(declarations);
        let sizes = row_sizes(declarations, &spans, set.directory.then_some(row_paths));
        let cut = crate::response::page_windows::select(paging, &sizes, page);
        let range = if cut.range.is_empty() {
            declarations.len()..declarations.len()
        } else {
            spans[cut.range.start].start..spans[cut.range.end - 1].end
        };
        Self {
            range,
            more: cut.more,
            pagination: cut.facts.to_value(),
        }
    }
}

/// The outline's diagnostics plus what its scan cut: a truncated candidate
/// scan, and files no page outlines.
fn outline_diagnostics(set: &SymbolSet) -> Vec<Value> {
    let mut diagnostics = set.diagnostics.clone();
    if set.truncated {
        let message =
            "Candidate scan stopped at its time budget; files beyond it were not outlined.";
        diagnostics.push(json!({"code":"structural.scan.truncated","message":message}));
    }
    if set.skipped > 0 {
        diagnostics.push(json!({
            "code":"symbols.filesSkipped",
            "message":format!("terminalLimit: {} files were not outlined (parse byte limit, encoding or read failure); no page reaches their declarations.", set.skipped),
        }));
    }
    diagnostics
}

/// `next`: the next page, the scan resume on the window's last page, and
/// the outline's leads (references, text search and read on page 1).
fn attach_symbol_leads(
    q: &AstSearchQuerySymbols,
    set: &SymbolSet,
    out: &mut Value,
    more: bool,
    expand_scan: Option<crate::tools::ast_search::scan_window::ScanWindow>,
) {
    let page = ast_search_query_symbols::page(q).max(1) as usize;
    if more
        && let Some(lead) = crate::response::paging::next_page_call(
            ToolId::AstSearch,
            crate::response::result::query_row(q, &[]),
            "page",
            page + 1,
            &[("snapshot", json!(set.snapshot))],
        )
    {
        out["calls"] = json!({ "nextPage": lead });
    }
    if let Some(window) = expand_scan.filter(|_| !more) {
        let mut nq = crate::response::result::query_row(q, &["snapshot"]);
        nq["scanOffset"] = json!(window.scan_offset);
        nq["page"] = json!(1);
        out["calls"]["expandScan"] =
            crate::response::result::Continuation::continued(ToolId::AstSearch, nq).build();
        crate::tools::ast_search::scan_window::mark_resumed(out);
    }
    if let Some(lead) = &set.references {
        let name = crate::tools::leads::lsp::lead_name(&lead["input"]["queries"][0]);
        out["calls"][name] = lead.clone();
    }
    // The kinds that declare an exact name lead first; the names an exact
    // entry cut follow, a list past the schema's item bound in
    // `broadenSearch2`, ….
    if page == 1 {
        let broaden = set
            .kinds_left_out
            .iter()
            .map(|left_out| left_out.lead(q))
            .chain(set.suppressed.iter().map(|_| Suppressed::lead(q)));
        for (index, lead) in broaden.enumerate() {
            out["calls"][crate::id::numbered("broadenSearch", index)] = lead;
        }
    }
    if let Some(lead) = set.text_search.as_ref().filter(|_| page == 1) {
        out["calls"]["findText"] = lead.clone();
    }
    if let Some(lead) = set.read.as_ref().filter(|_| page == 1) {
        out["calls"]["read"] = lead.clone();
    }
}

/// What an empty outline searched: the name (when asked) and the file count.
fn empty_hint(q: &AstSearchQuerySymbols, files_scanned: usize) -> String {
    let named = ast_search_query_symbols::name_filter(q)
        .map(|filter| {
            let names = filter
                .entries
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>();
            format!("named {} ", names.join(", "))
        })
        .unwrap_or_default();
    let files = if files_scanned == 1 { "file" } else { "files" };
    format!("0 declarations {named}in {files_scanned} {files}.")
}
