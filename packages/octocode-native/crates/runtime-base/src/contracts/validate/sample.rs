//! Values a schema accepts, built from the schema alone: the shape a
//! rejected field is shown in when the request's own value cannot stand.

use super::super::schema_facts::{
    child_schemas, item_schemas, property_order, property_schemas, resolve_ref, schema_keyword,
};
use serde_json::Value;

/// How deep a sample descends into nested objects and lists.
const DEPTH: usize = 8;

/// A visible placeholder for a required `field` the rejected row lacks, in
/// the shape of the example's `value`: the field's name in capitals
/// (`repo` → `REPO`, `matchString` → `MATCH_STRING`), never a value that
/// reads as a real name. `None` for a value that is not text.
pub(super) fn placeholder(field: &str, value: &Value) -> Option<Value> {
    let mut name = String::new();
    for (at, character) in field.chars().enumerate() {
        if at > 0 && character.is_ascii_uppercase() {
            name.push('_');
        }
        name.push(character.to_ascii_uppercase());
    }
    match value {
        Value::String(_) => Some(Value::String(name)),
        Value::Array(items) if items.first().is_some_and(Value::is_string) => {
            Some(Value::Array(vec![Value::String(name)]))
        }
        _ => None,
    }
}

/// The schemas that describe the value at `path` (object keys, list
/// indexes) under `schema`: one per form of the schema that declares it.
pub(super) fn schemas_at<'a>(
    schema: &'a Value,
    defs: &'a Value,
    path: &[String],
) -> Vec<&'a Value> {
    let mut found = vec![schema];
    for segment in path {
        found = schemas_at_step(found, defs, segment);
    }
    found
}

/// [`schemas_at`] for a fault inside `own` (the row's value of the field
/// `path` starts with): at each step, a form that pins a field to a literal
/// the row's value there does not hold is left out, so the schemas are
/// those of the form the row's own selector names (a clasify `choice`
/// question's `criteria`, not `noul`'s). A value whose selectors name no
/// form keeps every form.
fn schemas_of_own<'a>(
    schema: &'a Value,
    defs: &'a Value,
    path: &[String],
    own: &Value,
) -> Vec<&'a Value> {
    let mut found = vec![schema];
    for (depth, segment) in path.iter().enumerate() {
        // The row itself is not at hand: its field is taken in every form.
        let holder = depth
            .checked_sub(1)
            .and_then(|inside| own.pointer(&crate::json_pointer::of(&path[1..=inside])));
        let narrowed = found
            .iter()
            .flat_map(|schema| children_of(schema, defs, segment, holder, 0))
            .collect::<Vec<_>>();
        found = if narrowed.is_empty() {
            schemas_at_step(found, defs, segment)
        } else {
            narrowed
        };
    }
    found
}

/// One step of [`schemas_at`]: the schemas of `segment` under each of `found`.
fn schemas_at_step<'a>(found: Vec<&'a Value>, defs: &'a Value, segment: &str) -> Vec<&'a Value> {
    found
        .into_iter()
        .flat_map(|schema| {
            if segment.parse::<usize>().is_ok() {
                item_schemas(schema, defs)
            } else {
                child_schemas(schema, defs, segment)
            }
        })
        .collect()
}

/// The schemas of `segment` under `schema` and under each form it composes,
/// apart from a form `holder` (the value that holds the segment) selects
/// against ([`selects_against`]).
fn children_of<'a>(
    schema: &'a Value,
    defs: &'a Value,
    segment: &str,
    holder: Option<&Value>,
    depth: usize,
) -> Vec<&'a Value> {
    let shape = resolve_ref(schema, defs);
    if depth > DEPTH || holder.is_some_and(|holder| selects_against(shape, defs, holder)) {
        return Vec::new();
    }
    let own = if segment.parse::<usize>().is_ok() {
        shape.get("items")
    } else {
        shape["properties"].get(segment).or_else(|| {
            shape
                .get("additionalProperties")
                .filter(|schema| schema.is_object())
        })
    };
    let mut found = own.into_iter().collect::<Vec<_>>();
    for composition in ["allOf", "oneOf", "anyOf"] {
        for form in shape[composition].as_array().into_iter().flatten() {
            found.extend(children_of(form, defs, segment, holder, depth + 1));
        }
    }
    found
}

