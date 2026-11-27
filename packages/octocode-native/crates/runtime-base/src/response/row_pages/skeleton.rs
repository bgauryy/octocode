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
/// [`PageContextFields::keep_later`]), or a declared collection itself
/// (emptied beside the paged one, and dropped by the final materialization
/// pass once the part is whole). An object or array that only holds declared
/// collections off the pointer's path rides the first part: its collections
/// are delivered by the parts that page them, and repeating its context on
/// every later part would outgrow the page. A row-level fact of `data`
/// kept whole as required evidence (a total, an enum) stays only where the
/// part needs it to validate ([`PageContextFields::drop_restated`]): a
/// localSearch part that holds `files` validates without `matchCount`,
/// which the first part states. An element the part continues keeps its
/// members: they name it.
///
/// [`PageContextFields::keep_later`]: crate::contracts::output_fields::PageContextFields::keep_later
/// [`PageContextFields::drop_restated`]: crate::contracts::output_fields::PageContextFields::drop_restated
pub(super) fn lean_later_context(
    skeleton: &mut Value,
    pointer: &str,
    context: &crate::contracts::output_fields::PageContextFields,
    routes: &crate::contracts::output_fields::CollectionRoutes,
) {
    use crate::contracts::output_fields::PageContext;
    let collections = collection_paths(skeleton, routes);
    let holds = |path: &str| collections.iter().any(|held| held == path);
    // Required evidence kept whole beside the pointer's path, by pointer.
    let mut restated = Vec::new();
    {
        let mut node = Some(&mut *skeleton);
        let mut at = String::new();
        for segment in pointer.trim_start_matches('/').split('/') {
            let Some(current) = node else {
                break;
            };
            let key = crate::json_pointer::unescape(segment);
            if let Some(map) = current.as_object_mut() {
                map.retain(|name, value| {
                    let member = format!("{at}/{}", crate::json_pointer::escape(name));
                    if *name == key || name == PAGES_KEY || name == HINTS_KEY || holds(&member) {
                        return true;
                    }
                    let kept = context.keep_later(&member, value);
                    // A row-level fact (`data`'s own member, a total); an
                    // element on the way keeps its members, which name it.
                    if kept
                        && at == "/data"
                        && context.at(&member) == Some(PageContext::RequiredEvidence)
                        && !is_empty_form(value)
                    {
                        restated.push(member);
                    }
                    kept
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
    context.drop_restated(skeleton, &restated);
}

/// An empty string, list or record: required evidence in the empty form a
/// later part carries only to validate.
fn is_empty_form(value: &Value) -> bool {
    match value {
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(members) => members.is_empty(),
        _ => false,
    }
}

/// Take the walks (`next`) of the objects a split at `pointer` runs through,
/// below the row's `data` (whose guidance the plan sets aside as a whole),
/// as (pointer, walk): each such object repeats on every part of the split,
/// and its walk rides only the last ([`put_walks`]), as the row's own does.
pub(super) fn take_enclosing_walks(row: &mut Value, pointer: &str) -> Vec<(String, Value)> {
    let segments = pointer
        .trim_start_matches('/')
        .split('/')
        .collect::<Vec<_>>();
    let holders = &segments[..segments.len().saturating_sub(1)];
    let mut walks = Vec::new();
    let mut at = String::new();
    let mut node = Some(row);
    for segment in holders {
        let Some(current) = node else {
            break;
        };
        let key = crate::json_pointer::unescape(segment);
        node = match current {
            Value::Object(map) => map.get_mut(&key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            _ => None,
        };
        at = format!("{at}/{segment}");
        if at != "/data"
            && let Some(map) = node.as_deref_mut().and_then(Value::as_object_mut)
            && let Some(walk) = map.shift_remove(PAGES_KEY)
        {
            walks.push((at.clone(), walk));
        }
    }
    walks
}

/// Put `walks` back at their pointers in `value` (a part, or the row kept
/// whole after all). A later string window keeps, of each array on its way,
/// only the element on the way, so an index past such a one-element array
/// names that element.
pub(super) fn put_walks(value: &mut Value, walks: &[(String, Value)]) {
    for (pointer, walk) in walks {
        let mut node = Some(&mut *value);
        for segment in pointer.trim_start_matches('/').split('/') {
            let key = crate::json_pointer::unescape(segment);
            node = match node {
                Some(Value::Object(map)) => map.get_mut(&key),
                Some(Value::Array(items)) => {
                    let single = items.len() == 1;
                    let index = key.parse::<usize>().ok();
                    match index {
                        Some(index) if index < items.len() => items.get_mut(index),
                        Some(_) if single => items.first_mut(),
                        _ => None,
                    }
                }
                _ => None,
            };
        }
        if let Some(map) = node.and_then(Value::as_object_mut) {
            map.insert(PAGES_KEY.to_owned(), walk.clone());
        }
    }
}
