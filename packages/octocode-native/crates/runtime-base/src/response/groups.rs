//! F2: path-only rows grouped by directory (`{dir, files}`), one shared
//! implementation and its pricer for every listing view. An entry's path is
//! `dir + "/" + name` (`.` names the listed root), the rule core states for
//! `LocalDirectoryGroupSchema`. Grouping changes what a page counts
//! (entries, not groups, and a page that continues a group repeats its
//! header), so a tool that cuts its own pages prices entries with [`costs`]
//! and groups the page it cut with [`groups`]. A view that may list its
//! rows either way groups them only where that is shorter ([`group_rows`]):
//! a flat or small listing stays flat, so no view grows.
use serde_json::{Map, Value, json};

/// How entries form groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// One group per directory, in the order each first appears: the order
    /// only names the entries (a walk, a path sort), so an entry whose
    /// directory the listing returns to joins that directory's group.
    Merged,
    /// Each run of adjacent entries of one directory is a group: the order
    /// is the answer (a size, line or time rank), so a directory the rank
    /// returns to opens a new group.
    Runs,
}

/// One listed entry: its directory as the response spells it, and its item
/// in the group (a name, or an object whose `name` names it beside its own
/// fields, such as a count).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub dir: String,
    pub item: Value,
}

/// The `{dir, files}` groups of `entries`, in `order`.
pub fn groups(entries: impl IntoIterator<Item = Entry>, order: Order) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut at = std::collections::HashMap::<String, usize>::new();
    for Entry { dir, item } in entries {
        let adjacent = out
            .len()
            .checked_sub(1)
            .filter(|&last| out[last]["dir"] == dir.as_str());
        let merged = (order == Order::Merged)
            .then(|| at.get(&dir).copied())
            .flatten();
        let index = match adjacent.or(merged) {
            Some(index) => index,
            None => {
                out.push(json!({"dir": dir, "files": []}));
                at.insert(dir, out.len() - 1);
                out.len() - 1
            }
        };
        if let Some(items) = out[index]["files"].as_array_mut() {
            items.push(item);
        }
    }
    out
}

/// Serialized chars one entry adds to a page: `entry` always, plus `header`
/// when it opens a group: it starts the page, or it does not `continue` the
/// entry before it (another directory). A page repeats the header of a
/// group it continues, so every page reads on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cost {
    pub entry: usize,
    pub header: usize,
    pub continues: bool,
}

/// Each entry's [`Cost`], from its directory as the response spells it and
/// the chars of its item.
pub fn costs<'a>(entries: impl IntoIterator<Item = (&'a str, usize)>) -> Vec<Cost> {
    let mut headers = std::collections::HashMap::<&str, usize>::new();
    let mut previous: Option<&str> = None;
    entries
        .into_iter()
        .map(|(dir, item)| {
            let header = *headers.entry(dir).or_insert_with(|| header_chars(dir));
            let cost = Cost {
                entry: item + 1,
                header,
                continues: previous == Some(dir),
            };
            previous = Some(dir);
            cost
        })
        .collect()
}

/// Chars a group's header takes: `{"dir":…,"files":[]}` and the comma
/// after it.
pub fn header_chars(dir: &str) -> usize {
    octocode_engine::json_size::utf16_units(&json!({"dir": dir, "files": []})).unwrap_or(0) + 1
}

/// A row path's group directory and entry name: the parent directory, or
/// `.` for a name at the root.
#[must_use]
pub fn split(path: &str) -> (&str, &str) {
    match path.rsplit_once('/') {
        Some(("", name)) => ("/", name),
        Some((dir, name)) => (dir, name),
        None => (".", path),
    }
}

/// The path an entry names: `dir + "/" + name`, a root entry its name.
#[must_use]
pub fn path_of(dir: &str, name: &str) -> String {
    match dir {
        "." => name.to_owned(),
        "/" => format!("/{name}"),
        _ => format!("{dir}/{name}"),
    }
}

/// `rows` (objects naming their `path`) as `{dir, files}` groups when that
/// is shorter than the rows: a row with only its path becomes its name, a
/// row with more fields `{name, …}` with them. `None` keeps the rows flat:
/// they are fewer than two, a row is not path-keyed, or the groups would not
/// be shorter.
pub fn group_rows(rows: &[Value], order: Order) -> Option<Vec<Value>> {
    if rows.len() < 2 {
        return None;
    }
    let entries = rows
        .iter()
        .map(|row| {
            let fields = row.as_object()?;
            let (dir, name) = split(fields.get("path")?.as_str()?);
            let rest = fields
                .iter()
                .filter(|(key, _)| key.as_str() != "path")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Map<_, _>>();
            let item = if rest.is_empty() {
                json!(name)
            } else {
                let mut named = Map::new();
                named.insert("name".into(), json!(name));
                named.extend(rest);
                Value::Object(named)
            };
            Some(Entry {
                dir: dir.to_owned(),
                item,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let grouped = groups(entries, order);
    let size = |value: &[Value]| octocode_engine::json_size::utf16_units(value).unwrap_or(0);
    (size(&grouped) < size(rows)).then_some(grouped)
}

/// The rows `groups` lists, each with its `path` again, in group order:
/// what a reader rebuilds by `dir + "/" + name`.
pub fn rows_of(groups: &[Value]) -> Vec<Value> {
    groups
        .iter()
        .flat_map(|group| {
            let dir = group["dir"].as_str().unwrap_or(".").to_owned();
            group["files"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |item| match item {
                    Value::String(name) => json!({"path": path_of(&dir, name)}),
                    Value::Object(fields) => {
                        let mut row = Map::new();
                        let name = fields.get("name").and_then(Value::as_str).unwrap_or("");
                        row.insert("path".into(), json!(path_of(&dir, name)));
                        row.extend(
                            fields
                                .iter()
                                .filter(|(key, _)| key.as_str() != "name")
                                .map(|(key, value)| (key.clone(), value.clone())),
                        );
                        Value::Object(row)
                    }
                    other => other.clone(),
                })
        })
        .collect()
}

#[cfg(test)]
mod tests;
