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
        Census,
        super::super::RowPaths<'_>,
        super::stream::Bounds,
    ),
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<Value, octocode_runtime_base::response::result::ToolError> {
    let (page, census, row_paths, bounds) = presentation;
    let truncated = scan.truncated;
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
    let first_page = q.page() <= 1;
    if first_page && let Some(paths) = &census.skipped_paths {
        out["skippedPaths"] = paths.clone();
    }
    octocode_runtime_base::response::paging::push_warnings(
        &mut out,
        octocode_runtime_base::policy::include::parent_glob_warning(
            &q.include().unwrap_or_default(),
            &q.exclude().unwrap_or_default(),
        ),
    );
    octocode_runtime_base::response::paging::push_warnings(
        &mut out,
        nested_warning(q, page_groups),
    );
    // The scan's census, said on its first page; a later page of the same
    // scan repeats nothing (a scan window reads other files).
    if first_page {
        census.write(&mut out);
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
    // The coverage cuts are the scan's: each page names them; page 1 and
    // the last page, which no next page explains, add the notes.
    let incomplete = mark_coverage_cuts(&mut out, scan, &grouped, first_page || !more);
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

/// Mark each coverage cut of the scan as a partial reason on every page, so
/// a page read alone says why it is partial; the note that explains an
/// unproven test-code verdict is said where `stated` (page 1, beside the
/// census, and the final page). Returns whether the scan's coverage is
/// incomplete. Match pages are pagination, never a coverage cut.
fn mark_coverage_cuts(out: &mut Value, scan: &Scan, grouped: &Grouped, stated: bool) -> bool {
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
        let notes = if stated { vec![note] } else { Vec::new() };
        octocode_runtime_base::response::paging::mark_partial(
            out,
            PartialReason::FilesUnevaluated,
            &notes,
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

/// What a scan's first page carries besides its rows: what the walk left
/// out (`skippedPaths`, on its first window's), the static notes (files
/// without a grammar, recovered parses, how to narrow a multi-page set),
/// which the response stage says once, and the skipped-grammar counts.
/// The page cut charges page 1 for it ([`Census::page`]), so page 1 and
/// its census fit the window every later page fills.
pub(super) struct Census {
    skipped_paths: Option<Value>,
    notices: Vec<String>,
    skipped_grammars: Option<Value>,
    /// How to narrow a set over one page, once the cut says it is.
    narrow: Option<String>,
}

impl Census {
    pub(super) fn of(
        q: MatchQuery<'_>,
        scope: &Scope,
        scan: &Scan,
        grouped: &Grouped,
    ) -> Result<Self, octocode_runtime_base::response::result::ToolError> {
        use octocode_tools_shared::leads::skipped;
        let cut = budget_cut(scan);
        // The grammars this scan reads decide which files no grammar parses
        // leave absence unproven: the census warning counts them, and each is
        // its own `unparsed` entry beside the causes below.
        let read = octocode_tools_shared::leads::unparsed::SourceTypes::read_by(
            scope.language.as_deref().or(scope.inferred.as_deref()),
        );
        // What the walk left out is disclosed once, in `skippedPaths`, like
        // localSearch's walk: what its ignore, dot and default-exclude rules
        // skipped, and what no flag lifts (withheld entries, read errors).
        // Once per scan: on its first window (every window walks from the
        // first candidate, so a later one would repeat it).
        let skipped_paths = if q.scan_offset() == 0 {
            let skips = skipped::ledger_entries(
                &scope.canonical,
                &scan.skipped,
                (&scan.walk_errors).into(),
            );
            let walked = skipped::skipped_paths(skips, false, cut)?;
            let unparsed = skipped::unparsed_paths(scan.unparsed.skipped(Some(&read)), cut)?;
            skipped::appended(walked, unparsed)
                .map(|paths| serde_json::to_value(paths).unwrap_or_default())
        } else {
            None
        };
        // The files no grammar parses and the skipped-grammar counts are the
        // walk's to this window's end, and each window walks from the first
        // candidate: the first window says them, and the final window, whose
        // walk met every file, says them whole; a window between would repeat
        // them.
        let (unparsed, skipped_grammars) = if q.scan_offset() == 0 || !scan.truncated {
            scan.unparsed.disclosed(
                &super::types::SEARCH,
                Some(&read),
                grouped.skipped_grammars.clone(),
            )
        } else {
            (None, None)
        };
        let notices = [
            unparsed,
            recovered_notice(&grouped.recovered),
            recovered_bodies_notice(&grouped.recovered_bodies),
        ]
        .into_iter()
        .flatten()
        .collect();
        Ok(Self {
            skipped_paths,
            notices,
            skipped_grammars,
            narrow: None,
        })
    }

    /// The characters this census adds to page 1, with `narrow` among its
    /// notes.
    fn chars(&self, narrow: Option<&str>) -> usize {
        let mut census = serde_json::Map::new();
        if let Some(paths) = &self.skipped_paths {
            census.insert("skippedPaths".to_owned(), paths.clone());
        }
        if let Some(counts) = &self.skipped_grammars {
            census.insert("skippedGrammars".to_owned(), counts.clone());
        }
        let notes = self
            .notices
            .iter()
            .map(String::as_str)
            .chain(narrow)
            .collect::<Vec<_>>();
        if !notes.is_empty() {
            census.insert("warnings".to_owned(), json!(notes));
        }
        if census.is_empty() {
            return 0;
        }
        octocode_engine::json_size::utf16_units(&census).unwrap_or(0)
    }

    /// Page `q.page()` of `grouped`'s rows within `bounds`, page 1 charged
    /// this census. A set that fills more than one page names its size on
    /// page 1 (the response window and `limit` decide, as they cut every
    /// page): that note is charged too, its page count written at its
    /// widest (a page per match), and then states the count of the cut it
    /// was charged in. The cut depends on the scan alone, so every page of
    /// it cuts alike.
    pub(super) fn page(
        &mut self,
        q: MatchQuery<'_>,
        scan: &Scan,
        grouped: &Grouped,
        (bounds, budget, start): (super::stream::Bounds, usize, WindowStart),
    ) -> StreamPage {
        let cut = |census| super::stream::stream_page(q, &grouped.groups, bounds, (budget, census));
        // A window cut at a start of its pages was cut with the reserve:
        // show its pages as cut.
        let paged_window = scan.truncated && scan.filled && q.scan_offset() == 0;
        let reserved = |census: usize| {
            if paged_window {
                census.max(super::stream::census_reserve(budget))
            } else {
                census
            }
        };
        let page = cut(reserved(self.chars(None)));
        // A caller who set `limit` chose the page size: advice to read fewer
        // is noise; a window's pages count no final total, and the windows
        // before this one are counted, not paged here.
        let narrows = q.limit().is_none() && !scan.truncated && start == WindowStart::default();
        // A page over the budget is split into parts: its page count is not
        // the walk's number of calls, so none is stated.
        if !narrows || page.pages <= 1 || page.oversized {
            return page;
        }
        let files = grouped.groups.len();
        let widest = narrowing_notice(grouped.total_matches, files, grouped.total_matches);
        let page = cut(self.chars(Some(&widest)));
        self.narrow = (!page.oversized)
            .then(|| narrowing_notice(grouped.total_matches, files, page.pages as u64));
        page
    }

    /// Write the census on its page.
    fn write(self, out: &mut Value) {
        for notice in self.notices.into_iter().chain(self.narrow) {
            octocode_runtime_base::response::paging::push_notice(out, notice);
        }
        if let Some(counts) = self.skipped_grammars {
            out["skippedGrammars"] = counts;
        }
    }
}

/// A rule that names no node (no `kind` or `pattern` the node must be)
/// matches every node that satisfies it, so each node around a hit whose
/// text or descendants satisfy it too (a `regex` tests a node's whole
/// text): when the page's rows nest, the warning counts the rows inside
/// another and names the innermost kinds, the nodes a `kind` would keep.
fn nested_warning(q: MatchQuery<'_>, page_groups: &[Group]) -> Option<String> {
    if q.rule().as_ref().is_none_or(names_node) {
        return None;
    }
    let inside = page_groups
        .iter()
        .map(|group| group.nested.inside)
        .sum::<usize>();
    if inside == 0 {
        return None;
    }
    let kinds = page_groups
        .iter()
        .flat_map(|group| group.nested.innermost.iter().map(String::as_str))
        .collect::<std::collections::BTreeSet<_>>();
    let innermost = if kinds.is_empty() {
        String::new()
    } else {
        format!(
            " (innermost here: {})",
            kinds.into_iter().collect::<Vec<_>>().join(", ")
        )
    };
    let lie = if inside == 1 { "lies" } else { "lie" };
    Some(format!(
        "{} on this page {lie} inside another match: a rule with no kind or pattern also matches every node around a hit (regex tests a node's whole text); add kind to keep one node per hit{innermost}.",
        octocode_engine::text::counted(inside as u64, "match", "matches")
    ))
}

/// Whether `rule` fixes the node it matches by kind or pattern: itself, an
/// `all` member, or every `any` branch.
fn names_node(rule: &Value) -> bool {
    rule.get("kind").is_some()
        || rule.get("pattern").is_some()
        || rule["all"]
            .as_array()
            .is_some_and(|all| all.iter().any(names_node))
        || rule["any"]
            .as_array()
            .is_some_and(|any| !any.is_empty() && any.iter().all(names_node))
}

/// How a match set of `matches` in `files` that fills `pages` pages is
/// narrowed before every page is read.
fn narrowing_notice(matches: u64, files: usize, pages: u64) -> String {
    format!(
        "{matches} matches in {} fill {pages} pages; to read fewer, narrow the rule (pattern, has, inside, not) or the scope (include, exclude, a deeper path).",
        octocode_engine::text::counted(files as u64, "file", "files")
    )
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
