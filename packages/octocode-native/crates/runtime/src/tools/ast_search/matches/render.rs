//! Pages: files grouped into match pages, the page a query renders, why a
//! page is empty, and its continuations.
use super::*;

pub(super) fn group_files(
    q: MatchQuery<'_>,
    files: &[ScannedFile],
    budget: usize,
    inapplicable: &Inapplicable,
) -> Grouped {
    let mut grouped = Grouped::default();
    let file_list = file_list(q);
    let content_length = content_length(q);
    for (index, file) in files.iter().enumerate() {
        let ScannedFile {
            path,
            parser,
            matches,
            diagnostics,
            status,
            recovered,
            ..
        } = file;
        // A file of a grammar the query does not parse in is outside what it
        // applies to (named once in the notice), not a compile failure.
        let outside = status != "ok"
            && diagnostics.iter().any(is_compile_failure)
            && inapplicable.covers(parser);
        match status.as_str() {
            "ok" => {
                grouped.parsed_files += 1;
                grouped.scanned_files += 1;
                grouped.first_parsed.get_or_insert_with(|| path.clone());
                grouped.first_scanned.get_or_insert_with(|| path.clone());
            }
            "skippedByPreFilter" => {
                grouped.scanned_files += 1;
                grouped.first_scanned.get_or_insert_with(|| path.clone());
            }
            _ if outside => {}
            _ => {
                if let Some(diagnostic) = diagnostics.iter().find(|d| is_compile_failure(d)) {
                    grouped.compile_failed_files += 1;
                    grouped.compile_error.get_or_insert_with(|| {
                        ToolError::new(
                            crate::tools::ast_rule::INVALID_PATTERN,
                            diagnostic.message.clone(),
                        )
                    });
                } else if diagnostics
                    .iter()
                    .any(|diagnostic| SKIP_CODES.contains(&diagnostic.code.as_str()))
                {
                    // The scan counts it as a skip (`filesSkipped`).
                    grouped.skipped_files += 1;
                } else {
                    grouped.unevaluated_files += 1;
                }
            }
        }
        if *recovered {
            grouped.recovered.record(path.clone());
        }
        let total = matches.len();
        if total > 0 {
            grouped.total_matches += total as u64;
            grouped.groups.push(match matches {
                FileMatches::Held(held) if !file_list => {
                    let rows = held
                        .iter()
                        .map(|value| {
                            match_value(value, q.capture_text().unwrap_or(false), content_length)
                        })
                        .collect();
                    match_group(q, path.clone(), rows, budget)
                }
                FileMatches::Priced(prices) if !file_list => {
                    priced_group(q, path.clone(), prices, budget, index)
                }
                // A file view lists the file with its count; only a file
                // view's scan counts without pricing.
                _ => listed_file(q, path.clone(), total),
            });
        }
        if !outside && (status != "ok" || !diagnostics.is_empty()) {
            // A literal prefilter can skip thousands of files. Preserve the
            // coverage fact once instead of one identical diagnostic for
            // every skipped path.
            grouped.diagnostics.extend(
                diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code != "structural.prefilter.skipped")
                    .map(diag),
            );
        }
    }
    grouped
}

/// A `files`/`countMatches` view row: the file, with its count when asked.
pub(super) fn listed_file(q: MatchQuery<'_>, path: String, total: usize) -> Group {
    let row = if q.result_view().as_deref() == Some("countMatches") {
        json!({"path":path,"matchCount":total})
    } else {
        json!({"path":path})
    };
    Group {
        chars: octocode_engine::json_size::utf16_units(&row).unwrap_or(0) + 1,
        row,
        out_of_range: false,
        more_matches: false,
        truncated_captures: false,
        cut_chars: None,
        clipped: vec![],
        priced: None,
    }
}

/// One file's match pages, cut from the sizes of its rows.
pub(super) struct MatchPages {
    pub(super) windows: Vec<std::ops::Range<usize>>,
    /// The match page asked for.
    pub(super) page: usize,
    pub(super) out_of_range: bool,
    pub(super) more: bool,
    /// What the file adds to a file page: its first match page.
    pub(super) first_chars: usize,
}

