//! The match page a query renders: its page of the stream, coverage diagnostics,
//! warnings and partial reasons, and why an empty page is empty.
use super::continuations::{anchor_line, attach_match_continuations, tree_lead};
use super::gaps::ScopeGaps;
use super::query::MatchQuery;
use super::reads::row_file;
use super::rows::diag;
use super::snapshot::WindowStart;
use super::stream::StreamPage;
use super::types::{
    Group, Grouped, Scan, ScanCut, Scope, recovered_bodies_notice, recovered_notice,
};
use serde_json::{Value, json};

/// Scan-level coverage diagnostics the per-file groups do not carry. The
/// prefilter's count is the scan's census, said on the scan's first `page`
/// like the rest of it: a later page of the same scan (same snapshot)
/// reads the same files, while a scan window starts its own page 1. The
/// count only explains a result with no match (the skipped files hold none
/// by construction); beside matches it is an optimisation note that decides
/// no next call, so it is not said.
pub(super) fn scan_diagnostics(
    scope: &Scope,
    scan: &Scan,
    page: u32,
    has_matches: bool,
) -> Vec<Value> {
    let mut diagnostics = vec![];
    if scan.skipped_by_prefilter > 0 && page <= 1 && !has_matches {
        diagnostics.push(json!({
            "code":"structural.prefilter.skipped",
            "severity":"info",
            "stage":"scan",
            "message":format!("Literal prefilter skipped AST parsing for {} file(s) that lack a literal every match needs.", scan.skipped_by_prefilter),
        }));
    }
    diagnostics.extend(scan.diagnostics.iter().map(diag));
    if budget_cut(scan) {
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

/// The scan stopped at its time budget before its scope ended, not because
/// its page filled: files past it are a coverage gap until a window reads
/// them.
fn budget_cut(scan: &Scan) -> bool {
    scan.truncated && !scan.filled
}

pub(super) fn render_match_page(
    (build, q): (&str, MatchQuery<'_>),
    scope: &Scope,
    scan: &Scan,
    mut grouped: Grouped,
    snapshot: &str,
    gaps: ScopeGaps,
    presentation: (
        &StreamPage,
        super::super::RowPaths<'_>,
        super::stream::Bounds,
    ),
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<Value, octocode_runtime_base::response::result::ToolError> {
    let (page, row_paths, bounds) = presentation;
    let truncated = scan.truncated;
    let cut = budget_cut(scan);
    let diagnostics = octocode_tools_shared::ast_rule::group_diagnostics(merge_diagnostics(
        std::mem::take(&mut grouped.diagnostics),
        scan_diagnostics(scope, scan, q.page(), grouped.total_matches > 0),
    ));
    let groups = std::mem::take(&mut grouped.groups);
    // A window that ends before its scope does (its page filled, or its
    // time ran out) has no final count: `pagination.total` is a floor
    // (`countScope: loaded`), as a text search's traversal states it. The
    // window that ends the scope counts every window's rows.
    let floor = scan.truncated;
    let start = super::snapshot::listed_before(q);
    // The rows this window lists: in the file views its files, else its
    // stream's matches (past what the window before listed of a file it
    // ended inside, up to where this one ends inside one).
    let listed = if super::file_list(q) {
        groups.len() as u64
    } else {
        bounds.total(&groups) as u64
    };
    // A caller who set `limit` chose the page size: advice to read fewer is
    // noise; the windows before this one are counted, not paged here.
    let narrow = (q.limit().is_none() && !floor && start == WindowStart::default())
        .then(|| narrowing_notice(grouped.total_matches, groups.len(), page))
        .flatten();
    let page_groups = &groups[page.window.clone()];
    let more = page.more;
    let mut out = if floor {
        json!({})
    } else if super::file_list(q) {
        json!({"matchCount": grouped.total_matches.saturating_add(start.before)})
    } else {
        json!({"matchCount": listed.saturating_add(start.before)})
    };
    if let Some(inferred) = &scope.inferred {
        out["inferredLanguage"] = json!(inferred);
    }
    if let Some(excluded) = scan.test_code_excluded.stated() {
        out["testCodeExcluded"] = json!(excluded);
    }
    if !groups.is_empty() {
        out["files"] = Value::Array(listed_rows(page_groups, row_paths));
        out["pagination"] = page.pagination.clone();
        if floor {
            out["pagination"]["countScope"] = json!("loaded");
        }
    }
    if !diagnostics.is_empty() {
        out["diagnostics"] = json!(diagnostics);
    }
    // A file past a limit of its own is named by its diagnostic; the query
    // over each half of it is a call that can answer for it.
    if let Some([first, rest]) = source_limit_leads(q, scope, &diagnostics) {
        out["calls"]["narrowSearch"] = first;
        out["calls"]["narrowRest"] = rest;
    }
    // What the walk left out is disclosed once, on page 1, in
    // `skippedPaths`, like localSearch's walk: what its ignore, dot and
    // default-exclude rules skipped, and what no flag lifts (withheld
    // entries, read errors).
    let errors = &scan.walk_errors;
    let skips = octocode_tools_shared::leads::skipped::ledger_entries(
        &scope.canonical,
        &scan.skipped,
        errors.into(),
    );
    // The grammars this scan reads decide which files no grammar parses
    // leave absence unproven: the census warning counts them, and each is
    // its own `unparsed` entry beside the causes above.
    let read = octocode_tools_shared::leads::unparsed::SourceTypes::read_by(
        scope.language.as_deref().or(scope.inferred.as_deref()),
    );
    // Once per scan: its first page of its first window (every window
    // walks from the first candidate, so a later one would repeat it).
    if q.page() == 1 && q.scan_offset() == 0 {
        let grouped = octocode_tools_shared::leads::skipped::skipped_paths(skips, false, cut)?;
        let unparsed = octocode_tools_shared::leads::skipped::unparsed_paths(
            scan.unparsed.skipped(Some(&read)),
            cut,
        )?;
        if let Some(paths) = octocode_tools_shared::leads::skipped::appended(grouped, unparsed) {
            out["skippedPaths"] = serde_json::to_value(paths).unwrap_or_default();
        }
    }
    octocode_runtime_base::response::paging::push_warnings(
        &mut out,
        octocode_runtime_base::policy::include::parent_glob_warning(
            &q.include().unwrap_or_default(),
            &q.exclude().unwrap_or_default(),
        ),
    );
    // The scan's census, said on its first page; a later page of the same
    // scan repeats nothing (a scan window reads other files).
    if q.page() <= 1 {
        let (unparsed, skipped_grammars) = scan.unparsed.disclosed(
            &super::types::SEARCH,
            Some(&read),
            grouped.skipped_grammars.clone(),
        );
        push_census(
            &mut out,
            [
                unparsed,
                recovered_notice(&grouped.recovered),
                recovered_bodies_notice(&grouped.recovered_bodies),
                narrow,
            ],
            skipped_grammars,
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
    let incomplete = mark_coverage_cuts(&mut out, scan, &grouped);
    let empty = groups.is_empty() && !more && !incomplete;
    if empty {
        out["status"] = json!("empty");
        // An empty row is its empty list and what the scan covered
        // (`filesScanned`, skipped grammars, diagnostics): the counter that
        // says zero again is dropped, as localSearch drops it.
        if let Some(data) = out.as_object_mut() {
            data.shift_remove("matchCount");
            data.shift_insert(0, "files".to_owned(), Value::Array(Vec::new()));
        }
    }
    // The scan saw its whole scope: the row's later pages are pagination of
    // a complete result (the same on every page), and its expansions and
    // reads are drill-downs, not unread rest.
    if !groups.is_empty() && !incomplete && !truncated && scan.skipped.withheld.total() == 0 {
        out["coverage"] = json!({"state": "complete"});
    }
    // The next window counts on after this one's rows, and lists the file
    // this one ended inside from where its pages stopped.
    // A window that ends inside a file binds its pin to this build, whose
    // matches of that file the next window's carry counts.
    let listed_through = scan.listed_through.clone().unwrap_or_default();
    let pin = super::snapshot::counted(
        match scan.page_split {
            Some(_) => super::snapshot::built(&listed_through, build),
            None => listed_through,
        },
        WindowStart {
            before: start.before.saturating_add(listed),
            carry: scan.page_split.map_or(0, u64::from),
        },
    );
    attach_match_continuations(
        q,
        scope,
        &mut out,
        page_groups,
        ScanCut {
            truncated,
            filled: scan.filled,
            resume: scan.scan_resume,
            pin: &pin,
        },
        more,
        snapshot,
    )?;
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
    // A first window with no match and no page after it says how each pattern
    // parsed, beside the syntax lead: a reader compares the two in one step.
    if groups.is_empty() && !more && q.scan_offset() == 0 {
        attach_pattern_shapes(q, scope, &mut out);
    }
    attach_source_evidence(&mut out, page_groups, cancel)?;
    Ok(out)
}

/// The narrower calls, one per half, for the first file the diagnostics name
/// as past a source limit.
fn source_limit_leads(
    q: MatchQuery<'_>,
    scope: &Scope,
    diagnostics: &[Value],
) -> Option<[Value; 2]> {
    let path = diagnostics
        .iter()
        .filter(|d| d["code"] == super::continuations::SOURCE_LIMIT_CODE)
        .find_map(|d| d["path"].as_str())?;
    let file = if std::path::Path::new(path).is_absolute() {
        std::path::PathBuf::from(path)
    } else {
        scope.canonical.join(path)
    };
    super::continuations::narrow_leads(q, &file, path)
}

/// Mark each coverage cut of the scan as a partial reason, so a partial
/// final page (no `next`) still says why it is partial. Returns whether the
/// scan's coverage is incomplete. Match pages are pagination, never a coverage cut.
fn mark_coverage_cuts(out: &mut Value, scan: &Scan, grouped: &Grouped) -> bool {
    use octocode_runtime_base::response::paging::PartialReason;
    let skips = scan.skips;
    let skipped = skips.0 + skips.1 + skips.2 > 0
        || grouped.skipped_files > 0
        || scan.walk_errors.skipped > 0;
    let recovered = grouped.recovered.count > 0 || grouped.recovered_bodies.count > 0;
    let incomplete = budget_cut(scan) || skipped || recovered || grouped.unevaluated_files > 0;
    let unproven = octocode_tools_shared::test_code::unread_note(&scan.test_code_unread);
    let incomplete = incomplete || unproven.is_some();
    if let Some(note) = unproven {
        octocode_runtime_base::response::paging::mark_partial(
            out,
            PartialReason::FilesUnevaluated,
            &[note],
        );
    }
    for (cut, reason) in [
        (budget_cut(scan), super::super::scan_window::CUT_REASON),
        (skipped, PartialReason::FilesSkipped),
        (recovered, PartialReason::ParseRecovered),
        (
            grouped.unevaluated_files > 0,
            PartialReason::FilesUnevaluated,
        ),
    ] {
        if cut {
            octocode_runtime_base::response::paging::mark_partial(out, reason, &[]);
        }
    }
    incomplete
}

/// The message stating how `pattern` parsed.
fn shape_note(pattern: &str, shape: &str) -> String {
    format!("Pattern `{pattern}` parsed as {shape}.")
}

/// One `structural.pattern.shape` note per pattern the rule spells, nested
/// ones included, in each grammar the scope reads
/// ([`super::scope::push_by_grammar`]); a pattern spelled twice is stated
/// once.
fn attach_pattern_shapes(q: MatchQuery<'_>, scope: &Scope, out: &mut Value) {
    let grammars = super::scope::read_grammars(scope);
    let patterns = q.patterns();
    let found = grammars
        .iter()
        .map(|(_, extension)| {
            patterns
                .iter()
                .filter_map(|found| {
                    let pattern = found.written()?;
                    let shape = octocode_engine::structural::pattern_shape(pattern, extension)?;
                    Some(json!({
                        "code": "structural.pattern.shape",
                        "severity": "info",
                        "stage": "match",
                        "message": shape_note(pattern.text(), &shape),
                    }))
                })
                .collect()
        })
        .collect();
    super::scope::push_by_grammar(out, &grammars, found);
}

/// An empty match leads to the syntax tree where the query's node shape is
/// compared with the source: the file a single-file scope names, else a
/// near miss, the line of a parsed file that spells the query's anchor.
/// Another file of a directory is no near miss, and a declaration pattern's
/// near miss is the declaration of its kind and name, beside the rule
/// listing its kind ([`super::exact_miss::exact_miss`]).
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
    if let Some(lead) = tree.and_then(|(file, line)| tree_lead(q, scope, &file, line)) {
        out["calls"]["viewSyntax"] = lead;
    }
}

fn attach_source_evidence(
    out: &mut Value,
    groups: &[Group],
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<(), octocode_runtime_base::response::result::ToolError> {
    let mut projection = octocode_runtime_base::response::source_evidence::ProjectionBudget::new(
        octocode_runtime_base::response::source_evidence::ProjectionLimits::default(),
    );
    let descriptors = octocode_runtime_base::response::source_evidence::merge_descriptors(
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
fn push_census<const N: usize>(
    out: &mut Value,
    notices: [Option<String>; N],
    skipped_grammars: Option<Value>,
) {
    for notice in notices.into_iter().flatten() {
        octocode_runtime_base::response::paging::push_notice(out, notice);
    }
    if let Some(counts) = skipped_grammars {
        out["skippedGrammars"] = counts;
    }
}

/// The first page of a match set that fills more than one page (the
/// response window and `limit` decide, as they cut every page) names its
/// size and how to narrow it before every page is read.
fn narrowing_notice(matches: u64, files: usize, page: &StreamPage) -> Option<String> {
    // A page over the budget is split into parts: its page count is not
    // the walk's number of calls, so none is stated.
    let pages = (!page.oversized).then_some(page.pages)?;
    (page.more && pages > 1).then(|| {
        format!(
            "{matches} matches in {} fill {pages} pages; to read fewer, narrow the rule (pattern, has, inside, not) or the scope (include, exclude, a deeper path).",
            octocode_engine::text::counted(files as u64, "file", "files")
        )
    })
}

/// The file rows of a page, each path as delivered.
fn listed_rows(page_groups: &[Group], row_paths: super::super::RowPaths<'_>) -> Vec<Value> {
    page_groups
        .iter()
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
    // Earlier windows may have parsed what this one did not.
    let unparsed = facts.parsed == 0 && !facts.diagnosed && q.scan_offset() == 0;
    // include/exclude left the walk no file: the globs, not the grammar, are
    // why nothing parsed, unless a skipped walk holds the files.
    let glob_miss = unparsed
        && facts.scanned == 0
        && gaps.skipped.is_none()
        && (q.include().is_some() || q.exclude().is_some());
    if glob_miss {
        octocode_runtime_base::response::result::add_hints(
            out,
            [octocode_runtime_base::policy::include::GLOB_MISS_HINT],
        );
        if let Some(listing) = octocode_tools_shared::leads::files_listing::scope_listing(
            &q.path(),
            Default::default(),
            q.include_ignored() == Some(true),
            !q.default_excludes(),
        ) {
            out["calls"]["viewTree"] = listing;
        }
    } else if unparsed {
        let language = q
            .language()
            .map_or_else(String::new, |language| format!(" {language}"));
        octocode_runtime_base::response::result::add_hints(
            out,
            [format!("No{language} file under this path was parsed.")],
        );
    } else if facts.scanned > 0 || facts.diagnosed {
        // `matchCount: 0` and `filesScanned` state the coverage (none, when
        // every candidate was refused: its diagnostic says why); a hint
        // only names what the scan skipped.
        out["filesScanned"] = json!(facts.scanned);
    }
    // The grammars the scope does hold, each a query that runs: the direct
    // fix, ahead of lifting what the walk skipped.
    if unparsed && let Some(repair) = gaps.repair {
        out["calls"]["repair"] = repair;
    }
    if let Some(skipped) = gaps.skipped {
        skipped.attach(out);
    }
}
