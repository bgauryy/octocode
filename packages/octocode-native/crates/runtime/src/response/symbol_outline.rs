//! Declaration rows for astSearch `symbols` and lspSearch `documentSymbols`.
//!
//! Structured content carries the outline in source order, one object per
//! declaration (`symbolName`, `kind`, `line`, `endLine`, …). A declaration
//! with members nests every declaration listed after it inside it under
//! `members` (so they name no `parent`/`parentLine`); `kind` or
//! `exported: true` shared by every one of 2+ members is stated once in its
//! `shared`. A member whose parent is not in the rows (a filter dropped it)
//! stays top level with `parent` (+ `parentLine` when the name is
//! ambiguous). Pages count top-level rows: [`top_level_spans`] splits flat
//! rows into one span per top-level row, so a page never cuts a member from
//! its parent.

use serde_json::Value;

/// Public key order of a declaration (see [`declaration_rows`]).
const DECLARATION_ORDER: [&str; 11] = [
    "symbolName",
    "kind",
    "line",
    "endLine",
    "exported",
    "exportedAs",
    "docStartLine",
    "startLine",
    "column",
    "parent",
    "parentLine",
];

/// Member fields a parent states once in `shared` when every one of its
/// 2+ members has the same value.
const MEMBER_SHARED: [&str; 2] = ["kind", "exported"];

/// Public outline rows from flat internal ones (`name`, `character`,
/// `parent`, `parentLine`): `symbolName` and `column`, keys in one order,
/// each member nested in its parent's `members` (see the module docs).
pub(crate) fn declaration_rows(rows: &[Value]) -> Vec<Value> {
    let public: Vec<Value> = rows
        .iter()
        .map(|row| {
            let Some(fields) = row.as_object() else {
                return row.clone();
            };
            let mut public = serde_json::Map::new();
            for key in DECLARATION_ORDER {
                let source = match key {
                    "symbolName" => "name",
                    "column" => "character",
                    other => other,
                };
                if let Some(value) = fields.get(source).or_else(|| fields.get(key)) {
                    public.insert(key.into(), value.clone());
                }
            }
            for (key, value) in fields {
                if !matches!(key.as_str(), "name" | "character") && !public.contains_key(key) {
                    public.insert(key.clone(), value.clone());
                }
            }
            Value::Object(public)
        })
        .collect();
    nest_members(public)
}

/// The enclosing row of each flat row: the nearest preceding ancestor it
/// names (`parent`, and `parentLine` when given; the name is `symbolName`
/// or an internal row's `name`), or `None` for a top-level row. A row of
/// another file (`path`) than the row before it starts over.
fn enclosing_rows(rows: &[Value]) -> Vec<Option<usize>> {
    let name = |row: &Value| {
        row.get("symbolName")
            .or_else(|| row.get("name"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let mut enclosing = Vec::with_capacity(rows.len());
    // Ancestor chain of the current row, as row indices.
    let mut stack: Vec<usize> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if index > 0 && row.get("path") != rows[index - 1].get("path") {
            stack.clear();
        }
        let parent = row.get("parent").and_then(Value::as_str);
        let parent_line = row.get("parentLine").and_then(Value::as_u64);
        let found = parent.and_then(|parent| {
            stack.iter().rposition(|&ancestor| {
                let ancestor = &rows[ancestor];
                name(ancestor).as_deref() == Some(parent)
                    && parent_line
                        .is_none_or(|expected| ancestor["line"].as_u64() == Some(expected))
            })
        });
        match found {
            Some(position) => {
                stack.truncate(position + 1);
                enclosing.push(Some(stack[position]));
            }
            None => {
                stack.clear();
                enclosing.push(None);
            }
        }
        stack.push(index);
    }
    enclosing
}

/// One range of flat rows per top-level row: the row and every row nested
/// under it (see [`declaration_rows`]). The spans tile `rows` in order, so
/// pages over them reach each declaration once and never cut a member from
/// its parent.
pub(crate) fn top_level_spans(rows: &[Value]) -> Vec<std::ops::Range<usize>> {
    let roots: Vec<usize> = enclosing_rows(rows)
        .iter()
        .enumerate()
        .filter_map(|(index, parent)| parent.is_none().then_some(index))
        .collect();
    roots
        .iter()
        .enumerate()
        .map(|(position, &start)| start..roots.get(position + 1).copied().unwrap_or(rows.len()))
        .collect()
}

/// Nest each flat row under the nearest preceding ancestor it names
/// (`parent`, and `parentLine` when given), dropping the two fields; a row
/// whose parent is not an ancestor on the page stays top level with them.
fn nest_members(rows: Vec<Value>) -> Vec<Value> {
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); rows.len()];
    let mut roots = Vec::new();
    let mut nested = vec![false; rows.len()];
    for (index, parent) in enclosing_rows(&rows).into_iter().enumerate() {
        match parent {
            Some(parent) => {
                members[parent].push(index);
                nested[index] = true;
            }
            None => roots.push(index),
        }
    }
    let mut slots: Vec<Option<Value>> = rows.into_iter().map(Some).collect();
    for (index, is_nested) in nested.iter().enumerate() {
        if *is_nested && let Some(Value::Object(fields)) = slots[index].as_mut() {
            fields.shift_remove("parent");
            fields.shift_remove("parentLine");
        }
    }
    roots
        .into_iter()
        .map(|root| build_node(root, &mut slots, &members))
        .collect()
}

