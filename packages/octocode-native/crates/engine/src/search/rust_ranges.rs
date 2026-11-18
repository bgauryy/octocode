//! Rust-regex match ranges in one in-memory text (a file read's
//! `regex:"rust"`). It is the engine `localSearch` runs for the same value (the
//! `regex` crate, `^`/`$` per line), so one pattern gets one verdict and one
//! error reason in both tools. It is linear-time: no deadline, no worker, and
//! the verdict never depends on the text's size.
use super::span_sink::{CappedSpans, SpanSink};
use regex::RegexBuilder;

/// Why a Rust-regex range scan returned no ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustRangesError {
    /// The pattern does not compile; the parser's reason, without the
    /// echoed pattern (`unclosed group`, `backreferences are not supported`).
    InvalidPattern(String),
}

/// Byte ranges `[start, end)` of up to `max_matches` matches of `pattern` in
/// `input`, in order. `^`/`$` anchor per line; matches may span lines.
pub fn rust_find_ranges(
    pattern: &str,
    case_insensitive: bool,
    input: &str,
    max_matches: usize,
) -> Result<Vec<(usize, usize)>, RustRangesError> {
    rust_scan_ranges(
        pattern,
        case_insensitive,
        input,
        CappedSpans::new(max_matches),
    )
    .map(CappedSpans::into_spans)
}

/// Offer every match of `pattern` in `input` to `sink`, in order, until it
/// stops the scan; the sink comes back with what it kept.
pub fn rust_scan_ranges<S: SpanSink>(
    pattern: &str,
    case_insensitive: bool,
    input: &str,
    mut sink: S,
) -> Result<S, RustRangesError> {
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(case_insensitive)
        .multi_line(true)
        // `$` also anchors before `\r\n`, as on the PCRE2 path.
        .crlf(true)
        .build()
        .map_err(|error| RustRangesError::InvalidPattern(parse_reason(&error.to_string())))?;
    for found in regex.find_iter(input) {
        if !sink.accept((found.start(), found.end())) {
            break;
        }
    }
    Ok(sink)
}

/// The parser's own reason: the engine compiles the pattern inside a wrapper
/// group, so the raw error echoes that wrapper before its `error:` line.
pub(crate) fn parse_reason(raw: &str) -> String {
    raw.lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("error:"))
        .map(str::trim)
        .or_else(|| Some(raw.trim()))
        .filter(|reason| !reason.is_empty())
        .unwrap_or("invalid regex pattern")
        .to_owned()
}

#[cfg(test)]
mod tests;
