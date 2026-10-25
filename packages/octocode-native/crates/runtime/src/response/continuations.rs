//! Publish explicitly typed calls at schema-declared output positions.
//! No name-based routing, default stripping, scope rewriting or caller-state merging occurs here.
use crate::contracts::{self, ContractValidationError, ValidationIssue};
use crate::id::ToolId;
use serde_json::Value;

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
        let pointer = pointer(&path);
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
    if related {
        contracts::validate(target.as_str(), call["input"].clone())?;
        return Ok(true);
    }
    let mut input = call["input"].clone();
    let rows = contracts::prepare_saved_rows(target.as_str(), input.clone())?
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    input["queries"] = Value::Array(rows);
    let token = seal(target, input, origin)
        .ok_or_else(|| defect(&[], "Continuation storage failed; no handle was published."))?;
    call["input"] = serde_json::json!({"cursor":token});
    Ok(true)
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

pub fn visit_calls(
    tool: ToolId,
    structured: &Value,
    visit: &mut dyn FnMut(&Value, &[String]),
) -> Result<(), ContractValidationError> {
    let contract = contracts::tool_contract(tool).map_err(|message| defect(&[], message))?;
    let schema = &contract["outputSchema"];
    walk(
        structured,
        &[schema],
        &schema["$defs"],
        &mut Vec::new(),
        visit,
    );
    Ok(())
}

fn pointer(path: &[String]) -> String {
    path.iter()
        .map(|part| format!("/{}", part.replace('~', "~0").replace('/', "~1")))
        .collect()
}

fn remove_at(value: &mut Value, path: &[String]) {
    let Some((last, parent)) = path.split_last() else {
        return;
    };
    match value.pointer_mut(&pointer(parent)) {
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

fn walk(
    value: &Value,
    schemas: &[&Value],
    defs: &Value,
    path: &mut Vec<String>,
    visit: &mut dyn FnMut(&Value, &[String]),
) {
    let declared_call = schemas.iter().any(|schema| {
        !contracts::property_schemas(schema, defs, "kind").is_empty()
            && !contracts::property_schemas(schema, defs, "tool").is_empty()
            && !contracts::property_schemas(schema, defs, "input").is_empty()
    });
    if declared_call && super::result::is_call(value) {
        visit(value, path);
        return;
    }
    match value {
        Value::Array(items) => {
            let declared = schemas
                .iter()
                .flat_map(|schema| contracts::item_schemas(schema, defs))
                .collect::<Vec<_>>();
            if declared.is_empty() {
                return;
            }
            for (index, item) in items.iter().enumerate() {
                path.push(index.to_string());
                walk(item, &declared, defs, path, visit);
                path.pop();
            }
        }
        Value::Object(object) => {
            for (key, child) in object {
                let declared = schemas
                    .iter()
                    .flat_map(|schema| contracts::child_schemas(schema, defs, key))
                    .collect::<Vec<_>>();
                if !declared.is_empty() {
                    path.push(key.clone());
                    walk(child, &declared, defs, path, visit);
                    path.pop();
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
