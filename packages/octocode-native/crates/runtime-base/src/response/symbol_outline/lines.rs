//! Q5: the outline as line-first strings, one per declaration, so a page of
//! symbols reads like numbered source lines and every reader names a
//! declaration by its line (clasify's `path:line` unit, the gutter of a
//! numbered line). [`outline_lines`] writes them from the public rows
//! ([`super::declaration_rows`]); [`outline_row`] and [`rows_of_lines`] read
//! them back, so every field round-trips.
//!
//! One string: `"<line>\t<indent><kind> <symbolName>[ (<tags>)]"`. The
//! indent is two spaces per nesting depth (a member follows its parent one
//! depth deeper); `kind` is `-` when a row has none. The tags, in this order
//! and as they apply, joined by `, `:
//! - `end N`, `doc N`, `start N`, `col N`: `endLine`, `docStartLine`,
//!   `startLine`, `column`;
//! - `exported`; `as A|B`: each `exportedAs` name;
//! - `parent NAME at N` (`parent NAME` without a `parentLine`): a member
//!   listed outside its parent;
//! - `path P`;
//! - `test F`: a `testIdentity` that states only its framework;
//! - `more {…}`: every other field, compact JSON, always last.
use serde_json::{Map, Value};

/// The tag words, in the order a string states them.
const TAGS: [&str; 10] = [
    "end", "doc", "start", "col", "exported", "as", "parent", "path", "test", "more",
];

/// The fields a string states outside `more`.
const STATED: [&str; 14] = [
    "line",
    "kind",
    "symbolName",
    "members",
    "endLine",
    "docStartLine",
    "startLine",
    "column",
    "exported",
    "exportedAs",
    "parent",
    "parentLine",
    "path",
    "testIdentity",
];

/// Each declaration of public outline `rows` as one line-first string, its
/// members after it (module docs).
pub fn outline_lines(rows: &[Value]) -> Vec<String> {
    fn walk(rows: &[Value], depth: usize, out: &mut Vec<String>) {
        for row in rows {
            let Some(fields) = row.as_object() else {
                continue;
            };
            out.push(outline_line(fields, depth));
            if let Some(Value::Array(members)) = fields.get("members") {
                walk(members, depth + 1, out);
            }
        }
    }
    let mut out = Vec::with_capacity(rows.len());
    walk(rows, 0, &mut out);
    out
}

fn outline_line(fields: &Map<String, Value>, depth: usize) -> String {
    let number = |key: &str| fields.get(key).and_then(Value::as_u64);
    let text = |key: &str| fields.get(key).and_then(Value::as_str);
    let mut tags = Vec::new();
    let mut more = Map::new();
    for (tag, key) in [
        ("end", "endLine"),
        ("doc", "docStartLine"),
        ("start", "startLine"),
        ("col", "column"),
    ] {
        match number(key) {
            Some(value) => tags.push(format!("{tag} {value}")),
            None => keep(fields, key, &mut more),
        }
    }
    match fields.get("exported") {
        Some(Value::Bool(true)) => tags.push("exported".to_owned()),
        _ => keep(fields, "exported", &mut more),
    }
    match fields
        .get("exportedAs")
        .and_then(Value::as_array)
        .and_then(|names| {
            names
                .iter()
                .map(|name| name.as_str().filter(|name| !name.contains('|')))
                .collect::<Option<Vec<_>>>()
                .filter(|names| !names.is_empty())
        }) {
        Some(names) => tags.push(format!("as {}", names.join("|"))),
        None => keep(fields, "exportedAs", &mut more),
    }
    match (text("parent"), fields.get("parentLine")) {
        (Some(parent), Some(line)) if line.is_u64() => {
            tags.push(format!("parent {parent} at {line}"));
        }
        (Some(parent), None) => tags.push(format!("parent {parent}")),
        _ => {
            keep(fields, "parent", &mut more);
            keep(fields, "parentLine", &mut more);
        }
    }
    match text("path") {
        Some(path) => tags.push(format!("path {path}")),
        None => keep(fields, "path", &mut more),
    }
    match fields.get("testIdentity") {
        Some(Value::Object(test))
            if test.len() == 1
                && test
                    .get("framework")
                    .and_then(Value::as_str)
                    .is_some_and(|framework| !framework.contains([' ', ',', ')'])) =>
        {
            tags.push(format!("test {}", test["framework"].as_str().unwrap_or("")));
        }
        _ => keep(fields, "testIdentity", &mut more),
    }
    for (key, value) in fields {
        if !STATED.contains(&key.as_str()) {
            more.insert(key.clone(), value.clone());
        }
    }
    if !more.is_empty() {
        tags.push(format!("more {}", Value::Object(more)));
    }
    let line = fields
        .get("line")
        .map_or_else(String::new, ToString::to_string);
    let kind = text("kind").unwrap_or("-");
    let name = text("symbolName").unwrap_or("");
    let indent = "  ".repeat(depth);
    if tags.is_empty() {
        format!("{line}\t{indent}{kind} {name}")
    } else {
        format!("{line}\t{indent}{kind} {name} ({})", tags.join(", "))
    }
}

