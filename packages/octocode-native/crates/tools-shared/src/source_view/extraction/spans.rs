//! Byte spans a read's patterns select: literal, Rust regex, or PCRE2,
//! and the lines they touch.
use super::super::types::{ExtractionError, RegexSpans, local_fetch_query};
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_engine::search::span_sink::SpanSink;
use octocode_engine::text::case_fold::Folded;
use octocode_io::memo::{MemoryBudget, Reservation};
use std::ops::Range;

/// Byte spans of every pattern (a list matches any of its entries), in
/// source order. `truncated` is set when the memory budget had no room for
/// every match of a scan. `written` are the byte ranges of `content` its
/// redactions wrote: a match their own spelling makes is none, as in
/// localSearch ([`octocode_engine::search::text_search::written`]).
pub(super) fn match_spans(
    q: &LocalFetchQuery,
    content: &str,
    written: &[Range<usize>],
    patterns: &[&str],
) -> Result<RegexSpans, ExtractionError> {
    let mut found = RegexSpans::default();
    // Case-insensitive literals share one folded copy of the source.
    let mut folded: Option<Folded<'_>> = None;
    for (index, pattern) in patterns.iter().enumerate() {
        let sensitive = local_fetch_query::case_sensitive_for(q, pattern);
        let name = |error| failing_item(error, index, pattern, patterns.len());
        let mut one = if local_fetch_query::is_pcre2(q) {
            pcre2_ranges(pattern, sensitive, content, written).map_err(name)?
        } else if local_fetch_query::is_regex(q) {
            rust_ranges(pattern, sensitive, content, written).map_err(name)?
        } else {
            RegexSpans {
                held: None,
                spans: if sensitive {
                    sensitive_ranges(content, pattern, written)
                } else {
                    folded_ranges(
                        folded.get_or_insert_with(|| Folded::of(content)),
                        pattern,
                        written,
                    )
                },
                truncated: false,
                missed: Vec::new(),
            }
        };
        found.truncated |= one.truncated;
        if patterns.len() > 1 && one.spans.is_empty() && !one.truncated {
            found.missed.push(index);
        }
        if found.spans.is_empty() && one.held.is_some() {
            // The scan already holds its vector: the spans themselves become
            // the result, held once, with no copy beside them.
            found.spans = std::mem::take(&mut one.spans);
            found.held = one.held.take();
        } else if !append_held(&mut found, &one.spans) {
            found.truncated = true;
            break;
        }
        // `one`'s own hold ends with it: what `found` holds stays held.
    }
    if patterns.len() > 1 {
        found.spans.sort_unstable();
        found.spans.dedup();
    }
    Ok(found)
}

/// Copy `spans` onto `found`'s, holding the copy through `found`'s own
/// reservation before it is allocated (the source copy is still alive, so
/// both are counted). The hold is what the vector allocates: exactly the room
/// the copy needs ([`Vec::reserve_exact`], not `extend`'s rounding and
/// doubling), the new block held on top of the old one while it is copied,
/// then the old block's share given back. `false`, with nothing copied, when
/// the budget has no room for it: the caller reports a partial read.
fn append_held(found: &mut RegexSpans, spans: &[(usize, usize)]) -> bool {
    if found.held.is_none() {
        found.held = span_budget().reserve(0);
    }
    let size = std::mem::size_of::<(usize, usize)>();
    let old = found.spans.capacity();
    let needed = found.spans.len() + spans.len();
    if needed > old {
        if !found
            .held
            .as_mut()
            .is_some_and(|held| held.grow(needed * size))
        {
            return false;
        }
        found.spans.reserve_exact(spans.len());
        if let Some(held) = found.held.as_mut() {
            held.shrink((old + needed).saturating_sub(found.spans.capacity()) * size);
        }
    }
    found.spans.extend_from_slice(spans);
    true
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

/// The budget a regex read's match spans are held against: the process
/// memory budget ([`octocode_engine::memory::MEMORY`]), which also holds
/// every retained store. A scan whose spans no longer fit stops and says so:
/// the read is partial and its `findText` lead (delivered as
/// `hints.findText`) pages every match through localSearch.
pub(crate) fn span_budget() -> &'static MemoryBudget {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(budget) = SPAN_BUDGET.with(std::cell::Cell::get) {
        return budget;
    }
    &octocode_engine::memory::MEMORY
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    /// The budget this thread's regex reads hold their spans against, in
    /// place of the process one.
    static SPAN_BUDGET: std::cell::Cell<Option<&'static MemoryBudget>> =
        const { std::cell::Cell::new(None) };
}