pub(super) fn match_pages(
    q: MatchQuery<'_>,
    path: &str,
    sizes: &[usize],
    budget: usize,
) -> MatchPages {
    let paging = crate::response::page_windows::Paging::of(
        q.match_page_size()
            .map(|size| (size as usize).clamp(1, limits::MATCH_PAGE_SIZE_MAXIMUM)),
        budget,
    );
    let windows = paging.windows(sizes);
    let page = q.match_page().max(1) as usize;
    // The file's place on a file page is its first match page, whichever
    // match page is asked for, so the file pages never move.
    let first_chars = windows.first().map_or(0, |first| {
        sizes[first.clone()].iter().sum::<usize>() + path.len() + 32
    });
    MatchPages {
        out_of_range: page > windows.len(),
        more: page < windows.len(),
        windows,
        page,
        first_chars,
    }
}

/// A priced file's place on the file pages, cut from its prices exactly as
/// [`match_group`] cuts it from its rows; its rows are built only when its
/// file page is shown ([`rebuild_page`]).
pub(super) fn priced_group(
    q: MatchQuery<'_>,
    path: String,
    prices: &[u32],
    budget: usize,
    index: usize,
) -> Group {
    let sizes = prices
        .iter()
        .map(|price| *price as usize)
        .collect::<Vec<_>>();
    let pages = match_pages(q, &path, &sizes, budget);
    Group {
        row: json!({"path": path}),
        chars: pages.first_chars,
        out_of_range: pages.out_of_range,
        more_matches: pages.more,
        truncated_captures: false,
        cut_chars: None,
        clipped: vec![],
        priced: Some(index),
    }
}

/// A priced file changed after the scan priced it: its rows can no longer
/// be rebuilt as priced.
pub(super) struct SourceChanged;

/// Build the rows of the priced files on the file page `groups` shows,
/// from each file's scanned bytes (see
/// [`octocode_engine::structural::file_matches`]); every rebuilt row must
/// cost what the scan priced it at. A file whose scanned bytes did not
/// finish matching this time (an execution limit) is shown without rows,
/// as the scan shows a file its limit cut: each such file's diagnostics
/// are returned. The files parse in parallel (one parse per shown file was
/// the page's slowest serial step) and are applied in page order.
pub(super) fn rebuild_page(
    q: MatchQuery<'_>,
    scope: &Scope,
    files: &[ScannedFile],
    groups: &mut [Group],
    budget: usize,
) -> Result<Vec<Vec<StructuralDiagnostic>>, SourceChanged> {
    let mut priced = groups
        .iter()
        .enumerate()
        .filter_map(|(slot, group)| group.priced.map(|index| (slot, index)))
        .collect::<Vec<_>>();
    let workers = crate::tools::workers::workers(None, priced.len(), None);
    let rebuilt = crate::tools::workers::each_parallel(&mut priced, workers, |&mut (_, index)| {
        rebuild_file(q, scope, &files[index])
    });
    let mut incomplete = Vec::new();
    for ((slot, index), outcome) in priced.into_iter().zip(rebuilt) {
        let path = files[index].path.clone();
        groups[slot] = match outcome? {
            RebuiltFile::Rows(rows) => match_group(q, path, rows, budget),
            RebuiltFile::Incomplete(diagnostics) => {
                incomplete.push(diagnostics);
                unevaluated_group(path)
            }
        };
    }
    Ok(incomplete)
}

/// One priced file's rows rebuilt from its scanned bytes, or the
/// diagnostics of a match that did not finish this time.
enum RebuiltFile {
    Rows(Vec<MatchRow>),
    Incomplete(Vec<StructuralDiagnostic>),
}

/// The rows of one priced file, each costing what the scan priced it at.
fn rebuild_file(
    q: MatchQuery<'_>,
    scope: &Scope,
    file: &ScannedFile,
) -> Result<RebuiltFile, SourceChanged> {
    let (Some((source, digest)), FileMatches::Priced(prices)) = (&file.source, &file.matches)
    else {
        return Err(SourceChanged);
    };
    let matches = match octocode_engine::structural::file_matches(
        source,
        &parser_extension(scope, source),
        q.pattern().as_deref(),
        q.rule().as_deref(),
        digest,
        super::super::MAX_PARSE_SOURCE_BYTES as u64,
    ) {
        Ok(octocode_engine::structural::Rebuilt::Matches(matches)) => matches,
        Ok(octocode_engine::structural::Rebuilt::Incomplete(diagnostics)) => {
            return Ok(RebuiltFile::Incomplete(diagnostics));
        }
        Ok(octocode_engine::structural::Rebuilt::Changed) | Err(_) => {
            return Err(SourceChanged);
        }
    };
    let content_length = content_length(q);
    let rows = matches
        .iter()
        .map(|m| match_value(m, q.capture_text().unwrap_or(false), content_length))
        .collect::<Vec<_>>();
    let repriced = rows.iter().map(|row| u32_of_usize(row_chars(row)));
    if !repriced.eq(prices.iter().copied()) {
        return Err(SourceChanged);
    }
    Ok(RebuiltFile::Rows(rows))
}