fn build_node(index: usize, slots: &mut [Option<Value>], members: &[Vec<usize>]) -> Value {
    let mut node = slots[index].take().unwrap_or(Value::Null);
    let mut children: Vec<Value> = members[index]
        .iter()
        .map(|&child| build_node(child, slots, members))
        .collect();
    let Some(fields) = node.as_object_mut() else {
        return node;
    };
    if children.is_empty() {
        return node;
    }
    if children.len() >= 2 {
        let mut shared = serde_json::Map::new();
        for key in MEMBER_SHARED {
            let first = children[0].get(key).cloned();
            if let Some(value) = first
                && children.iter().all(|child| child.get(key) == Some(&value))
            {
                shared.insert(key.to_owned(), value);
            }
        }
        if !shared.is_empty() {
            for child in &mut children {
                if let Some(child) = child.as_object_mut() {
                    for key in shared.keys() {
                        child.shift_remove(key);
                    }
                }
            }
            fields.insert("shared".into(), Value::Object(shared));
        }
    }
    fields.insert("members".into(), Value::Array(children));
    node
}

/// Flat declaration objects for outline rows: each member following its
/// parent with `parent` and `parentLine` restored, and the parent's
/// `shared` fields filled in. The inverse of [`declaration_rows`].
#[cfg(test)]
pub(crate) fn flatten_members(rows: &[Value]) -> Vec<Value> {
    fn walk(rows: &[Value], parent: Option<(&Value, &Value)>, out: &mut Vec<Value>) {
        for row in rows {
            let Some(fields) = row.as_object() else {
                out.push(row.clone());
                continue;
            };
            let mut flat = fields.clone();
            let members = flat.shift_remove("members");
            let shared = flat.shift_remove("shared");
            if let Some((name, line)) = parent {
                flat.insert("parent".into(), name.clone());
                flat.insert("parentLine".into(), line.clone());
            }
            out.push(Value::Object(flat));
            let Some(Value::Array(mut members)) = members else {
                continue;
            };
            if let Some(Value::Object(shared)) = &shared {
                for member in &mut members {
                    if let Some(member) = member.as_object_mut() {
                        for (key, value) in shared {
                            member.entry(key.clone()).or_insert_with(|| value.clone());
                        }
                    }
                }
            }
            let name = fields.get("symbolName").unwrap_or(&Value::Null);
            let line = fields.get("line").unwrap_or(&Value::Null);
            walk(&members, Some((name, line)), out);
        }
    }
    let mut out = Vec::with_capacity(rows.len());
    walk(rows, None, &mut out);
    out
}

#[cfg(test)]
mod tests;
