//! The final result: ordering, the collection cap, span collapsing, and stats.

use super::DEFAULT_MAX_SNIPPET_CHARS;
use super::collect::collection_limit;
use super::mode::{Mode, lists_match_density, only_matching};
use super::state::{CollectResult, FileRec, saturate_u32};
use crate::search::text_parser::assemble_file;
use crate::types::{
    CapReason, SearchOutput, SearchSort, SpanValues, TextSearchFile, TextSearchMatch,
    TextSearchOptions, TextSearchResult, TextSearchStats,
};
use std::collections::HashMap;

fn bytes_as_i64(bytes: u64) -> i64 {
    bytes.min(i64::MAX as u64) as i64
}

/// Sort `recs` into the final result order and truncate to the collection cap.
/// Returns whether any records were dropped by the cap.
pub(super) fn sort_and_cap(opts: &TextSearchOptions, mode: Mode, recs: &mut Vec<FileRec>) -> bool {
    sort_recs(opts, mode, recs);
    if let Some(max) = collection_limit(opts)
        && recs.len() > max
    {
        recs.truncate(max);
        return true;
    }
    false
}

pub(super) fn sort_recs(opts: &TextSearchOptions, mode: Mode, recs: &mut [FileRec]) {
    if preserves_traversal_order(opts) {
        return;
    }
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
/// with the path so a cap always retains the same records.
///
/// * `modified` / `accessed` / `created`: ascending timestamp.
/// * `matchCount`: descending [`rank_weight`] (the most-matched files survive
///   the collection cap).
/// * `relevance`: an [`identifier_search`](super::mode::identifier_search) first ranks source files whose hit
///   declares the name (the definition answers "where is X"), then descending
///   [`rank_weight`], then source paths before test, generated, and vendored
///   paths, then descending summed line weight (declaration > code >
///   comment/string), see [`relevance`](crate::search::relevance). Path-list views have no per-file
///   density: source paths first, then path.
/// * default and `path`: lexicographic by full path.
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
        SearchSort::Relevance if lists_match_density(mode) => (b.declares && !b.demoted)
            .cmp(&(a.declares && !a.demoted))
            .then_with(|| a.generated.cmp(&b.generated))
            .then_with(|| b.covered.cmp(&a.covered))
            .then_with(|| rank_weight(opts, mode, b).cmp(&rank_weight(opts, mode, a)))
            .then_with(|| a.demoted.cmp(&b.demoted))
            .then_with(|| b.line_weight.cmp(&a.line_weight))
            .then_with(|| a.path.cmp(&b.path)),
        SearchSort::Relevance => a
            .generated
            .cmp(&b.generated)
            .then_with(|| a.demoted.cmp(&b.demoted))
            .then_with(|| a.path.cmp(&b.path)),
        SearchSort::Path | SearchSort::Traversal => a.path.cmp(&b.path),
    };
    if opts.sort_reverse.unwrap_or(false) {
        order.reverse()
    } else {
        order
    }
}

pub(super) fn preserves_traversal_order(opts: &TextSearchOptions) -> bool {
    opts.sort == SearchSort::Traversal
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
    collected: CollectResult,
) -> TextSearchResult {
    let CollectResult {
        mut recs,
        files_searched,
        bytes_searched,
        files_matched,
        submatches,
        matched_lines,
        span_capped,
        timed_out,
        size_skipped,
        binary_quit,
        binary_files,
        binary_file_count,
        skipped_binary_count,
        file_terms_dropped,
        depth_cut_dirs,
        skipped_binary_extensions,
        pruned_dirs,
        cancelled,
        error_count,
        first_error,
        pattern_error_count,
    } = collected;
    // Workers retain a bounded, order-independent candidate set; the final
    // sort + truncate keeps the first `max_collected_files` in result order.
    let retained_truncated = sort_and_cap(opts, mode, &mut recs);
    let cap_truncated = retained_truncated || files_matched > recs.len() as u64;

    // Cap reasons in a fixed order, so equal results carry equal reasons.
    let cap_reasons: Vec<CapReason> = [
        (cap_truncated, CapReason::MaxCollectedFiles),
        (span_capped, CapReason::MaxOnlyMatchingPerFile),
        (timed_out, CapReason::Pcre2Deadline),
        (size_skipped, CapReason::MaxFileSize),
        (cancelled, CapReason::Cancelled),
        (binary_quit, CapReason::BinaryQuit),
    ]
    .into_iter()
    .filter_map(|(hit, reason)| hit.then_some(reason))
    .collect();
    let capped = !cap_reasons.is_empty();

    let context = opts.context;
    let max_snippet = opts.max_snippet_chars.unwrap_or(DEFAULT_MAX_SNIPPET_CHARS) as usize;

    // Totals cover every kept file, including files the cap dropped from the
    // returned list (see `maxCollectedFiles`).
    let files_matched = saturate_u32(files_matched);
    let total_submatches = saturate_u32(submatches);
    let total_matched_lines = saturate_u32(matched_lines);

    let spans = match opts.output {
        SearchOutput::Spans(spans) => Some(spans),
        _ => None,
    };
    let bytes_searched = Some(bytes_as_i64(bytes_searched));
    let files: Vec<TextSearchFile> = recs
        .into_iter()
        .map(|r| {
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
                        match_count: matches.len() as u32,
                        matches,
                        source: None,
                    }
                }
                Mode::Normal => assemble_file(r.path, r.entry, context, max_snippet),
                // files-only / files-without-match: path list, matchCount 1, no
                // snippets.
                Mode::FilesOnly | Mode::FilesWithoutMatch => TextSearchFile {
                    path: r.path,
                    match_count: 1,
                    matches: Vec::new(),
                    source: None,
                },
                Mode::CountLines => TextSearchFile {
                    path: r.path,
                    match_count: r.matched_lines,
                    matches: Vec::new(),
                    source: None,
                },
                Mode::CountMatches => TextSearchFile {
                    path: r.path,
                    match_count: r.submatches,
                    matches: Vec::new(),
                    source: None,
                },
            };
            file.source = source;
            file
        })
        .collect();

    // Every view describes the same collection. Count-lines never enumerates
    // submatches, so it has no occurrence count: `match_count` is unknown
    // there (its line count is `matched_lines`); completeness and resource
    // evidence stay shared.
    let stats = TextSearchStats {
        match_count: (!matches!(mode, Mode::CountLines)).then_some(total_submatches),
        matched_lines: Some(total_matched_lines),
        files_matched: Some(files_matched),
        files_searched: Some(saturate_u32(files_searched)),
        bytes_searched,
        capped: Some(capped),
        cap_reasons,
        error_count: Some(error_count),
        first_error,
        pattern_error_count: (pattern_error_count > 0).then_some(pattern_error_count),
        binary_files: (!binary_files.is_empty()).then_some(binary_files),
        binary_file_count: binary_quit.then_some(binary_file_count),
        skipped_binary_count: (skipped_binary_count > 0).then_some(skipped_binary_count),
        skipped_binary_extensions: (!skipped_binary_extensions.is_empty())
            .then_some(skipped_binary_extensions),
        pruned_dirs: (!pruned_dirs.is_empty()).then_some(pruned_dirs),
        file_terms_dropped: (file_terms_dropped > 0).then_some(file_terms_dropped),
        depth_cut_dirs: (depth_cut_dirs > 0).then_some(depth_cut_dirs),
    };

    TextSearchResult { files, stats }
}