/// A shown file whose matches were not evaluated on this page: its path,
/// no rows and no match pages to continue.
pub(super) fn unevaluated_group(path: String) -> Group {
    Group {
        row: json!({"path": path}),
        chars: 0,
        out_of_range: false,
        more_matches: false,
        truncated_captures: false,
        cut_chars: None,
        clipped: vec![],
        priced: None,
    }
}

/// One file's match page, with per-file pagination when it is a subset.
pub(super) fn match_group(
    q: MatchQuery<'_>,
    path: String,
    rows: Vec<MatchRow>,
    budget: usize,
) -> Group {
    let total = rows.len();
    let sizes = rows.iter().map(row_chars).collect::<Vec<_>>();
    let MatchPages {
        windows,
        page: match_page,
        out_of_range,
        more: more_matches,
        first_chars,
    } = match_pages(q, &path, &sizes, budget);
    let window = crate::response::page_windows::nth_page(&windows, match_page)
        .cloned()
        .unwrap_or(total..total);
    let page_rows = &rows[window];
    // Rows whose text was cut to a header or that hide capture text:
    // `captureText:true` (next.expandCaptures) returns them whole.
    let truncated_captures = page_rows.iter().any(|row| row.withheld);
    let cut_chars = page_rows.iter().filter_map(|row| row.cut).max();
    let clipped = page_rows
        .iter()
        .filter(|row| row.shown_cut)
        .map(|row| row.lines)
        .collect();
    let selected = page_rows
        .iter()
        .map(|row| row.value.clone())
        .collect::<Vec<_>>();
    let mut row = json!({"path":path,"matches":selected});
    // A file whose matches span match pages states its page facts (the
    // page stage counts the matches left from them); `next.nextMatchPage`
    // is the continuation, so the facts do not restate it.
    if windows.len() > 1 || out_of_range {
        let facts = match q.match_page_size() {
            Some(size) => crate::response::pages::PageFacts::counted(
                match_page,
                (size as usize).clamp(1, limits::MATCH_PAGE_SIZE_MAXIMUM),
                total,
            ),
            None => crate::response::pages::PageFacts::sized(
                match_page,
                windows.len(),
                total,
                more_matches,
            ),
        };
        row["pagination"] = facts.out_of_range(out_of_range).to_value();
    }
    Group {
        chars: first_chars,
        row,
        out_of_range,
        more_matches,
        truncated_captures,
        cut_chars,
        clipped,
        priced: None,
    }
}

