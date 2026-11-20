//! Files grouped for the match stream: each file's match sizes (held rows,
//! or prices whose rows are rebuilt when a page shows them), or its listed
//! row; a page fills the slices of the files it shows ([`fill_page`]).
use super::super::RowPaths;
use super::page_rows::PageRows;
use super::query::MatchQuery;
use super::rows::{MatchRow, diag};
use super::scan::parser_extension;
use super::types::{Group, Grouped, Nesting, SKIP_CODES, ScannedFile, Scope};
use super::{file_list, row_chars};
use octocode_engine::num::saturating_u32;
use octocode_engine::structural::{FileMatches, StructuralDiagnostic};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::response::result::ToolError;
use octocode_tools_shared::ast_rule::is_compile_failure;
use serde_json::json;

pub(super) fn group_files(
    q: MatchQuery<'_>,
    files: &[ScannedFile],
    row_paths: RowPaths<'_>,
    cancel: &dyn CancellationCheck,
) -> Result<Grouped, ToolError> {
    let mut grouped = Grouped::default();
    let file_list = file_list(q);
    for (index, file) in files.iter().enumerate() {
        let ScannedFile {
            path,
            matches,
            diagnostics,
            status,
            recovered,
            recovered_bodies,
            ..
        } = file;
        // A file whose parser rejects the query cannot match it: it is outside
        // what the query applies to (`Inapplicable` counts it), not unread. A
        // query no candidate parses never reaches a scan (`resolve_scope`).
        let outside = status != "ok" && diagnostics.iter().any(is_compile_failure);
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
                if diagnostics
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
        for body in recovered_bodies {
            grouped.recovered_bodies.record(path, body);
        }
        let total = matches.len();
        if total > 0 {
            grouped.total_matches += total as u64;
            grouped.groups.push(match matches {
                FileMatches::Held(held) if !file_list => {
                    let rows = held
                        .iter()
                        .map(|value| super::match_row(q, value, cancel))
                        .collect::<Result<Vec<_>, _>>()?;
                    let sizes = rows.iter().map(row_chars).collect();
                    let mut group = matched_group(path.clone(), sizes, row_paths);
                    group.held = Some(rows);
                    group
                }
                FileMatches::Priced(prices) if !file_list => {
                    let sizes = prices.iter().map(|price| *price as usize).collect();
                    let mut group = matched_group(path.clone(), sizes, row_paths);
                    group.priced = Some(index);
                    group
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
    Ok(grouped)
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
        source_descriptors: Vec::new(),
        chars,
        sizes: Vec::new(),
        held: None,
        row,
        truncated_captures: false,
        nested: Nesting::default(),
        priced: None,
    }
}

/// A file with matches of `sizes` (each row's size, held or priced): its
/// header is what it adds to each page its matches are on, sized with its
/// path as delivered.
fn matched_group(path: String, sizes: Vec<usize>, row_paths: RowPaths<'_>) -> Group {
    Group {
        source_descriptors: Vec::new(),
        chars: header_chars(&path, row_paths),
        sizes,
        held: None,
        row: json!({"path": path}),
        truncated_captures: false,
        nested: Nesting::default(),
        priced: None,
    }
}

/// What the header of the file shown as `path` adds to each page its
/// matches are on: its path as delivered and its row's wrapper.
pub(super) fn header_chars(path: &str, row_paths: RowPaths<'_>) -> usize {
    row_paths.chars(path) + 32
}

/// A priced file changed after the scan priced it: its rows can no longer
/// be rebuilt as priced.
#[derive(Debug)]
pub(super) enum RebuildError {
    SourceChanged,
    Evidence(ToolError),
}

/// Show the matches `slices` names of each file a page shows (`groups`, in
/// order): a held file's rows, or a priced file's rows rebuilt from its
/// scanned bytes (see [`octocode_engine::structural::file_matches`]); every
/// rebuilt row must cost what the scan priced it at. A file whose scanned
/// bytes did not finish matching this time (an execution limit) is shown
/// without rows, as the scan shows a file its limit cut: each such file's
/// diagnostics are returned. The files parse in parallel (one parse per
/// shown file was the page's slowest serial step) and are applied in page
/// order. A file whose matches span pages parses once for them all
/// ([`PageRows`]). The file views (no slices) show their rows as listed.
pub(super) fn fill_page(
    q: MatchQuery<'_>,
    scope: &Scope,
    files: &[ScannedFile],
    (groups, slices): (&mut [Group], &[std::ops::Range<usize>]),
    row_paths: RowPaths<'_>,
    page_rows: Option<PageRows<'_>>,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<Vec<StructuralDiagnostic>>, RebuildError> {
    for (group, slice) in groups.iter_mut().zip(slices) {
        if let Some(held) = group.held.take() {
            show(group, &held, slice.clone());
        }
    }
    let mut priced = groups
        .iter()
        .zip(slices)
        .enumerate()
        .filter_map(|(slot, (group, slice))| group.priced.map(|index| (slot, index, slice.clone())))
        .collect::<Vec<_>>();
    // The scan priced only the matches on the scope's side of the
    // test-code line; the rebuild keeps the same ones (it counts nothing).
    let scoped = octocode_tools_shared::test_code::Scoped::of(q.test_code(), row_paths.paths);
    let workers = octocode_tools_shared::workers::workers(priced.len(), None);
    let rebuilt = octocode_tools_shared::workers::each_parallel(
        &mut priced,
        workers,
        |&mut (_, index, _)| match page_rows.and_then(|rows| rows.get(&files[index].path)) {
            Some(rows) => Ok(RebuiltFile::Kept(rows)),
            None => rebuild_file(q, scope, &files[index], (scoped.as_ref(), cancel)),
        },
    );
    let mut incomplete = Vec::new();
    for ((slot, index, slice), outcome) in priced.into_iter().zip(rebuilt) {
        let group = &mut groups[slot];
        group.priced = None;
        match outcome? {
            RebuiltFile::Kept(rows) => show(group, &rows, slice),
            RebuiltFile::Rows(rows) => {
                show(group, &rows, slice.clone());
                // Another page shows the file's other matches.
                if slice.len() < rows.len()
                    && let Some(page_rows) = page_rows
                {
                    page_rows.keep(&files[index].path, rows);
                }
            }
            // Shown with its path alone, as the scan shows a file its limit
            // cut; its sizes stay, so the page cut does not move.
            RebuiltFile::Incomplete(diagnostics) => incomplete.push(diagnostics),
        }
    }
    Ok(incomplete)
}

/// One priced file's rows rebuilt from its scanned bytes, or the
/// diagnostics of a match that did not finish this time.
enum RebuiltFile {
    Kept(std::sync::Arc<Vec<MatchRow>>),
    Rows(Vec<MatchRow>),
    Incomplete(Vec<StructuralDiagnostic>),
}

/// The rows of one priced file, each costing what the scan priced it at.
fn rebuild_file(
    q: MatchQuery<'_>,
    scope: &Scope,
    file: &ScannedFile,
    (scoped, cancel): (
        Option<&octocode_tools_shared::test_code::Scoped>,
        &dyn CancellationCheck,
    ),
) -> Result<RebuiltFile, RebuildError> {
    let (Some((source, digest)), FileMatches::Priced(prices)) = (&file.source, &file.matches)
    else {
        return Err(RebuildError::SourceChanged);
    };
    #[cfg(test)]
    super::page_rows::rebuilt(source);
    let matches = match octocode_engine::structural::file_matches(
        source,
        &parser_extension(scope, source),
        None,
        q.rule().as_ref(),
        digest,
        (
            octocode_runtime_base::security::SourceLimits::REAL.whole_source as u64,
            Some(&super::super::source::judge()),
        ),
        &|| cancel.check().is_err(),
    ) {
        Ok(octocode_engine::structural::Rebuilt::Matches(matches)) => matches,
        Ok(octocode_engine::structural::Rebuilt::Incomplete(diagnostics)) => {
            return Ok(RebuiltFile::Incomplete(diagnostics));
        }
        Err(error) => {
            return Err(RebuildError::Evidence(super::super::errors::native_error(
                error,
            )));
        }
        Ok(octocode_engine::structural::Rebuilt::Changed) => {
            return Err(RebuildError::SourceChanged);
        }
    };
    let rows = matches
        .iter()
        .filter(|m| scoped.is_none_or(|scoped| scoped.sides(source, m.start_line)))
        .map(|m| super::match_row(q, m, cancel))
        .collect::<Result<Vec<_>, _>>()
        .map_err(RebuildError::Evidence)?;
    let repriced = rows.iter().map(|row| saturating_u32(row_chars(row)));
    if !repriced.eq(prices.iter().copied()) {
        return Err(RebuildError::SourceChanged);
    }
    Ok(RebuiltFile::Rows(rows))
}

/// `group`'s row with the matches `slice` a page shows of its file's `rows`.
/// A row another match of the file repeats in line and text, on this page or
/// another, is marked to keep its column
/// ([`octocode_tools_shared::same_line`]).
fn show(group: &mut Group, rows: &[MatchRow], slice: std::ops::Range<usize>) {
    let shares_line = octocode_tools_shared::same_line::repeated(rows, slice.clone(), |row| {
        (
            row.value.get("line").and_then(serde_json::Value::as_u64),
            row.value.get("value").and_then(serde_json::Value::as_str),
        )
    });
    let rows = &rows[slice];
    // Rows whose text was cut to a header or that hide capture text:
    // `includeCaptures:true` (next.expandCaptures) returns them whole.
    group.truncated_captures = rows.iter().any(|row| row.withheld);
    group.nested = nesting(rows);
    group.row["matches"] = rows
        .iter()
        .zip(shares_line)
        .map(|(row, shares_line)| {
            let mut value = row.value.clone();
            if shares_line {
                octocode_tools_shared::same_line::mark(&mut value);
            }
            value
        })
        .collect();
    group.source_descriptors.clear();
    for descriptor in rows
        .iter()
        .filter_map(|row| row.source_descriptors.as_ref())
    {
        if !group
            .source_descriptors
            .iter()
            .any(|known| std::sync::Arc::ptr_eq(known, descriptor))
        {
            group
                .source_descriptors
                .push(std::sync::Arc::clone(descriptor));
        }
    }
}

/// How `rows` (one file's, in any order) nest: each row inside another
/// (a node within another's extent; of two with one extent, the later),
/// and the kinds of the innermost rows inside one, which enclose no other.
fn nesting(rows: &[MatchRow]) -> Nesting {
    let mut order = (0..rows.len()).collect::<Vec<_>>();
    // Outer before inner: by start, then the later end first.
    order.sort_by(|&a, &b| {
        let (a, b) = (&rows[a].node, &rows[b].node);
        a.start.cmp(&b.start).then(b.end.cmp(&a.end))
    });
    let mut open: Vec<usize> = Vec::new();
    let mut inside = vec![false; rows.len()];
    let mut encloses = vec![false; rows.len()];
    for index in order {
        let node = &rows[index].node;
        while open
            .last()
            .is_some_and(|&outer| rows[outer].node.end < node.end)
        {
            open.pop();
        }
        if let Some(&outer) = open.last() {
            inside[index] = true;
            encloses[outer] = true;
        }
        open.push(index);
    }
    Nesting {
        inside: inside.iter().filter(|inside| **inside).count(),
        innermost: (0..rows.len())
            .filter(|&index| inside[index] && !encloses[index])
            .filter_map(|index| rows[index].node.kind.clone())
            .collect(),
    }
}
