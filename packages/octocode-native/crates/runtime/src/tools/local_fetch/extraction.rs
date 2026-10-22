use super::types::{
    ExtractionError, ExtractionErrorKind, LineRange, LocalFetchQuery, NextCalls, RegexSpans,
    line_read, uncovered,
};
use crate::tools::local_fetch::types::local_fetch_query;
use octocode_engine::text::case_fold::Folded;

pub struct Extraction {
    pub text: String,
    pub source_lines: Option<Vec<usize>>,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub match_ranges: Vec<LineRange>,
    pub matched_lines: Vec<usize>,
    pub count: Option<usize>,
    pub warnings: Vec<String>,
    /// Continuations to the data this view selected but did not return.
    pub next: NextCalls,
    /// The declarations a `block:true` match read widened hits to.
    pub blocks: Vec<super::types::DeclaredBlock>,
    /// A regex scan stopped at its match limit: the selection is a prefix
    /// of the matches (warned, with a `findText` lead that pages them all;
    /// the response delivers it as `hints.findText`).
    pub match_limited: bool,
}

fn follow_up(query: LocalFetchQuery) -> Option<super::types::FetchCall> {
    Some(super::types::FetchCall { query })
}
/// A matched line longer than this (minified or bundled source) is read in
/// byte windows unless the caller chose a context.
const LONG_LINE_BYTES: usize = 2_000;
const LONG_LINE_CONTEXT_BYTES: usize = 200;
/// The rest of a declaration a match window cut is shown inline up to this
/// size (about one lead's worth: tool, path, line span).
const INLINE_REST_BYTES: usize = 200;
/// Largest source whose declarations a match read parses to inline a cut
/// declaration's rest: the parse of a larger file costs far more than the
/// few lines it may add, so such a read keeps its plain context windows
/// (`block` or `ranges` reads the rest, as for a rest over
/// [`INLINE_REST_BYTES`]).
const INLINE_PARSE_MAX_BYTES: usize = 256 * 1024;
/// The `<line>\t` gutter each inlined line adds to the response.
const INLINE_GUTTER_BYTES: usize = 5;

/// View-line sentinel for an omission marker (source lines are 1-based).
pub const OMISSION_LINE: usize = 0;

/// Separates non-adjacent requested windows (ranges or match windows), so a
/// gap never reads as lines cut from a span the read asked for: a requested
/// span is never elided.
pub(super) fn omission_marker(start: usize, end: usize) -> String {
    crate::response::numbered::omission_marker(
        start,
        end,
        crate::response::numbered::Omitted::NotRequested,
    )
}
pub fn line_count(s: &str) -> usize {
    s.split_inclusive('\n').count()
}
pub fn extract(q: &LocalFetchQuery, content: &str) -> Result<Extraction, ExtractionError> {
    if selects_whole(q) {
        return Ok(Extraction::whole(content.into()));
    }
    let patterns = local_fetch_query::match_strings(q);
    let requested = local_fetch_query::line_ranges(q);
    let lines = content.split_inclusive('\n').collect::<Vec<_>>();
    if !patterns.is_empty() {
        return match_extract(q, content, &lines, &patterns);
    }
    ranges_extract(q, content, &lines, &requested)
}

/// A read with no pattern and no range selects the whole text.
pub(super) fn selects_whole(q: &LocalFetchQuery) -> bool {
    local_fetch_query::match_strings(q).is_empty() && local_fetch_query::line_ranges(q).is_empty()
}

impl Extraction {
    /// A read with no pattern and no range: the whole text, as given.
    pub(super) fn whole(text: String) -> Self {
        Self {
            text,
            source_lines: None,
            start: None,
            end: None,
            match_ranges: vec![],
            matched_lines: vec![],
            count: None,
            warnings: vec![],
            next: NextCalls::default(),
            blocks: vec![],
            match_limited: false,
        }
    }
}

