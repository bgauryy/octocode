use super::{Edit, Preference, Repair};
use serde_json::Value;

pub(super) fn apply(input: &Value, edits: &[Edit], preference: Preference) -> Option<Repair> {
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
            // Rows that set the field to different values have no one
            // envelope value.
            Edit::Lift(path) => {
                let field = path.last()?;
                let value = pointer(&repaired, path)?.clone();
                let envelope = repaired.as_object_mut()?;
                if envelope.get(field).is_some_and(|set| *set != value) {
                    return None;
                }
                envelope.insert(field.clone(), value);
                removals.push(path.clone());
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

pub(super) fn pointer<'a>(input: &'a Value, path: &[String]) -> Option<&'a Value> {
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
