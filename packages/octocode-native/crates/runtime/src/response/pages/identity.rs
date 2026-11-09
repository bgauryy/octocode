//! A repository identity every listed row of a page shares is stated once,
//! on the page's `data`: GitHub search rows otherwise repeat the same
//! `owner`, `repo` and `commitSha` on each row. Rows of different
//! repositories keep their own. The output contract decides which fields
//! move: a field moves only where the tool's schema declares it on `data`
//! and lets a row omit it (an optional page-context field), so a row's own
//! required identity (a commit row's `commitSha`) always stays.
use crate::contracts::output_fields::{PageContext, page_context_fields};
use crate::id::ToolId;
use serde_json::{Map, Value};

/// The identity a row states and a page may state for all of its rows, in
/// order. `owner` and `repo` name one repository only together, so they
/// move as one; a commit names a state of that repository, so `commitSha`
/// moves only once no row still states its own repository.
const GROUPS: [&[&str]; 2] = [&["owner", "repo"], &["commitSha"]];
/// The group a later group's move waits on.
const REPOSITORY: &[&str] = GROUPS[0];

/// States each identity group every listed row of each result row shares
/// once on that result row's `data`, as `tool`'s output contract declares.
pub(crate) fn state_shared_identity(tool: ToolId, structured: &mut Value) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
            state_shared_identity_on(tool, data);
        }
    }
}

/// [`state_shared_identity`] for one result row's `data`: a tool that sizes
/// its rows before the response stage measures them as the stage states them.
pub(crate) fn state_shared_identity_on(tool: ToolId, data: &mut Map<String, Value>) {
    let context = page_context_fields(tool);
    hoist(data, &|pointer| context.at(pointer));
}

/// [`state_shared_identity`] for one `data`, with `declared` answering what
/// the contract keeps at a row pointer ([`PageContextFields::at`]).
fn hoist(data: &mut Map<String, Value>, declared: &dyn Fn(&str) -> Option<PageContext>) {
    let collections = data
        .iter()
        .filter(|(_, value)| value.as_array().is_some_and(|items| items.iter().any(Value::is_object)))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for group in GROUPS {
        let movable = |collection: &str| {
            group.iter().all(|field| {
                declared(&format!("/data/{collection}/0/{field}")) == Some(PageContext::Optional)
            })
        };
        let carriers = collections
            .iter()
            .filter(|collection| {
                data[collection.as_str()]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| carries_any(item, group)))
            })
            .cloned()
            .collect::<Vec<_>>();
        let repository_on_rows = group != REPOSITORY
            && carriers.iter().any(|collection| {
                data[collection.as_str()]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| carries_any(item, REPOSITORY)))
            });
        if carriers.is_empty()
            || repository_on_rows
            || !carriers.iter().all(|collection| movable(collection))
            || !group.iter().all(|field| declared(&format!("/data/{field}")).is_some())
        {
            continue;
        }
        let Some(shared) = shared_values(data, &carriers, group) else {
            continue;
        };
        if group
            .iter()
            .zip(&shared)
            .any(|(field, value)| data.get(*field).is_some_and(|stated| stated != value))
        {
            continue;
        }
        for collection in &carriers {
            for item in data[collection.as_str()]
                .as_array_mut()
                .into_iter()
                .flatten()
                .filter_map(Value::as_object_mut)
            {
                for field in group {
                    item.shift_remove(*field);
                }
            }
        }
        for (field, value) in group.iter().zip(shared) {
            data.insert((*field).to_owned(), value);
        }
    }
}

fn carries_any(item: &Value, group: &[&str]) -> bool {
    group.iter().any(|field| item.get(*field).is_some())
}

/// The values of `group` every object item of `collections` states, when
/// all state the same ones.
fn shared_values(
    data: &Map<String, Value>,
    collections: &[String],
    group: &[&str],
) -> Option<Vec<Value>> {
    let mut items = collections
        .iter()
        .flat_map(|collection| data[collection.as_str()].as_array().into_iter().flatten());
    let first = items.next()?;
    let values = group
        .iter()
        .map(|field| first.get(*field).filter(|value| !value.is_null()).cloned())
        .collect::<Option<Vec<_>>>()?;
    items
        .all(|item| {
            group
                .iter()
                .zip(&values)
                .all(|(field, value)| item.get(*field) == Some(value))
        })
        .then_some(values)
}

#[cfg(test)]
mod tests;