/// Line ranges joined into one view: sorted, overlapping or adjacent ranges
/// merged, clamped to the file end; with `block`, each widened to its
/// enclosing declaration.
fn ranges_extract(
    q: &LocalFetchQuery,
    content: &str,
    lines: &[&str],
    requested: &[LineRange],
) -> Result<Extraction, ExtractionError> {
    let total = lines.len();
    let mut warnings = vec![];
    let mut kept: Vec<LineRange> = vec![];
    for range in requested {
        if range.start > total {
            warnings.push(format!(
                "Range {}-{} starts past the file end ({total} lines); skipped.",
                range.start, range.end
            ));
            continue;
        }
        if range.end > total {
            warnings.push(format!(
                "Range {}-{} adjusted to end at {total} (file end).",
                range.start, range.end
            ));
        }
        kept.push(LineRange {
            start: range.start.max(1),
            end: range.end.min(total),
        });
    }
    if kept.is_empty() {
        return Err(ExtractionError::new(
            ExtractionErrorKind::InvalidInput,
            format!("Requested lines start past the file end ({total} lines)"),
        ));
    }
    let mut rest = vec![];
    if local_fetch_query::block(q) {
        kept = super::block::widen_ranges(
            content,
            local_fetch_query::path(q),
            kept,
            &mut warnings,
            &mut rest,
        );
    }
    let windows = merge_ranges(kept);
    let (text, selected) = windows_text(lines, &windows);
    // A plain read that stops inside a declaration offers no reread: the
    // caller chose these lines, and `block` or `ranges` reads the rest.
    let next = NextCalls {
        continue_block: line_read(q, &uncovered(&merge_ranges(rest), &windows)).and_then(follow_up),
        ..NextCalls::default()
    };
    Ok(Extraction {
        text,
        source_lines: Some(selected),
        start: windows.first().map(|r| r.start),
        end: windows.last().map(|r| r.end),
        match_ranges: vec![],
        matched_lines: vec![],
        count: None,
        warnings,
        next,
        blocks: vec![],
        match_limited: false,
    })
}

/// The unseen lines of the first declaration in `blocks` (in hit order)
/// that `shown` does not cover whole; empty when every one is shown.
fn first_unseen(blocks: Vec<LineRange>, shown: &[LineRange]) -> Vec<LineRange> {
    blocks
        .into_iter()
        .map(|block| uncovered(&[block], shown))
        .find(|rest| !rest.is_empty())
        .unwrap_or_default()
}

/// Sorted ranges with overlapping or adjacent ones merged.
fn merge_ranges(ranges: Vec<LineRange>) -> Vec<LineRange> {
    crate::tools::line_spans::merge_spans(ranges.into_iter().map(|range| (range.start, range.end)))
        .into_iter()
        .map(|(start, end)| LineRange { start, end })
        .collect()
}

/// The text of `windows` (sorted, disjoint), with one omission marker
/// between non-adjacent windows; `selected` maps each view line to its
/// source line (markers map to [`OMISSION_LINE`]).
fn windows_text(lines: &[&str], windows: &[LineRange]) -> (String, Vec<usize>) {
    let mut selected = vec![];
    let mut text = String::new();
    let mut prev_end: Option<usize> = None;
    for r in windows {
        if let Some(prev) = prev_end.filter(|prev| r.start > prev + 1) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n')
            }
            text.push_str(&omission_marker(prev + 1, r.start - 1));
            selected.push(OMISSION_LINE)
        }
        for line in r.start..=r.end {
            text.push_str(lines[line - 1]);
            selected.push(line)
        }
        prev_end = Some(r.end)
    }
    (text, selected)
}

/// Byte spans of every pattern (a list matches any of its entries), in
/// source order. `truncated` is set when any regex scan hit its match limit.
fn match_spans(
    q: &LocalFetchQuery,
    content: &str,
    patterns: &[&str],
) -> Result<RegexSpans, ExtractionError> {
    let mut found = RegexSpans::default();
    // Case-insensitive literals share one folded copy of the source.
    let mut folded: Option<Folded<'_>> = None;
    for (index, pattern) in patterns.iter().enumerate() {
        let sensitive = local_fetch_query::case_sensitive_for(q, pattern);
        let name = |error| failing_item(error, index, pattern, patterns.len());
        let one = if local_fetch_query::is_pcre2(q) {
            pcre2_ranges(pattern, sensitive, content).map_err(name)?
        } else if local_fetch_query::is_regex(q) {
            rust_ranges(pattern, sensitive, content).map_err(name)?
        } else {
            RegexSpans {
                spans: if sensitive {
                    sensitive_ranges(content, pattern)
                } else {
                    folded
                        .get_or_insert_with(|| Folded::of(content))
                        .ranges(pattern)
                        .into_iter()
                        .map(|span| (span.start, span.end))
                        .collect()
                },
                truncated: false,
            }
        };
        found.truncated |= one.truncated;
        found
            .spans
            .extend(one.spans.into_iter().filter(|&(start, end)| {
                !local_fetch_query::whole_word(q) || is_whole_word(content, start, end)
            }));
    }
    if patterns.len() > 1 {
        found.spans.sort_unstable();
        found.spans.dedup();
    }
    Ok(found)
}