/// Whether the object `value` holds a field that `shape` pins to other
/// literals (`const`, `enum`): the value names another form of the union
/// `shape` is a form of.
fn selects_against(shape: &Value, defs: &Value, value: &Value) -> bool {
    let (Some(fields), Some(properties)) = (value.as_object(), shape["properties"].as_object())
    else {
        return false;
    };
    properties.iter().any(|(name, property)| {
        let Some(held) = fields.get(name) else {
            return false;
        };
        let property = resolve_ref(property, defs);
        match (property.get("const"), property["enum"].as_array()) {
            (Some(constant), _) => constant != held,
            (None, Some(members)) => !members.contains(held),
            (None, None) => false,
        }
    })
}

/// A value `schema` accepts, or `None` when the schema gives no basis for
/// one (a text pattern, an unknown format). A declared `default`, `const`
/// or the first `enum` member stands for the value; an object holds each
/// property it declares; text is the placeholder of `name`
/// ([`placeholder`]) and a date is the epoch.
pub(super) fn sample(schema: &Value, defs: &Value, name: &str) -> Option<Value> {
    sample_within(schema, defs, name, 0)
}

fn sample_within(schema: &Value, defs: &Value, name: &str, depth: usize) -> Option<Value> {
    if depth > DEPTH {
        return None;
    }
    if let Some(default) = schema_keyword(schema, defs, "default") {
        return Some(default.clone());
    }
    let shape = resolve_ref(schema, defs);
    if let Some(constant) = shape.get("const") {
        return Some(constant.clone());
    }
    if let Some(first) = shape["enum"].as_array().and_then(|members| members.first()) {
        return Some(first.clone());
    }
    for union in ["oneOf", "anyOf"] {
        if let Some(branches) = shape[union].as_array() {
            return branches
                .iter()
                .find_map(|branch| sample_within(branch, defs, name, depth + 1));
        }
    }
    let kind = match &shape["type"] {
        Value::String(kind) => Some(kind.as_str()),
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .find(|kind| *kind != "null"),
        _ => shape.get("properties").map(|_| "object"),
    }?;
    match kind {
        "object" => sample_object(shape, defs, name, depth),
        "array" => {
            let item = item_schemas(shape, defs)
                .into_iter()
                .find_map(|item| sample_within(item, defs, name, depth + 1))?;
            let count = shape["minItems"].as_u64().map_or(1, |least| least.max(1));
            Some(Value::Array(
                (0..count).map(|_| item.clone()).collect::<Vec<_>>(),
            ))
        }
        "string" => sample_text(shape, name),
        "integer" | "number" => Some(
            shape
                .get("minimum")
                .cloned()
                .or_else(|| {
                    let above = shape["exclusiveMinimum"].as_i64()?;
                    Some(Value::from(above + 1))
                })
                .unwrap_or_else(|| Value::from(0)),
        ),
        "boolean" => Some(Value::Bool(false)),
        _ => None,
    }
}

/// Text a string schema accepts: the first of the field's placeholder, an
/// epoch date-time, an epoch date and a one-day duration that passes the
/// schema's `format` and `pattern`; `None` when none does.
fn sample_text(shape: &Value, name: &str) -> Option<Value> {
    let placeholder = placeholder(name, &Value::String(String::new()))?;
    let pattern = shape["pattern"].as_str();
    let format = shape["format"].as_str();
    [
        placeholder.as_str().unwrap_or_default(),
        "1970-01-01T00:00:00Z",
        "1970-01-01",
        "-P1D",
    ]
    .into_iter()
    .find(|text| {
        let by_format = match format {
            None => true,
            Some("date-time") => text.contains('T') && text.ends_with('Z'),
            Some("date") => text.len() == 10 && text.starts_with(char::is_numeric),
            Some(_) => false,
        };
        by_format && pattern.is_none_or(|pattern| super::schema::pattern_matches(pattern, text))
    })
    .map(|text| Value::String(text.to_owned()))
}

