use super::super::ValidationIssue;
use super::{
    Edit, bound_value, dot_relative, kind_respelling, line_range, name_respelling, pointer,
    span_respelling,
};
use serde_json::Value;

/// A list sent to a row field (`queries.N.field`) whose schema takes one
/// value, when the field accepts every item: one row per item. A row that
/// sets a field anchored to one value (`x-anchoredBy`: the `line` of one
/// `symbolName`) has no such split: each row would keep the one anchor.
fn split_list(input: &Value, issue: &ValidationIssue) -> Option<Edit> {
    let [queries, index, field] = issue.path.as_slice() else {
        return None;
    };
    (queries == "queries").then_some(())?;
    let items = issue.received.as_ref()?.as_array()?;
    let schema = issue.schema.as_ref()?;
    let row = pointer(input, &issue.path[..2])?.as_object()?;
    let anchored = schema
        .get("x-anchoredBy")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|anchor| row.contains_key(anchor));
    (!anchored).then_some(())?;
    let accepted = !items.is_empty()
        && items.iter().all(|item| {
            super::super::validate_input_schema(schema, schema, &mut item.clone(), &mut Vec::new())
                .is_ok()
        });
    accepted.then_some(())?;
    Some(Edit::Split {
        index: index.parse().ok()?,
        field: field.clone(),
    })
}

