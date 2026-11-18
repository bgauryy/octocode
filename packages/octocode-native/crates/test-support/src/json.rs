//! JSON values for tests, written as JSON text and parsed when the test runs.

use serde_json::Value;
use std::panic::Location;

/// The value `text` spells. Keys keep their order, a repeated key keeps its first place
/// and takes its last value, and a number keeps its kind (`1` is an integer, `1.0` a
/// float), as `serde_json::json!` builds them.
///
/// # Panics
///
/// When `text` is not valid JSON, naming the caller's file and line.
#[track_caller]
#[expect(
    clippy::panic,
    reason = "a fixture that is not JSON fails the test that wrote it, at its call site"
)]
pub fn json_fixture(text: &str) -> Value {
    match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => panic!("invalid JSON fixture at {}: {error}", Location::caller()),
    }
}

#[cfg(test)]
mod tests;
