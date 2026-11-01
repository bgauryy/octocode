//! The match page a query renders: its file page, coverage diagnostics,
//! warnings and partial reasons, and why an empty page is empty.
use super::continuations::{
    anchor_line, attach_match_continuations, compile_failed_end, tree_lead,
};
use super::gaps::ScopeGaps;
use super::query::MatchQuery;
use super::reads::row_file;
use super::rows::diag;
use super::types::{Group, Grouped, Scan, ScanCut, Scope, recovered_notice, unparsed_notice};
use serde_json::{Value, json};

/// Scan-level coverage diagnostics the per-file groups do not carry.
pub(super) fn scan_diagnostics(scope: &Scope, scan: &Scan) -> Vec<Value> {
    let mut diagnostics = vec![];
    if scan.skipped_by_prefilter > 0 {
        diagnostics.push(json!({
            "code":"structural.prefilter.skipped",
            "severity":"info",
            "stage":"scan",
            "message":format!("Literal-anchor prefilter skipped AST parsing for {} file(s) that cannot contain the anchor.", scan.skipped_by_prefilter),
        }));
    }
    diagnostics.extend(scan.diagnostics.iter().map(diag));
    if scan.truncated {
        let message =
            "Candidate scan stopped at its time budget; files beyond it were not evaluated.";
        diagnostics.push(json!({
            "code":"structural.scan.truncated",
            "severity":"warning",
            "stage":"scan",
            "message":message,
            "path":scope.canonical.to_string_lossy()
        }));
    }
    diagnostics
}

