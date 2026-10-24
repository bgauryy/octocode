//! Files grouped into match pages: each file's match rows (held, priced
//! and rebuilt when shown, or listed), cut into its match pages.
use super::super::RowPaths;
use super::query::MatchQuery;
use super::rows::{MatchRow, diag, match_value};
use super::scan::parser_extension;
use super::scope::Inapplicable;
use super::types::{Group, Grouped, SKIP_CODES, ScannedFile, Scope};
use super::{content_length, file_list, row_chars};
use crate::num::u32_of_usize;
use crate::response::result::ToolError;
use crate::tools::ast_rule::is_compile_failure;
use octocode_contract_types::ErrorCode;
use octocode_engine::structural::{FileMatches, StructuralDiagnostic};
use serde_json::json;

pub(super) fn group_files(
    q: MatchQuery<'_>,
    files: &[ScannedFile],
    budget: usize,
    row_paths: RowPaths<'_>,
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
            }
            "skippedByPreFilter" => {
                grouped.scanned_files += 1;
            }
            _ if outside => {}
            _ => {
                if let Some(diagnostic) = diagnostics.iter().find(|d| is_compile_failure(d)) {
                    grouped.compile_failed_files += 1;
                    grouped.compile_error.get_or_insert_with(|| {
                        ToolError::new(ErrorCode::InvalidPattern, diagnostic.message.clone())
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
                    match_group(q, path.clone(), rows, budget, row_paths)
                }
                FileMatches::Priced(prices) if !file_list => {
                    priced_group(q, path.clone(), prices, budget, row_paths, index)
                }
                // A file view lists the file with its count; only a file
                // view's scan counts without pricing.
                _ => listed_file(q, path.clone(), total, row_paths),
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

/// A `files`/`countMatches` view row: the file, with its count when asked;
/// sized with its path as delivered.
pub(super) fn listed_file(
    q: MatchQuery<'_>,
    path: String,
    total: usize,
    row_paths: RowPaths<'_>,
) -> Group {
    let row = if q.result_view().as_deref() == Some("countMatches") {
        json!({"path":path,"matchCount":total})
    } else {
        json!({"path":path})
    };
    // The row's text with its shown path swapped for the delivered one.
    let chars = octocode_engine::json_size::utf16_units(&row)
        .unwrap_or(0)
        .saturating_sub(octocode_engine::json_size::string_body_units(&path))
        + row_paths.chars(&path)
        + 1;
    Group {
        chars,
        row,
        out_of_range: false,
        more_matches: false,
        truncated_captures: false,
        clipped: vec![],
        priced: None,
    }
}

/// One file's match pages, cut from the sizes of its rows: the match page
/// asked for ([`crate::response::page_windows::select`]) and what the file
/// adds to a file page.
pub(super) struct MatchPages {
    pub(super) cut: crate::response::page_windows::PageCut,
    /// What the file adds to a file page: its first match page.
    pub(super) first_chars: usize,
}

impl MatchPages {
    /// The match page asked for lies past the file's last one.
    pub(super) fn out_of_range(&self) -> bool {
        self.cut.facts.past_end().is_some()
    }
}

pub(super) fn match_pages(
    q: MatchQuery<'_>,
    path: &str,
    sizes: &[usize],
    budget: usize,
    row_paths: RowPaths<'_>,
) -> MatchPages {
    let paging = crate::response::page_windows::Paging::of(None, budget);
    // The file's place on a file page is its first match page, whichever
    // match page is asked for, so the file pages never move.
    let first = crate::response::page_windows::select(paging, sizes, 1).range;
    MatchPages {
        cut: crate::response::page_windows::select(paging, sizes, q.match_page() as usize),
        first_chars: sizes[first].iter().sum::<usize>() + row_paths.chars(path) + 32,
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
    row_paths: RowPaths<'_>,
    index: usize,
) -> Group {
    let sizes = prices
        .iter()
        .map(|price| *price as usize)
        .collect::<Vec<_>>();
    let pages = match_pages(q, &path, &sizes, budget, row_paths);
    Group {
        row: json!({"path": path}),
        chars: pages.first_chars,
        out_of_range: pages.out_of_range(),
        more_matches: pages.cut.more,
        truncated_captures: false,
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
    row_paths: RowPaths<'_>,
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
            RebuiltFile::Rows(rows) => match_group(q, path, rows, budget, row_paths),
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
        None,
        q.rule().as_ref(),
        digest,
        super::super::source::MAX_PARSE_SOURCE_BYTES as u64,
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
    row_paths: RowPaths<'_>,
) -> Group {
    let sizes = rows.iter().map(row_chars).collect::<Vec<_>>();
    let pages = match_pages(q, &path, &sizes, budget, row_paths);
    let (out_of_range, more_matches) = (pages.out_of_range(), pages.cut.more);
    let first_chars = pages.first_chars;
    let window = pages.cut.range.clone();
    let page_rows = &rows[window];
    // Rows whose text was cut to a header or that hide capture text:
    // `captureText:true` (next.expandCaptures) returns them whole.
    let truncated_captures = page_rows.iter().any(|row| row.withheld);
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
    if pages.cut.pages > 1 || out_of_range {
        row["pagination"] = pages.cut.facts.to_value();
    }
    Group {
        chars: first_chars,
        row,
        out_of_range,
        more_matches,
        truncated_captures,
        clipped,
        priced: None,
    }
}
