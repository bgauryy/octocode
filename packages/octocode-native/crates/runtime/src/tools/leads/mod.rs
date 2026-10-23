//! Follow-up leads shared by several tools, built once: leads one tool
//! offers into another (so no tool imports the target tool's internals), an
//! empty search's broadening, and the prose hint beside them.

pub(crate) mod broaden;
pub(crate) mod find_file;
pub(crate) mod lsp;
pub(crate) mod skipped;

use crate::id::continuation_channels::{HINT_TEXT_KEY, HINTS_KEY};
use serde_json::{Value, json};

/// Append one prose hint to `row`'s hints, after any it holds: a list gets
/// it last, a `{text}` map in its text list; a row without hints gets a
/// one-hint list.
pub(crate) fn push_hint(row: &mut Value, hint: impl Into<String>) {
    let hint = Value::String(hint.into());
    match &mut row[HINTS_KEY] {
        Value::Array(hints) => hints.push(hint),
        Value::Object(hints) => match hints.get_mut(HINT_TEXT_KEY) {
            Some(Value::Array(text)) => text.push(hint),
            _ => {
                hints.insert(HINT_TEXT_KEY.into(), json!([hint]));
            }
        },
        hints => *hints = json!([hint]),
    }
}

/// Run an optional probe that only adds a lead or a note. A panic in it (an
/// engine bug on pathological input) yields `None`, so the row keeps the
/// primary result it already has instead of failing as a whole.
pub(crate) fn best_effort<T>(probe: impl FnOnce() -> T) -> Option<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(probe)).ok()
}

#[cfg(test)]
mod push_hint_tests {
    use serde_json::json;

    #[test]
    fn a_hint_goes_after_the_hints_a_row_holds_in_either_shape() {
        let mut row = json!({});
        super::push_hint(&mut row, "a");
        super::push_hint(&mut row, "b");
        assert_eq!(row["hints"], json!(["a", "b"]));
        let mut row = json!({"hints": {"text": ["a"], "read": {}}});
        super::push_hint(&mut row, "b");
        assert_eq!(row["hints"], json!({"text": ["a", "b"], "read": {}}));
        let mut row = json!({"hints": {"read": {}}});
        super::push_hint(&mut row, "b");
        assert_eq!(row["hints"], json!({"read": {}, "text": ["b"]}));
    }
}

#[cfg(test)]
mod best_effort_tests {
    #[test]
    fn a_panicking_probe_is_absent_and_a_finished_one_is_kept() {
        assert_eq!(super::best_effort(|| 7), Some(7));
        let finishes = std::hint::black_box(false);
        assert_eq!(super::best_effort(|| assert!(finishes, "probe bug")), None);
    }
}
