//! F3: a value the contract derives is omitted where it holds the derived
//! value: a schema constant, the value of another field of the row, or a
//! template over one. The reader recovers it from that rule (the output
//! reference states each), so nothing is lost; a field holding any other
//! value keeps it. A field whose source is absent stays.
use super::pattern::{Match, Pattern};
use super::plan::{Omit, Piece, Recurse, When};
use serde_json::Value;

/// Omits each `entries` field of `data` that holds its derived value.
pub(crate) fn apply(entries: &[Omit], data: &mut Value) {
    for entry in entries {
        // Later matches first, so removing one never moves another.
        for found in entry.at.matches(data).into_iter().rev() {
            if derived(&entry.when, data, &found).as_ref() == data.pointer(&found.pointer()) {
                remove(data, &found.path);
            }
        }
    }
}

/// [`apply`] again on every item a `refs` collection nests in an item of
/// its own shape (a declaration's `members`), at any depth: each entry is
/// re-rooted on the nested item ([`Omit::within`]).
pub(crate) fn through(refs: &[Recurse], entries: &[Omit], data: &mut Value) {
    for Recurse { target, tail } in refs {
        let local = entries
            .iter()
            .filter_map(|entry| entry.within(target))
            .collect::<Vec<_>>();
        if local.is_empty() {
            continue;
        }
        for found in target.matches(data) {
            if let Some(item) = data.pointer_mut(&found.pointer()) {
                nested(tail, &local, item);
            }
        }
    }
}

/// [`apply`] with `local` on each item `tail` reaches below `item`, then
/// below each of those.
pub(crate) fn nested(tail: &Pattern, local: &[Omit], item: &mut Value) {
    for found in tail.matches(item) {
        if let Some(inner) = item.pointer_mut(&found.pointer()) {
            apply(local, inner);
            nested(tail, local, inner);
        }
    }
}

/// The value `when` derives for the field at `found`, or `None` when its
/// source is absent.
fn derived(when: &When, data: &Value, found: &Match) -> Option<Value> {
    let field = |pattern: &super::pattern::Pattern| {
        let path = pattern.bind(&found.bound)?;
        data.pointer(&crate::json_pointer::of(&path))
    };
    match when {
        When::Const(value) => Some(value.clone()),
        When::Equals(source) => field(source).cloned(),
        When::CountOf(source) => field(source)?
            .as_array()
            .map(|items| Value::from(items.len())),
        When::Template(pieces) => pieces
            .iter()
            .map(|piece| match piece {
                Piece::Text(text) => Some(text.clone()),
                Piece::Field(source) => field(source)?.as_str().map(str::to_owned),
            })
            .collect::<Option<String>>()
            .map(Value::String),
    }
}

/// Removes the member `path` names from `value`.
pub(crate) fn remove(value: &mut Value, path: &[String]) {
    let Some((last, parent)) = path.split_last() else {
        return;
    };
    if let Some(object) = value
        .pointer_mut(&crate::json_pointer::of(parent))
        .and_then(Value::as_object_mut)
    {
        object.shift_remove(last.as_str());
    }
}
