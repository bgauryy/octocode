//! Unit tests of the read pipeline's private stages.

use super::*;
use crate::local_fetch::tests::{Paths, fetch_with, q, qj};
use octocode_contract_types::LocalFetchQuery;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::security::scan::Passthrough;
use octocode_runtime_base::test_support::Fixture;
use std::fs;
use std::path::PathBuf;

#[cfg(unix)]
mod open;
mod secrets;
mod size_limits;
mod streamed;
mod streamed_matches;
mod torn;

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
fn binary_sniff_does_not_reserve_the_whole_source() {
    let mut file = tempfile::tempfile().expect("source");
    std::io::Write::write_all(&mut file, b"\0binary").expect("binary prefix");
    file.set_len(MAX_SOURCE_BYTES).expect("source size");
    std::io::Seek::rewind(&mut file).expect("rewind");
    let (_, sample) = read_sample(file).expect("sample");
    assert_eq!(
        octocode_engine::text::sniff(&sample),
        octocode_engine::text::BinaryVerdict::Binary
    );
    assert!(
        sample.capacity() <= BINARY_SAMPLE_BYTES,
        "an immediate binary refusal must retain only its sniff, not the whole source"
    );
}
