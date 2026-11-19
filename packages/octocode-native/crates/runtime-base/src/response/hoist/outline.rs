//! Q5: declaration lists as line-first outline strings
//! ([`crate::response::symbol_outline::outline_lines`]), formed here from
//! the sealed objects and never by a tool. The output gate judges a
//! declaration's fields as leaves; some of its detectors read a value only
//! where it starts its text (`PrivateKey = <key>`), so a string a tool joined
//! before the gate (`"1\tvariable PrivateKey = …"`) could carry what the gate
//! hides in a `symbolName`. Joining the redacted leaves after the gate keeps
//! every redaction; the string is a gate fixed point.
use super::pattern::Pattern;
use serde_json::Value;

/// Turns each list `lists` names (object declarations) into its outline
/// strings; a list that already holds strings, or is empty, stays.
pub(crate) fn apply(lists: &[Pattern], data: &mut Value) {
    for list in lists {
        for found in list.matches(data).into_iter().rev() {
            if let Some(Value::Array(items)) = data.pointer_mut(&found.pointer())
                && !items.is_empty()
                && items.iter().all(Value::is_object)
            {
                *items = crate::response::symbol_outline::outline_lines(items)
                    .into_iter()
                    .map(Value::String)
                    .collect();
            }
        }
    }
}