pub(super) fn render_match_page(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
    mut grouped: Grouped,
    snapshot: &str,
    gaps: ScopeGaps,
    presentation: (usize, super::super::RowPaths<'_>),
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Value, crate::response::result::ToolError> {
    let (budget, row_paths) = presentation;
    let truncated = scan.truncated;
    let diagnostics = crate::tools::ast_rule::group_diagnostics(merge_diagnostics(
        std::mem::take(&mut grouped.diagnostics),
        scan_diagnostics(scope, scan),
    ));
    let groups = std::mem::take(&mut grouped.groups);
    let page = file_page(q, &groups, budget);
    let narrow = narrowing_notice(grouped.total_matches, &page);
    let page_groups = &groups[page.window.clone()];
    let more = page.more;
    let mut out =
        json!({"searchEngine":"structural","snapshot":snapshot,"matchCount":grouped.total_matches});
    if let Some(inferred) = &scope.inferred {
        out["inferredLanguage"] = json!(inferred);
    }
    if let Some(excluded) = scan.test_code_excluded.stated() {
        out["testCodeExcluded"] = json!(excluded);
    }
    if !groups.is_empty() {
        out["files"] = Value::Array(listed_rows(page_groups, row_paths));
        out["pagination"] = page.pagination;
    }
    if !diagnostics.is_empty() {
        out["diagnostics"] = json!(diagnostics);
    }
    // What no flag lifts (withheld entries, read errors) is disclosed once,
    // on page 1, in `skippedPaths`, like localSearch's walk.
    let root = scope.canonical.to_string_lossy();
    let errors = &scan.walk_errors;
    let skips = crate::tools::leads::skipped::unlifted(&root, &scan.withheld, errors.into());
    if q.page() == 1
        && let Some(paths) = crate::tools::leads::skipped::skipped_paths(skips, false, truncated)?
    {
        out["skippedPaths"] = serde_json::to_value(paths).unwrap_or_default();
    }
    crate::response::paging::push_warnings(
        &mut out,
        crate::policy::include::parent_glob_warning(
            &q.include().unwrap_or_default(),
            &q.exclude().unwrap_or_default(),
        ),
    );
    // The scan's census, said on its first file page; a later page of the
    // same scan repeats nothing (a scan window reads other files).
    let later_page = ["page", "matchPage"]
        .iter()
        .any(|field| q.to_value()[*field].as_u64().is_some_and(|page| page > 1));
    if !later_page {
        push_census(
            &mut out,
            [
                unparsed_notice(&scan.unparsed),
                recovered_notice(&grouped.recovered),
                narrow,
            ],
            grouped.skipped_grammars.clone(),
        );
    }
    // An empty page explains itself, skipped walks included; the matches of
    // a page that has some are those of the walk the defaults left, so the
    // skipped files of the grammar are named once, beside them, with their
    // lead.
    let mut include_skipped = None;
    if groups.is_empty() {
        explain_empty(
            q,
            &mut out,
            EmptyFacts {
                parsed: grouped.parsed_files,
                scanned: grouped.scanned_files,
                diagnosed: !diagnostics.is_empty(),
            },
            gaps,
        );
    } else if let Some(skipped) = gaps.skipped {
        include_skipped = Some(skipped);
    }
    let more_matches = page_groups.iter().any(|group| group.more_matches);
    let incomplete = mark_coverage_cuts(&mut out, scan, &grouped, more_matches);
    let empty = groups.is_empty() && !more && !incomplete;
    if empty {
        out["status"] = json!("empty");
    }
    // Every file and match is listed and the scan saw its whole scope: the
    // row's expansions and reads are drill-downs, not unread rest.
    let unchecked = include_skipped
        .as_ref()
        .is_some_and(|skipped| skipped.disclosure.unchecked);
    if !groups.is_empty()
        && !more
        && !incomplete
        && scan.withheld.total() == 0
        && !more_matches
        && !unchecked
    {
        out["coverage"] = json!({"state": "complete"});
    }
    attach_match_continuations(
        q,
        scope,
        &mut out,
        page_groups,
        ScanCut {
            truncated,
            resume: scan.scan_resume,
        },
        more,
        snapshot,
    );
    if grouped.compile_failed_files > 0 {
        compile_failed_end(q, scope, &mut out, snapshot);
    }
    // After the page leads, which replace `next`.
    if let Some(skipped) = include_skipped {
        skipped.attach(&mut out);
    }
    // `coverage.state: "complete"` tells a reader the row's pages are
    // drill-downs; a row with no page needs no such note.
    let has_page = out["calls"]
        .as_object()
        .is_some_and(|next| next.values().any(|call| call["kind"] == "continue"));
    if !has_page && let Some(fields) = out.as_object_mut() {
        fields.shift_remove("coverage");
    }
    if empty {
        attach_syntax_lead(q, scope, &mut out, grouped.first_parsed.as_deref());
    }
    attach_source_evidence(&mut out, page_groups, cancel)?;
    Ok(out)
}

/// Mark each coverage cut of the scan as a partial reason, so a partial
/// final page (no `next`) still says why it is partial. Returns whether the
/// scan's coverage is incomplete (any cut but later match pages).
fn mark_coverage_cuts(out: &mut Value, scan: &Scan, grouped: &Grouped, more_matches: bool) -> bool {
    use crate::response::paging::PartialReason;
    let skips = scan.skips;
    let skipped = skips.0 + skips.1 + skips.2 > 0
        || grouped.skipped_files > 0
        || scan.walk_errors.skipped > 0;
    let recovered = grouped.recovered.count > 0;
    let incomplete = scan.truncated
        || skipped
        || recovered
        || grouped.compile_failed_files > 0
        || grouped.unevaluated_files > 0;
    for (cut, reason) in [
        (scan.truncated, super::super::scan_window::CUT_REASON),
        (skipped, PartialReason::FilesSkipped),
        (recovered, PartialReason::ParseRecovered),
        (
            grouped.compile_failed_files > 0,
            PartialReason::CompileFailed,
        ),
        (
            grouped.unevaluated_files > 0,
            PartialReason::FilesUnevaluated,
        ),
        // A file's later match pages (`next.nextMatchPage`): the row's own
        // file pagination does not state them.
        (more_matches, PartialReason::MatchPages),
    ] {
        if cut {
            crate::response::paging::mark_partial(out, reason, &[]);
        }
    }
    incomplete
}

/// An empty match leads to the syntax tree where the query's node shape is
/// compared with the source: the file a single-file scope names, else a
/// near miss, the line of a parsed file that spells the query's anchor.
/// Another file of a directory is no near miss, and a declaration pattern's
/// near miss is the declaration of its kind and name
/// ([`super::exact_miss::exact_miss`]), else the rule listing its kind
/// ([`super::exact_miss::kind_broaden`]).
fn attach_syntax_lead(
    q: MatchQuery<'_>,
    scope: &Scope,
    out: &mut Value,
    first_parsed: Option<&str>,
) {
    let near = first_parsed
        .and_then(|path| row_file(scope, &json!({"path": path})))
        .and_then(|file| anchor_line(q, &file).map(|line| (file, Some(line))));
    let tree = if scope.is_file {
        Some(near.unwrap_or_else(|| (scope.canonical.clone(), None)))
    } else {
        near.filter(|_| super::exact_miss::pattern_declaration(q, scope).is_none())
    };
    if let Some((file, line)) = tree {
        out["calls"]["viewSyntax"] = tree_lead(q, scope, &file, line);
    }
}

fn attach_source_evidence(
    out: &mut Value,
    groups: &[Group],
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<(), crate::response::result::ToolError> {
    let mut projection = crate::response::source_evidence::ProjectionBudget::new(
        crate::response::source_evidence::ProjectionLimits::default(),
    );
    let descriptors = crate::response::source_evidence::merge_descriptors(
        groups
            .iter()
            .flat_map(|group| &group.source_descriptors)
            .map(AsRef::as_ref),
        &mut projection,
        cancel,
    )?;
    super::super::evidence::attach_page(out, &descriptors, cancel)
}

/// The files' diagnostics plus the scan's: the scan repeats the diagnostics
/// of the files it cut short, and each file keeps one.
pub(super) fn merge_diagnostics(mut diagnostics: Vec<Value>, scan: Vec<Value>) -> Vec<Value> {
    for diagnostic in scan {
        if !diagnostics.iter().any(|known| {
            known["code"] == diagnostic["code"]
                && known["path"] == diagnostic["path"]
                && known["message"] == diagnostic["message"]
        }) {
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

/// A first page's census: the static notes (files without a grammar,
/// recovered parses, how to narrow a multi-page set), which the response
/// stage says once, and the skipped-grammar counts.
fn push_census(out: &mut Value, notices: [Option<String>; 3], skipped_grammars: Option<Value>) {
    for notice in notices.into_iter().flatten() {
        crate::response::paging::push_notice(out, notice);
    }
    if let Some(counts) = skipped_grammars {
        out["skippedGrammars"] = counts;
    }
}

/// The first page of a match set that fills more than one page (the
/// response window and `limit` decide, as they cut every page) names its
/// size and how to narrow it before every page is read.
fn narrowing_notice(matches: u64, page: &FilePage) -> Option<String> {
    let pages = page.pagination["totalPages"].as_u64()?;
    let files = page.pagination["totalItems"].as_u64()?;
    (page.more && pages > 1).then(|| {
        format!(
            "{matches} matches in {files} files fill {pages} pages; to read fewer, narrow the rule (pattern, has, inside, not) or the scope (include, exclude, a deeper path)."
        )
    })
}

/// The file page a query asks for.
pub(super) struct FilePage {
    /// Its groups, as a range of every file's groups.
    pub(super) window: std::ops::Range<usize>,
    /// Files are left after it.
    pub(super) more: bool,
    /// Its pagination block.
    pub(super) pagination: Value,
}

/// Cuts `groups` into pages bounded by the response window
/// and picks page `q.page()`; a page past the end is empty.
pub(super) fn file_page(q: MatchQuery<'_>, groups: &[Group], budget: usize) -> FilePage {
    let paging = crate::response::page_windows::Paging::of(q.limit(), budget);
    let sizes = groups.iter().map(|group| group.chars).collect::<Vec<_>>();
    let rows = groups.iter().map(|group| group.rows).collect::<Vec<_>>();
    let cut = crate::response::page_windows::select_weighted(
        paging,
        &sizes,
        Some(&rows),
        q.page() as usize,
    );
    FilePage {
        window: cut.range,
        more: cut.more,
        pagination: cut.facts.to_value(),
    }
}

/// The file rows of a page. A later match page lists only the files it
/// still has matches for: a file whose matches ended on an earlier match
/// page adds no row (unless no file has any left).
fn listed_rows(page_groups: &[Group], row_paths: super::super::RowPaths<'_>) -> Vec<Value> {
    let listed = page_groups
        .iter()
        .filter(|group| !group.out_of_range)
        .collect::<Vec<_>>();
    let rows = if listed.is_empty() {
        page_groups.iter().collect()
    } else {
        listed
    };
    rows.into_iter()
        .map(|group| {
            let mut row = group.row.clone();
            if let Some(path) = row["path"].as_str() {
                row["path"] = json!(row_paths.delivered(path));
            }
            row
        })
        .collect()
}

/// What an empty match page read: the files that parsed and the files read
/// (parsed, or skipped by the anchor prefilter), and whether any diagnostic
/// already explains it.
pub(super) struct EmptyFacts {
    pub(super) parsed: u32,
    pub(super) scanned: u32,
    pub(super) diagnosed: bool,
}

/// The hints and leads of a page without files: no file of the requested
/// grammar parsed (the pattern is not the cause), or the files read hold no
/// match.
pub(super) fn explain_empty(
    q: MatchQuery<'_>,
    out: &mut Value,
    facts: EmptyFacts,
    gaps: ScopeGaps,
) {
    if facts.parsed == 0 && !facts.diagnosed {
        let language = q
            .language()
            .map_or_else(String::new, |language| format!(" {language}"));
        out["hints"] = json!([format!("No{language} file under this path was parsed.")]);
        if let Some(skipped) = gaps.skipped {
            skipped.attach(out);
        }
        // The grammars the scope does hold, each a query that runs.
        if let Some(repair) = gaps.repair {
            out["calls"]["repair"] = repair;
        }
    } else if facts.scanned > 0 {
        // `matchCount: 0` and `filesScanned` state the coverage; a hint
        // only names what the scan skipped.
        out["filesScanned"] = json!(facts.scanned);
        if let Some(skipped) = gaps.skipped {
            skipped.attach(out);
        }
    }
}
