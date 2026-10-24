//! Byte spans a read's patterns select: literal, Rust regex, or PCRE2,
//! and the lines they touch.
use super::super::types::{ExtractionError, RegexSpans, local_fetch_query};
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_engine::text::case_fold::Folded;

/// Byte spans of every pattern (a list matches any of its entries), in
/// source order. `truncated` is set when any regex scan hit its match limit.
pub(super) fn match_spans(
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
    if error.code != ErrorCode::InvalidPattern {
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
pub(crate) const MAX_REGEX_MATCHES: usize = 100_000;

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
            ExtractionError::new(ErrorCode::InvalidPattern, reason)
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
    pub(crate) static RUST_REGEX_RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
                ExtractionError::new(ErrorCode::InvalidPattern, message)
            }
            Pcre2RangesError::Unavailable(message) => {
                ExtractionError::new(ErrorCode::ExecutionFailed, message)
            }
        },
    )?;
    let truncated = spans.len() > MAX_REGEX_MATCHES;
    spans.truncate(MAX_REGEX_MATCHES);
    Ok(RegexSpans { spans, truncated })
}

/// 1-based lines any span touches: every line of a multiline match, not
/// just its start. A match ending at the next line's start does not touch it.
pub(super) fn hit_lines(lines: &[&str], spans: &[(usize, usize)]) -> Vec<usize> {
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
            hits.push(i + 1);
        }
        line_start = line_end;
    }
    hits
}

/// Literal matches in original byte coordinates ([`Folded`] maps the
/// case-insensitive ones back).
#[cfg(test)]
pub(crate) fn literal_ranges(content: &str, pattern: &str, sensitive: bool) -> Vec<(usize, usize)> {
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
