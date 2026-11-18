//! What text states once instead of on every row it holds for: the
//! directory a row's paths share, the scope a graph row's paths lie in, a
//! row's counters, what a walk skipped, the fields every row of a batch
//! shares, and the fields every item of a list shares. Each stays a fact of
//! the result, said once; `--json` keeps every row whole.
use super::follow::split_stated;
use super::table::is_word;
use super::words;
use octocode_runtime_base::id::ToolId;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// The fields of `data` whose items name the files a row prints.
const ROW_FILES: [&str; 2] = ["files", "matches"];

/// The field of `data` naming the paths a row's walk skipped: each entry's
/// `path` is a file or the directory holding what was left out.
const SKIPPED: &str = "skippedPaths";

/// Every `path` string under `value`, and every listing group's `dir`
/// (a directory: `true`).
fn row_paths<'a>(value: &'a Value, out: &mut BTreeSet<(&'a str, bool)>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                match value {
                    Value::String(path) if key == "path" => {
                        out.insert((path, false));
                    }
                    Value::String(dir) if key == "dir" && fields.contains_key("files") => {
                        out.insert((dir, true));
                    }
                    other => row_paths(other, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| row_paths(item, out)),
        _ => {}
    }
}

/// `value` with each `path` under `root/` spelled relative to `root` (the
/// root itself as `.`), and each listing group's `dir` too (the root itself
/// as `""`, which a listing prints `./`).
fn strip_root(value: &mut Value, root: &str) {
    match value {
        Value::Object(fields) => {
            let group = fields.contains_key("files");
            for (key, value) in fields.iter_mut() {
                match value {
                    Value::String(path) if key == "path" || (key == "dir" && group) => {
                        if key == "dir" && path == root {
                            path.clear();
                        } else if path == root {
                            // A skipped entry naming the root directory itself.
                            *path = ".".to_owned();
                        } else if let Some(rest) = path
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

/// The directory every path of a row (its `files`, `matches` and the
/// `skippedPaths` entries, the directory such an entry names included) lies
/// under, when stating it once is shorter than repeating it on each path:
/// each of their paths is rewritten relative to it, so `root/path` is the
/// path the result names and `path:N` stays a line. A row that states the
/// `root` its paths are named from (evidence outside the workspace) has it
/// taken into that one root. `None` (nothing rewritten) when the files
/// share no directory or the note would cost more than it saves.
pub(super) fn relative_paths(data: &mut Map<String, Value>) -> Option<String> {
    let stated = match data.shift_remove("root") {
        Some(Value::String(root)) => Some(root),
        Some(other) => {
            data.insert("root".to_owned(), other);
            None
        }
        None => None,
    };
    let shared = shared_dir(data);
    match (stated, shared) {
        (Some(stated), Some(shared)) => Some(format!("{stated}/{shared}")),
        (Some(stated), None) => Some(stated),
        (None, shared) => shared,
    }
}

/// [`relative_paths`] of the paths as the row names them.
fn shared_dir(data: &mut Map<String, Value>) -> Option<String> {
    let mut paths = BTreeSet::new();
    for key in ROW_FILES {
        if let Some(files) = data.get(key) {
            row_paths(files, &mut paths);
        }
    }
    // A skipped entry may name the directory holding what was left out,
    // which the root may be.
    if let Some(entries) = data
        .get(SKIPPED)
        .and_then(|skipped| skipped["entries"].as_array())
    {
        paths.extend(
            entries
                .iter()
                .filter_map(|entry| entry["path"].as_str())
                .map(|path| (path, true)),
        );
    }
    if paths.len() < 2 {
        return None;
    }
    let mut shared: Option<Vec<&str>> = None;
    for (path, dir) in &paths {
        let mut parts: Vec<&str> = path.split('/').collect();
        if !dir {
            parts.pop();
        }
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
    for key in ROW_FILES.into_iter().chain([SKIPPED]) {
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

/// `skippedPaths` as one note, the same beside content and on a row without
/// it: each entry by its path, then any other field it carries (`count`,
/// `reason`) as words in `[ … ]`; a run of entries of one cause after that
/// cause, said once (`hidden: .agents [count 25], .github`); runs joined by
/// `; `, then the object's own fields as words (`sampled true`). Left as it
/// is when it is not entries of fields, which the outline then prints.
pub(super) fn skipped_line(fields: &mut Map<String, Value>) {
    let Some(line) = fields.get("skippedPaths").and_then(skipped_text) else {
        return;
    };
    fields.insert("skippedPaths".into(), Value::String(line));
}

/// [`skipped_line`]'s text of a `skippedPaths` value.
fn skipped_text(skipped: &Value) -> Option<String> {
    let skipped = skipped.as_object()?;
    let entries = skipped.get("entries")?.as_array()?;
    // Each run: its cause, and its entries as text.
    let mut runs: Vec<(Option<String>, Vec<String>)> = Vec::new();
    for entry in entries {
        let mut rest = entry.as_object()?.clone();
        let cause = take_text(&mut rest, "cause");
        let mut text = take_text(&mut rest, "path").unwrap_or_default();
        if !rest.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&format!("[{}]", words(&rest)));
        }
        match runs.last_mut() {
            Some((last, items)) if *last == cause => items.push(text),
            _ => runs.push((cause, vec![text])),
        }
    }
    let mut parts: Vec<String> = runs
        .into_iter()
        .map(|(cause, items)| match cause {
            Some(cause) => format!("{cause}: {}", items.join(", ")),
            None => items.join(", "),
        })
        .collect();
    let mut own = skipped.clone();
    own.shift_remove("entries");
    if !own.is_empty() {
        parts.push(words(&own));
    }
    Some(parts.join("; "))
}

/// The string `fields` states under `key`, taken out of them; any other
/// value stays, to print as words.
fn take_text(fields: &mut Map<String, Value>, key: &str) -> Option<String> {
    let text = fields.get(key)?.as_str()?.to_owned();
    fields.shift_remove(key);
    Some(text)
}

/// The fields a graph answer names nodes, files and directories in: one
/// that names the query's scope itself reads `.` under it.
const NODE_FIELDS: [&str; 15] = [
    "id",
    "path",
    "changed",
    "subject",
    "from",
    "to",
    "target",
    "members",
    "dirs",
    "nodes",
    "rows",
    "files",
    "reachedFrom",
    "affectedEntrypoints",
    "testsToRun",
];

/// A graph row's references to the snapshot it read, named by id: no path
/// of the scope.
const SNAPSHOT_FIELDS: [&str; 2] = ["graph", "baseline"];

/// A graph row's paths relative to the scope its query names (`path`): the
/// answer names each file from the workspace root, the scope included, so
/// the row states the scope once as its root instead. Rewritten are the
/// strings of the row's `data` (its snapshot references aside) that spell a
/// path under the scope, a `path:line` mention alike, and a node field
/// naming the scope itself (`.`). A value with a space (text) or a `#` (a
/// symbol id, named from the snapshot) stays as the result spells it. `None`
/// (nothing rewritten) when the query names no scope below the workspace or
/// no value lies under it; calls are separate and keep their paths.
pub(super) fn under_scope(row: &mut Map<String, Value>, query: &Value) -> Option<String> {
    let scope = query["queries"][0]["path"].as_str()?;
    let scope = scope
        .strip_prefix("./")
        .unwrap_or(scope)
        .trim_end_matches('/');
    if scope.is_empty() || scope == "." {
        return None;
    }
    let data = row.get_mut("data")?.as_object_mut()?;
    let mut rewritten = false;
    for (key, value) in data.iter_mut() {
        if !SNAPSHOT_FIELDS.contains(&key.as_str()) {
            relative_to(value, key, scope, &mut rewritten);
        }
    }
    rewritten.then(|| scope.to_owned())
}

/// [`under_scope`] of one value under `key`.
fn relative_to(value: &mut Value, key: &str, scope: &str, rewritten: &mut bool) {
    match value {
        Value::String(text) if !text.contains(char::is_whitespace) && !text.contains('#') => {
            let relative = text
                .strip_prefix(scope)
                .and_then(|rest| rest.strip_prefix('/'))
                .filter(|rest| !rest.is_empty())
                .map(str::to_owned);
            if let Some(relative) = relative {
                *text = relative;
                *rewritten = true;
            } else if *text == scope && NODE_FIELDS.contains(&key) {
                *text = ".".to_owned();
                *rewritten = true;
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| relative_to(item, key, scope, rewritten)),
        Value::Object(fields) => {
            for (key, value) in fields.iter_mut() {
                if !SNAPSHOT_FIELDS.contains(&key.as_str()) {
                    relative_to(value, key, scope, rewritten);
                }
            }
        }
        _ => {}
    }
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
    let groups = octocode_runtime_base::contracts::tool_contract_named(tool.as_str())
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
