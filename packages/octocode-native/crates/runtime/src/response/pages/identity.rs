//! A value every listed row of a page shares is stated once, on the page's
//! `data`: GitHub search rows otherwise repeat the same `owner`, `repo` and
//! `commitSha` on each row, and graph traversal rows the same `from`, `edge`
//! and `verify`. Rows that differ keep their own. The output contract decides
//! what moves: its `pageContextFields.statedOnce` lists the groups of row
//! fields a page may state once, and a group moves only where the tool's
//! schema declares each field on `data` and lets a row omit it (an optional
//! page-context field), so a row's own required field (a commit row's
//! `commitSha`) always stays. A page repeats what it states on every part,
//! so each part reads alone.
use crate::contracts::output_fields::{PageContext, page_context_fields};
use crate::id::ToolId;
use serde_json::{Map, Value};

/// A group of row fields a page states once, as core declares it
/// (`pageContextFields.statedOnce`). `after` names the fields of an earlier
/// group that must no longer sit on any row before this group moves (a
/// commit names a state of a repository, so `commitSha` waits for `owner`
/// and `repo`).
#[derive(Debug)]
pub(crate) struct Group {
    fields: Vec<String>,
    after: Vec<String>,
}

#[expect(
    clippy::expect_used,
    reason = "a missing or malformed embedded group list is a contract defect caught for every tool by contract tests"
)]
pub(crate) fn stated_once(tool: ToolId) -> &'static [Group] {
    static GROUPS: [std::sync::OnceLock<Vec<Group>>; ToolId::ALL.len()] =
        [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    GROUPS[tool.index()].get_or_init(|| {
        let contract =
            crate::contracts::tool_contract(tool).expect("embedded contract contains every tool");
        let names = |value: &Value| {
            value
                .as_array()
                .map(|names| {
                    names
                        .iter()
                        .map(|name| name.as_str().expect("group field name").to_owned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        contract["pageContextFields"]["statedOnce"]
            .as_array()
            .expect("core declares each tool's stated-once groups")
            .iter()
            .map(|group| Group {
                fields: names(&group["fields"]),
                after: names(&group["after"]),
            })
            .collect()
    })
}

/// States each group every listed row of each result row shares
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
    hoist(data, stated_once(tool), &|pointer| context.at(pointer));
}

/// [`state_shared_identity`] for one `data`, with `declared` answering what
/// the contract keeps at a row pointer ([`PageContextFields::at`]) and
/// `groups` the groups the contract lets a page state once, in order.
fn hoist(
    data: &mut Map<String, Value>,
    groups: &[Group],
    declared: &dyn Fn(&str) -> Option<PageContext>,
) {
    let collections = data
        .iter()
        .filter(|(_, value)| {
            value
                .as_array()
                .is_some_and(|items| items.iter().any(Value::is_object))
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for Group {
        fields: group,
        after,
    } in groups
    {
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
        let after_on_rows = carriers.iter().any(|collection| {
            data[collection.as_str()]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| carries_any(item, after)))
        });
        if carriers.is_empty()
            || after_on_rows
            || !carriers.iter().all(|collection| movable(collection))
            || !group
                .iter()
                .all(|field| declared(&format!("/data/{field}")).is_some())
        {
            continue;
        }
        let Some(shared) = shared_values(data, &carriers, group) else {
            continue;
        };
        if group.iter().zip(&shared).any(|(field, value)| {
            data.get(field.as_str())
                .is_some_and(|stated| stated != value)
        }) {
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
                    item.shift_remove(field.as_str());
                }
            }
        }
        for (field, value) in group.iter().zip(shared) {
            data.insert(field.clone(), value);
        }
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

#[cfg(test)]
mod tests;