/// Keeps `key` of `fields` (a value its tag cannot state) for `more`.
fn keep(fields: &Map<String, Value>, key: &str, more: &mut Map<String, Value>) {
    if let Some(value) = fields.get(key) {
        more.insert(key.to_owned(), value.clone());
    }
}

/// The row an [`outline_lines`] string states, without members, and its
/// nesting depth; `None` for a string that is not one.
pub fn outline_row(text: &str) -> Option<(usize, Value)> {
    let (line, rest) = text.split_once('\t')?;
    let line: u64 = line.parse().ok()?;
    let body = rest.trim_start_matches(' ');
    let indent = rest.len() - body.len();
    if indent % 2 != 0 {
        return None;
    }
    let (kind, rest) = body.split_once(' ')?;
    let (name, tags) = split_tags(rest);
    let mut row = Map::new();
    row.insert("symbolName".into(), Value::from(name));
    if kind != "-" {
        row.insert("kind".into(), Value::from(kind));
    }
    row.insert("line".into(), Value::from(line));
    for tag in tags {
        let (word, value) = tag.split_once(' ').unwrap_or((tag, ""));
        let number = || value.parse::<u64>().ok().map(Value::from);
        match word {
            "end" => {
                row.insert("endLine".into(), number()?);
            }
            "doc" => {
                row.insert("docStartLine".into(), number()?);
            }
            "start" => {
                row.insert("startLine".into(), number()?);
            }
            "col" => {
                row.insert("column".into(), number()?);
            }
            "exported" => {
                row.insert("exported".into(), Value::Bool(true));
            }
            "as" => {
                let names = value.split('|').map(Value::from).collect();
                row.insert("exportedAs".into(), Value::Array(names));
            }
            "parent" => match value
                .rsplit_once(" at ")
                .and_then(|(parent, at)| Some((parent, at.parse::<u64>().ok()?)))
            {
                Some((parent, at)) => {
                    row.insert("parent".into(), Value::from(parent));
                    row.insert("parentLine".into(), Value::from(at));
                }
                None => {
                    row.insert("parent".into(), Value::from(value));
                }
            },
            "path" => {
                row.insert("path".into(), Value::from(value));
            }
            "test" => {
                row.insert(
                    "testIdentity".into(),
                    serde_json::json!({"framework": value}),
                );
            }
            "more" => {
                let Value::Object(more) = serde_json::from_str(value).ok()? else {
                    return None;
                };
                row.extend(more);
            }
            _ => return None,
        }
    }
    Some((indent / 2, Value::Object(row)))
}

/// A string's name and its tags: the last ` (` whose group, up to the `)`
/// that ends the string, reads as tags opens them; otherwise the rest is
/// all name (a name may hold ` (` itself).
fn split_tags(rest: &str) -> (&str, Vec<&str>) {
    let Some(inner) = rest.strip_suffix(')') else {
        return (rest, Vec::new());
    };
    let opens = inner
        .match_indices(" (")
        .map(|(at, _)| at)
        .collect::<Vec<_>>();
    for at in opens.into_iter().rev() {
        // ` (` is ASCII, so both edges are character boundaries.
        if let (Some(name), Some(tags)) = (rest.get(..at), inner.get(at + 2..).and_then(tags_of)) {
            return (name, tags);
        }
    }
    (rest, Vec::new())
}

/// Whether `text` starts with a tag word, alone or before its value.
fn opens_tag(text: &str) -> bool {
    TAGS.iter().any(|word| {
        text.strip_prefix(word).is_some_and(|after| {
            after.is_empty() || after.starts_with(' ') || after.starts_with(',')
        })
    })
}

/// A group's tags: it is split before each `, ` that opens a tag word, and
/// `more` takes the rest. `None` when the group opens no tag.
fn tags_of(group: &str) -> Option<Vec<&str>> {
    if !opens_tag(group) {
        return None;
    }
    let mut tags = Vec::new();
    let mut tag = group;
    // The `, ` before each tag word ends the tag before it; `more` holds the
    // rest whole (its JSON may hold `, `).
    while !tag.starts_with("more ") {
        let split = tag.match_indices(", ").find_map(|(at, _)| {
            let next = tag.get(at + 2..)?;
            opens_tag(next).then_some((tag.get(..at)?, next))
        });
        let Some((own, next)) = split else {
            break;
        };
        tags.push(own);
        tag = next;
    }
    tags.push(tag);
    Some(tags)
}

/// The nested public rows `lines` ([`outline_lines`]) state: each string's
/// row, a deeper string a member of the nearest shallower one before it.
/// `None` when a string is not an outline line or nests too deep.
pub fn rows_of_lines(lines: &[Value]) -> Option<Vec<Value>> {
    fn attach(rows: &mut Vec<Value>, depth: usize, row: Value) -> Option<()> {
        if depth == 0 {
            rows.push(row);
            return Some(());
        }
        let parent = rows.last_mut()?;
        let members = parent
            .as_object_mut()?
            .entry("members")
            .or_insert_with(|| Value::Array(Vec::new()));
        attach(members.as_array_mut()?, depth - 1, row)
    }
    let mut rows = Vec::new();
    for line in lines {
        let (depth, row) = outline_row(line.as_str()?)?;
        attach(&mut rows, depth, row)?;
    }
    Some(rows)
}

#[cfg(test)]
mod tests;
