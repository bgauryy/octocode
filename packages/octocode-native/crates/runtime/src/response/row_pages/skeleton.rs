use super::collections::collection_paths;
use crate::response::result::{HINTS_KEY, PAGES_KEY};
use serde_json::{Value, json};

/// The innermost array element a paged array at `pointer` lies in, as
/// (array pointer, element index): the element `rowPart.continues` names on
/// a later part. Every element on the path to it continues too, so a split
/// that reaches through single-element arrays (a file's only call row and
/// its call sites) names the deepest one, not just the outermost.
pub(super) fn enclosing_element(value: &Value, pointer: &str) -> Option<(String, usize)> {
    let (holder, _paged) = pointer.rsplit_once('/')?;
    let mut prefix = String::new();
    let mut node = value;
    let mut innermost = None;
    for segment in holder.split('/').skip(1) {
        let key = crate::json_pointer::unescape(segment);
        node = match node {
            Value::Array(items) => {
                let index = key.parse().ok()?;
                innermost = Some((prefix.clone(), index));
                items.get(index)?
            }
            Value::Object(members) => members.get(&key)?,
            _ => return None,
        };
        prefix = format!("{prefix}/{segment}");
    }
    innermost
}

/// Serialized length a `rowPart.continues` marker adds.
pub(super) fn continues_chars(array: &str, index: usize) -> usize {
    octocode_engine::json_size::utf16_units(&json!({"continues": {"array": array, "index": index}}))
        .unwrap_or(0)
        - 1
}

/// A later part of a split row (a listing whose files page, a pull request
/// whose comments page) names the row and the elements it continues, not
/// their description again: on every object the paged `pointer` passes
/// through, the row itself included, a member rides the first part only
/// unless it is the pointer's next step, guidance, a core-declared
/// page-context field at that row pointer (`pageContextFields`: identity,
/// next and disclosure fields whole, required evidence emptied, see
/// [`PageContextFields::keep_later`]), or a holder of a declared collection
/// (emptied beside the paged one, and dropped by the final materialization
/// pass once the part is whole).
///
/// [`PageContextFields::keep_later`]: crate::contracts::output_fields::PageContextFields::keep_later
pub(super) fn lean_later_context(
    skeleton: &mut Value,
    pointer: &str,
    context: &crate::contracts::output_fields::PageContextFields,
    routes: &crate::contracts::output_fields::CollectionRoutes,
) {
    let holders = collection_paths(skeleton, routes);
    let holds = |path: &str| {
        holders.iter().any(|held| {
            held.strip_prefix(path)
                .is_some_and(|tail| tail.is_empty() || tail.starts_with('/'))
        })
    };
    let mut node = Some(skeleton);
    let mut at = String::new();
    for segment in pointer.trim_start_matches('/').split('/') {
        let Some(current) = node else {
            return;
        };
        let key = crate::json_pointer::unescape(segment);
        if let Some(map) = current.as_object_mut() {
            map.retain(|name, value| {
                let member = format!("{at}/{}", crate::json_pointer::escape(name));
                *name == key
                    || name == PAGES_KEY
                    || name == HINTS_KEY
                    || holds(&member)
                    || context.keep_later(&member, value)
            });
        }
        node = match current {
            Value::Object(map) => map.get_mut(&key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            _ => None,
        };
        at = format!("{at}/{segment}");
    }
}
