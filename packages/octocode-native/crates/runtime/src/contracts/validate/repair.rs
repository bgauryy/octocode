//! The corrected call of a rejected input: the one `repair` hint of a
//! validation error. It is derived from the issues alone, so no tool or field
//! name appears here. Exact fixes keep what the caller asked:
//!
//! - an issue whose schema carries its fix (`setFields`: row fields to set
//!   or remove) gets exactly that: a joined `owner/repo` pair split, a
//!   repository web URL's parts, a `.git` suffix dropped (see
//!   `super::repository`), the omitted selector of the one form a field
//!   belongs to (`nameRegex` → `operation:"files"`);
//! - an array value the schema's enum does not list is dropped (the field
//!   goes with its last value);
//! - a scalar the enum does not list becomes the one listed value it means
//!   (`super::enum_value`: case, separators, an on/off word, an
//!   abbreviation, a near spelling; `entryType:"file"` → `"f"`,
//!   `regex:true` → `"rust"`);
//! - an unknown field set to `true` whose name one row field's enum lists
//!   becomes that value (`merged:true` → `state:"merged"`);
//! - an unknown field named for a kind of value one row field takes
//!   (`x-valueKinds`: `branch:"main"` → `ref:"main"`) becomes that field with
//!   its value, unless the row already sets it;
//! - a number outside its bounds is clamped to the nearest bound, except a
//!   size or count of zero or less (a field whose minimum is positive). When
//!   the field's default is that minimum it is set to it (exact); else it is
//!   dropped, so the default applies (lossy, named). A field the row requires
//!   names what to read, so a value below its minimum gets no lead: any
//!   bound would be a guess, and the message names the allowed range;
//! - a typed value sent as JSON text becomes that value, after validation,
//!   and a whole number or boolean sent to a text field its text (`3` →
//!   `"3"`);
//! - a literal string becomes a single-item array only when that array
//!   satisfies the field schema;
//! - a line-range item spelled another way becomes its `start-end` span
//!   (`"3:6"`, `[3,6]` → `"3-6"`);
//! - a list sent to a row field that takes one value, every item of which
//!   that field accepts, becomes one row per item (the row's other fields
//!   copied into each);
//! - a field that applies only with a sibling the row lacks is removed (it
//!   had no effect);
//! - a `start…`/`end…` pair of line numbers becomes the one span of the
//!   row's span field (`startLine:5, endLine:9` → `ranges:["5-9"]`);
//! - an envelope field the rows declare moves into every row that lacks it;
//! - a bare array of rows becomes the `{"queries":[...]}` envelope;
//! - a row that is itself an envelope (`{"queries":[...], ...}`) is replaced
//!   by its rows: its envelope fields (`responseLength`, …) move beside
//!   `queries` and its other fields into each of its rows.
//!
//! The issues those edits reveal (the lifted rows are validated for the
//! first time) are repaired in a further pass, so one lead fixes all. An
//! issue at a path another edit of the same pass sets (a missing `repo`
//! the `owner:"a/b"` split fills) needs no fix of its own.
//!
//! Dropping any other unknown field, or a field of another form when the
//! row's own selector picks its form, loses a constraint the caller set, so
//! that repair is lossy: it names the dropped fields and the lead must say
//! so. A fix that reads a value approximately (a URL's ref, which may hold
//! `/`) names those fields too. An unknown field whose name is a near miss
//! of a known one is never dropped (the caller may have meant that one).
//! Any other issue (a missing field, an unlisted scalar no rule maps) needs
//! a value only the caller knows, so the call has no lead and the issue's
//! message names the fix. The repaired input must also pass the caller's
//! admission, so a lead never names a call that is rejected again.

use serde_json::{Value, json};

use super::ValidationIssue;

/// One edit at a path (object keys and array indices).
enum Edit {
    Set(Vec<String>, Value),
    Remove(Vec<String>),
    /// A constraint the caller set, removed: the repair is lossy.
    Drop(Vec<String>),
    /// An envelope field moved into every row that lacks it.
    IntoRows(String),
    /// A bare array of rows wrapped as the `queries` envelope.
    Wrap,
    /// Row `index` replaced by its own rows; its `envelope` fields move
    /// beside `queries`, its other fields into each lifted row.
    Flatten {
        index: usize,
        envelope: Vec<String>,
    },
    /// Row `index` sent a list to `field`, which takes one value: one row
    /// per item, each with the row's other fields.
    Split {
        index: usize,
        field: String,
    },
    /// A field removed and the field that says it set beside it.
    Respell {
        remove: Vec<String>,
        set: Vec<String>,
        value: Value,
    },
    /// Fields of the object at `parent` set together (`None`: removed);
    /// `approximate` names those whose value is a best reading.
    Fields {
        parent: Vec<String>,
        fields: Vec<(String, Option<Value>)>,
        approximate: Vec<String>,
    },
}

