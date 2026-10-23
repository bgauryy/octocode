use super::{Memo, Visit};
use crate::contracts::{ContractValidationError, validate_query};
use crate::id::ToolId;
use serde_json::{Map, Value};

/// Compact one `next`/`hints` call against the tool it names.
pub(super) fn compact_call(call: &mut Value, ctx: &mut Visit<'_, '_>, path: &[String]) {
    let target = call.get("tool").and_then(Value::as_str).map(str::to_owned);
    let (within, issues) = match (target, call.get_mut("query")) {
        (Some(tool), Some(query)) => (
            Some("query"),
            compact_rows(&tool, query, ctx.memo)
                .err()
                .map(|error| error.issues),
        ),
        _ => (None, None),
    };
    ctx.issues
        .extend(issues.into_iter().flatten().map(|mut issue| {
            let mut full = path.to_vec();
            full.extend(within.map(str::to_owned));
            full.append(&mut issue.path);
            issue.path = full;
            issue
        }));
}

/// Every row of a `{queries}` input (or a flat row) compacted against `tool`.
fn compact_rows(
    tool: &str,
    query: &mut Value,
    memo: &mut Memo,
) -> Result<(), ContractValidationError> {
    let rows: Vec<&mut Value> = match query.get_mut("queries").and_then(Value::as_array_mut) {
        Some(rows) => rows.iter_mut().collect(),
        None => vec![query],
    };
    let mut issues = Vec::new();
    for (index, row) in rows.into_iter().enumerate() {
        let row_issues = match compact_query(tool, row, memo) {
            Ok(()) => Vec::new(),
            Err(error) => error.issues,
        };
        issues.extend(row_issues.into_iter().map(|mut issue| {
            issue
                .path
                .splice(0..0, ["queries".to_owned(), index.to_string()]);
            issue
        }));
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

/// Drop each field whose removal validates to the identical query, so the
/// compact call executes exactly like the full one. Only a field the
/// contract can restore (a default) or that validation drops is tried; any
/// other present field stays. An invalid query is an error and unchanged.
///
/// Fields restore independently in the common case: a candidate drops when
/// validation dropped it or its value is the field's one declared default,
/// and one validation proves the set. A field with several defaults (an
/// operation's own) reads its restored value from one validation of the
/// query without every candidate instead. Only a query whose fields
/// interact falls back to trying each candidate in turn.
pub(super) fn compact_query(
    tool: &str,
    query: &mut Value,
    memo: &mut Memo,
) -> Result<(), ContractValidationError> {
    let Some(object) = query.as_object() else {
        return Ok(());
    };
    let key = (tool.to_owned(), query.to_string());
    if let Some(compact) = memo.get(&key) {
        *query = compact.clone();
        return Ok(());
    }
    let full = validate_query(tool, query.clone())?;
    let restorable = ToolId::from_name(tool).map(crate::contracts::restorable_fields);
    let candidates: Vec<&String> = object
        .keys()
        .filter(|field| {
            full.get(field.as_str()).is_none()
                || restorable.is_none_or(|fields| fields.contains_key(field.as_str()))
        })
        .collect();
    let mut compact = object.clone();
    if !candidates.is_empty() {
        // The restored value of each candidate: its one declared default,
        // else what validation without every candidate restores.
        let predicted = restorable.filter(|fields| {
            candidates.iter().all(|field| {
                full.get(field.as_str()).is_none()
                    || fields.get(field.as_str()).is_some_and(Option::is_some)
            })
        });
        let restored = match predicted {
            Some(_) => None,
            None => {
                let mut stripped = object.clone();
                for field in &candidates {
                    stripped.remove(field.as_str());
                }
                validate_query(tool, Value::Object(stripped)).ok()
            }
        };
        for field in &candidates {
            let full_value = full.get(field.as_str());
            let same = match predicted {
                Some(fields) => {
                    full_value.is_none()
                        || fields.get(field.as_str()).copied().flatten() == full_value
                }
                None => restored
                    .as_ref()
                    .is_some_and(|restored| restored.get(field.as_str()) == full_value),
            };
            if same {
                compact.remove(field.as_str());
            }
        }
        let proven = compact.len() == object.len()
            || validate_query(tool, Value::Object(compact.clone()))
                .is_ok_and(|validated| validated == full);
        if !proven {
            compact = object.clone();
            for field in &candidates {
                let Some(removed) = compact.remove(field.as_str()) else {
                    continue;
                };
                let restored = validate_query(tool, Value::Object(compact.clone()))
                    .is_ok_and(|validated| validated == full);
                if !restored {
                    compact.insert((*field).clone(), removed);
                }
            }
        }
    }
    // Keep the caller's field order for the fields that remain.
    let ordered: Map<String, Value> = object
        .iter()
        .filter_map(|(field, _)| compact.get(field).map(|v| (field.clone(), v.clone())))
        .collect();
    *query = Value::Object(ordered);
    memo.insert(key, query.clone());
    Ok(())
}
