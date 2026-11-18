//! A fact every result of a batch that reads one source repeats is stated
//! once: graph results of one snapshot otherwise each state its root,
//! creation time, freshness and coverage. The output contract decides what
//! moves (`pageContextFields.batchStatedOnce`): each group names the pointer
//! that identifies what a result read (`by`, such as a snapshot id) and the
//! pointers of that reading's facts (`fields`), each under a result row. A
//! later result whose `by` an earlier one shares omits each field whose
//! value equals the one last stated for that `by`; a different value is
//! stated. `by` stays on every result, so each names what it read, and a
//! result without it (a failed row) is left whole. Results are kept in
//! batch order on every page, so the result that states a fact precedes
//! every result that omits it.
use crate::id::ToolId;
use serde_json::Value;

/// A group of result facts a batch states once per `by` value, as core
/// declares it (`pageContextFields.batchStatedOnce`).
#[derive(Debug)]
pub(crate) struct BatchGroup {
    by: String,
    fields: Vec<String>,
}

#[expect(
    clippy::expect_used,
    reason = "a malformed embedded group list is a contract defect caught for every tool by contract tests"
)]
fn batch_stated_once(tool: ToolId) -> &'static [BatchGroup] {
    static GROUPS: [std::sync::OnceLock<Vec<BatchGroup>>; ToolId::ALL.len()] =
        [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    GROUPS[tool.index()].get_or_init(|| {
        let contract =
            crate::contracts::tool_contract(tool).expect("embedded contract contains every tool");
        // A contract that declares no group states every result whole.
        contract["pageContextFields"]["batchStatedOnce"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|group| BatchGroup {
                by: group["by"].as_str().expect("group by").to_owned(),
                fields: group["fields"]
                    .as_array()
                    .expect("group fields")
                    .iter()
                    .map(|field| field.as_str().expect("group field").to_owned())
                    .collect(),
            })
            .collect()
    })
}

/// States each fact `tool`'s contract lets a batch state once on the first
/// result that reads it ([`BatchGroup`]).
pub(crate) fn state_once_per_batch(tool: ToolId, structured: &mut Value) {
    let groups = batch_stated_once(tool);
    if groups.is_empty() {
        return;
    }
    if let Some(results) = structured.get_mut("results").and_then(Value::as_array_mut) {
        state_once(results, groups);
    }
}

/// [`state_once_per_batch`] over `results` in batch order.
fn state_once(results: &mut [Value], groups: &[BatchGroup]) {
    for group in groups {
        // Per `by` value and field, the value last stated.
        let mut stated: Vec<(Value, &str, Value)> = Vec::new();
        for result in results.iter_mut() {
            if result["status"] == "error" {
                continue;
            }
            let Some(by) = result
                .pointer(&group.by)
                .filter(|by| !by.is_null())
                .cloned()
            else {
                continue;
            };
            for field in &group.fields {
                let Some(value) = result.pointer(field).cloned() else {
                    continue;
                };
                match stated
                    .iter_mut()
                    .find(|(read, name, _)| *read == by && *name == field.as_str())
                {
                    Some((_, _, last)) if *last == value => remove(result, field),
                    Some((_, _, last)) => *last = value,
                    None => stated.push((by.clone(), field, value)),
                }
            }
        }
    }
}

/// Removes the member `pointer` names from `value`.
fn remove(value: &mut Value, pointer: &str) {
    let Some((parent, key)) = pointer.rsplit_once('/') else {
        return;
    };
    if let Some(object) = value.pointer_mut(parent).and_then(Value::as_object_mut) {
        object.shift_remove(&crate::json_pointer::unescape(key));
    }
}

#[cfg(test)]
mod tests;