/// Scan-level coverage diagnostics the per-file groups do not carry.
pub(super) fn scan_diagnostics(q: MatchQuery<'_>, scope: &Scope, scan: &Scan) -> Vec<Value> {
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
        let message = match q.max_files() {
            Some(limit) => format!(
                "Candidate scan hit the maxFiles limit ({limit}); files beyond it were not evaluated."
            ),
            None => super::super::scan_window::ceiling_note(scan.scan_resume).map_or_else(
                || {
                    "Candidate scan stopped at its time budget; files beyond it were not evaluated."
                        .to_owned()
                },
                |note| {
                    format!("Candidate scan stopped; files beyond it were not evaluated. {note}")
                },
            ),
        };
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
    grouped: Grouped,
    snapshot: &str,
    gaps: ScopeGaps,
    budget: usize,
) -> Value {
    let truncated = scan.truncated;
    let diagnostics = crate::tools::ast_rule::group_diagnostics(merge_diagnostics(
        grouped.diagnostics,
        scan_diagnostics(q, scope, scan),
    ));
    let groups = grouped.groups;
    let page = file_page(q, &groups, budget);
    let page_groups = &groups[page.window.clone()];
    let more = page.more;
    let mut out =
        json!({"searchEngine":"structural","snapshot":snapshot,"matchCount":grouped.total_matches});
    if let Some(inferred) = &scope.inferred {
        out["inferredLanguage"] = json!(inferred);
    }
    if !groups.is_empty() {
        out["files"] = Value::Array(listed_rows(page_groups));
        out["pagination"] = page.pagination;
    }
    if !diagnostics.is_empty() {
        out["diagnostics"] = json!(diagnostics)
    }
    // Withheld entries are disclosed once, like localSearch's walk.
    let withheld = scan.withheld.notice();
    crate::response::paging::push_warnings(&mut out, withheld.clone());
    crate::response::paging::push_warnings(&mut out, unparsed_notice(&scan.unparsed));
    crate::response::paging::push_warnings(&mut out, grouped.inapplicable_notice.clone());
    crate::response::paging::push_warnings(&mut out, recovered_notice(&grouped.recovered));
    // An empty page explains itself, skipped walks included; the matches of
    // a page that has some are those of the walk the defaults left, so the
    // skipped files of the grammar are named once, beside them, with their
    // lead.
    let mut include_skipped = None;
    if groups.is_empty() {
        explain_empty(
            q,
            scope,
            &mut out,
            EmptyFacts {
                parsed: grouped.parsed_files,
                scanned: grouped.scanned_files,
                diagnosed: !diagnostics.is_empty(),
            },
            gaps,
        );
    } else if let Some(skipped) = gaps.skipped {
        let language = scope.language.as_deref().or(scope.inferred.as_deref());
        crate::response::paging::push_warnings(&mut out, [skipped.notice(language)]);
        include_skipped = Some(skipped.lead);
    }
    let skips = scan.skips;
    let skipped = skips.0 + skips.1 + skips.2 > 0 || grouped.skipped_files > 0;
    let recovered = grouped.recovered.count > 0;
    let incomplete = truncated
        || skipped
        || recovered
        || grouped.compile_failed_files > 0
        || grouped.unevaluated_files > 0;
    let more_matches = page_groups.iter().any(|group| group.more_matches);
    // Each coverage cut names itself, so a partial final page (no `next`)
    // still says why it is partial.
    for (cut, reason) in [
        (
            truncated,
            super::super::scan_window::cut_reason(q.max_files()),
        ),
        (skipped, "filesSkipped"),
        (recovered, "parseRecovered"),
        (grouped.compile_failed_files > 0, "compileFailed"),
        (grouped.unevaluated_files > 0, "filesUnevaluated"),
        // A file's later match pages (`next.nextMatchPage`): the row's own
        // file pagination does not state them.
        (more_matches, "matchPages"),
    ] {
        if cut {
            crate::response::paging::mark_partial(&mut out, reason, &[]);
        }
    }
    let empty = groups.is_empty() && !more && !incomplete;
    if empty {
        out["status"] = json!("empty");
    }
    // Every file and match is listed and the scan saw its whole scope: the
    // row's expansions and reads are drill-downs, not unread rest.
    if !groups.is_empty() && !more && !incomplete && withheld.is_none() && !more_matches {
        out["complete"] = json!(true);
    }
    attach_match_continuations(
        q,
        scope,
        &mut out,
        page_groups,
        ScanCut {
            truncated,
            resume: scan.scan_resume,
            unwalked: scan.unwalked.as_ref(),
        },
        more,
        snapshot,
    );
    if grouped.compile_failed_files > 0 {
        compile_failed_end(q, scope, &mut out, snapshot);
    }
    // After the page leads, which replace `next`.
    if let Some(lead) = include_skipped {
        out["next"]["includeSkipped"] = lead;
    }
    // `complete` tells a reader the row's pages are drill-downs; a row with
    // no page needs no such note.
    let has_page = out["next"].as_object().is_some_and(|next| {
        next.keys()
            .any(|name| crate::id::channel(ToolId::AstSearch, name) == crate::id::Channel::Page)
    });
    if !has_page && let Some(fields) = out.as_object_mut() {
        fields.shift_remove("complete");
    }
    // An empty match leads to the syntax tree of a parsed file (else of a
    // file the anchor prefilter read), where the pattern's node shape can be
    // compared with the source.
    if empty
        && let Some(file) = grouped
            .first_parsed
            .or(grouped.first_scanned)
            .and_then(|path| row_file(scope, &json!({"path": path})))
    {
        out["next"]["viewTree"] = tree_lead(q, scope, &file);
    }
    out
}

