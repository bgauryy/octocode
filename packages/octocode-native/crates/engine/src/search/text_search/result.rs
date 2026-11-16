//! The final result: ordering, span collapsing, and stats.

use super::mode::{Mode, only_matching};
use super::state::{CollectResult, FileRec};
use crate::num::{saturating_i64, saturating_u32};
use crate::search::text_parser::assemble_file;
use crate::types::{
    CapReason, SearchOutput, SearchSort, SpanValues, TextSearchFile, TextSearchMatch,
    TextSearchOptions, TextSearchResult, TextSearchStats,
};
use std::collections::HashMap;

/// Sort `recs` into the final result order; every record is kept.
pub(super) fn sort_recs(opts: &TextSearchOptions, mode: Mode, recs: &mut [FileRec]) {
    recs.sort_by(|a, b| compare_recs(opts, mode, a, b));
}

/// The per-file weight `matchCount` ordering ranks by: the unit the view
/// reports as each file's match count (matched lines for line views, spans or
/// submatches otherwise).
fn rank_weight(opts: &TextSearchOptions, mode: Mode, rec: &FileRec) -> u32 {
    match mode {
        Mode::CountLines => rec.matched_lines,
        Mode::Normal if !only_matching(opts) => rec.matched_lines,
        _ => rec.submatches,
    }
}

/// Total order over collected files, including `sort_reverse`. Every key ends
/// with the path so the order is repeatable.
///
/// * `modified` / `accessed` / `created`: ascending timestamp.
/// * `matchCount`: descending [`rank_weight`] (the most-matched files first).
/// * `path`: lexicographic by full path.
/// * `traversal`: [`walk_order`].
pub(super) fn compare_recs(
    opts: &TextSearchOptions,
    mode: Mode,
    a: &FileRec,
    b: &FileRec,
) -> std::cmp::Ordering {
    let order = match opts.sort {
        SearchSort::Modified | SearchSort::Accessed | SearchSort::Created => a
            .sort_time
            .cmp(&b.sort_time)
            .then_with(|| a.path.cmp(&b.path)),
        SearchSort::MatchCount => rank_weight(opts, mode, b)
            .cmp(&rank_weight(opts, mode, a))
            .then_with(|| a.path.cmp(&b.path)),
        SearchSort::Path => a.path.cmp(&b.path),
        SearchSort::Traversal => walk_order(&a.path, &b.path),
    };
    if opts.sort_reverse.unwrap_or(false) {
        order.reverse()
    } else {
        order
    }
}

/// The walk order of two paths: siblings by name, depth first, so paths
/// compare component by component (`a/x` before `a-b/y` before `a.c`).
pub(crate) fn walk_order(a: &str, b: &str) -> std::cmp::Ordering {
    std::path::Path::new(a).cmp(std::path::Path::new(b))
}

fn collapse_unique_matches(
    matches: Vec<TextSearchMatch>,
    include_counts: bool,
) -> Vec<TextSearchMatch> {
    let mut seen: HashMap<String, usize> = HashMap::with_capacity(matches.len());
    let mut unique: Vec<TextSearchMatch> = Vec::with_capacity(matches.len());

    for mut matched in matches {
        if let Some(index) = seen.get(matched.value.as_str()).copied() {
            if include_counts {
                let next = unique[index].count.unwrap_or(1).saturating_add(1);
                unique[index].count = Some(next);
            }
            continue;
        }

        if include_counts {
            matched.count = Some(1);
        }
        seen.insert(matched.value.clone(), unique.len());
        unique.push(matched);
    }

    if include_counts {
        unique.sort_by_key(|matched| std::cmp::Reverse(matched.count.unwrap_or(1)));
    }

    unique
}

pub(super) fn build_result(
    opts: &TextSearchOptions,
    mode: Mode,
    mut collected: CollectResult,
) -> TextSearchResult {
    let mut recs = std::mem::take(&mut collected.recs);
    sort_recs(opts, mode, &mut recs);
    // The returned files whose values are decoded text, in result order.
    let not_utf8_files = recs
        .iter()
        .filter(|rec| rec.not_utf8)
        .map(|rec| rec.path.clone())
        .collect::<Vec<_>>();
    let utf8_bom_files = recs
        .iter()
        .filter(|rec| rec.utf8_bom)
        .map(|rec| rec.path.clone())
        .collect::<Vec<_>>();
    let files = recs
        .into_iter()
        .map(|rec| file_of(opts, mode, rec))
        .collect();
    let stats = build_stats(opts, mode, collected, (not_utf8_files, utf8_bom_files));
    TextSearchResult { files, stats }
}