/// Whether the span `[start, end)` stands alone as a word: no word character
/// (alphanumeric or `_`) touches either side, as in localSearch `wholeWord`.
fn is_whole_word(content: &str, start: usize, end: usize) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    !content[..start].chars().next_back().is_some_and(word)
        && !content[end..].chars().next().is_some_and(word)
}

/// A pattern that does not compile names its item, so the fix lands on the
/// right entry of a list: `matchString[2] "def post("`.
fn failing_item(
    mut error: ExtractionError,
    index: usize,
    pattern: &str,
    count: usize,
) -> ExtractionError {
    if error.kind != ExtractionErrorKind::InvalidPattern {
        return error;
    }
    let item = if count > 1 {
        format!("matchString[{index}]")
    } else {
        "matchString".to_owned()
    };
    let quoted = serde_json::to_string(pattern).unwrap_or_default();
    error.detail = format!("{item} {quoted}: {}", error.detail);
    error
}

/// Most matches one regex read records (`regex:"rust"` and `"pcre2"`). A
/// scan that finds more stops here and says so: the read is partial and
/// its `findText` lead (delivered as `hints.findText`) pages every match
/// through localSearch.
pub(super) const MAX_REGEX_MATCHES: usize = 100_000;

/// `regex:"rust"` spans: the Rust regex engine `localSearch` runs, so a
/// pattern has one verdict and one reason in both tools, whatever the
/// source's size.
fn rust_ranges(
    pattern: &str,
    case_sensitive: bool,
    content: &str,
) -> Result<RegexSpans, ExtractionError> {
    use octocode_engine::search::rust_ranges::RustRangesError;
    use octocode_engine::search::rust_ranges::rust_find_ranges;
    #[cfg(test)]
    RUST_REGEX_RUNS.with(|runs| runs.set(runs.get() + 1));
    // One probe past the limit tells a capped prefix from the whole set.
    let probe = MAX_REGEX_MATCHES + 1;
    let mut spans = rust_find_ranges(pattern, !case_sensitive, content, probe).map_err(
        |RustRangesError::InvalidPattern(reason)| {
            ExtractionError::new(ExtractionErrorKind::InvalidPattern, reason)
        },
    )?;
    let truncated = spans.len() > MAX_REGEX_MATCHES;
    spans.truncate(MAX_REGEX_MATCHES);
    Ok(RegexSpans { spans, truncated })
}

