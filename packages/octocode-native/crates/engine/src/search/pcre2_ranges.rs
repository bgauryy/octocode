//! PCRE2 match ranges in one in-memory text (a file read's `regex:"pcre2"`),
//! under the same worker slots as a `-P` search and a wall-clock deadline.
use std::time::Duration;

use grep_matcher::Matcher;

use super::span_sink::{CappedSpans, SpanSink};
use super::text_search::{
    ACTIVE_PCRE2_WORKERS, MAX_ACTIVE_PCRE2_WORKERS, Pcre2WorkerSlot, pcre2_builder,
    release_worker_slot, try_acquire_worker_slot,
};

/// Wall-clock ceiling of one range scan. Nothing bounds the time of one
/// PCRE2 match, and the scan runs inside one uninterruptible `find_iter`, so
/// the caller stops waiting for its worker after
/// `PCRE2_SEARCH_DEADLINE + PCRE2_DEADLINE_GRACE`.
const PCRE2_SEARCH_DEADLINE: Duration = Duration::from_secs(5);

/// Extra time past [`PCRE2_SEARCH_DEADLINE`] the caller waits for a stuck
/// range-scan worker.
const PCRE2_DEADLINE_GRACE: Duration = Duration::from_secs(2);

/// Why a PCRE2 range scan returned no ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pcre2RangesError {
    /// The pattern does not compile.
    InvalidPattern(String),
    /// The scan could not run or finish: worker slots saturated, the
    /// deadline passed, or the worker failed.
    Unavailable(String),
}

/// Byte ranges `[start, end)` of up to `max_matches` matches of `pattern` in
/// `input`, in order. `^`/`$` anchor per line; matches may span lines.
pub fn pcre2_find_ranges(
    pattern: &str,
    case_insensitive: bool,
    input: &str,
    max_matches: usize,
) -> Result<Vec<(usize, usize)>, Pcre2RangesError> {
    pcre2_scan_ranges(
        pattern,
        case_insensitive,
        input,
        CappedSpans::new(max_matches),
    )
    .map(CappedSpans::into_spans)
}

/// Offer every match of `pattern` in `input` to `sink`, in order, until it
/// stops the scan; the sink comes back with what it kept. The sink runs on
/// the scan's worker thread.
pub fn pcre2_scan_ranges<S: SpanSink + Send + 'static>(
    pattern: &str,
    case_insensitive: bool,
    input: &str,
    sink: S,
) -> Result<S, Pcre2RangesError> {
    scan_ranges(
        pattern,
        case_insensitive,
        input,
        sink,
        PCRE2_SEARCH_DEADLINE + PCRE2_DEADLINE_GRACE,
    )
}

fn scan_ranges<S: SpanSink + Send + 'static>(
    pattern: &str,
    case_insensitive: bool,
    input: &str,
    mut sink: S,
    deadline: Duration,
) -> Result<S, Pcre2RangesError> {
    let mut builder = pcre2_builder();
    builder
        .caseless(case_insensitive)
        .multi_line(true)
        .crlf(true);
    let matcher = builder
        .build(pattern)
        .map_err(|error| Pcre2RangesError::InvalidPattern(error.to_string()))?;
    if !try_acquire_worker_slot(&ACTIVE_PCRE2_WORKERS, MAX_ACTIVE_PCRE2_WORKERS) {
        return Err(Pcre2RangesError::Unavailable(
            "too many PCRE2 regex scans are in flight; retry shortly or use regex:\"rust\"".into(),
        ));
    }
    let input = input.to_owned();
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("pcre2-ranges".into())
        .spawn(move || {
            // Release the slot when this thread exits, even after the caller
            // stopped waiting for an uninterruptible match.
            let _slot = Pcre2WorkerSlot;
            let scanned = matcher.find_iter(input.as_bytes(), |found| {
                sink.accept((found.start(), found.end()))
            });
            let _ = tx.send(scanned.map(|()| sink).map_err(|error| error.to_string()));
        });
    if spawned.is_err() {
        // No worker will run to release the reserved slot.
        release_worker_slot(&ACTIVE_PCRE2_WORKERS);
    }
    spawned.map_err(|error| Pcre2RangesError::Unavailable(error.to_string()))?;
    match rx.recv_timeout(deadline) {
        Ok(Ok(sink)) => Ok(sink),
        Ok(Err(error)) => Err(Pcre2RangesError::Unavailable(error)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Err(Pcre2RangesError::Unavailable(format!(
                "PCRE2 regex passed its {}s deadline; simplify it or use regex:\"rust\"",
                deadline.as_secs()
            )))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(Pcre2RangesError::Unavailable(
            "PCRE2 regex worker stopped unexpectedly".into(),
        )),
    }
}

#[cfg(test)]
mod tests;
