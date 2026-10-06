//! PCRE2 match ranges in one in-memory text (a file read's `regex:"pcre2"`),
//! under the same worker slots and wall-clock deadline as a `-P` search.
use std::time::Duration;

use grep_matcher::Matcher;
use grep_pcre2::RegexMatcherBuilder;

use super::ripgrep_search::{
    ACTIVE_PCRE2_WORKERS, MAX_ACTIVE_PCRE2_WORKERS, PCRE2_DEADLINE_GRACE,
    PCRE2_MAX_JIT_STACK_BYTES, PCRE2_SEARCH_DEADLINE, Pcre2WorkerSlot, release_worker_slot,
    try_acquire_worker_slot,
};

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
    find_ranges(
        pattern,
        case_insensitive,
        input,
        max_matches,
        PCRE2_SEARCH_DEADLINE + PCRE2_DEADLINE_GRACE,
    )
}

fn find_ranges(
    pattern: &str,
    case_insensitive: bool,
    input: &str,
    max_matches: usize,
    deadline: Duration,
) -> Result<Vec<(usize, usize)>, Pcre2RangesError> {
    let mut builder = RegexMatcherBuilder::new();
    builder
        .caseless(case_insensitive)
        .multi_line(true)
        .crlf(true)
        .utf(true)
        .ucp(true)
        .jit_if_available(true)
        .max_jit_stack_size(Some(PCRE2_MAX_JIT_STACK_BYTES));
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
            let mut ranges = Vec::new();
            let scanned = matcher.find_iter(input.as_bytes(), |found| {
                ranges.push((found.start(), found.end()));
                ranges.len() < max_matches
            });
            let _ = tx.send(scanned.map(|()| ranges).map_err(|error| error.to_string()));
        });
    if spawned.is_err() {
        // No worker will run to release the reserved slot.
        release_worker_slot(&ACTIVE_PCRE2_WORKERS);
    }
    spawned.map_err(|error| Pcre2RangesError::Unavailable(error.to_string()))?;
    match rx.recv_timeout(deadline) {
        Ok(Ok(ranges)) => Ok(ranges),
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

/// Live PCRE2 workers (tests).
#[cfg(test)]
fn active_workers() -> usize {
    ACTIVE_PCRE2_WORKERS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_lookaround_and_multiline_ranges_in_original_bytes() {
        let text = "let a = 1;\nfn alpha() {\n  beta()\n}\n";
        let ranges = pcre2_find_ranges(r"(?<=fn )alpha", false, text, 100).expect("ranges");
        assert_eq!(ranges, vec![(14, 19)]);
        assert_eq!(&text[14..19], "alpha");
        let multiline =
            pcre2_find_ranges(r"alpha\(\) \{\n\s+beta", false, text, 100).expect("multiline");
        assert_eq!(multiline.len(), 1);
        assert_eq!(&text[multiline[0].0..multiline[0].1], "alpha() {\n  beta");
        // ^ anchors per line.
        assert_eq!(
            pcre2_find_ranges("^fn", false, text, 100).expect("anchor"),
            vec![(11, 13)]
        );
    }

    #[test]
    fn case_and_backreferences_follow_the_flags() {
        let text = "Foo foo FOO abab";
        assert_eq!(
            pcre2_find_ranges("foo", true, text, 100)
                .expect("caseless")
                .len(),
            3
        );
        assert_eq!(
            pcre2_find_ranges("foo", false, text, 100).expect("sensitive"),
            vec![(4, 7)]
        );
        assert_eq!(
            pcre2_find_ranges(r"(ab)\1", false, text, 100).expect("backref"),
            vec![(12, 16)]
        );
        assert_eq!(
            pcre2_find_ranges("o", true, text, 2).expect("capped").len(),
            2
        );
    }

    #[test]
    fn invalid_patterns_and_deadlines_are_typed() {
        assert!(matches!(
            pcre2_find_ranges("(", false, "x", 10),
            Err(Pcre2RangesError::InvalidPattern(_))
        ));
        let before = active_workers();
        // A zero deadline stops waiting at once; a scan that still beats it
        // is a complete answer, never an invalid pattern.
        let slow = find_ranges("a", false, &"a".repeat(1 << 20), usize::MAX, Duration::ZERO);
        assert!(
            matches!(&slow, Err(Pcre2RangesError::Unavailable(message)) if message.contains("deadline"))
                || slow.as_ref().is_ok_and(|ranges| ranges.len() == 1 << 20),
            "{:?}",
            slow.as_ref().map(Vec::len)
        );
        // The worker releases its slot when it finishes.
        for _ in 0..200 {
            if active_workers() <= before {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
