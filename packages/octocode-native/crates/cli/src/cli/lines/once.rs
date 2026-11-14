//! What text states once instead of on every row it holds for: the
//! directory a row's files share, a row's counters, what a walk skipped
//! beside the content, the fields every row of a batch shares, and the
//! fields every item of a list shares. Each stays a fact of the result, said
//! once; `--json` keeps every row whole.
use super::follow::split_stated;
use super::table::is_word;
use octocode_native::id::ToolId;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// The fields of `data` whose items name the files a row prints.
const ROW_FILES: [&str; 2] = ["files", "matches"];

/// Every `path` string under `value`.
fn row_paths<'a>(value: &'a Value, out: &mut BTreeSet<&'a str>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                match value {
                    Value::String(path) if key == "path" => {
                        out.insert(path);
                    }
                    other => row_paths(other, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| row_paths(item, out)),
        _ => {}
    }
}

/// `value` with each `path` under `root/` spelled relative to `root`.
fn strip_root(value: &mut Value, root: &str) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields.iter_mut() {
                match value {
                    Value::String(path) if key == "path" => {
                        if let Some(rest) = path
                            .strip_prefix(root)
                            .and_then(|rest| rest.strip_prefix('/'))
                        {
                            *path = rest.to_owned();
                        }
                    }
                    other => strip_root(other, root),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| strip_root(item, root)),
        _ => {}
    }
}

/// The directory every file of a row (its `files` and `matches`) lies
/// under, when stating it once is shorter than repeating it on each file:
/// each of their paths is rewritten relative to it, so `root/path` is the
/// path the result names and `path:N` stays a line. `None` (nothing
/// rewritten) when the files share no directory or the note would cost
/// more than it saves.
pub(super) fn relative_paths(data: &mut Map<String, Value>) -> Option<String> {
    let mut paths = BTreeSet::new();
    for key in ROW_FILES {
        if let Some(files) = data.get(key) {
            row_paths(files, &mut paths);
        }
    }
    if paths.len() < 2 {
        return None;
    }
    let mut shared: Option<Vec<&str>> = None;
    for path in &paths {
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop();
        shared = Some(match shared {
            None => parts,
            Some(shared) => shared
                .into_iter()
                .zip(parts)
                .take_while(|(left, right)| left == right)
                .map(|(part, _)| part)
                .collect(),
        });
    }
    let root = shared?.join("/");
    // `# root: <root>` and its line break, against `<root>/` on each file.
    let note = "# root: \n".len() + root.len();
    if root.is_empty() || root == "." || paths.len() * (root.len() + 1) <= note {
        return None;
    }
    for key in ROW_FILES {
        if let Some(files) = data.get_mut(key) {
            strip_root(files, &root);
        }
    }
    Some(root)
}

/// A row's whole-number fields as one note, in their order, a run of equal
/// values naming its fields together: `matchCount=matchedLineCount 13
/// matchedFiles 5 filesScanned 1255`. Returns the note and the other fields.
pub(super) fn counters(fields: Map<String, Value>) -> (Option<String>, Map<String, Value>) {
    let (counts, rest): (Map<String, Value>, Map<String, Value>) = fields
        .into_iter()
        .partition(|(_, value)| value.as_u64().is_some());
    let mut runs: Vec<(Vec<String>, u64)> = Vec::new();
    for (key, value) in counts {
        let value = value.as_u64().unwrap_or_default();
        match runs.last_mut() {
            Some((keys, last)) if *last == value => keys.push(key),
            _ => runs.push((vec![key], value)),
        }
    }
    let note = runs
        .into_iter()
        .map(|(keys, value)| format!("{} {value}", keys.join("=")))
        .collect::<Vec<_>>()
        .join(" ");
    ((!note.is_empty()).then_some(note), rest)
}

/// `skippedPaths` beside content as how many paths each cause skipped, in
/// the order the entries name the causes: `25 hidden, 4 withheld`, then
/// `(sampled)` when the entries are a sample. The entries themselves print
/// on a row without content and stay whole in `--json`.
pub(super) fn skipped_counts(fields: &mut Map<String, Value>) {
    let Some(Value::Object(skipped)) = fields.get("skippedPaths") else {
        return;
    };
    let Some(Value::Array(entries)) = skipped.get("entries") else {
        return;
    };
    let mut causes: Vec<(String, u64)> = Vec::new();
    for entry in entries {
        let Some(cause) = entry.get("cause").and_then(Value::as_str) else {
            return;
        };
        let count = entry.get("count").and_then(Value::as_u64).unwrap_or(1);
        match causes.iter_mut().find(|(known, _)| known == cause) {
            Some((_, total)) => *total += count,
            None => causes.push((cause.to_owned(), count)),
        }
    }
    let mut note = causes
        .iter()
        .map(|(cause, count)| format!("{count} {cause}"))
        .collect::<Vec<_>>()
        .join(", ");
    if skipped.get("sampled") == Some(&Value::Bool(true)) {
        note.push_str(" (sampled)");
    }
    fields.insert("skippedPaths".into(), Value::String(note));
}

