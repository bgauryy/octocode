//! What a whole read serves of a source written while it is read. A live
//! log appended to after each read is served as of the bytes one read took,
//! the start of one version of it, and the row says so; a save in place
//! after the read is never served as such a start; a source that keeps
//! changing is refused, saying so, with a recovery that fits. Read from the
//! served result.

use super::*;
use crate::local_fetch::executor::execute_local_fetch;
use crate::local_fetch::tests::{Paths, q};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::security::ContentSecurity;
use octocode_runtime_base::test_support::Fixture;
use serde_json::Value;
use std::path::PathBuf;

/// The log's lines when it is first read.
fn log() -> String {
    (1..=40).map(|n| format!("request {n} served\n")).collect()
}

/// One localFetch of `path`, as an agent receives its result.
fn served(path: &Path, dir: &Path) -> Value {
    let result = execute_local_fetch(
        &q(path),
        &Paths(dir.to_path_buf()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        false,
    );
    let data = serde_json::to_value(&result).expect("row");
    octocode_runtime_base::test_support::staged_response(ToolId::LocalFetch, vec![data])
}

/// Appends `text` to the file at `path`, as a log writer does.
fn append(path: &Path, text: &str) {
    use std::io::Write as _;
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .expect("append");
}

/// Registers `write` for the file at `path`, made after each read of it ends
/// and before the read is proved, while it returns `true`; returns the inode
/// it is registered under.
#[cfg(unix)]
fn after_each_read(path: &Path, write: impl FnMut() -> bool + Send + 'static) -> u64 {
    use std::os::unix::fs::MetadataExt as _;
    let inode = fs::metadata(path).expect("metadata").ino();
    octocode_engine::text::BEFORE_PROOF
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((inode, Box::new(write)));
    inode
}

/// Drops what is still registered for `inode`.
#[cfg(unix)]
fn unregister_read(inode: u64) {
    octocode_engine::text::BEFORE_PROOF
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .retain(|(registered, _)| *registered != inode);
}

/// Registers `write` for `path`, made after each sample a read takes while
/// it returns `true`; returns the canonical path it is registered under.
fn after_each_sample(path: &Path, write: impl FnMut() -> bool + Send + 'static) -> PathBuf {
    let source = fs::canonicalize(path).expect("canonical source");
    AFTER_SAMPLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((source.clone(), Box::new(write)));
    source
}

/// Drops what is still registered for `source`.
fn unregister_sample(source: &Path) {
    AFTER_SAMPLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .retain(|(path, _)| path != source);
}

/// The row's warnings, joined.
fn warnings(out: &Value) -> String {
    out["results"][0]["data"]["warnings"].to_string()
}

/// A log appended to after every read of it never holds still for a read,
/// but each read is the start of one version: it is served as of the bytes
/// it took, every line of the log as first read, and the row says the file
/// grew and how much it shows.
#[cfg(unix)]
#[test]
fn a_log_appended_after_every_read_is_served_as_of_its_read() {
    let temp = Fixture::new();
    let path = temp.0.join("live.log");
    fs::write(&path, log()).expect("fixture");
    let target = path.clone();
    let inode = after_each_read(&path, move || {
        append(&target, "request appended after the read\n");
        true
    });
    let out = served(&path, &temp.0);
    unregister_read(inode);
    let data = &out["results"][0]["data"];
    assert!(data["errorCode"].is_null(), "the log is served: {out}");
    let content = data["content"].as_str().unwrap_or_default();
    assert!(
        content.contains("request 1 served") && content.contains("request 40 served"),
        "every line of the log as read is shown: {out}"
    );
    assert!(
        !content.contains("appended after the read"),
        "no byte past the read is shown: {out}"
    );
    let warnings = warnings(&out);
    assert!(
        warnings.contains("grew while it was read")
            && warnings.contains(&format!("as of {} bytes", log().len())),
        "the row says the file grew and how much of it is shown: {out}"
    );
}

/// A log appended to between the sample a read takes and its rest, every
/// time it is read, is read whole each time: a read that is the file whole
/// is served as that version, and says nothing of growth.
#[test]
fn a_log_appended_while_every_read_is_under_way_is_served() {
    let temp = Fixture::new();
    let path = temp.0.join("live.log");
    fs::write(&path, log()).expect("fixture");
    let target = path.clone();
    let source = after_each_sample(&path, move || {
        append(&target, "request appended while it is read\n");
        true
    });
    let out = served(&path, &temp.0);
    unregister_sample(&source);
    let data = &out["results"][0]["data"];
    assert!(data["errorCode"].is_null(), "the log is served: {out}");
    let content = data["content"].as_str().unwrap_or_default();
    assert!(
        content.contains("request 1 served") && content.contains("request 40 served"),
        "every line of the log is shown: {out}"
    );
    assert!(
        !warnings(&out).contains("grew while it was read"),
        "a read of the file whole says nothing of growth: {out}"
    );
}

/// A save in place after a read, longer than the read and starting with
/// other bytes, is no version the read is the start of: the source is read
/// again, and the version that read settled on is served whole.
#[cfg(unix)]
#[test]
fn a_save_in_place_after_the_read_is_not_served_as_its_start() {
    let temp = Fixture::new();
    let path = temp.0.join("live.txt");
    fs::write(&path, "first version, line one\n").expect("fixture");
    let target = path.clone();
    let inode = after_each_read(&path, move || {
        fs::write(
            &target,
            "second version, line one\nsecond version, line two\n",
        )
        .expect("rewrite in place");
        false
    });
    let out = served(&path, &temp.0);
    unregister_read(inode);
    let data = &out["results"][0]["data"];
    let content = data["content"].as_str().unwrap_or_default();
    assert!(
        content.contains("second version, line two") && !content.contains("first version"),
        "the version the read settled on is served whole: {out}"
    );
    assert!(
        !warnings(&out).contains("grew while it was read"),
        "a rewrite is no growth: {out}"
    );
}

/// A source rewritten after every sample a read takes never settles: the
/// row says the file kept changing while it was read, and its recovery is
/// to wait or read a still copy, not to check the path, its permissions or
/// its encoding.
#[test]
fn a_source_that_keeps_changing_is_refused_saying_so() {
    let temp = Fixture::new();
    let path = temp.0.join("live.txt");
    fs::write(&path, "one version\n").expect("fixture");
    let (target, mut writes) = (path.clone(), 0u64);
    let source = after_each_sample(&path, move || {
        writes += 1;
        let version = if writes % 2 == 1 {
            "another version, a longer one\n"
        } else {
            "one version\n"
        };
        fs::write(&target, version).expect("rewrite in place");
        true
    });
    let out = served(&path, &temp.0);
    unregister_sample(&source);
    let data = &out["results"][0]["data"];
    assert!(data["content"].is_null(), "nothing is served: {out}");
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|error| error.contains("kept changing while it was read")),
        "the row names why: {out}"
    );
    let hints = data["hints"].to_string();
    assert!(
        hints.contains("stops changing") && !hints.contains("permissions"),
        "the recovery fits a file that keeps changing: {out}"
    );
}