impl Edit {
    /// The paths this edit gives a value (or removes as part of a fix).
    fn sets(&self) -> Vec<Vec<String>> {
        let child = |parent: &[String], field: &str| {
            let mut path = parent.to_vec();
            path.push(field.to_owned());
            path
        };
        match self {
            Edit::Set(path, _) => vec![path.clone()],
            Edit::Respell { set, .. } => vec![set.clone()],
            Edit::Fields { parent, fields, .. } => fields
                .iter()
                .map(|(field, _)| child(parent, field))
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Further passes for issues a pass's edits reveal.
const PASSES: usize = 3;

/// A repaired input, the caller's fields it dropped, and the fields it set
/// to a best reading (both empty: exact).
#[derive(Debug, PartialEq)]
pub(crate) struct Repair {
    pub input: Value,
    pub dropped: Vec<String>,
    pub(crate) approximate: Vec<String>,
}

/// `input` with every issue fixed, or `None` when any issue has no fix or
/// `check` refuses the result with no issue left to fix. `check` returns
/// the issues of a candidate (empty: refused for a reason no edit fixes);
/// the issues a pass reveals are fixed by the next. A relocation the rows
/// do not accept falls back to dropping the field (lossy, named at the
/// caller's envelope path).
pub(crate) fn repair_input(
    input: &Value,
    issues: &[ValidationIssue],
    check: impl Fn(&Value) -> Result<(), Vec<ValidationIssue>>,
) -> Option<Repair> {
    repair_pass(input, issues, &check, PASSES)
}

fn repair_pass(
    input: &Value,
    issues: &[ValidationIssue],
    check: &dyn Fn(&Value) -> Result<(), Vec<ValidationIssue>>,
    passes: usize,
) -> Option<Repair> {
    if issues.is_empty() || passes == 0 {
        return None;
    }
    let fixes = issues
        .iter()
        .map(|issue| fix(input, issue))
        .collect::<Vec<_>>();
    // An issue with no fix of its own is settled when another edit of this
    // pass sets its path (the missing `repo` an `owner:"a/b"` split sets).
    let set = fixes
        .iter()
        .flatten()
        .flat_map(Edit::sets)
        .collect::<Vec<_>>();
    let edits = issues
        .iter()
        .zip(fixes)
        .map(|(issue, fix)| match fix {
            Some(edit) => Some(Some(edit)),
            None => set.contains(&issue.path).then_some(None),
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let relocated: Vec<&str> = edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::IntoRows(field) => Some(field.as_str()),
            _ => None,
        })
        .collect();
    let attempt = |preference: Preference| {
        let repair = apply(input, &edits, preference)?;
        match check(&repair.input) {
            Ok(()) => Some(repair),
            Err(revealed) => {
                let mut rest = repair_pass(&repair.input, &revealed, check, passes - 1)?;
                for (named, earlier) in [
                    (&mut rest.dropped, repair.dropped),
                    (&mut rest.approximate, repair.approximate),
                ] {
                    named.extend(earlier);
                    named.sort();
                    named.dedup();
                }
                Some(rest)
            }
        }
    };
    // A relocated field a later pass drops from the rows was not accepted
    // there: drop it where the caller set it instead, so the lead names it.
    let rejected_relocation = |repair: &Repair| {
        repair.dropped.iter().any(|path| {
            let path: Vec<&str> = path.split('.').collect();
            matches!(path.as_slice(), ["queries", _, field] if relocated.contains(field))
        })
    };
    // The preferred form first, then the fallback of the edits that have one.
    let mut refused = None;
    for relocate in if relocated.is_empty() {
        vec![true]
    } else {
        vec![true, false]
    } {
        let Some(repair) = attempt(Preference { relocate }) else {
            continue;
        };
        if relocate && rejected_relocation(&repair) {
            refused.get_or_insert(repair);
            continue;
        }
        return Some(repair);
    }
    refused
}

/// Which form of the edits that have a fallback a pass applies: preferred,
/// an envelope field relocates into the rows (else it is dropped).
#[derive(Clone, Copy)]
struct Preference {
    relocate: bool,
}

fn apply(input: &Value, edits: &[Edit], preference: Preference) -> Option<Repair> {
    let mut repaired = input.clone();
    let mut removals = Vec::new();
    let mut dropped = Vec::new();
    let mut approximate = Vec::new();
    // Shape edits stand alone: they change the row indices other edits name,
    // so a pass with one has only shape edits (later rows first).
    if edits.iter().any(|edit| matches!(edit, Edit::Wrap)) {
        repaired = serde_json::json!({ "queries": repaired });
    }
    let mut flattens = edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::Flatten { index, envelope } => Some((*index, envelope)),
            _ => None,
        })
        .collect::<Vec<_>>();
    flattens.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
    for (index, envelope) in flattens {
        flatten(&mut repaired, index, envelope)?;
    }
    for edit in edits {
        match edit {
            Edit::Set(path, value) => *pointer_mut(&mut repaired, path)? = value.clone(),
            Edit::Remove(path) => removals.push(path.clone()),
            Edit::Drop(path) => {
                dropped.push(path.join("."));
                removals.push(path.clone());
            }
            Edit::IntoRows(field) if preference.relocate => {
                let value = repaired.get(field)?.clone();
                for row in repaired.get_mut("queries")?.as_array_mut()? {
                    let row = row.as_object_mut()?;
                    if !row.contains_key(field) {
                        row.insert(field.clone(), value.clone());
                    }
                }
                removals.push(vec![field.clone()]);
            }
            Edit::IntoRows(field) => {
                dropped.push(field.clone());
                removals.push(vec![field.clone()]);
            }
            Edit::Respell { remove, set, value } => {
                let (field, parent) = set.split_last()?;
                pointer_mut(&mut repaired, parent)?
                    .as_object_mut()?
                    .insert(field.clone(), value.clone());
                removals.push(remove.clone());
            }
            Edit::Fields {
                parent,
                fields,
                approximate: guessed,
            } => {
                let object = pointer_mut(&mut repaired, parent)?.as_object_mut()?;
                for (field, value) in fields {
                    match value {
                        Some(value) => {
                            object.insert(field.clone(), value.clone());
                        }
                        None => {
                            let mut path = parent.clone();
                            path.push(field.clone());
                            removals.push(path);
                        }
                    }
                }
                approximate.extend(guessed.iter().map(|field| {
                    let mut path = parent.clone();
                    path.push(field.clone());
                    path.join(".")
                }));
            }
            Edit::Wrap | Edit::Flatten { .. } | Edit::Split { .. } => {}
        }
    }
    remove_all(&mut repaired, removals)?;
    // Splits change row indices, so they apply after every edit that names
    // one, later rows first; two lists in one row have no single reading.
    let mut splits = edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::Split { index, field } => Some((*index, field)),
            _ => None,
        })
        .collect::<Vec<_>>();
    splits.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
    if splits.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return None;
    }
    for (index, field) in splits {
        split(&mut repaired, index, field)?;
    }
    for named in [&mut dropped, &mut approximate] {
        named.sort();
        named.dedup();
    }
    Some(Repair {
        input: repaired,
        dropped,
        approximate,
    })
}