/// An object holding each property its schema requires, by its sample: a
/// property the caller may leave out is no part of the fix, and its sample
/// would state a filter nobody asked for (`bytes: {max: 0, min: 0}` selects
/// only empty files). A schema that requires none but refuses an empty
/// object holds its first property that has a sample, which shows the shape.
/// `None` when a required property has no sample, or nothing can be held.
fn sample_object(shape: &Value, defs: &Value, name: &str, depth: usize) -> Option<Value> {
    let required = shape["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    let sample_of = |property: &str| {
        property_schemas(shape, defs, property)
            .into_iter()
            .find_map(|schema| sample_within(schema, defs, property, depth + 1))
    };
    let mut fields = serde_json::Map::new();
    for property in property_order(shape, defs) {
        if required.contains(&property) {
            fields.insert(property.to_owned(), sample_of(property)?);
        }
    }
    if fields.is_empty() && shape.get("minProperties").is_some() {
        let declared = property_order(shape, defs)
            .into_iter()
            .find_map(|name| Some((name, sample_of(name)?)));
        match declared {
            Some((name, value)) => {
                fields.insert(name.to_owned(), value);
            }
            None => return sample_record(shape, defs, name, depth),
        }
    }
    Some(Value::Object(fields))
}

/// A record (an object of any keys, each value of one schema) that refuses
/// an empty object: as many entries as it requires, each key a numbered
/// placeholder of the field's `name` (`CRITERIA_1`, `CRITERIA_2`: a key
/// names what the caller chooses, so none reads as a real one) and each
/// value the sample of the record's value schema. `None` when the values
/// have no sample.
fn sample_record(shape: &Value, defs: &Value, name: &str, depth: usize) -> Option<Value> {
    let values = shape
        .get("additionalProperties")
        .filter(|values| values.is_object())?;
    let value = sample_within(values, defs, name, depth + 1)?;
    let key = placeholder(name, &Value::String(String::new()))?;
    let key = key.as_str()?;
    let count = shape["minProperties"]
        .as_u64()
        .map_or(1, |least| least.max(1));
    Some(Value::Object(
        (1..=count)
            .map(|entry| (format!("{key}_{entry}"), value.clone()))
            .collect(),
    ))
}

