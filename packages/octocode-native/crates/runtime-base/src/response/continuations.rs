//! Publish explicitly typed calls at schema-declared output positions.
//! No name-based routing, default stripping, scope rewriting or caller-state merging occurs here.
use crate::contracts::{self, ContractValidationError, ValidationIssue};
use crate::id::ToolId;
use serde_json::Value;

/// The name of every linear walk (core `continuationChannels`).
pub const LINEAR_WALK: &str = "nextPage";
/// The envelope's one continuation: the call that continues every
/// unfinished row of the reply. A reply that carries it is unfinished.
pub const RESPONSE_NEXT: &str = "next";
/// When [`RESPONSE_NEXT`]'s saved work stops answering, and the fact that a
/// page was read back from saved output: envelope members beside it.
pub const RESPONSE_EXPIRES_AT: &str = "expiresAt";
pub const RESPONSE_REPLAYED: &str = "replayed";

pub struct Scope<'a> {
    pub is_available: &'a dyn Fn(&str) -> bool,
}

/// [`checked_input`], or a caller's copy of it that reads back what it
/// checked before.
pub type Check<'a> = &'a dyn Fn(ToolId, bool, &Value) -> Result<Value, ContractValidationError>;

/// Validate related requests and retain complete normalized continuation requests before exposing handles.
pub fn finalize(
    structured: &mut Value,
    tool: ToolId,
    scope: &Scope<'_>,
    seal: &mut dyn FnMut(ToolId, Value, Option<usize>) -> Option<String>,
) -> Result<(), ContractValidationError> {
    finalize_with(structured, tool, scope, &checked_input, seal)
}

/// [`finalize`], each call checked by `check`: the runtime passes one that
/// reads back a call it checked on an earlier request.
pub fn finalize_with(
    structured: &mut Value,
    tool: ToolId,
    scope: &Scope<'_>,
    check: Check<'_>,
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
        match publish(call, scope, check, seal, origin) {
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
    check: Check<'_>,
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
    let input = check(target, related, &call["input"])?;
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
/// continued work that is what its cursor saves. A pure function of the
/// contract, the target, the kind and the input.
pub fn checked_input(
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

/// Whether `name` under a row's `next` is a linear walk of `tool`:
/// `nextPage`, or a channel core declares linear (one section of a row
/// paged by section, ghGetHistoryItem's `next.<section>`).
fn is_linear(tool: ToolId, name: &str) -> bool {
    name == LINEAR_WALK
        || contracts::parsed_contract()
            .ok()
            .and_then(|contract| {
                contract.pointer(&format!(
                    "/continuationChannels/{}/row/{name}",
                    tool.as_str()
                ))
            })
            .is_some_and(|kind| kind == "linear" || kind["kind"] == "linear")
}

/// Where the rows of `structured` carry a linear walk: every `next.<walk>`
/// ([`is_linear`]) at a schema-declared call position under `results`.
pub fn linear_walks(
    tool: ToolId,
    structured: &Value,
) -> Result<Vec<Vec<String>>, ContractValidationError> {
    let mut walks = Vec::new();
    visit_calls(tool, structured, &mut |call, path| {
        let linear = path.first().is_some_and(|key| key == "results")
            && matches!(path, [.., next, name] if next == "next" && is_linear(tool, name))
            && call["kind"] == "continue";
        if linear {
            walks.push(path.to_vec());
        }
    })?;
    Ok(walks)
}

/// The distinct handles of `walks`, in document order: a lead repeated on
/// several rows is one walk.
pub fn walk_handles(structured: &Value, walks: &[Vec<String>]) -> Vec<String> {
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
pub fn hoist(structured: &mut Value, walks: &[Vec<String>], tool: ToolId, handle: &str) {
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
    structured[RESPONSE_NEXT] =
        crate::response::result::Continuation::cursor(tool.as_str(), handle);
}

/// Drop `kind` from every call at a position `tool`'s output schema declares
/// in `structured`, once the response is final: a served call is its `tool`
/// and `input`, and the channel that holds it says what it does (the
/// envelope's `next` and a row's `next.*` continue this answer, `related[]`
/// starts new work). The runtime routes, publishes and validates calls by `kind`
/// before this point.
pub fn drop_call_kinds(
    tool: ToolId,
    structured: &mut Value,
) -> Result<(), ContractValidationError> {
    let mut positions = Vec::new();
    visit_calls(tool, structured, &mut |_, path| {
        positions.push(path.to_vec());
    })?;
    for path in positions {
        if let Some(Value::Object(call)) = structured.pointer_mut(&crate::json_pointer::of(&path)) {
            call.shift_remove("kind");
        }
    }
    Ok(())
}

/// Call `visit` with every call at a position `tool`'s output schema declares
/// in `structured`, and its path, in document order: a call the runtime is
/// still shaping (with its `kind`) or one it served (without).
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