/// Replaces row `index` of `input.queries` with its own rows: a field of the
/// row that `envelope` names moves beside `queries` (unless the caller set
/// it there), any other field into each lifted row that lacks it.
fn flatten(input: &mut Value, index: usize, envelope: &[String]) -> Option<()> {
    let rows = input.get_mut("queries")?.as_array_mut()?;
    let mut row = match rows.get(index)? {
        Value::Object(row) => row.clone(),
        Value::Array(inner) => {
            let mut row = serde_json::Map::new();
            row.insert("queries".into(), Value::Array(inner.clone()));
            row
        }
        _ => return None,
    };
    let Value::Array(mut inner) = row.shift_remove("queries")? else {
        return None;
    };
    let mut lifted_envelope = Vec::new();
    for (field, value) in row {
        if envelope.contains(&field) {
            lifted_envelope.push((field, value));
            continue;
        }
        for inner_row in &mut inner {
            let inner_row = inner_row.as_object_mut()?;
            if !inner_row.contains_key(&field) {
                inner_row.insert(field.clone(), value.clone());
            }
        }
    }
    rows.splice(index..=index, inner);
    let object = input.as_object_mut()?;
    for (field, value) in lifted_envelope {
        object.entry(field).or_insert(value);
    }
    Some(())
}

/// Replaces row `index` of `input.queries` with one row per item of its
/// list-valued `field`, each keeping the row's other fields.
fn split(input: &mut Value, index: usize, field: &str) -> Option<()> {
    let rows = input.get_mut("queries")?.as_array_mut()?;
    let row = rows.get(index)?.as_object()?;
    let items = row.get(field)?.as_array()?;
    let lifted = items
        .iter()
        .map(|item| {
            let mut row = row.clone();
            row.insert(field.to_owned(), item.clone());
            Value::Object(row)
        })
        .collect::<Vec<_>>();
    rows.splice(index..=index, lifted);
    Some(())
}

/// A list sent to a row field (`queries.N.field`) whose schema takes one
/// value, when the field accepts every item: one row per item.
fn split_list(issue: &ValidationIssue) -> Option<Edit> {
    let [queries, index, field] = issue.path.as_slice() else {
        return None;
    };
    (queries == "queries").then_some(())?;
    let items = issue.received.as_ref()?.as_array()?;
    let schema = issue.schema.as_ref()?;
    let accepted = !items.is_empty()
        && items.iter().all(|item| {
            super::validate_input_schema(schema, schema, &mut item.clone(), &mut Vec::new()).is_ok()
        });
    accepted.then_some(())?;
    Some(Edit::Split {
        index: index.parse().ok()?,
        field: field.clone(),
    })
}

