//! A rejected call's `repair` hint: the corrected call when every issue has
//! one exact fix, on the whole-call envelope and on an isolated batch row.

use octocode_native::contracts;
use serde_json::Value;

mod containers;
mod exact;
mod refusals;

/// The repair a lead carries is a call its own tool admits unchanged.
fn assert_executable(tool: &str, repair: &Value) {
    assert_eq!(repair["tool"], tool, "{repair}");
    let admitted = contracts::prepare_many_and_validate(tool, repair["query"].clone());
    assert!(admitted.is_ok(), "lead rejected: {admitted:?}: {repair}");
}