/// Holds a test's budget in place of the process one for this thread's
/// regex reads until it drops, then restores the one before it.
#[cfg(any(test, feature = "test-support"))]
pub struct SpanBudgetOverride(Option<&'static MemoryBudget>);

#[cfg(any(test, feature = "test-support"))]
impl SpanBudgetOverride {
    pub fn of(budget: &'static MemoryBudget) -> Self {
        Self(SPAN_BUDGET.with(|cell| cell.replace(Some(budget))))
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for SpanBudgetOverride {
    fn drop(&mut self) {
        SPAN_BUDGET.with(|cell| cell.set(self.0));
    }
}

/// A scan's matches, held against a budget as the vector grows. A match
/// the budget cannot hold stops the scan: the spans so far are the prefix
/// the read reports.
struct HeldSpans {
    spans: Vec<(usize, usize)>,
    /// `None` when the budget had no room to start.
    held: Option<Reservation<'static>>,
    refused: bool,
}

impl HeldSpans {
    fn new(budget: &'static MemoryBudget) -> Self {
        Self {
            spans: Vec::new(),
            held: budget.reserve(0),
            refused: false,
        }
    }

    fn into_spans(self) -> RegexSpans {
        RegexSpans {
            spans: self.spans,
            truncated: self.refused,
            held: self.held,
            missed: Vec::new(),
        }
    }
}

impl SpanSink for HeldSpans {
    fn accept(&mut self, span: (usize, usize)) -> bool {
        self.accept_observed(span, || {})
    }
}

impl HeldSpans {
    /// [`SpanSink::accept`], calling `at_realloc` once the vector's new block
    /// is held and before it is allocated: the peak a test measures.
    fn accept_observed(&mut self, span: (usize, usize), at_realloc: impl FnOnce()) -> bool {
        if self.spans.len() == self.spans.capacity() {
            // The vector doubles. A realloc may hold the old block and the new
            // one at once, so the whole new block is held before it exists
            // (on top of the old one already held), and the old block's
            // share is returned once the copy is done.
            let old = self.spans.capacity();
            let new = old.max(1) * 2;
            let size = std::mem::size_of::<(usize, usize)>();
            let granted = self.held.as_mut().is_some_and(|held| held.grow(new * size));
            if !granted {
                self.refused = true;
                return false;
            }
            at_realloc();
            self.spans.reserve_exact(new - old);
            if let Some(held) = self.held.as_mut() {
                held.shrink((old + new).saturating_sub(self.spans.capacity()) * size);
            }
        }
        self.spans.push(span);
        true
    }
}

/// `regex:"rust"` spans: the Rust regex engine `localSearch` runs, so a
/// pattern has one verdict and one reason in both tools, whatever the
/// source's size.
fn rust_ranges(
    pattern: &str,
    case_sensitive: bool,
    content: &str,
    written: &[Range<usize>],
) -> Result<RegexSpans, ExtractionError> {
    use octocode_engine::search::rust_ranges::RustRangesError;
    use octocode_engine::search::rust_ranges::rust_scan_ranges;
    #[cfg(any(test, feature = "test-support"))]
    RUST_REGEX_RUNS.with(|runs| runs.set(runs.get() + 1));
    rust_scan_ranges(
        pattern,
        !case_sensitive,
        content,
        written,
        HeldSpans::new(span_budget()),
    )
    .map(HeldSpans::into_spans)
    .map_err(|RustRangesError::InvalidPattern(reason)| {
        ExtractionError::new(ErrorCode::InvalidPattern, reason)
    })
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    /// `regex:"rust"` scans ([`rust_ranges`]) this thread ran, for tests
    /// that count them.
    pub static RUST_REGEX_RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `regex:"pcre2"` spans through the engine's deadline-bounded PCRE2 scan,
/// with the read's error prefixes (`invalidPattern`, `executionFailed`).
fn pcre2_ranges(
    pattern: &str,
    case_sensitive: bool,
    content: &str,
    written: &[Range<usize>],
) -> Result<RegexSpans, ExtractionError> {
    use octocode_engine::search::pcre2_ranges::Pcre2RangesError;
    use octocode_engine::search::pcre2_ranges::pcre2_scan_ranges;
    pcre2_scan_ranges(
        pattern,
        !case_sensitive,
        content,
        written,
        HeldSpans::new(span_budget()),
    )
    .map(HeldSpans::into_spans)
    .map_err(|error| match error {
        Pcre2RangesError::InvalidPattern(message) => {
            ExtractionError::new(ErrorCode::InvalidPattern, message)
        }
        Pcre2RangesError::Unavailable(message) => {
            ExtractionError::new(ErrorCode::ExecutionFailed, message)
        }
    })
}

/// 1-based lines any span touches: every line of a multiline match, not
/// just its start. A match ending at the next line's start does not touch it.
pub(super) fn hit_lines(lines: &super::LineOffsets, spans: &[(usize, usize)]) -> Vec<usize> {
    let mut hits = vec![];
    let mut next_match = 0;
    for (i, (line_start, line_end)) in lines.spans().enumerate() {
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
    }
    hits
}

/// Literal matches in original byte coordinates ([`Folded`] maps the
/// case-insensitive ones back).
#[cfg(any(test, feature = "test-support"))]
pub fn literal_ranges(content: &str, pattern: &str, sensitive: bool) -> Vec<(usize, usize)> {
    if sensitive {
        sensitive_ranges(content, pattern, &[])
    } else {
        folded_ranges(&Folded::of(content), pattern, &[])
    }
}

fn sensitive_ranges(content: &str, pattern: &str, written: &[Range<usize>]) -> Vec<(usize, usize)> {
    if written.is_empty() {
        return content
            .match_indices(pattern)
            .map(|(start, matched)| (start, start + matched.len()))
            .collect();
    }
    let mut spans = Vec::new();
    let Ok(()) = octocode_engine::search::text_search::written::each_own_in(
        content,
        written,
        |text: &str, at| {
            Ok::<_, std::convert::Infallible>(
                (!pattern.is_empty())
                    .then(|| text.get(at..).and_then(|rest| rest.find(pattern)))
                    .flatten()
                    .map(|found| at + found..at + found + pattern.len()),
            )
        },
        |found| {
            spans.push((found.start, found.end));
            true
        },
    );
    spans
}

/// Case-insensitive literal spans in original byte coordinates. The search
/// runs over the folded text, so no written byte can be refilled there: a
/// match taking one is none, and the search goes on from the next folded
/// character, as [`octocode_engine::search::text_search::written::each_own_in`]
/// does for a literal (a literal never matches the filler a refill writes).
fn folded_ranges(
    folded: &Folded<'_>,
    pattern: &str,
    written: &[Range<usize>],
) -> Vec<(usize, usize)> {
    if written.is_empty() {
        return folded
            .ranges(pattern)
            .into_iter()
            .map(|span| (span.start, span.end))
            .collect();
    }
    let needle = pattern.to_lowercase();
    let text = folded.text();
    let mut spans = Vec::new();
    let mut at = 0;
    while !needle.is_empty()
        && let Some(found) = text.get(at..).and_then(|rest| rest.find(&needle))
    {
        let start = at + found;
        let span = folded.span(start..start + needle.len());
        if octocode_engine::search::text_search::written::takes(written, span.clone()) {
            at = start
                + text
                    .get(start..)
                    .and_then(|rest| rest.chars().next())
                    .map_or(1, char::len_utf8);
            continue;
        }
        spans.push((span.start, span.end));
        at = start + needle.len();
    }
    spans
}

#[cfg(test)]
mod tests;
