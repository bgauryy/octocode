//! F1: a value every listed item of a holder's collections shares is stated
//! once, on the holder: GitHub search rows otherwise repeat the same `owner`,
//! `repo` and `commitSha` on each row, and graph traversal rows the same
//! `from`. Items that differ keep their own. The output contract decides
//! what moves: a group moves only where the tool's schema declares each
//! field on the holder and lets an item omit it (an optional page-context
//! field), so an item's own required field (a commit row's `commitSha`)
//! always stays. A page repeats what it states on every part, so each part
//! reads alone. Holders deeper in a row go first, then outward to `data`.
use super::pattern::is_call;
use super::plan::Holder;
use crate::contracts::output_fields::PageContext;
use serde_json::{Map, Value};

/// States each `holders` group on every holder of `data` that all its
/// collections' items share it on; `declared` answers what the contract
/// keeps at a row pointer (`/data/…`).
pub(crate) fn apply(
    holders: &[Holder],
    data: &mut Value,
    declared: &dyn Fn(&str) -> Option<PageContext>,
) {
    let mut deepest = holders.iter().collect::<Vec<_>>();
    // Innermost first; a stable sort keeps the contract's order (and each
    // group's `after`) within one depth.
    deepest.sort_by_key(|holder| std::cmp::Reverse(holder.at.tokens().len()));
    for holder in deepest {
        for found in holder.at.matches(data).into_iter().rev() {
            let at = format!("/data{}", found.pointer());
            if let Some(object) = data
                .pointer_mut(&found.pointer())
                .and_then(Value::as_object_mut)
                .filter(|object| !is_call(object))
            {
                state_on(object, holder, &|member| declared(&format!("{at}{member}")));
            }
        }
    }
}

/// [`apply`] for one holder object; `declared` takes pointers under it.
pub(crate) fn state_on(
    holder_data: &mut Map<String, Value>,
    Holder { fields, after, .. }: &Holder,
    declared: &dyn Fn(&str) -> Option<PageContext>,
) {
    let collections = holder_data
        .iter()
        .filter(|(key, value)| {
            !matches!(key.as_str(), "next" | "related")
                && value
                    .as_array()
                    .is_some_and(|items| items.iter().any(Value::is_object))
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    let movable = |collection: &str| {
        fields.iter().all(|field| {
            declared(&format!(
                "/{}/0/{}",
                crate::json_pointer::escape(collection),
                crate::json_pointer::escape(field)
            )) == Some(PageContext::Optional)
        })
    };
    let carriers = collections
        .iter()
        .filter(|collection| {
            holder_data[collection.as_str()]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| carries_any(item, fields)))
        })
        .cloned()
        .collect::<Vec<_>>();
    let after_on_items = carriers.iter().any(|collection| {
        holder_data[collection.as_str()]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| carries_any(item, after)))
    });
    if carriers.is_empty()
        || after_on_items
        || !carriers.iter().all(|collection| movable(collection))
        || !fields
            .iter()
            .all(|field| declared(&format!("/{}", crate::json_pointer::escape(field))).is_some())
    {
        return;
    }
    let Some(shared) = shared_values(holder_data, &carriers, fields) else {
        return;
    };
    if fields.iter().zip(&shared).any(|(field, value)| {
        holder_data
            .get(field.as_str())
            .is_some_and(|stated| stated != value)
    }) {
        return;
    }
    for collection in &carriers {
        for item in holder_data[collection.as_str()]
            .as_array_mut()
            .into_iter()
            .flatten()
            .filter_map(Value::as_object_mut)
        {
            for field in fields {
                item.shift_remove(field.as_str());
            }
        }
    }
    for (field, value) in fields.iter().zip(shared) {
        holder_data.insert(field.clone(), value);
    }
}

fn carries_any(item: &Value, group: &[String]) -> bool {
    group.iter().any(|field| item.get(field.as_str()).is_some())
}

/// The values of `group` every object item of `collections` states, when
/// all state the same ones.
fn shared_values(
    data: &Map<String, Value>,
    collections: &[String],
    group: &[String],
) -> Option<Vec<Value>> {
    let mut items = collections
        .iter()
        .flat_map(|collection| data[collection.as_str()].as_array().into_iter().flatten());
    let first = items.next()?;
    let values = group
        .iter()
        .map(|field| {
            first
                .get(field.as_str())
                .filter(|value| !value.is_null())
                .cloned()
        })
        .collect::<Option<Vec<_>>>()?;
    items
        .all(|item| {
            group
                .iter()
                .zip(&values)
                .all(|(field, value)| item.get(field.as_str()) == Some(value))
        })
        .then_some(values)
}