#[cfg(test)]
thread_local! {
    /// `regex:"rust"` scans ([`rust_ranges`]) this thread ran, for tests
    /// that count them.
    pub(super) static RUST_REGEX_RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `regex:"pcre2"` spans through the engine's deadline-bounded PCRE2 scan,
/// with the read's error prefixes (`invalidPattern`, `executionFailed`).
fn pcre2_ranges(
    pattern: &str,
    case_sensitive: bool,
    content: &str,
) -> Result<RegexSpans, ExtractionError> {
    use octocode_engine::search::pcre2_ranges::Pcre2RangesError;
    use octocode_engine::search::pcre2_ranges::pcre2_find_ranges;
    // One probe past the limit tells a capped prefix from the whole set.
    let probe = MAX_REGEX_MATCHES + 1;
    let mut spans = pcre2_find_ranges(pattern, !case_sensitive, content, probe).map_err(
        |error| match error {
            Pcre2RangesError::InvalidPattern(message) => {
                ExtractionError::new(ExtractionErrorKind::InvalidPattern, message)
            }
            Pcre2RangesError::Unavailable(message) => {
                ExtractionError::new(ExtractionErrorKind::ExecutionFailed, message)
            }
        },
    )?;
    let truncated = spans.len() > MAX_REGEX_MATCHES;
    spans.truncate(MAX_REGEX_MATCHES);
    Ok(RegexSpans { spans, truncated })
}

/// A requested line range and the byte span it covers.
type Region = (LineRange, (usize, usize));

/// Each requested range clamped to the file, with its byte span in
/// `lines`' source: where a region grep counts hits.
fn region_bytes(lines: &[&str], ranges: &[LineRange]) -> Vec<Region> {
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut offset = 0;
    for line in lines {
        starts.push(offset);
        offset += line.len();
    }
    starts.push(offset);
    ranges
        .iter()
        .filter(|range| range.start <= lines.len())
        .map(|range| {
            let end = range.end.min(lines.len());
            (
                LineRange {
                    start: range.start,
                    end,
                },
                (starts[range.start - 1], starts[end]),
            )
        })
        .collect()
}

/// 1-based lines any span touches: every line of a multiline match, not
/// just its start. A match ending at the next line's start does not touch it.
fn hit_lines(lines: &[&str], spans: &[(usize, usize)]) -> Vec<usize> {
    let mut hits = vec![];
    let mut line_start = 0;
    let mut next_match = 0;
    for (i, line) in lines.iter().enumerate() {
        let line_end = line_start + line.len();
        while spans
            .get(next_match)
            .is_some_and(|(start, end)| *start < line_start && *end <= line_start)
        {
            next_match += 1;
        }
        if spans
            .get(next_match)
            .is_some_and(|(start, _)| *start < line_end)
        {
            hits.push(i + 1)
        }
        line_start = line_end;
    }
    hits
}

/// A match window that stops inside its declaration shows the rest inline
/// when that costs no more than [`INLINE_REST_BYTES`]; a larger rest stays
/// unshown, with no reread lead (`block` or `ranges` reads it).
fn inline_cut_declarations(
    content: &str,
    q: &LocalFetchQuery,
    lines: &[&str],
    hits: &[usize],
    ranges: &mut Vec<LineRange>,
) {
    let Some(blocks) = super::block::enclosing(content, local_fetch_query::path(q), hits) else {
        return;
    };
    let mut rest = uncovered(&merge_ranges(blocks), ranges);
    let requested = local_fetch_query::line_ranges(q);
    if !requested.is_empty() {
        rest = merge_ranges(
            rest.into_iter()
                .flat_map(|range| {
                    requested.iter().filter_map(move |requested| {
                        let start = range.start.max(requested.start);
                        let end = range.end.min(requested.end);
                        (start <= end).then_some(LineRange { start, end })
                    })
                })
                .collect(),
        );
    }
    let rest_bytes: usize = rest
        .iter()
        .flat_map(|range| &lines[range.start - 1..range.end])
        .map(|line| line.len() + INLINE_GUTTER_BYTES)
        .sum();
    if rest_bytes <= INLINE_REST_BYTES {
        *ranges = merge_ranges(std::mem::take(ranges).into_iter().chain(rest).collect());
    }
}

/// Most bytes of whole lines a default match window (no `contextLines`)
/// shows around each hit: long-line sources get fewer
/// lines, and `next.expandContext` shows the full default window.
const MATCH_WINDOW_BYTES: usize = 2 * 1024;

/// The default window around `hit` (1-based): up to `context` lines on
/// each side while the window's line bytes fit [`MATCH_WINDOW_BYTES`]; the
/// hit line is always whole. `true` when the window was narrowed.
fn byte_bounded_window(lines: &[&str], hit: usize, context: usize) -> (LineRange, bool) {
    let mut used = lines[hit - 1].len();
    let (mut start, mut end) = (hit, hit);
    let (mut before, mut after) = (true, true);
    for _ in 0..context {
        if before && start > 1 && used + lines[start - 2].len() <= MATCH_WINDOW_BYTES {
            start -= 1;
            used += lines[start - 1].len();
        } else {
            before = false;
        }
        if after && end < lines.len() && used + lines[end].len() <= MATCH_WINDOW_BYTES {
            end += 1;
            used += lines[end - 1].len();
        } else {
            after = false;
        }
    }
    let full = LineRange {
        start: hit.saturating_sub(context).max(1),
        end: (hit + context).min(lines.len()),
    };
    let narrowed = start > full.start || end < full.end;
    (LineRange { start, end }, narrowed)
}

/// The same read with the full default window around each hit.
fn expand_context_lead(q: &LocalFetchQuery) -> Option<super::types::FetchCall> {
    let mut wide = q.clone();
    wide.context_lines = Some(DEFAULT_MATCH_CONTEXT_LINES as i64);
    wide.offset = None;
    wide.unit = None;
    wide.length = None;
    wide.snapshot = None;
    follow_up(wide)
}

/// The whole matched lines a long-line match showed only byte windows of.
fn whole_lines_lead(q: &LocalFetchQuery) -> Option<super::types::FetchCall> {
    let mut whole = q.clone();
    whole.context_lines = Some(0);
    whole.offset = None;
    whole.unit = None;
    whole.length = None;
    whole.snapshot = None;
    follow_up(whole)
}

/// Lines shown around each `matchString` hit when `contextLines` is unset:
/// contract `limits.defaultReadContextLines`.
pub(crate) const DEFAULT_MATCH_CONTEXT_LINES: usize = crate::id::DEFAULT_READ_CONTEXT_LINES;

/// The requested ranges' byte spans, with `spans` kept to the hits inside
/// them: with `ranges`, only those hits count, and their windows stay inside
/// the range that holds them. A range past the file end is invalid input.
fn ranged_spans(
    q: &LocalFetchQuery,
    lines: &[&str],
    spans: &mut Vec<(usize, usize)>,
) -> Result<Vec<Region>, ExtractionError> {
    let regions = region_bytes(lines, &local_fetch_query::line_ranges(q));
    if local_fetch_query::has_ranges(q) {
        if regions.is_empty() {
            return Err(ExtractionError::new(
                ExtractionErrorKind::InvalidInput,
                format!(
                    "Requested lines start past the file end ({} lines)",
                    lines.len()
                ),
            ));
        }
        spans.retain(|(start, end)| {
            regions
                .iter()
                .any(|(_, (from, to))| (*from..*to).contains(start) && end <= to)
        });
    }
    Ok(regions)
}

/// Each hit's context window, kept inside the requested range that holds
/// it, and whether a byte bound narrowed any: an unset context sizes each
/// window by bytes as well as lines, so a long-line (minified) source does
/// not return kilobytes per hit.
fn context_windows(
    q: &LocalFetchQuery,
    lines: &[&str],
    hits: &[usize],
    regions: &[Region],
) -> (Vec<LineRange>, bool) {
    let context = local_fetch_query::context_lines(q).unwrap_or(DEFAULT_MATCH_CONTEXT_LINES);
    let default_window = local_fetch_query::context_lines(q).is_none();
    let mut narrowed = false;
    let windows = hits
        .iter()
        .map(|&line| {
            let window = if default_window {
                let (window, cut) = byte_bounded_window(lines, line, context);
                narrowed |= cut;
                window
            } else {
                LineRange {
                    start: line.saturating_sub(context).max(1),
                    end: (line + context).min(lines.len()),
                }
            };
            let region = regions
                .iter()
                .map(|(range, _)| range)
                .find(|range| (range.start..=range.end).contains(&line));
            match region {
                Some(region) => LineRange {
                    start: window.start.max(region.start),
                    end: window.end.min(region.end),
                },
                None => window,
            }
        })
        .collect();
    (windows, narrowed)
}

fn match_extract(
    q: &LocalFetchQuery,
    content: &str,
    lines: &[&str],
    patterns: &[&str],
) -> Result<Extraction, ExtractionError> {
    let RegexSpans {
        mut spans,
        truncated,
    } = match_spans(q, content, patterns)?;
    let regions = ranged_spans(q, lines, &mut spans)?;
    let hits = hit_lines(lines, &spans);
    // The last line the capped scan reached: matches after it went unseen.
    let limit_line = truncated.then(|| hits.last().copied()).flatten();
    if hits.is_empty() {
        return Ok(Extraction {
            text: String::new(),
            source_lines: Some(vec![]),
            start: None,
            end: None,
            match_ranges: vec![],
            matched_lines: vec![],
            count: Some(0),
            warnings: vec![],
            next: NextCalls::default(),
            blocks: vec![],
            match_limited: false,
        });
    }
    let context = local_fetch_query::context_lines(q).unwrap_or(DEFAULT_MATCH_CONTEXT_LINES);
    let mut warnings = vec![];
    let (windows, narrowed) = context_windows(q, lines, &hits, &regions);
    let mut oversized = vec![];
    let mut blocks = vec![];
    let windows = if local_fetch_query::block(q) {
        super::block::widen_matches(
            content,
            local_fetch_query::path(q),
            &hits,
            windows,
            &mut warnings,
            &mut oversized,
            &mut blocks,
        )
    } else {
        windows
    };
    let mut ranges = merge_ranges(windows);
    // A requested declaration too large to show whole is unread evidence of
    // this read: its rest is a page.
    let mut next = NextCalls {
        continue_block: line_read(q, &first_unseen(oversized, &ranges)).and_then(follow_up),
        ..NextCalls::default()
    };
    if !local_fetch_query::block(q)
        && local_fetch_query::context_lines(q) != Some(0)
        && content.len() <= INLINE_PARSE_MAX_BYTES
        && !hits
            .iter()
            .any(|line| lines[line - 1].len() > LONG_LINE_BYTES)
    {
        inline_cut_declarations(content, q, lines, &hits, &mut ranges);
    }
    // `selected` maps each emitted view line to its source line; omission
    // markers between non-adjacent windows map to OMISSION_LINE.
    let (mut text, mut selected) = windows_text(lines, &ranges);
    // `contextLines:0` asks for the matched lines themselves, whole; any
    // other context cannot show neighbouring lines of a minified line
    // without its kilobytes, so the hit's bytes are the window.
    let long_lines = !local_fetch_query::block(q)
        && local_fetch_query::context_lines(q) != Some(0)
        && hits
            .iter()
            .any(|line| lines[line - 1].len() > LONG_LINE_BYTES);
    if narrowed && !long_lines {
        let full = merge_ranges(
            hits.iter()
                .map(|&line| LineRange {
                    start: line.saturating_sub(context).max(1),
                    end: (line + context).min(lines.len()),
                })
                .collect(),
        );
        if !uncovered(&full, &ranges).is_empty() {
            warnings.push(format!(
                "Long lines: each hit shows whole lines within {MATCH_WINDOW_BYTES} bytes, fewer than ±{DEFAULT_MATCH_CONTEXT_LINES}."
            ));
            next.expand_context = expand_context_lead(q);
        }
    }
    if long_lines {
        warnings.push(format!(
            "longMatchedLines: a matched line exceeds {LONG_LINE_BYTES} bytes (minified source), so each match is shown with {LONG_LINE_CONTEXT_BYTES} bytes of context and no neighbouring lines (`contextLines:0` returns the matched lines whole); `... [N bytes omitted] ...` marks gaps."
        ));
        next.whole_lines = whole_lines_lead(q);
    }
    if let Some(line) = limit_line {
        warnings.push(format!(
            "Regex match limit: the scan stopped after {MAX_REGEX_MATCHES} matches at line {line}; matches after it are not selected; narrow the regex to select them."
        ));
        next.text_search = match_limit_lead(q, patterns);
    }
    if long_lines {
        let context_bytes = if regions.is_empty() {
            LONG_LINE_CONTEXT_BYTES
        } else {
            spans = spans
                .into_iter()
                .filter_map(|(start, end)| {
                    let (_, (from, to)) = regions
                        .iter()
                        .find(|(_, (from, to))| (*from..*to).contains(&start))?;
                    Some((
                        start.saturating_sub(LONG_LINE_CONTEXT_BYTES).max(*from),
                        end.saturating_add(LONG_LINE_CONTEXT_BYTES).min(*to),
                    ))
                })
                .collect();
            0
        };
        (text, selected) = byte_windows(content, spans, context_bytes);
        let mut source_lines = selected
            .iter()
            .copied()
            .filter(|line| *line != OMISSION_LINE)
            .collect::<Vec<_>>();
        source_lines.dedup();
        ranges = super::executor::compress_ranges(&source_lines);
    }
    Ok(Extraction {
        text,
        source_lines: Some(selected),
        start: ranges.first().map(|r| r.start),
        end: ranges.last().map(|r| r.end),
        match_ranges: ranges,
        matched_lines: hits.clone(),
        count: Some(hits.len()),
        warnings,
        next,
        blocks,
        match_limited: limit_line.is_some(),
    })
}

/// A localSearch over the same file and pattern(s): it streams and pages
/// every match, past the read's in-process match limit.
fn match_limit_lead(q: &LocalFetchQuery, patterns: &[&str]) -> Option<serde_json::Value> {
    search_lead(q, local_fetch_query::path(q), patterns)
}

/// A localSearch under `path` for exactly what the read looked for, so the
/// lead never matches more than the read would: the regex dialect, each
/// pattern's case rule and `wholeWord` carry over. Several patterns join as
/// one alternation, each keeping its own case rule; literal ones are
/// escaped first.
pub(super) fn search_lead(
    q: &LocalFetchQuery,
    path: &str,
    patterns: &[&str],
) -> Option<serde_json::Value> {
    use crate::id::ToolId;
    let dialect = if local_fetch_query::is_pcre2(q) {
        "pcre2"
    } else {
        "rust"
    };
    let case = |pattern: &str| {
        if local_fetch_query::case_sensitive_for(q, pattern) {
            "sensitive"
        } else {
            "insensitive"
        }
    };
    let mut row = match patterns {
        [] => return None,
        [one] => {
            let mut row = serde_json::json!({
                "path": path,
                "matchString": one,
                "caseMode": case(one),
            });
            if local_fetch_query::is_regex(q) {
                row["regex"] = serde_json::json!(dialect);
            }
            row
        }
        many => {
            let alternation = crate::tools::match_list::alternation(
                many.iter()
                    .map(|pattern| (*pattern, local_fetch_query::case_sensitive_for(q, pattern))),
                local_fetch_query::is_regex(q),
            );
            serde_json::json!({
                "path": path,
                "matchString": alternation,
                "regex": dialect,
                "caseMode": "sensitive",
            })
        }
    };
    if local_fetch_query::whole_word(q) {
        row["wholeWord"] = serde_json::json!(true);
    }
    Some(crate::response::result::Continuation::new(ToolId::LocalSearch, row).build())
}

/// Byte windows of `bytes` around each match span, with `... [N bytes
/// omitted] ...` between them (and, for long lines, before the first and
/// after the last): the text and the source line of every view line.
pub(super) fn byte_windows(
    content: &str,
    spans: Vec<(usize, usize)>,
    bytes: usize,
) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut last_end = 0;
    let mut selected = Vec::new();
    let starts = octocode_engine::text::line_starts(content);
    for (start, end) in spans {
        let mut a = start.saturating_sub(bytes);
        while !content.is_char_boundary(a) {
            a += 1
        }
        let mut b = (end + bytes).min(content.len());
        while b < content.len() && !content.is_char_boundary(b) {
            b += 1
        }
        // Overlapping windows continue from the previous end; re-emitting
        // the overlap would fabricate text that is not in the source.
        if !text.is_empty() {
            a = a.max(last_end)
        }
        if a >= b {
            continue;
        }
        if !text.is_empty() && a > last_end {
            if !text.ends_with('\n') {
                text.push('\n')
            }
            text.push_str(&format!("... [{} bytes omitted] ...\n", a - last_end));
            selected.push(OMISSION_LINE);
        } else if text.is_empty() {
            // The first window starting mid-line says what precedes it.
            let line_start = content[..a].rfind('\n').map_or(0, |at| at + 1);
            if a > line_start {
                text.push_str(&format!("... [{} bytes omitted] ...\n", a - line_start));
                selected.push(OMISSION_LINE);
            }
        }
        let mut source_line = octocode_engine::text::line_at(&starts, a);
        for record in content[a..b].split_inclusive('\n') {
            if text.is_empty() || text.ends_with('\n') {
                selected.push(source_line);
            }
            text.push_str(record);
            source_line += usize::from(record.ends_with('\n'));
        }
        last_end = b
    }
    // The last window ending mid-line says what follows it.
    let line_end = content[last_end..]
        .find('\n')
        .map_or(content.len(), |at| last_end + at);
    if !text.is_empty() && line_end > last_end {
        if !text.ends_with('\n') {
            text.push('\n')
        }
        text.push_str(&format!(
            "... [{} bytes omitted] ...\n",
            line_end - last_end
        ));
        selected.push(OMISSION_LINE);
    }
    (text, selected)
}

/// Literal matches in original byte coordinates ([`Folded`] maps the
/// case-insensitive ones back).
#[cfg(test)]
pub(super) fn literal_ranges(content: &str, pattern: &str, sensitive: bool) -> Vec<(usize, usize)> {
    if sensitive {
        sensitive_ranges(content, pattern)
    } else {
        Folded::of(content)
            .ranges(pattern)
            .into_iter()
            .map(|span| (span.start, span.end))
            .collect()
    }
}

fn sensitive_ranges(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    content
        .match_indices(pattern)
        .map(|(start, matched)| (start, start + matched.len()))
        .collect()
}
