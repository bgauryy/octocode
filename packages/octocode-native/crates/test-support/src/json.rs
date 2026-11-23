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

/// A commit row's `field` (`commitSha`, `committedAt`, `author`,
/// `messageHeadline`, or any other object field), read from either form a
/// commit list holds: the object, or the one line
/// `"<commitSha> <committedAt> <author>: <messageHeadline>"`.
pub fn commit_field(commit: &Value, field: &str) -> Value {
    let Some(line) = commit.as_str() else {
        return commit.get(field).cloned().unwrap_or(Value::Null);
    };
    let mut parts = line.splitn(3, ' ');
    let (sha, date, rest) = (parts.next(), parts.next(), parts.next().unwrap_or(""));
    let (author, headline) = rest.split_once(": ").unwrap_or((rest, ""));
    match field {
        "commitSha" => sha.map_or(Value::Null, |sha| Value::String(sha.to_owned())),
        "committedAt" => date.map_or(Value::Null, |date| Value::String(date.to_owned())),
        "author" => Value::String(author.to_owned()),
        "messageHeadline" => Value::String(headline.to_owned()),
        _ => Value::Null,
    }
}