/// The file a record is, as the view `mode` lists it.
pub(super) fn file_of(opts: &TextSearchOptions, mode: Mode, r: FileRec) -> TextSearchFile {
    let spans = match opts.output {
        SearchOutput::Spans(spans) => Some(spans),
        _ => None,
    };
    let source = r.source;
    let mut file = match mode {
        Mode::Normal if let Some(spans) = spans => {
            let matches = match spans {
                SpanValues::Every => r.om_matches,
                SpanValues::Unique => collapse_unique_matches(r.om_matches, false),
                SpanValues::Counted => collapse_unique_matches(r.om_matches, true),
            };
            TextSearchFile {
                path: r.path,
                match_count: saturating_u32(matches.len()),
                matches,
                source: None,
                term_counts: Vec::new(),
            }
        }
        Mode::Normal => assemble_file(r.path, r.entry, opts.context),
        // files-only / files-without-match: path list, matchCount 1, no
        // snippets.
        Mode::FilesOnly | Mode::FilesWithoutMatch => TextSearchFile {
            path: r.path,
            match_count: 1,
            matches: Vec::new(),
            source: None,
            term_counts: Vec::new(),
        },
        Mode::CountLines => TextSearchFile {
            path: r.path,
            match_count: r.matched_lines,
            matches: Vec::new(),
            source: None,
            term_counts: r.term_counts,
        },
        Mode::CountMatches => TextSearchFile {
            path: r.path,
            match_count: r.submatches,
            matches: Vec::new(),
            source: None,
            term_counts: r.term_counts,
        },
    };
    file.source = source;
    file
}

/// The stats of `collected` (its files are not read: they are built apart),
/// with the returned files whose values are decoded text.
pub(super) fn build_stats(
    opts: &TextSearchOptions,
    mode: Mode,
    collected: CollectResult,
    (not_utf8_files, utf8_bom_files): (Vec<String>, Vec<String>),
) -> TextSearchStats {
    let CollectResult {
        recs: _,
        files_searched,
        bytes_searched,
        files_matched,
        submatches,
        matched_lines,
        size_skipped,
        binary_quit,
        binary_files,
        binary_file_count,
        skipped_binary_count,
        file_terms_dropped,
        depth_cut_dirs,
        skipped_binary_extensions,
        cancelled,
        error_count,
        first_error,
        pattern_error_count,
        test_code_excluded,
    } = collected;
    // Cap reasons in a fixed order, so equal results carry equal reasons.
    let cap_reasons: Vec<CapReason> = [
        (size_skipped, CapReason::MaxFileSize),
        (cancelled, CapReason::Cancelled),
        (binary_quit, CapReason::BinaryQuit),
    ]
    .into_iter()
    .filter_map(|(hit, reason)| hit.then_some(reason))
    .collect();
    let capped = !cap_reasons.is_empty();

    let files_matched = saturating_u32(files_matched);
    let total_submatches = saturating_u32(submatches);
    let total_matched_lines = saturating_u32(matched_lines);
    let bytes_searched = Some(saturating_i64(bytes_searched));

    // Every view describes the same collection. Count-lines never enumerates
    // submatches, so it has no occurrence count: `match_count` is unknown
    // there (its line count is `matched_lines`); completeness and resource
    // evidence stay shared.
    TextSearchStats {
        match_count: (!matches!(mode, Mode::CountLines)).then_some(total_submatches),
        matched_lines: Some(total_matched_lines),
        files_matched: Some(files_matched),
        files_searched: Some(saturating_u32(files_searched)),
        bytes_searched,
        capped: Some(capped),
        cap_reasons,
        error_count: Some(error_count),
        first_error,
        pattern_error_count: (pattern_error_count > 0).then_some(pattern_error_count),
        test_code_excluded: opts.test_code.as_ref().map(|_| test_code_excluded),
        binary_files: (!binary_files.is_empty()).then_some(binary_files),
        binary_file_count: binary_quit.then_some(binary_file_count),
        not_utf8_files: (!not_utf8_files.is_empty()).then_some(not_utf8_files),
        utf8_bom_files: (!utf8_bom_files.is_empty()).then_some(utf8_bom_files),
        skipped_binary_count: (skipped_binary_count > 0).then_some(skipped_binary_count),
        skipped_binary_extensions: (!skipped_binary_extensions.is_empty())
            .then_some(skipped_binary_extensions),
        file_terms_dropped: (file_terms_dropped > 0).then_some(file_terms_dropped),
        depth_cut_dirs: (depth_cut_dirs > 0).then_some(depth_cut_dirs),
    }
}