/// The edit that clears `issue`, when there is one.
fn fix(input: &Value, issue: &ValidationIssue) -> Option<Edit> {
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
    // Validation read the fix from the row itself (`super::repository`, an
    // omitted selector): set exactly those fields.
    if let Some(fields) = super::repository::set_fields(issue) {
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
    if let Some(edit) = line_range(issue) {
        return Some(edit);
    }
    match issue.rule_id.as_str() {
        super::super::ENVELOPE_ARRAY => input.is_array().then_some(Edit::Wrap),
        super::super::ENVELOPE_NESTED => {
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
            // A field another form of the row declares had no effect here.
            // When the row's own selector picks its form, removing it drops
            // what the caller set (lossy, named); else (a sibling only the
            // caller can fill is missing) removing it is exact.
            if super::sibling_requirement(issue).is_some() {
                let conflict = issue.schema.as_ref().is_some_and(|schema| {
                    schema.get("siblingConflict") == Some(&Value::Bool(true))
                });
                return Some(if conflict {
                    Edit::Drop(path)
                } else {
                    Edit::Remove(path)
                });
            }
            if super::suggest_field(field, &known).is_some() {
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
            if pointer(input, parent)?.is_array() {
                return Some(Edit::Remove(path));
            }
            // The one listed value the scalar means; with none (or several)
            // only the caller knows, and dropping it would widen the call.
            let value = super::enum_value::enum_value(allowed, issue.received.as_ref()?, field)?;
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
        "schema.type" if issue.received.as_ref().is_some_and(Value::is_array) => split_list(issue),
        // A whole number or a boolean for a text field: its text is exact
        // (a fraction is not: `1.10` already arrived as `1.1`).
        "schema.type" if issue.schema.as_ref()?.get("type")? == "string" => {
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
                        Err(_) if kind == "array" && !super::shape::looks_like_json_array(text) => {
                            let schema = issue.schema.as_ref()?;
                            let mut wrapped = Value::Array(vec![issue.received.as_ref()?.clone()]);
                            super::validate_input_schema(
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

/// A `start…`/`end…` pair of line numbers (`startLine`/`endLine`,
/// `lineStart`/`lineEnd`) the row's span field (`ranges`) spells exactly:
/// the start becomes that field's one span, the end goes. `None` when the
/// pair is incomplete or not an ascending pair of positive integers, or the
/// row already sets the span field.
fn span_respelling(input: &Value, issue: &ValidationIssue) -> Option<Edit> {
    let span = issue.schema.as_ref()?.get("spanField")?.as_str()?;
    let (field, parent) = issue.path.split_last()?;
    let row = pointer(input, parent)?.as_object()?;
    if row.contains_key(span) {
        return None;
    }
    let side = |name: &str| {
        let lower = name.to_ascii_lowercase();
        [("start", true), ("end", false)]
            .into_iter()
            .find_map(|(word, start)| {
                let stem = lower
                    .strip_prefix(word)
                    .or_else(|| lower.strip_suffix(word))?;
                Some((start, stem.to_owned()))
            })
    };
    let (is_start, stem) = side(field)?;
    let (_, other_value) = row.iter().find(|(name, _)| {
        name.as_str() != field.as_str()
            && side(name).is_some_and(|(start, other)| start != is_start && other == stem)
    })?;
    let line = |value: &Value| value.as_u64().filter(|line| *line > 0);
    let (start, end) = if is_start {
        (line(&row[field.as_str()])?, line(other_value)?)
    } else {
        (line(other_value)?, line(&row[field.as_str()])?)
    };
    if start > end {
        return None;
    }
    Some(if is_start {
        let mut set = parent.to_vec();
        set.push(span.to_owned());
        Edit::Respell {
            remove: issue.path.clone(),
            set,
            value: serde_json::json!([format!("{start}-{end}")]),
        }
    } else {
        Edit::Remove(issue.path.clone())
    })
}

/// An unknown field set to `true` whose name the enum of one row field lists
/// (`nameFields`): that field set to the name (`merged:true` →
/// `state:"merged"`). `None` when the row already sets that field.
fn name_respelling(input: &Value, issue: &ValidationIssue) -> Option<Edit> {
    let schema = issue.schema.as_ref()?;
    let [target] = schema.get("nameFields")?.as_array()?.as_slice() else {
        return None;
    };
    let (field, parent) = issue.path.split_last()?;
    let row = pointer(input, parent)?.as_object()?;
    let target = target.as_str()?;
    if row.get(field) != Some(&Value::Bool(true)) || row.contains_key(target) {
        return None;
    }
    let mut set = parent.to_vec();
    set.push(target.to_owned());
    Some(Edit::Respell {
        remove: issue.path.clone(),
        set,
        value: Value::from(field.as_str()),
    })
}

/// An unknown field named for a kind of value one row field takes
/// (`kindFields`, from the field's `x-valueKinds`): that field set to the
/// value (`branch:"main"` → `ref:"main"`). `None` when the row already sets
/// that field, so a conflicting value is never overwritten.
fn kind_respelling(input: &Value, issue: &ValidationIssue) -> Option<Edit> {
    let schema = issue.schema.as_ref()?;
    let [target] = schema.get("kindFields")?.as_array()?.as_slice() else {
        return None;
    };
    let (field, parent) = issue.path.split_last()?;
    let row = pointer(input, parent)?.as_object()?;
    let target = target.as_str()?;
    if row.contains_key(target) {
        return None;
    }
    let mut set = parent.to_vec();
    set.push(target.to_owned());
    Some(Edit::Respell {
        remove: issue.path.clone(),
        set,
        value: row.get(field)?.clone(),
    })
}

/// A line-range item (`ranges.N`, whose schema is the line-range pattern)
/// spelled another way, or one such scalar sent for the whole list: its
/// `start-end` span (`"3:6"` → `"3-6"`, `ranges:"3:6"` → `["3-6"]`).
fn line_range(issue: &ValidationIssue) -> Option<Edit> {
    if !matches!(issue.rule_id.as_str(), "schema.pattern" | "schema.type") {
        return None;
    }
    let schema = issue.schema.as_ref()?;
    let received = issue.received.as_ref()?;
    let is_range =
        |schema: &Value| schema.get("pattern") == Some(&json!(super::shape::LINE_RANGE_PATTERN));
    let span = super::shape::line_range_reading(received)?;
    if is_range(schema) {
        return Some(Edit::Set(issue.path.clone(), json!(span)));
    }
    (is_range(schema.get("items")?) && !received.is_array())
        .then(|| Edit::Set(issue.path.clone(), json!([span])))
}

/// A bound as the number the schema wrote: integral bounds stay integers.
fn bound_value(bound: f64) -> Value {
    if bound.fract() == 0.0 && bound.abs() < 9.0e15 {
        Value::from(bound as i64)
    } else {
        Value::from(bound)
    }
}

/// Remove every path, later array elements first so earlier indices stay
/// valid, then every array the removals emptied, which leaves its field.
fn remove_all(input: &mut Value, mut paths: Vec<Vec<String>>) -> Option<()> {
    paths.sort_by(|a, b| compare(b, a));
    paths.dedup();
    let mut emptied = Vec::new();
    for path in &paths {
        let (last, parent_path) = path.split_last()?;
        let parent = pointer_mut(input, parent_path)?;
        match parent {
            Value::Object(object) => {
                object.shift_remove(last)?;
            }
            Value::Array(items) => {
                let index = last.parse::<usize>().ok().filter(|i| *i < items.len())?;
                items.remove(index);
                if items.is_empty() {
                    emptied.push(parent_path.to_vec());
                }
            }
            _ => return None,
        }
    }
    emptied.sort_by(|a, b| compare(b, a));
    emptied.dedup();
    for path in emptied {
        let (field, parent_path) = path.split_last()?;
        if let Value::Object(object) = pointer_mut(input, parent_path)? {
            object.shift_remove(field);
        }
    }
    Some(())
}

/// Paths in order, indices by number: `queries.10` follows `queries.2`.
fn compare(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b) {
        let order = match (x.parse::<usize>(), y.parse::<usize>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => x.cmp(y),
        };
        if order.is_ne() {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

fn pointer<'a>(input: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter().try_fold(input, |value, segment| match value {
        Value::Object(object) => object.get(segment),
        Value::Array(items) => items.get(segment.parse::<usize>().ok()?),
        _ => None,
    })
}

fn pointer_mut<'a>(input: &'a mut Value, path: &[String]) -> Option<&'a mut Value> {
    path.iter().try_fold(input, |value, segment| match value {
        Value::Object(object) => object.get_mut(segment),
        Value::Array(items) => items.get_mut(segment.parse::<usize>().ok()?),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn issue(
        rule: &str,
        path: &[&str],
        schema: Option<Value>,
        received: Option<Value>,
    ) -> ValidationIssue {
        ValidationIssue {
            rule_id: rule.into(),
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            message: String::new(),
            schema,
            received,
        }
    }

    fn always(_: &Value) -> Result<(), Vec<ValidationIssue>> {
        Ok(())
    }

    /// `accepts` as a check that refuses with nothing left to fix.
    fn only(
        accepts: impl Fn(&Value) -> bool,
    ) -> impl Fn(&Value) -> Result<(), Vec<ValidationIssue>> {
        move |candidate| accepts(candidate).then_some(()).ok_or_else(Vec::new)
    }

    #[test]
    fn unlisted_array_values_drop_and_the_emptied_field_goes() {
        let input = json!({"queries":[{"sections":["files","x","patches","y"],"only":["z"]}]});
        let allowed = json!({"enum":["files","patches"]});
        let issues = [
            issue(
                "schema.enum",
                &["queries", "0", "sections", "1"],
                Some(allowed.clone()),
                Some(json!("x")),
            ),
            issue(
                "schema.enum",
                &["queries", "0", "sections", "3"],
                Some(allowed.clone()),
                Some(json!("y")),
            ),
            issue(
                "schema.enum",
                &["queries", "0", "only", "0"],
                Some(allowed),
                Some(json!("z")),
            ),
        ];
        let repaired = repair_input(&input, &issues, always).expect("exact fix");
        assert_eq!(
            repaired.input,
            json!({"queries":[{"sections":["files","patches"]}]})
        );
        assert!(repaired.dropped.is_empty());
    }

    #[test]
    fn row_indices_past_nine_remove_in_numeric_order() {
        let rows = (0..12).map(|i| json!({"extra": i})).collect::<Vec<_>>();
        let input = json!({"queries": rows});
        let issues = [2, 10, 11].map(|i| {
            let index = i.to_string();
            issue(
                "schema.unknown-field",
                &["queries", &index, "extra"],
                None,
                None,
            )
        });
        let repaired = repair_input(&input, &issues, always).expect("lossy fix");
        assert_eq!(
            repaired.dropped,
            ["queries.10.extra", "queries.11.extra", "queries.2.extra"]
        );
        let repaired = repaired.input;
        assert!(
            repaired["queries"]
                .as_array()
                .expect("rows")
                .iter()
                .enumerate()
                .all(|(i, row)| { matches!(i, 2 | 10 | 11) == row.get("extra").is_none() })
        );
    }

    #[test]
    fn scalar_enum_case_and_number_bounds_have_one_exact_fix() {
        let input = json!({"queries":[{"view":" Files ","n":500,"m":0.5}]});
        let allowed = json!({"enum":["files","paths"]});
        let issues = [
            issue(
                "schema.enum",
                &["queries", "0", "view"],
                Some(allowed),
                Some(json!(" Files ")),
            ),
            issue(
                "schema.range",
                &["queries", "0", "n"],
                Some(json!({"minimum":1,"maximum":100})),
                Some(json!(500)),
            ),
            issue(
                "schema.range",
                &["queries", "0", "m"],
                Some(json!({"minimum":1})),
                Some(json!(0.5)),
            ),
        ];
        let repaired = repair_input(&input, &issues, always).expect("exact fix");
        assert_eq!(
            repaired.input,
            json!({"queries":[{"view":"files","n":100,"m":1}]})
        );
        assert!(repaired.dropped.is_empty());
    }

    #[test]
    fn a_fix_that_needs_the_caller_has_no_lead() {
        // An unlisted value no rule maps to one listed value.
        let input = json!({"queries":[{"view":"none"}]});
        let unlisted = issue(
            "schema.enum",
            &["queries", "0", "view"],
            Some(json!({"enum":["files","paths"]})),
            Some(json!("none")),
        );
        assert!(repair_input(&input, &[unlisted], always).is_none());
        let missing = issue("schema.required", &["queries", "0", "path"], None, None);
        let unknown = issue(
            "schema.unknown-field",
            &["queries", "0", "view"],
            None,
            None,
        );
        assert!(repair_input(&input, &[unknown, missing], always).is_none());
        assert!(repair_input(&input, &[], always).is_none());
    }

    /// An unlisted scalar becomes the one listed value it means; one no rule
    /// maps is never dropped (its default would widen the call): no lead.
    #[test]
    fn an_unlisted_scalar_maps_to_its_one_value_or_has_no_lead() {
        let lead = |allowed: Value, field: &str, received: Value| {
            let input = json!({"queries":[{"path":"a.ts", field: received.clone()}]});
            let unlisted = issue(
                "schema.enum",
                &["queries", "0", field],
                Some(json!({ "enum": allowed })),
                Some(received),
            );
            repair_input(&input, &[unlisted], always).map(|repaired| {
                assert!(repaired.dropped.is_empty(), "{repaired:?}");
                repaired.input["queries"][0][field].clone()
            })
        };
        assert_eq!(
            lead(json!(["f", "d"]), "entryType", json!("file")),
            Some(json!("f"))
        );
        assert_eq!(
            lead(json!(["literal", "rust", "pcre2"]), "regex", json!("regex")),
            Some(json!("rust"))
        );
        assert_eq!(
            lead(
                json!(["pullRequest", "issue", "commit"]),
                "operation",
                json!("pr")
            ),
            Some(json!("pullRequest"))
        );
        assert_eq!(
            lead(json!(["standard", "symbols"]), "minify", json!("none")),
            None
        );
        assert_eq!(
            lead(
                json!(["smart", "sensitive", "insensitive"]),
                "caseMode",
                json!("ignore")
            ),
            None
        );
    }

    /// An unknown field set to `true` whose name one row field lists becomes
    /// that field's value, exactly; `false`, or a row already setting that
    /// field, keeps the unknown field (dropped, named).
    #[test]
    fn a_true_field_named_by_an_enum_value_becomes_that_value() {
        let named = |row: Value| {
            let input = json!({ "queries": [row] });
            let unknown = issue(
                "schema.unknown-field",
                &["queries", "0", "merged"],
                Some(json!({"knownFields":["state","owner"],"nameFields":["state"]})),
                None,
            );
            repair_input(&input, &[unknown], always).expect("a lead")
        };
        let exact = named(json!({"owner":"o","merged":true}));
        assert_eq!(
            exact.input,
            json!({"queries":[{"owner":"o","state":"merged"}]})
        );
        assert!(exact.dropped.is_empty());
        for row in [
            json!({"merged":false}),
            json!({"merged":true,"state":"open"}),
        ] {
            assert_eq!(named(row).dropped, ["queries.0.merged"]);
        }
    }

    /// A whole number or boolean sent to a text field becomes its text; a
    /// fraction (whose text JSON already lost) has no lead.
    #[test]
    fn a_number_or_boolean_for_a_text_field_becomes_its_text() {
        let text = |received: Value| {
            let input = json!({"queries":[{"version": received.clone()}]});
            let typed = issue(
                "schema.type",
                &["queries", "0", "version"],
                Some(json!({"type":"string","minLength":1})),
                Some(received),
            );
            repair_input(&input, &[typed], always).map(|repaired| repaired.input)
        };
        assert_eq!(text(json!(3)), Some(json!({"queries":[{"version":"3"}]})));
        assert_eq!(
            text(json!(true)),
            Some(json!({"queries":[{"version":"true"}]}))
        );
        assert_eq!(text(json!(1.1)), None);
    }

    /// A line-range item spelled another way becomes its span; one sent
    /// for the whole list becomes the one-span list.
    #[test]
    fn a_line_range_typo_becomes_its_span() {
        let item = json!({"type":"string","pattern":crate::contracts::validate::shape::LINE_RANGE_PATTERN});
        let list = json!({"type":"array","items": item.clone()});
        let input = json!({"queries":[{"ranges":["3:6",[8,9],"5:2"]}]});
        let at = |index: &str, received: Value, rule: &str| {
            issue(
                rule,
                &["queries", "0", "ranges", index],
                Some(item.clone()),
                Some(received),
            )
        };
        let fixed = repair_input(
            &input,
            &[
                at("0", json!("3:6"), "schema.pattern"),
                at("1", json!([8, 9]), "schema.type"),
            ],
            always,
        )
        .expect("exact");
        assert_eq!(
            fixed.input["queries"][0]["ranges"],
            json!(["3-6", "8-9", "5:2"])
        );
        assert!(repair_input(&input, &[at("2", json!("5:2"), "schema.pattern")], always).is_none());
        let scalar = json!({"queries":[{"ranges":"3..6"}]});
        let whole = issue(
            "schema.type",
            &["queries", "0", "ranges"],
            Some(list),
            Some(json!("3..6")),
        );
        let fixed = repair_input(&scalar, &[whole], always).expect("exact");
        assert_eq!(fixed.input, json!({"queries":[{"ranges":["3-6"]}]}));
    }

    /// An issue with no fix of its own is settled by another edit of the
    /// same pass that sets its path; a fix's approximate fields are named.
    #[test]
    fn an_issue_another_edit_sets_needs_no_fix_and_approximations_are_named() {
        let input = json!({"queries":[{"path":"https://github.com/o/r/blob/main/a.rs"}]});
        let url = issue(
            "schema.repository-url",
            &["queries", "0", "path"],
            Some(
                json!({"setFields":{"owner":"o","repo":"r","ref":"main","path":"a.rs"},
                "approximateFields":["ref","path"]}),
            ),
            None,
        );
        let missing = issue("schema.required", &["queries", "0", "repo"], None, None);
        let unrelated = issue("schema.required", &["queries", "0", "token"], None, None);
        let repaired =
            repair_input(&input, &[url.clone(), missing.clone()], always).expect("settled");
        assert_eq!(
            repaired.input,
            json!({"queries":[{"path":"a.rs","owner":"o","repo":"r","ref":"main"}]})
        );
        assert!(repaired.dropped.is_empty());
        assert_eq!(repaired.approximate, ["queries.0.path", "queries.0.ref"]);
        assert!(repair_input(&input, &[url, missing, unrelated], always).is_none());
    }

    #[test]
    fn dropping_an_unmapped_caller_field_is_named_as_lossy() {
        let input = json!({"queries":[{"matchString":"x","glob":"*.ts"}]});
        let unknown = issue(
            "schema.unknown-field",
            &["queries", "0", "glob"],
            Some(json!({"knownFields":["matchString","include","path"]})),
            None,
        );
        let repaired = repair_input(&input, &[unknown], always).expect("lossy repair");
        assert_eq!(repaired.input, json!({"queries":[{"matchString":"x"}]}));
        assert_eq!(repaired.dropped, ["queries.0.glob"]);
    }

    /// A field inert without a sibling only the caller can fill is removed
    /// exactly; one whose form the row's own selector excludes is dropped,
    /// named.
    #[test]
    fn a_field_inert_without_its_sibling_is_removed_or_dropped() {
        let input = json!({"queries":[{"path":"a","contextLines":2}]});
        let inert = |extra: Value| {
            let mut schema = json!({"knownFields":["path"],"siblingRequires":["matchString"]});
            schema
                .as_object_mut()
                .expect("object")
                .extend(extra.as_object().cloned().unwrap_or_default());
            issue(
                "schema.unknown-field",
                &["queries", "0", "contextLines"],
                Some(schema),
                None,
            )
        };
        let removed = repair_input(&input, &[inert(json!({}))], always).expect("exact repair");
        assert_eq!(removed.input, json!({"queries":[{"path":"a"}]}));
        assert!(removed.dropped.is_empty());
        let conflict = inert(json!({"siblingConflict": true}));
        let dropped = repair_input(&input, &[conflict], always).expect("lossy repair");
        assert_eq!(dropped.input, json!({"queries":[{"path":"a"}]}));
        assert_eq!(dropped.dropped, ["queries.0.contextLines"]);
    }

    /// A field of a form whose omitted selector is one literal sets it.
    #[test]
    fn a_field_of_one_unselected_branch_sets_its_selector() {
        let input = json!({"queries":[{"path":".","nameRegex":"x"}]});
        let field = issue(
            "schema.unknown-field",
            &["queries", "0", "nameRegex"],
            Some(
                json!({"knownFields":["path"],"siblingRequires":["operation:\"files\""],
                "setFields":{"operation":"files"}}),
            ),
            None,
        );
        let repaired = repair_input(&input, &[field], always).expect("exact");
        assert_eq!(
            repaired.input,
            json!({"queries":[{"path":".","nameRegex":"x","operation":"files"}]})
        );
        assert!(repaired.dropped.is_empty() && repaired.approximate.is_empty());
    }

    #[test]
    fn an_envelope_field_the_rows_accept_moves_into_each_row() {
        let input = json!({"mainGoal":"g","queries":[{"path":"a"},{"path":"b","mainGoal":"own"}]});
        let root = issue("schema.unknown-field", &["mainGoal"], None, None);
        let rows_accept = |candidate: &Value| candidate.get("mainGoal").is_none();
        let repaired = repair_input(&input, std::slice::from_ref(&root), only(rows_accept))
            .expect("relocation");
        assert_eq!(
            repaired.input,
            json!({"queries":[{"path":"a","mainGoal":"g"},{"path":"b","mainGoal":"own"}]})
        );
        assert!(repaired.dropped.is_empty());
        // Rows that refuse the field: the relocation falls back to a drop.
        let rows_refuse = |candidate: &Value| {
            candidate["queries"].as_array().is_some_and(|rows| {
                rows.iter()
                    .all(|row| row.get("mainGoal") != Some(&json!("g")))
            })
        };
        let dropped = repair_input(&input, &[root], only(rows_refuse)).expect("lossy fallback");
        assert_eq!(dropped.dropped, ["mainGoal"]);
    }

    /// Zero of a positive size (`responseLength: 0`) is dropped so the
    /// default applies, named as lossy, instead of becoming one-char pages;
    /// when the field's default is that minimum, setting it is exact. A
    /// field the row requires names what to read, so a value below its
    /// minimum gets no lead.
    #[test]
    fn an_unsized_field_drops_to_its_default_unless_required() {
        let input = json!({"responseLength":0,"queries":[{"path":"a","line":-2}]});
        let unsized_issue = |path: &[&str], schema: Value, received| {
            issue("schema.range", path, Some(schema), Some(json!(received)))
        };
        let length = unsized_issue(&["responseLength"], json!({"minimum":1,"maximum":50000}), 0);
        let repaired = repair_input(&input, std::slice::from_ref(&length), always).expect("drop");
        assert_eq!(repaired.input, json!({"queries":[{"path":"a","line":-2}]}));
        assert_eq!(repaired.dropped, ["responseLength"]);
        let line = unsized_issue(
            &["queries", "0", "line"],
            json!({"minimum":1,"maximum":50000}),
            -2,
        );
        let requires_line = |candidate: &Value| match candidate.pointer("/queries/0/line") {
            Some(_) => Ok(()),
            None => Err(vec![issue(
                "schema.required",
                &["queries", "0", "line"],
                None,
                None,
            )]),
        };
        assert_eq!(repair_input(&input, &[line], requires_line), None);
        // Above the maximum a required field still clamps: that bound is
        // the nearest value the caller can have meant.
        let large = unsized_issue(
            &["queries", "0", "line"],
            json!({"minimum":1,"maximum":50000}),
            90_000,
        );
        let repaired = repair_input(&input, &[large], requires_line).expect("clamp");
        assert_eq!(repaired.input["queries"][0]["line"], 50_000);
    }

    /// A size whose default is its minimum (`historyDepth: 0`, default 1)
    /// is set to it: the same call as dropping it, exact, nothing lost.
    #[test]
    fn an_unsized_field_whose_default_is_its_minimum_is_set_exactly() {
        let input = json!({"queries":[{"historyDepth":0}]});
        let depth = issue(
            "schema.range",
            &["queries", "0", "historyDepth"],
            Some(json!({"minimum":1,"maximum":50,"default":1})),
            Some(json!(0)),
        );
        let repaired = repair_input(&input, &[depth], always).expect("exact");
        assert_eq!(repaired.input, json!({"queries":[{"historyDepth":1}]}));
        assert!(repaired.dropped.is_empty() && repaired.approximate.is_empty());
    }

    /// An unknown field named for a kind of value one row field takes
    /// becomes that field, exactly; a row already setting it keeps today's
    /// handling (the unknown field is dropped, named).
    #[test]
    fn a_field_named_for_a_kind_of_value_becomes_the_field_that_takes_it() {
        let unknown = |path: &[&str], kinds: Value| {
            issue(
                "schema.unknown-field",
                path,
                Some(json!({"knownFields":["ref","path"],"kindFields":kinds})),
                None,
            )
        };
        let input = json!({"queries":[{"branch":"main","path":"a"}]});
        let repaired = repair_input(
            &input,
            &[unknown(&["queries", "0", "branch"], json!(["ref"]))],
            always,
        )
        .expect("exact");
        assert_eq!(
            repaired.input,
            json!({"queries":[{"ref":"main","path":"a"}]})
        );
        assert!(repaired.dropped.is_empty() && repaired.approximate.is_empty());
        let both = json!({"queries":[{"branch":"main","ref":"v1"}]});
        let repaired = repair_input(
            &both,
            &[unknown(&["queries", "0", "branch"], json!(["ref"]))],
            always,
        )
        .expect("lossy");
        assert_eq!(repaired.dropped, ["queries.0.branch"]);
        assert_eq!(repaired.input, json!({"queries":[{"ref":"v1"}]}));
        // Several fields take that kind: only the caller knows which.
        let repaired = repair_input(
            &input,
            &[unknown(&["queries", "0", "branch"], json!(["ref", "path"]))],
            always,
        )
        .expect("lossy");
        assert_eq!(repaired.dropped, ["queries.0.branch"]);
    }

    /// An unknown envelope field the rows refuse is named where the caller
    /// set it, never at the row path the relocation tried.
    #[test]
    fn a_refused_relocation_names_the_envelope_field() {
        let check = |value: &Value| {
            crate::contracts::prepare_many_and_validate("localFetch", value.clone())
                .map(|_| ())
                .map_err(|error| error.issues)
        };
        let input = json!({"queries":[{"path":"a.txt","mainGoal":"g","reasoning":"r"}],
            "bogusTop":1,"responseLength":0});
        let issues = check(&input).expect_err("unknown field and unsized page");
        let repaired = repair_input(&input, &issues, check).expect("lossy repair");
        assert_eq!(repaired.dropped, ["bogusTop", "responseLength"]);
        assert_eq!(
            repaired.input,
            json!({"queries":[{"path":"a.txt","mainGoal":"g","reasoning":"r"}]})
        );
    }

    #[test]
    fn a_number_or_boolean_sent_as_text_becomes_the_value() {
        let input = json!({"queries":[{"n":"3","b":" true ","x":"three"}]});
        let typed = |field: &str, kind: &str, text: &str| {
            issue(
                "schema.type",
                &["queries", "0", field],
                Some(json!({"type":kind})),
                Some(json!(text)),
            )
        };
        let repaired = repair_input(
            &input,
            &[typed("n", "integer", "3"), typed("b", "boolean", " true ")],
            always,
        )
        .expect("exact coercion");
        assert_eq!(
            repaired.input,
            json!({"queries":[{"n":3,"b":true,"x":"three"}]})
        );
        assert!(repair_input(&input, &[typed("x", "integer", "three")], always).is_none());
    }

    #[test]
    fn an_encoded_object_repairs_against_the_actual_tool_schema() {
        let questions = json!({"present":{"type":"noul","instructions":"Is the word present?"}});
        let input = json!({"queries":[{"state":"word","questions":questions.to_string()}]});
        let check = |value: &Value| {
            crate::contracts::prepare_many_and_validate("clasify", value.clone())
                .map(|_| ())
                .map_err(|error| error.issues)
        };
        let issues = check(&input).expect_err("object must not be a string");
        let repaired = repair_input(&input, &issues, check).expect("validated object repair");
        assert_eq!(repaired.input["queries"][0]["questions"], questions);
        assert!(repaired.dropped.is_empty());
    }

    #[test]
    fn singleton_repair_preserves_literals_and_requires_the_whole_array_schema() {
        for (schema, text, expected) in [
            (
                json!({"type":"array","items":{"type":"string"}}),
                " [ab]*.ts ",
                true,
            ),
            (
                json!({"type":"array","items":{"enum":["allowed"]}}),
                "different",
                false,
            ),
            (
                json!({"type":"array","minItems":2,"items":{"type":"string"}}),
                "one",
                false,
            ),
            (
                json!({"type":"array","items":{"type":"number"}}),
                "one",
                false,
            ),
            (
                json!({"type":"array","items":{"type":"string"}}),
                "[\"unfinished",
                false,
            ),
        ] {
            let input = json!({"value":text});
            let problem = issue("schema.type", &["value"], Some(schema), Some(json!(text)));
            let repair = repair_input(&input, &[problem], always);
            assert_eq!(repair.is_some(), expected, "{text}");
            if let Some(repair) = repair {
                assert_eq!(repair.input, json!({"value":[text]}));
                assert!(repair.dropped.is_empty());
            }
        }
    }

    /// A boolean for an on/off enum becomes the value it means.
    #[test]
    fn a_boolean_for_an_on_off_enum_becomes_its_value() {
        for (allowed, received, expected) in [
            (json!(["literal", "rust", "pcre2"]), true, json!("rust")),
            (json!(["literal", "rust", "pcre2"]), false, json!("literal")),
            (json!(["off", "on", "dotall"]), true, json!("on")),
        ] {
            let input = json!({"queries":[{"field": received}]});
            let problem = issue(
                "schema.enum",
                &["queries", "0", "field"],
                Some(json!({"enum": allowed})),
                Some(json!(received)),
            );
            let repaired = repair_input(&input, &[problem], always).expect("exact");
            assert_eq!(repaired.input["queries"][0]["field"], expected);
            assert!(repaired.dropped.is_empty());
        }
    }

    /// A list for a one-value row field becomes one row per item, the row's
    /// other fields copied, later rows keeping their order; a list the field
    /// rejects an item of, an empty list, or two lists in one row has no lead.
    #[test]
    fn a_list_for_a_one_value_field_becomes_one_row_per_item() {
        let string = json!({"type":"string","minLength":1});
        let list = |row: &str, field: &str, received: Value| {
            issue(
                "schema.type",
                &["queries", row, field],
                Some(string.clone()),
                Some(received),
            )
        };
        let input = json!({"queries":[
            {"matchString":["a","b"],"path":"src"},
            {"matchString":"c","path":"lib"}
        ]});
        let repaired = repair_input(
            &input,
            &[list("0", "matchString", json!(["a", "b"]))],
            always,
        )
        .expect("exact");
        assert_eq!(
            repaired.input,
            json!({"queries":[
                {"matchString":"a","path":"src"},
                {"matchString":"b","path":"src"},
                {"matchString":"c","path":"lib"}
            ]})
        );
        assert!(repaired.dropped.is_empty());
        for received in [json!([]), json!(["a", ""]), json!(["a", 1])] {
            let input = json!({"queries":[{"matchString": received.clone()}]});
            let problem = list("0", "matchString", received.clone());
            assert!(
                repair_input(&input, &[problem], always).is_none(),
                "{received}"
            );
        }
        let input = json!({"queries":[{"matchString":["a","b"],"path":["x","y"]}]});
        let both = [
            list("0", "matchString", json!(["a", "b"])),
            list("0", "path", json!(["x", "y"])),
        ];
        assert!(repair_input(&input, &both, always).is_none());
    }

    /// A bare array of rows becomes the envelope, exactly.
    #[test]
    fn a_bare_array_becomes_the_envelope() {
        let input = json!([{"path":"a"},{"path":"b"}]);
        let wrap = issue(crate::contracts::ENVELOPE_ARRAY, &[], None, None);
        let repaired = repair_input(&input, &[wrap], always).expect("exact");
        assert_eq!(
            repaired.input,
            json!({"queries":[{"path":"a"},{"path":"b"}]})
        );
        assert!(repaired.dropped.is_empty());
    }

    /// A nested envelope's rows replace it; its envelope fields move beside
    /// `queries` and its row fields into each lifted row; the issues the
    /// lifted rows then show are repaired in the next pass (an unknown
    /// field is dropped, named).
    #[test]
    fn a_nested_envelope_is_lifted_and_its_fields_kept_or_named() {
        let input = json!({"queries":[
            {"path":"z"},
            {"bogus":1,"responseLength":3000,"extras":["stats"],
                "queries":[{"path":"a"},{"path":"b","extras":["hints"]}]}
        ]});
        let nested = issue(
            crate::contracts::ENVELOPE_NESTED,
            &["queries", "1", "queries"],
            Some(json!({"envelopeFields":["responseLength"]})),
            None,
        );
        let check = |candidate: &Value| -> Result<(), Vec<ValidationIssue>> {
            let rows = candidate["queries"].as_array().cloned().unwrap_or_default();
            let unknown = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.get("bogus").is_some())
                .map(|(i, _)| {
                    let index = i.to_string();
                    issue(
                        "schema.unknown-field",
                        &["queries", &index, "bogus"],
                        None,
                        None,
                    )
                })
                .collect::<Vec<_>>();
            if unknown.is_empty() {
                Ok(())
            } else {
                Err(unknown)
            }
        };
        let repaired = repair_input(&input, &[nested], check).expect("lifted");
        assert_eq!(
            repaired.input,
            json!({"queries":[
                {"path":"z"},
                {"path":"a","extras":["stats"]},
                {"path":"b","extras":["hints"]}
            ],"responseLength":3000})
        );
        assert_eq!(repaired.dropped, ["queries.1.bogus", "queries.2.bogus"]);
    }

    #[test]
    fn a_repair_the_caller_refuses_is_not_offered() {
        let input = json!({"queries":[{"extra":1}]});
        let unknown = issue(
            "schema.unknown-field",
            &["queries", "0", "extra"],
            None,
            None,
        );
        assert!(repair_input(&input, &[unknown], only(|_| false)).is_none());
    }
}