/// The `data` objects and lists every row of a batch holds with one value,
/// printed once before the rows (the graph a batch of queries read, its
/// freshness). A scalar stays on each row: two answers that happen to agree
/// (`affected: true`) are each row's own verdict. Only rows that print as
/// `key: value` text share fields (`generic`); a failed row, a page or an
/// incomplete mark ([`split_stated`]) is never shared; a page of one row
/// shares nothing.
pub(super) fn shared_fields(
    rows: &[Value],
    generic: impl Fn(&Map<String, Value>) -> bool,
) -> Map<String, Value> {
    let mut data = Vec::new();
    for row in rows {
        let Some(row) = row.as_object() else {
            return Map::new();
        };
        let failed = row.get("status").and_then(Value::as_str) == Some("error");
        match row.get("data").and_then(Value::as_object) {
            Some(fields) if !failed && generic(row) => data.push(fields),
            _ => return Map::new(),
        }
    }
    let Some((first, others)) = data.split_first().filter(|_| data.len() > 1) else {
        return Map::new();
    };
    let (_, candidates) = split_stated((*first).clone());
    candidates
        .into_iter()
        .filter(|(key, value)| {
            (value.is_object() || value.is_array())
                && others.iter().all(|row| row.get(key) == Some(value))
        })
        .collect()
}

/// `rows` as [`shared_fields`] compares them: a row's object that holds a
/// batch group's `by` (`graph` with its `id`) and omits members the tool's
/// contract lets a batch state once (`pageContextFields.batchStatedOnce`:
/// `root`, `createdAt`) holds those an earlier row stated for that `by`, so a
/// later row naming only a stated snapshot agrees with it and prints nothing
/// new. A fact outside that object stays as its row states it: a row may lack
/// it because its operation reports none.
pub(super) fn with_batch_facts(tool: ToolId, rows: &[Value]) -> Vec<Value> {
    let mut whole = rows.to_vec();
    let groups = octocode_native::contracts::tool_contract_named(tool.as_str())
        .and_then(Result::ok)
        .and_then(|contract| contract["pageContextFields"]["batchStatedOnce"].as_array());
    for group in groups.into_iter().flatten() {
        let Some(by) = group["by"].as_str() else {
            continue;
        };
        let Some((holder, _)) = by.rsplit_once('/') else {
            continue;
        };
        let members: Vec<String> = group["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|field| field.as_str()?.rsplit_once('/'))
            .filter(|(parent, _)| *parent == holder)
            .map(|(_, key)| key.replace("~1", "/").replace("~0", "~"))
            .collect();
        // Per `by` value and member, the value last stated.
        let mut stated: Vec<(Value, &str, Value)> = Vec::new();
        for row in &mut whole {
            if row["status"] == "error" {
                continue;
            }
            let Some(read) = row.pointer(by).filter(|read| !read.is_null()).cloned() else {
                continue;
            };
            let Some(object) = row.pointer_mut(holder).and_then(Value::as_object_mut) else {
                continue;
            };
            for member in &members {
                let last = stated
                    .iter_mut()
                    .find(|(name, key, _)| *name == read && key == member);
                match (object.get(member), last) {
                    (Some(value), Some((_, _, last))) => *last = value.clone(),
                    (Some(value), None) => stated.push((read.clone(), member, value.clone())),
                    (None, Some((_, _, last))) => {
                        object.insert(member.clone(), last.clone());
                    }
                    (None, None) => {}
                }
            }
        }
    }
    whole
}

/// The fields every object of `items` holds with one single-word value
/// (nested objects alike, field by field), when there are two or more
/// items; `skip_first` keeps each item's first field, its identity, on the
/// item.
pub(super) fn item_constants(items: &[Value], skip_first: bool) -> Map<String, Value> {
    let objects: Option<Vec<&Map<String, Value>>> = items.iter().map(Value::as_object).collect();
    let Some(objects) = objects.filter(|objects| objects.len() > 1) else {
        return Map::new();
    };
    let mut constants = Map::new();
    let keys = objects[0].keys().skip(usize::from(skip_first));
    for key in keys {
        let values: Option<Vec<&Value>> = objects.iter().map(|item| item.get(key)).collect();
        let Some(values) = values else { continue };
        if is_word(values[0]) && values.iter().all(|value| *value == values[0]) {
            constants.insert(key.clone(), values[0].clone());
        } else if values.iter().all(|value| value.is_object()) {
            let nested: Vec<Value> = values.into_iter().cloned().collect();
            let inner = item_constants(&nested, false);
            if !inner.is_empty() {
                constants.insert(key.clone(), Value::Object(inner));
            }
        }
    }
    constants
}

/// `item` without the `constants` ([`item_constants`]) it holds; a nested
/// object left empty goes with them.
pub(super) fn without_constants(item: &Value, constants: &Map<String, Value>) -> Value {
    let Some(fields) = item.as_object() else {
        return item.clone();
    };
    let mut kept = Map::new();
    for (key, value) in fields {
        match constants.get(key) {
            Some(Value::Object(inner)) if value.is_object() => {
                let rest = without_constants(value, inner);
                if rest.as_object().is_none_or(|rest| !rest.is_empty()) {
                    kept.insert(key.clone(), rest);
                }
            }
            Some(_) => {}
            None => {
                kept.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(kept)
}
