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
mod tests {
    use super::*;

    #[test]
    fn finds_multiline_ranges_in_original_bytes() {
        let text = "let a = 1;\nfn alpha() {\n  beta()\n}\n";
        let ranges = rust_find_ranges(r"alpha\(\) \{\n\s+beta", false, text, 100).expect("ranges");
        assert_eq!(ranges.len(), 1);
        assert_eq!(&text[ranges[0].0..ranges[0].1], "alpha() {\n  beta");
        // ^ anchors per line.
        assert_eq!(
            rust_find_ranges("^fn", false, text, 100).expect("anchor"),
            vec![(11, 13)]
        );
        // $ also anchors before CRLF.
        assert_eq!(
            rust_find_ranges("a$", false, "a\r\nb", 100).expect("crlf"),
            vec![(0, 1)]
        );
    }

    #[test]
    fn case_and_limit_follow_the_arguments() {
        let text = "Foo foo FOO";
        assert_eq!(rust_find_ranges("foo", true, text, 100).unwrap().len(), 3);
        assert_eq!(
            rust_find_ranges("foo", false, text, 100).unwrap(),
            vec![(4, 7)]
        );
        assert_eq!(rust_find_ranges("o", true, text, 2).unwrap().len(), 2);
        // The caller passes one past its limit to tell a capped scan apart.
        assert_eq!(rust_find_ranges("o", true, text, 0).unwrap().len(), 0);
    }

    #[test]
    fn a_scan_offers_matches_until_its_sink_stops_it() {
        struct StopAfter(usize, Vec<(usize, usize)>);
        impl SpanSink for StopAfter {
            fn accept(&mut self, span: (usize, usize)) -> bool {
                self.1.push(span);
                self.1.len() < self.0
            }
        }
        let sink = rust_scan_ranges("o", false, "o o o o", StopAfter(3, vec![])).unwrap();
        assert_eq!(sink.1, vec![(0, 1), (2, 3), (4, 5)]);
        assert!(rust_scan_ranges("(", false, "x", StopAfter(1, vec![])).is_err());
    }

    #[test]
    fn the_verdict_never_depends_on_the_text_size() {
        let big = "ab\n".repeat(200_000);
        assert_eq!(rust_find_ranges("ab", false, &big, 5).unwrap().len(), 5);
        assert!(rust_find_ranges("(ab", false, &big, 5).is_err());
    }

    #[test]
    fn backreferences_and_lookaround_are_invalid_for_this_engine() {
        // Same verdict and reason `localSearch` gives (`regex:"pcre2"` owns both).
        assert_eq!(
            rust_find_ranges(r"(\w)\1", false, "aa", 10),
            Err(RustRangesError::InvalidPattern(
                "backreferences are not supported".into()
            ))
        );
        let lookaround = rust_find_ranges("(?<=fn )x", false, "fn x", 10);
        assert!(
            matches!(&lookaround, Err(RustRangesError::InvalidPattern(reason)) if reason.contains("look-around")),
            "{lookaround:?}"
        );
        assert_eq!(
            rust_find_ranges("(", false, "x", 10),
            Err(RustRangesError::InvalidPattern("unclosed group".into()))
        );
    }

    #[test]
    fn matches_the_reason_localsearch_reports() {
        for pattern in [r"(\w)\1", "(", "[", "a{2,1}"] {
            let checked = crate::search::text_pattern::validate_text_pattern(pattern, false, false);
            let Err(RustRangesError::InvalidPattern(reason)) =
                rust_find_ranges(pattern, false, "x", 1)
            else {
                panic!("{pattern} must not compile");
            };
            assert_eq!(
                reason,
                parse_reason(&checked.error.expect("validator error")),
                "{pattern}"
            );
        }
    }
}
