//! Publish explicitly typed calls at schema-declared output positions.
//! No name-based routing, default stripping, scope rewriting or caller-state merging occurs here.
use crate::contracts::{self, ContractValidationError, ValidationIssue};
use crate::id::ToolId;
use serde_json::Value;

/// The name of every linear walk (core `continuationChannels`).
pub(crate) const LINEAR_WALK: &str = "nextPage";

pub(crate) struct Scope<'a> {
    pub is_available: &'a dyn Fn(&str) -> bool,
}

/// Validate related requests and retain complete normalized continuation requests before exposing handles.
pub(crate) fn finalize(
    structured: &mut Value,
    tool: ToolId,
    scope: &Scope<'_>,
    seal: &mut dyn FnMut(ToolId, Value, Option<usize>) -> Option<String>,
) -> Result<(), ContractValidationError> {
    let mut positions = Vec::new();
    visit_calls(tool, structured, &mut |_, path| {
        positions.push(path.to_vec());
    })?;
    let mut issues = Vec::new();
    for path in positions.into_iter().rev() {
        let pointer = crate::json_pointer::of(&path);
        let origin = path
            .first()
            .filter(|key| key.as_str() == "results")
            .and_then(|_| path.get(1)?.parse::<usize>().ok())
            .and_then(|position| {
                structured
                    .get("results")?
                    .get(position)?
                    .get("index")?
                    .as_u64()
            })
            .and_then(|index| usize::try_from(index).ok());
        let Some(call) = structured.pointer_mut(&pointer) else {
            continue;
        };
        match publish(call, scope, seal, origin) {
            Ok(true) => {}
            Ok(false) => remove_at(structured, &path),
            Err(error) => issues.extend(error.issues.into_iter().map(|mut issue| {
                issue.path.splice(0..0, path.iter().cloned());
                issue
            })),
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

fn publish(
    call: &mut Value,
    scope: &Scope<'_>,
    seal: &mut dyn FnMut(ToolId, Value, Option<usize>) -> Option<String>,
    origin: Option<usize>,
) -> Result<bool, ContractValidationError> {
    let target = call["tool"]
        .as_str()
        .and_then(ToolId::from_name)
        .ok_or_else(|| defect(&["tool"], "Continuation names no declared tool."))?;
    let related = call["kind"] == "related";
    if !(scope.is_available)(target.as_str()) {
        return if related {
            Ok(false)
        } else {
            Err(defect(
                &[],
                "Required continuation targets an unavailable tool.",
            ))
        };
    }
    let input = checked_input(target, related, &call["input"])?;
    if related {
        return Ok(true);
    }
    let token = seal(target, input, origin)
        .ok_or_else(|| defect(&[], "Continuation storage failed; no handle was published."))?;
    call["input"] = serde_json::json!({"cursor":token});
    Ok(true)
}

/// The contract check every call passes before it is published: a related
/// call's input validates as a fresh request, continued work's as a saved
/// request with its private progress. `Ok` holds the validated input; for
/// continued work that is what its cursor saves.
pub(crate) fn checked_input(
    target: ToolId,
    related: bool,
    input: &Value,
) -> Result<Value, ContractValidationError> {
    if related {
        return contracts::validate(target.as_str(), input.clone());
    }
    let rows = contracts::prepare_saved_rows(target.as_str(), input.clone())?
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let mut prepared = input.clone();
    prepared["queries"] = Value::Array(rows);
    Ok(prepared)
}

fn defect(path: &[&str], message: &str) -> ContractValidationError {
    ContractValidationError {
        issues: vec![ValidationIssue {
            rule_id: "output.continuation".into(),
            path: path.iter().map(|part| (*part).into()).collect(),
            message: message.into(),
            schema: None,
            received: None,
        }],
    }
}

/// Where the rows of `structured` carry a linear walk: every `next.nextPage`
/// at a schema-declared call position under `results`.
pub(crate) fn linear_walks(
    tool: ToolId,
    structured: &Value,
) -> Result<Vec<Vec<String>>, ContractValidationError> {
    let mut walks = Vec::new();
    visit_calls(tool, structured, &mut |call, path| {
        let linear = path.first().is_some_and(|key| key == "results")
            && path.ends_with(&["next".to_owned(), LINEAR_WALK.to_owned()])
            && call["kind"] == "continue";
        if linear {
            walks.push(path.to_vec());
        }
    })?;
    Ok(walks)
}

/// The distinct handles of `walks`, in document order: a lead repeated on
/// several rows is one walk.
pub(crate) fn walk_handles(structured: &Value, walks: &[Vec<String>]) -> Vec<String> {
    let mut handles = Vec::new();
    for walk in walks {
        if let Some(handle) = structured
            .pointer(&format!("{}/input/cursor", crate::json_pointer::of(walk)))
            .and_then(Value::as_str)
            && !handles.iter().any(|known| known == handle)
        {
            handles.push(handle.to_owned());
        }
    }
    handles
}

/// Take `walks` off the rows and state their one continuation, `handle`, as
/// the response's.
pub(crate) fn hoist(structured: &mut Value, walks: &[Vec<String>], tool: ToolId, handle: &str) {
    for walk in walks {
        remove_at(structured, walk);
        let parent = &walk[..walk.len() - 1];
        if structured
            .pointer(&crate::json_pointer::of(parent))
            .and_then(Value::as_object)
            .is_some_and(serde_json::Map::is_empty)
        {
            remove_at(structured, parent);
        }
    }
    structured["responsePagination"] = serde_json::json!({
        "hasMore": true,
        "next": {"kind": "continue", "tool": tool.as_str(), "input": {"cursor": handle}},
    });
}

/// Call `visit` with every call at a position `tool`'s output schema declares
/// in `structured`, and its path, in document order.
pub fn visit_calls(
    tool: ToolId,
    structured: &Value,
    visit: &mut dyn FnMut(&Value, &[String]),
) -> Result<(), ContractValidationError> {
    call_tree::visit(tool, structured, visit).map_err(|message| defect(&[], message))
}

fn remove_at(value: &mut Value, path: &[String]) {
    let Some((last, parent)) = path.split_last() else {
        return;
    };
    match value.pointer_mut(&crate::json_pointer::of(parent)) {
        Some(Value::Object(object)) => {
            object.remove(last);
        }
        Some(Value::Array(items)) => {
            if let Ok(index) = last.parse::<usize>() {
                items.remove(index);
            }
        }
        _ => {}
    }
}

mod call_tree;
#[cfg(test)]
mod tests;