/// Every literal the schemas at `path` under `tool_name`'s query name
/// (`const`, `enum`, through unions), each once: the values a field that
/// selects a form may hold.
pub(super) fn literals(tool_name: &str, path: &[String]) -> Vec<Value> {
    fn collect(schema: &Value, defs: &Value, out: &mut Vec<Value>) {
        let shape = resolve_ref(schema, defs);
        let named = shape["enum"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(shape.get("const"));
        for literal in named {
            if !out.contains(literal) {
                out.push(literal.clone());
            }
        }
        for union in ["oneOf", "anyOf"] {
            for branch in shape[union].as_array().into_iter().flatten() {
                collect(branch, defs, out);
            }
        }
    }
    let Some(tool) = crate::id::ToolId::from_name(tool_name)
        .and_then(|id| crate::contracts::tool_contract(id).ok())
    else {
        return Vec::new();
    };
    let schema = resolve_ref(&tool["querySchema"], &Value::Null);
    let defs = &schema["$defs"];
    let mut out = Vec::new();
    for schema in schemas_at(schema, defs, path) {
        collect(schema, defs, &mut out);
    }
    out
}

/// The row's `own` value of the field its `faults` lie in (each a path from
/// the field, as the row holds it), with each fault shown in a shape the
/// query schema accepts: a wrong value is replaced by a sample of its
/// schema ([`sample`]), a key the schema does not declare is removed, a
/// required key the value lacks is set. One candidate per form of the schema
/// that declares a fault, the plainest first.
pub(super) fn sampled(tool_name: &str, own: &Value, faults: &[&[String]]) -> Vec<Value> {
    let Some(tool) = crate::id::ToolId::from_name(tool_name)
        .and_then(|id| crate::contracts::tool_contract(id).ok())
    else {
        return Vec::new();
    };
    let schema = resolve_ref(&tool["querySchema"], &Value::Null);
    let defs = &schema["$defs"];
    let (settled, left) = settle_named_siblings(schema, defs, own, faults);
    let mut candidates = shown_faults(schema, defs, &settled, &left);
    // A sibling found without the row's form (its `operation` is not in
    // `own`) may belong to another form: the faults as sent follow, each
    // shown by its own schema.
    if left.len() < faults.len() {
        for candidate in shown_faults(schema, defs, own, faults) {
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

/// `own` with each of `faults` shown ([`sampled`]).
fn shown_faults(schema: &Value, defs: &Value, own: &Value, faults: &[&[String]]) -> Vec<Value> {
    let samples = faults
        .iter()
        .map(|fault| {
            // A list item is named by its list: `extensions.0` is an
            // `EXTENSIONS` text, never a `0`.
            let name = fault
                .iter()
                .rev()
                .find(|segment| segment.parse::<usize>().is_err())
                .map_or("", String::as_str);
            let sent = own.pointer(&crate::json_pointer::of(&fault[1..]));
            schemas_of_own(schema, defs, fault, own)
                .into_iter()
                .filter_map(|schema| {
                    // The value sent, in the type its field takes (the fix
                    // its error names: `"5"`, not `5`), before a sample.
                    sent.and_then(|sent| {
                        super::repair::repaired_value(schema, defs, sent)
                            .or_else(|| text_within_pattern(schema, defs, sent))
                            .or_else(|| nearest_member(schema, defs, sent))
                    })
                    .or_else(|| sample(schema, defs, name))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let forms = samples.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let mut candidates: Vec<Value> = Vec::new();
    for form in 0..forms {
        let mut value = own.clone();
        for (fault, samples) in faults.iter().zip(&samples) {
            let sample = samples.get(form).or(samples.first());
            if !show_fault(&mut value, &fault[1..], sample) {
                break;
            }
        }
        for fault in faults {
            if schemas_of_own(schema, defs, fault, own).is_empty() {
                fill_emptied_parent(schema, defs, &mut value, fault);
            }
        }
        if !candidates.contains(&value) {
            candidates.push(value);
        }
    }
    candidates
}

/// The member of the enum `schema` declares that `sent` is nearest to by
/// edit distance, in any case (the first of equals): the valid value a
/// refused text most likely meant, so a field at fault keeps a valid value
/// in the example rather than none (`merged` for `open`/`closed` shows
/// `closed`). `None` for a schema without text members or a value that is
/// not text.
fn nearest_member(schema: &Value, defs: &Value, sent: &Value) -> Option<Value> {
    let sent = sent.as_str()?.to_lowercase();
    resolve_ref(schema, defs)["enum"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .min_by_key(|member| super::levenshtein(&member.to_lowercase(), &sent))
        .map(|member| Value::String(member.to_owned()))
}

/// The text `sent` reduced to the characters its schema's `pattern`
/// accepts one by one (`.js` → `js` for an extension that may hold no `.`),
/// when that text satisfies the pattern whole and differs from `sent`:
/// the fix the constraint itself states, from the caller's own value.
fn text_within_pattern(schema: &Value, defs: &Value, sent: &Value) -> Option<Value> {
    let shape = resolve_ref(schema, defs);
    let pattern = shape["pattern"].as_str()?;
    let sent = sent.as_str()?;
    let kept = sent
        .chars()
        .filter(|character| super::schema::pattern_matches(pattern, &character.to_string()))
        .collect::<String>();
    (kept != sent && !kept.is_empty() && super::schema::pattern_matches(pattern, &kept))
        .then_some(Value::String(kept))
}

/// `own` with each fault that is a text outside an enum naming a boolean
/// sibling of its object (`state: "merged"` beside `isMerged`) replaced by
/// that sibling set true and the refused key removed; the faults left. The
/// sibling is the declared boolean property whose name, after `is`, starts
/// the text or is started by it, in any case or separator style.
fn settle_named_siblings<'a>(
    schema: &Value,
    defs: &Value,
    own: &Value,
    faults: &[&'a [String]],
) -> (Value, Vec<&'a [String]>) {
    let fold = |text: &str| {
        text.chars()
            .filter(|character| character.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let mut settled = own.clone();
    let mut left = Vec::new();
    for fault in faults {
        let named = (|| {
            let (leaf, parent) = fault.split_last()?;
            if fault.len() < 2 {
                return None;
            }
            let sent = fold(
                own.pointer(&crate::json_pointer::of(&fault[1..]))?
                    .as_str()?,
            );
            let outside_enum = schemas_of_own(schema, defs, fault, own)
                .into_iter()
                .any(|shape| resolve_ref(shape, defs)["enum"].is_array());
            if sent.len() < 3 || !outside_enum {
                return None;
            }
            schemas_of_own(schema, defs, parent, own)
                .into_iter()
                .find_map(|holder| {
                    let holder = resolve_ref(holder, defs);
                    property_order(holder, defs).into_iter().find(|name| {
                        let folded = fold(name);
                        let Some(rest) = folded.strip_prefix("is") else {
                            return false;
                        };
                        name != leaf
                            && (rest.starts_with(&sent) || sent.starts_with(rest))
                            && child_schemas(holder, defs, name)
                                .into_iter()
                                .any(|child| resolve_ref(child, defs)["type"] == "boolean")
                    })
                })
                .map(str::to_owned)
        })();
        let holder = named.as_ref().and_then(|_| {
            settled.pointer_mut(&crate::json_pointer::of(&fault[1..fault.len() - 1]))
        });
        match (named, holder.and_then(Value::as_object_mut)) {
            (Some(sibling), Some(holder)) => {
                holder.shift_remove(fault[fault.len() - 1].as_str());
                holder.insert(sibling, Value::Bool(true));
            }
            _ => left.push(*fault),
        }
    }
    (settled, left)
}

/// A key the schema does not declare, removed from an object that then
/// holds nothing, leaves the object a sample of its schema (a time range
/// shows its first bound), so the field stays in the example.
fn fill_emptied_parent(schema: &Value, defs: &Value, value: &mut Value, fault: &[String]) {
    let Some((_, parent)) = fault.split_last() else {
        return;
    };
    let path = crate::json_pointer::of(&parent[1.min(parent.len())..]);
    let Some(held) = value.pointer_mut(&path) else {
        return;
    };
    if parent.is_empty() || !held.as_object().is_some_and(serde_json::Map::is_empty) {
        return;
    }
    let name = parent.last().map_or("", String::as_str);
    if let Some(filled) = schemas_at(schema, defs, parent)
        .into_iter()
        .find_map(|shape| sample(shape, defs, name))
    {
        *held = filled;
    }
}

/// Sets `sample` at `path` under `value`, or removes what is there when
/// there is no sample; `false` when the path leads nowhere a fault can be
/// shown (the fault stays).
fn show_fault(value: &mut Value, path: &[String], sample: Option<&Value>) -> bool {
    let Some((leaf, parents)) = path.split_last() else {
        return sample.is_some_and(|sample| {
            *value = sample.clone();
            true
        });
    };
    let mut at = &mut *value;
    for segment in parents {
        at = match at {
            Value::Object(fields) => match fields.get_mut(segment) {
                Some(next) => next,
                None => return false,
            },
            Value::Array(items) => match segment
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get_mut(index))
            {
                Some(next) => next,
                None => return false,
            },
            _ => return false,
        };
    }
    match (at, sample) {
        (Value::Object(fields), Some(sample)) => {
            fields.insert(leaf.clone(), sample.clone());
            true
        }
        (Value::Object(fields), None) => {
            fields.shift_remove(leaf);
            true
        }
        (Value::Array(items), Some(sample)) => leaf
            .parse::<usize>()
            .ok()
            .and_then(|index| items.get_mut(index))
            .map(|item| *item = sample.clone())
            .is_some(),
        _ => false,
    }
}