/// The edit that clears `issue`, when there is one.
pub(super) fn fix(input: &Value, issue: &ValidationIssue) -> Option<Edit> {
    let path = issue.path.clone();
    // A field that conflicts with a sibling the caller set: removing it
    // keeps every constraint the sibling states.
    if issue
        .schema
        .as_ref()
        .is_some_and(|schema| schema.get("conflictsWith").is_some())
    {
        return Some(Edit::Remove(path));
    }
    // Validation read the fix from the row itself (`super::super::repository`, an
    // omitted selector): set exactly those fields.
    if let Some(fields) = super::super::repository::set_fields(issue) {
        let approximate = issue
            .schema
            .as_ref()
            .and_then(|schema| schema.get("approximateFields")?.as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let (_, parent) = path.split_last()?;
        return Some(Edit::Fields {
            parent: parent.to_vec(),
            fields: fields
                .iter()
                .map(|(field, value)| (field.clone(), (!value.is_null()).then(|| value.clone())))
                .collect(),
            approximate,
        });
    }
    if let Some(edit) = line_range(issue).or_else(|| dot_relative(issue)) {
        return Some(edit);
    }
    match issue.rule_id.as_str() {
        crate::contracts::ENVELOPE_ARRAY => input.is_array().then_some(Edit::Wrap),
        crate::contracts::ENVELOPE_NESTED => {
            let [queries, index, ..] = path.as_slice() else {
                return None;
            };
            (queries == "queries").then_some(())?;
            let envelope = issue
                .schema
                .as_ref()
                .and_then(|schema| schema.get("envelopeFields")?.as_array())
                .map(|fields| {
                    fields
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            Some(Edit::Flatten {
                index: index.parse().ok()?,
                envelope,
            })
        }
        "schema.unknown-field" => {
            let known = issue
                .schema
                .as_ref()
                .and_then(|schema| schema.get("knownFields")?.as_array())
                .map(|fields| fields.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                .unwrap_or_default();
            let field = path.last()?;
            if let Some(edit) = span_respelling(input, issue)
                .or_else(|| name_respelling(input, issue))
                .or_else(|| kind_respelling(input, issue))
            {
                return Some(edit);
            }
            // A row field the envelope takes moves beside `queries`.
            if super::super::envelope_field(issue) {
                return Some(Edit::Lift(path));
            }
            // A field another form of the row declares had no effect here.
            // When the row's own selector picks its form, removing it drops
            // what the caller set (lossy, named); else (a sibling only the
            // caller can fill is missing) removing it is exact.
            if super::super::sibling_requirement(issue).is_some() {
                let conflict = issue.schema.as_ref().is_some_and(|schema| {
                    schema.get("siblingConflict") == Some(&Value::Bool(true))
                });
                return Some(if conflict {
                    Edit::Drop(path)
                } else {
                    Edit::Remove(path)
                });
            }
            // The caller meant a known field (a near spelling, or one that
            // expresses this constraint): only the caller can say how.
            if super::super::suggest_field(field, &known).is_some()
                || super::super::cover_field(issue).is_some()
            {
                return None;
            }
            if let [field] = path.as_slice()
                && input.get("queries").is_some_and(Value::is_array)
            {
                return Some(Edit::IntoRows(field.clone()));
            }
            Some(Edit::Drop(path))
        }
        "schema.enum" => {
            let allowed = issue.schema.as_ref()?.get("enum")?.as_array()?;
            let (field, parent) = path.split_last()?;
            // A list value no item of the enum means (`comments` is no
            // misspelled `commits`): dropping it reads less than asked.
            if pointer(input, parent)?.is_array() {
                return Some(Edit::Drop(path));
            }
            // The one listed value the scalar means; with none (or several)
            // only the caller knows, and dropping it would widen the call.
            let value =
                super::super::enum_value::enum_value(allowed, issue.received.as_ref()?, field)?;
            Some(Edit::Set(path, Value::from(value)))
        }
        "schema.range" => {
            let schema = issue.schema.as_ref()?;
            let number = issue.received.as_ref()?.as_f64()?;
            let limit = |key: &str| schema.get(key).and_then(Value::as_f64);
            let bound = match (
                limit("minimum").filter(|minimum| number < *minimum),
                limit("maximum").filter(|maximum| number > *maximum),
            ) {
                (Some(bound), _) | (None, Some(bound)) => bound,
                (None, None) => return None,
            };
            // Zero or less of a positive size or count asks for no usable
            // amount: clamping it to the minimum serves one unit (a page of
            // one char), so it is dropped (lossy) and the default applies.
            // When the default is that minimum, setting it says the same,
            // exactly. A required field has no default: the dropped row
            // fails `required`, which has no fix, so the call has no lead.
            if number <= 0.0
                && bound > 0.0
                && schema.get("default").and_then(Value::as_f64) != Some(bound)
            {
                return Some(Edit::Drop(path));
            }
            Some(Edit::Set(path, bound_value(bound)))
        }
        "schema.type" if issue.received.as_ref().is_some_and(Value::is_array) => {
            split_list(input, issue)
        }
        // A whole number or a boolean for a text field: its text is exact
        // (a fraction is not: `1.10` already arrived as `1.1`). A count
        // range (`x-countRange`: `">=100"`, `"10..50"`) has no one reading
        // of a bare number: at least that many, or exactly that many.
        "schema.type"
            if issue.schema.as_ref()?.get("type")? == "string"
                && issue.schema.as_ref()?.get("x-countRange").is_none() =>
        {
            let text = match issue.received.as_ref()? {
                Value::Bool(flag) => flag.to_string(),
                Value::Number(number) if number.is_i64() || number.is_u64() => number.to_string(),
                _ => return None,
            };
            Some(Edit::Set(path, Value::String(text)))
        }
        "schema.type" => {
            let text = issue.received.as_ref()?.as_str()?.trim();
            let value = match issue.schema.as_ref()?.get("type")?.as_str()? {
                "integer" => Value::from(text.parse::<i64>().ok()?),
                "number" => {
                    serde_json::Number::from_f64(text.parse::<f64>().ok()?).map(Value::Number)?
                }
                "boolean" => Value::Bool(text.parse::<bool>().ok()?),
                kind @ ("array" | "object") => {
                    let parsed: Value = match serde_json::from_str(text) {
                        Ok(parsed) => parsed,
                        Err(_)
                            if kind == "array"
                                && !super::super::shape::looks_like_json_array(text) =>
                        {
                            let schema = issue.schema.as_ref()?;
                            let mut wrapped = Value::Array(vec![issue.received.as_ref()?.clone()]);
                            super::super::validate_input_schema(
                                schema,
                                schema,
                                &mut wrapped,
                                &mut Vec::new(),
                            )
                            .ok()?;
                            wrapped
                        }
                        Err(_) => return None,
                    };
                    if (kind == "array" && !parsed.is_array())
                        || (kind == "object" && !parsed.is_object())
                    {
                        return None;
                    }
                    parsed
                }
                _ => return None,
            };
            Some(Edit::Set(path, value))
        }
        _ => None,
    }
}
