//! Unit tests of the read pipeline's private stages.

use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::scan::Passthrough;
use crate::tools::local_fetch::tests::{Paths, fetch_with, q, qj};
use crate::tools::test_support::Fixture;
use octocode_contract_types::LocalFetchQuery;
use std::fs;
use std::path::PathBuf;

#[cfg(unix)]
mod open;
mod secrets;
mod size_limits;
mod streamed;

/// Read `req` under `dir` with a passthrough scan.
fn fetch(req: &LocalFetchQuery, paths: &Paths) -> LocalFetchResult {
    fetch_with(req, paths, &Passthrough)
}

/// A source in `dir` just past the whole-file ceiling, of `line(n)` lines
/// for n = 1, 2, …: its path and line count.
fn big_log(dir: &std::path::Path, line: impl Fn(usize) -> String) -> (PathBuf, usize) {
    let path = dir.join("big.log");
    let mut text = String::new();
    let mut lines = 0usize;
    while text.len() as u64 <= MAX_SOURCE_BYTES {
        lines += 1;
        text.push_str(&line(lines));
    }
    fs::write(&path, &text).expect("write");
    (path, lines)
}

/// The line `big_log` writes as line `n` unless a test names its own.
fn payload_line(n: usize) -> String {
    format!("line {n} payload payload payload payload\n")
}

#[test]
fn preserves_millisecond_precision() {
    use std::time::{Duration, UNIX_EPOCH};
    assert_eq!(
        system_time_iso(UNIX_EPOCH + Duration::from_millis(1_600_000_000_443)),
        Some("2020-09-13T12:26:40.443Z".into())
    );
}
