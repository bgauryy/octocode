//! How text lays out what a row states: the directory its printed paths
//! share on one `# root:` line with those paths relative to it, its counters
//! and what its walk skipped on one line each, and a field every item of one
//! list holds with one value after the list's key. Layout only: text adds no
//! fact the JSON lacks, and what a batch's rows share is stated once by the
//! response alone (`response::hoist`).
use super::listing::entry_name;
use super::table::is_word;
use super::words;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// The fields of `data` whose items name the files a row prints.
const ROW_FILES: [&str; 2] = ["files", "matches"];

/// The field of `data` whose string items are a listing's flat rows, each
/// naming its entry by path (`<path>[/][ (<fields>)]`).
const FLAT_ROWS: &str = "files";

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
                        } else if let Some(rest) = under(path, root) {
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

/// `path` relative to `root`, when it lies under it.
fn under<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    path.strip_prefix(root)?.strip_prefix('/')
}

/// The one layout rule for paths: the directory every printed path of a row
/// lies under (its `files` and `matches`, a listing's group headings and,
/// when `flat`, its flat rows, and the `skippedPaths` entries), when stating
/// it once is shorter than repeating it on each path. Each of those paths is
/// rewritten relative to it, so the returned root, `/` and a printed path
/// are the path the result names, and `path:N` stays a line. A row that
/// states the `root` its paths are named from (evidence outside the
/// workspace) has it taken into that one root (`<stated>/<shared>`), so text
/// prints one `# root:` line and the same join reads every path. `None`
/// (nothing rewritten) for one path, for paths that share no directory, or
/// when the line would cost more than it saves.
pub(super) fn relative_paths(data: &mut Map<String, Value>, flat: bool) -> Option<String> {
    let stated = match data.shift_remove("root") {
        Some(Value::String(root)) => Some(root),
        Some(other) => {
            data.insert("root".to_owned(), other);
            None
        }
        None => None,
    };
    let shared = shared_dir(data, flat);
    match (stated, shared) {
        (Some(stated), Some(shared)) => Some(format!("{stated}/{shared}")),
        (Some(stated), None) => Some(stated),
        (None, shared) => shared,
    }
}

/// [`relative_paths`] of the paths as the row names them.
fn shared_dir(data: &mut Map<String, Value>, flat: bool) -> Option<String> {
    let mut paths = BTreeSet::new();
    for key in ROW_FILES {
        if let Some(files) = data.get(key) {
            row_paths(files, &mut paths);
        }
    }
    // A flat row names a file or a directory by its path: either lies in the
    // directory before its last part, so no row is the root itself.
    if flat {
        let rows = data.get(FLAT_ROWS).and_then(Value::as_array);
        paths.extend(
            rows.into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|row| {
                    let name = entry_name(row);
                    (name.strip_suffix('/').unwrap_or(name), false)
                }),
        );
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
    // `# root: <root>` and its line break, against `<root>/` on each path.
    let note = "# root: \n".len() + root.len();
    if root.is_empty() || root == "." || paths.len() * (root.len() + 1) <= note {
        return None;
    }
    for key in ROW_FILES.into_iter().chain([SKIPPED]) {
        if let Some(files) = data.get_mut(key) {
            strip_root(files, &root);
        }
    }
    if flat && let Some(Value::Array(rows)) = data.get_mut(FLAT_ROWS) {
        for row in rows {
            if let Value::String(text) = row
                && let Some(rest) = under(text, &root)
            {
                *text = rest.to_owned();
            }
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
    let Some(line) = fields.get(SKIPPED).and_then(skipped_text) else {
        return;
    };
    fields.insert(SKIPPED.into(), Value::String(line));
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