/// Files whose grammar the query does not compile in are on no page of any
/// call with this query: a terminal limit (their diagnostics name them and
/// the repair). When one grammar of the scope compiles it, `narrowScope`
/// is the same query over that grammar's files alone, a result with no such
/// gap.
pub(super) fn compile_failed_end(
    q: MatchQuery<'_>,
    scope: &Scope,
    out: &mut Value,
    snapshot: &str,
) {
    crate::response::paging::terminal_limit(out, "compileFailed", &[]);
    if let Some(language) = scope.inferred.as_deref().filter(|_| q.language().is_none()) {
        out["next"]["narrowScope"] = continuation_with(
            q,
            json!({"language": language, "page": 1, "matchPage": 1, "scanOffset": null, "snapshot": null}),
            snapshot,
        );
    }
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

/// The file page a query asks for.
pub(super) struct FilePage {
    /// Its groups, as a range of every file's groups.
    pub(super) window: std::ops::Range<usize>,
    /// Files are left after it.
    pub(super) more: bool,
    /// Its pagination block.
    pub(super) pagination: Value,
}

/// Cuts `groups` into pages (by `pageSize`, else by the response window)
/// and picks page `q.page()`; a page past the end is empty.
pub(super) fn file_page(q: MatchQuery<'_>, groups: &[Group], budget: usize) -> FilePage {
    use crate::response::page_windows::Paging;
    use crate::response::pages::PageFacts;
    let paging = Paging::of(
        q.page_size()
            .map(|size| (size as usize).clamp(1, limits::PAGE_SIZE_MAXIMUM)),
        budget,
    );
    let page = q.page().max(1) as usize;
    let sizes = groups.iter().map(|group| group.chars).collect::<Vec<_>>();
    let windows = paging.windows(&sizes);
    let window = crate::response::page_windows::nth_page(&windows, page)
        .cloned()
        .unwrap_or(groups.len()..groups.len());
    let more = page < windows.len();
    let facts = match paging {
        Paging::Count(size) => PageFacts::counted(page, size, groups.len()),
        Paging::Chars(_) => PageFacts::sized(page, windows.len(), groups.len(), more),
    };
    FilePage {
        window,
        more,
        pagination: facts.out_of_range(page > windows.len()).to_value(),
    }
}

/// The file rows of a page. A later match page lists only the files it
/// still has matches for: a file whose matches ended on an earlier match
/// page adds no row (unless no file has any left).
pub(super) fn listed_rows(page_groups: &[Group]) -> Vec<Value> {
    let listed = page_groups
        .iter()
        .filter(|group| !group.out_of_range)
        .collect::<Vec<_>>();
    let rows = if listed.is_empty() {
        page_groups.iter().collect()
    } else {
        listed
    };
    rows.into_iter().map(|group| group.row.clone()).collect()
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
    scope: &Scope,
    out: &mut Value,
    facts: EmptyFacts,
    gaps: ScopeGaps,
) {
    if facts.parsed == 0 && !facts.diagnosed {
        let language = q
            .language()
            .map_or_else(String::new, |language| format!(" {language}"));
        let mut hint = format!("No{language} file under this path was parsed.");
        if let Some(skipped) = gaps.skipped {
            hint.push(' ');
            hint.push_str(
                &skipped.notice(Some(language.trim_start()).filter(|name| !name.is_empty())),
            );
            out["next"]["includeSkipped"] = skipped.lead;
        }
        out["hints"] = json!([hint]);
        // The grammars the scope does hold, each a query that runs.
        if let Some(repair) = gaps.repair {
            out["next"]["repair"] = repair;
        }
    } else if facts.scanned > 0 {
        // What the empty answer covered: the files read, in which grammar.
        let language = scope.language.clone().or_else(|| {
            crate::tools::ast_rule::file_grammar(&scope.canonical).map(|(grammar, _)| grammar)
        });
        out["filesScanned"] = json!(facts.scanned);
        let mut hints = vec![crate::tools::ast_rule::empty_match_hint(
            facts.scanned,
            language.as_deref(),
        )];
        if let Some(skipped) = gaps.skipped {
            hints.push(skipped.notice(language.as_deref()));
            out["next"]["includeSkipped"] = skipped.lead;
        }
        out["hints"] = json!(hints);
    }
}

/// `next` pages, the scan and value expansions, and the read of the top
/// file's hits.
pub(super) fn attach_match_continuations(
    q: MatchQuery<'_>,
    scope: &Scope,
    out: &mut Value,
    page_groups: &[Group],
    cut: ScanCut<'_>,
    more: bool,
    snapshot: &str,
) {
    let truncated = cut.truncated;
    let page = q.page().max(1) as usize;
    let match_page = q.match_page().max(1) as usize;
    let has_more_matches = page_groups.iter().any(|group| group.more_matches);
    let has_truncated_captures = page_groups.iter().any(|group| group.truncated_captures);
    let cut_chars = page_groups.iter().filter_map(|group| group.cut_chars).max();
    let max_content_length = limits::MATCH_CONTENT_LENGTH_MAXIMUM;
    let content_length = q
        .match_content_length()
        .map_or(limits::MATCH_CONTENT_LENGTH_DEFAULT, |length| {
            length as usize
        })
        .clamp(1, limits::MATCH_CONTENT_LENGTH_MAXIMUM);
    // The scan continues after the files this window evaluated
    // (`scanOffset`), so the lead is offered once the window's last file page
    // is reached.
    let expand_scan = super::super::scan_window::expand_scan(
        truncated,
        q.max_files(),
        cut.resume,
        super::super::scan_window::ScanLimits::MATCH,
    )
    .map(|window| {
        let mut expand =
            json!({"scanOffset":window.scan_offset,"page":1,"matchPage":1,"snapshot":null});
        if let Some(bound) = window.max_files {
            expand["maxFiles"] = json!(bound);
        }
        expand
    });
    if truncated && !more && expand_scan.is_none() {
        crate::response::paging::terminal_limit(
            out,
            super::super::scan_window::cut_reason(q.max_files()),
            &[],
        );
        if let Some(unwalked) = cut.unwalked {
            let query = q.to_value();
            unwalked.attach(out, |reach| super::super::unwalked::narrowed(&query, reach));
        }
    }
    let cursor = |out: &mut Value, field: &str, next: usize| {
        crate::response::paging::next_cursor(out, ToolId::AstSearch, Some("match"), field, next)
    };
    // Finish each file page's matches before advancing the file/scan axis.
    // Otherwise every match page reissues the same next file or scan window.
    let next_page = (more && !has_more_matches)
        .then(|| cursor(out, "page", page + 1))
        .flatten();
    let next_match_page = has_more_matches
        .then(|| cursor(out, "matchPage", match_page + 1))
        .flatten();
    let with = |changes: Value| continuation_with(q, changes, snapshot);
    if let Some(next_page) = next_page {
        // A new file page restarts per-file match pagination.
        out["next"] = json!({"nextPage":with(json!({"page":next_page,"matchPage":1}))})
    }
    if let Some(next_match_page) = next_match_page {
        out["next"]["nextMatchPage"] = with(json!({"matchPage":next_match_page}));
    }
    if let Some(expand) = expand_scan.filter(|_| !more && !has_more_matches) {
        out["next"]["expandScan"] = with(expand);
    }
    // A clipped value is display clipping of a listed match, not a coverage
    // gap: expandCaptures re-runs with captures and the whole length when
    // captures are hidden; otherwise (or past the length maximum) the
    // clipped rows' lines are read whole.
    let whole_length = cut_chars.map(|chars| chars.min(max_content_length));
    let expand_captures = has_truncated_captures && !q.capture_text().unwrap_or(false);
    if expand_captures {
        let mut expand = json!({"captureText":true});
        if let Some(length) = whole_length.filter(|length| *length > content_length) {
            expand["matchContentLength"] = json!(length);
        }
        out["next"]["expandCaptures"] = with(expand);
    }
    if !expand_captures || cut_chars.is_some_and(|chars| chars > max_content_length) {
        read_clipped_lines(scope, out, page_groups);
    }
    if let Some(read) = page_groups
        .first()
        .and_then(|top| read_top_hits(scope, &top.row))
    {
        out["next"]["read"] = read;
    }
}
