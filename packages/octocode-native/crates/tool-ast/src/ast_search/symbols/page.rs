use super::super::RowPaths;
use super::super::judged_rows::NamePlaces;
use super::ast_search_query_symbols;
use super::left_out::Suppressed;
use super::rows::group_by_file;
use super::select::SymbolSet;
use octocode_contract_types::{AstSearchChannel, AstSearchQuerySymbols};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::{Continuation, insert_next};
use octocode_runtime_base::response::symbol_outline::declaration_rows;
use octocode_tools_shared::leads::skipped;
use serde_json::{Value, json};

/// One page of `set`, and where each of its rows was named, in the order
/// the page lists them.
pub(crate) fn render_page(
    q: &AstSearchQuerySymbols,
    set: &SymbolSet,
    row_paths: RowPaths<'_>,
    response_window: Option<usize>,
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<(Value, NamePlaces), octocode_runtime_base::response::result::ToolError> {
    // A page without a snapshot pins nothing: it pages the current outline,
    // as `match` and structureSearch do; only a snapshot that no longer
    // matches is stale.
    if ast_search_query_symbols::page(q) > 1
        && ast_search_query_symbols::snapshot(q).is_some_and(|given| given != set.snapshot)
    {
        return Ok((
            octocode_runtime_base::response::result::stale_snapshot(
                octocode_runtime_base::response::result::restart_query(
                    ToolId::AstSearch,
                    &octocode_runtime_base::response::result::query_row(q, &[]),
                ),
            ),
            NamePlaces::default(),
        ));
    }
    let (skipped, truncated) = (set.skipped, set.truncated);
    let page = ast_search_query_symbols::page(q).max(1) as usize;
    let cut = SymbolPage::of(q, set, response_window);
    let unreadable = set.walk_errors.0 > 0;
    let unproven = octocode_tools_shared::test_code::unread_note(&set.test_code_unread);
    let incomplete = truncated || skipped > 0 || unreadable || unproven.is_some();
    let rows = &set.declarations[cut.range.clone()];
    let places = NamePlaces::of(
        rows,
        set.name_columns.get(cut.range.clone()).unwrap_or_default(),
    );
    // Every scan cut continues ([`scan_window::expand_scan`]): a cut by the
    // scan time budget resumes after the files this window outlined
    // (`scanOffset`), so it is offered once the window's last page is reached.
    let expand_scan = crate::ast_search::scan_window::expand_scan(truncated, set.scan_resume);
    let mut out = json!({"operation":"symbols","filesScanned":set.files_scanned,"filesSkipped":skipped,"diagnostics":outline_diagnostics(set)});
    octocode_runtime_base::response::paging::set_partial(&mut out, incomplete || set.recovered);
    if let Some(excluded) = set.test_code_excluded.stated() {
        out["testCodeExcluded"] = json!(excluded);
    }
    if let Some(note) = unproven {
        octocode_runtime_base::response::paging::mark_partial(
            &mut out,
            octocode_runtime_base::response::paging::PartialReason::FilesUnevaluated,
            &[note],
        );
    }
    // Each coverage cut names itself, so a partial final page (no `next`)
    // still says why it is partial.
    for (cut, reason) in [
        (truncated, crate::ast_search::scan_window::CUT_REASON),
        (
            skipped > 0 || unreadable,
            octocode_runtime_base::response::paging::PartialReason::FilesSkipped,
        ),
        (
            set.recovered,
            octocode_runtime_base::response::paging::PartialReason::ParseRecovered,
        ),
    ] {
        if cut {
            octocode_runtime_base::response::paging::mark_partial(&mut out, reason, &[]);
        }
    }
    // What the walk skipped is disclosed like localSearch does, once, on
    // page 1; an empty outline leads over it in `note_skipped`. What the
    // exact entries left out is a fact of the whole result, so page 1
    // states it once with its lead.
    if page == 1
        && let Some(shown) = set.skipped_paths.as_ref().and_then(|paths| {
            skipped::grouped(
                paths.entries.iter().cloned(),
                true,
                paths.sampled == Some(true),
            )
        })
    {
        out["skippedPaths"] = serde_json::to_value(shown).unwrap_or_default();
    }
    octocode_runtime_base::response::paging::push_warnings(
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
            .chain(octocode_runtime_base::policy::include::parent_glob_warning(
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
    out["pagination"] = cut.pagination;
    // `limit` bounds top-level declarations, each shown with its members, so
    // the rows returned can exceed it: say so where a limit meets members.
    if q.limit.is_some() && rows.len() > cut.top_level {
        octocode_runtime_base::response::paging::push_warnings(
            &mut out,
            [format!(
                "limit counts top-level declarations: {} shown with {} members.",
                cut.top_level,
                rows.len() - cut.top_level
            )],
        );
    }
    if skipped > 0 || unreadable {
        octocode_runtime_base::response::paging::terminal_limit(
            &mut out,
            octocode_runtime_base::response::paging::PartialReason::FilesSkipped,
            &[],
        );
    }
    attach_symbol_leads(q, set, &mut out, cut.more, expand_scan);
    if set.declarations.is_empty() && !incomplete {
        out["status"] = json!("empty");
        octocode_runtime_base::response::result::add_hints(
            &mut out,
            [empty_hint(q, set.files_scanned)],
        );
    }
    super::super::evidence::attach_page(&mut out, &set.source_descriptors, cancel)?;
    Ok((out, places))
}

/// The top-level rows of one symbols page.
struct SymbolPage {
    /// The page's rows (with their members) in the declarations.
    range: std::ops::Range<usize>,
    /// Top-level rows the page holds (`limit` counts these).
    top_level: usize,
    /// Top-level rows are left after the page.
    more: bool,
    /// Its pagination block.
    pagination: Value,
}

impl SymbolPage {
    /// Top-level rows fitting the response budget, at most `limit`; a page
    /// past the end is empty.
    fn of(q: &AstSearchQuerySymbols, set: &SymbolSet, response_window: Option<usize>) -> Self {
        let declarations = &set.declarations;
        let page = ast_search_query_symbols::page(q).max(1) as usize;
        let budget = octocode_runtime_base::response::page_windows::page_budget(response_window);
        let paging = octocode_runtime_base::response::page_windows::Paging::of(
            q.limit
                .map(|limit| octocode_engine::num::saturating_usize(limit.get())),
            budget,
        );
        // Pages count top-level rows; each carries its members.
        let cut = octocode_runtime_base::response::page_windows::select(paging, &set.sizes, page);
        let range = if cut.range.is_empty() {
            declarations.len()..declarations.len()
        } else {
            set.spans[cut.range.start].start..set.spans[cut.range.end - 1].end
        };
        Self {
            range,
            top_level: cut.range.len(),
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
        let message = "Candidate scan stopped at its time or memory budget; files beyond it were not outlined.";
        diagnostics.push(json!({"code":"structural.scan.truncated","message":message}));
    }
    if set.skipped > 0 {
        let mut skipped = json!({
            "code":"symbols.filesSkipped",
            "message":format!("terminalLimit: {} not outlined (parse byte limit, encoding or read failure); no page reaches those declarations.", octocode_engine::text::counted(set.skipped, "file", "files")),
        });
        // A file too large to parse is still searched and read as text: the
        // skip says how, once, as a match's skip of it does.
        let too_large = set.diagnostics.iter().any(|diagnostic| {
            diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.starts_with(TOO_LARGE_SKIP))
        });
        if let Some(recovery) = too_large
            .then(|| {
                octocode_tools_shared::ast_rule::diagnostic_recovery(
                    "structural.file.tooLarge",
                    None,
                )
            })
            .flatten()
        {
            skipped["recovery"] = json!(recovery);
        }
        diagnostics.push(skipped);
    }
    diagnostics
}

/// The scan's code of a file skipped because it is too large to parse, which
/// starts its skip row's message.
const TOO_LARGE_SKIP: &str = "graph.scan.fileTooLarge:";

/// `next`: the next page, the scan resume on the window's last page, and
/// the outline's leads (references and text search on page 1).
fn attach_symbol_leads(
    q: &AstSearchQuerySymbols,
    set: &SymbolSet,
    out: &mut Value,
    more: bool,
    expand_scan: Option<crate::ast_search::scan_window::ScanWindow>,
) {
    let page = ast_search_query_symbols::page(q).max(1) as usize;
    if more
        && let Some(lead) = octocode_runtime_base::response::collection::continued_page(
            ToolId::AstSearch,
            octocode_runtime_base::response::result::query_row(q, &[]),
            "page",
            page + 1,
            &[("snapshot", json!(set.snapshot))],
        )
    {
        out["calls"] =
            octocode_runtime_base::response::collection::walk_calls(ToolId::AstSearch, None, lead);
    }
    if let Some(window) = expand_scan.filter(|_| !more) {
        if let Some(scan_offset) = std::num::NonZeroU64::new(u64::from(window.scan_offset)) {
            let expand = AstSearchQuerySymbols {
                page: std::num::NonZeroU64::MIN,
                scan_offset: Some(scan_offset),
                snapshot: set.scan_pin.as_deref().and_then(|pin| pin.parse().ok()),
                ..q.clone()
            };
            insert_next(
                &mut out["calls"],
                AstSearchChannel::ExpandScan,
                Continuation::continued_query(expand),
            );
        }
        crate::ast_search::scan_window::mark_resumed(out);
    }
    if let Some(lead) = &set.references {
        let name = octocode_tools_shared::leads::lsp::lead_name(&lead["input"]["queries"][0]);
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
        for lead in broaden {
            octocode_runtime_base::response::result::place_numbered(out, "broadenSearch", lead);
        }
    }
    if let Some(lead) = set.text_search.as_ref().filter(|_| page == 1) {
        out["calls"]["findText"] = lead.clone();
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
