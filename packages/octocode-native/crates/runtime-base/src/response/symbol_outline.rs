//! Declaration rows for astSearch `symbols` and lspSearch `documentSymbols`.
//!
//! Structured content carries the outline in source order, one object per
//! declaration (`symbolName`, `kind`, `line`, `endLine`, …). A declaration
//! with members nests every declaration listed after it inside it under
//! `members` (so they name no `parent`/`parentLine`). Each member retains
//! its own kind and export status. A member whose parent is not in the rows
//! stays top level with `parent` (+ `parentLine` when the name is
//! ambiguous). Pages count top-level rows: [`top_level_spans`] splits flat
//! rows into one span per top-level row, so a page never cuts a member from
//! its parent.

use serde_json::Value;

// Private identities survive paging and filtering without entering the public outline.
pub const DECLARATION_INDEX: &str = "_declarationIndex";
pub const PARENT_INDEX: &str = "_parentIndex";

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

/// Public outline rows from flat internal ones (`name`, `character`,
/// `parent`, `parentLine`): `symbolName` and `column`, keys in one order,
/// each member nested in its parent's `members` (see the module docs).
pub fn declaration_rows(rows: &[Value]) -> Vec<Value> {
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
    let mut nested = nest_members(public);
    fn remove_identity(rows: &mut [Value]) {
        for row in rows {
            if let Some(fields) = row.as_object_mut() {
                fields.shift_remove(DECLARATION_INDEX);
                fields.shift_remove(PARENT_INDEX);
                if let Some(Value::Array(members)) = fields.get_mut("members") {
                    remove_identity(members);
                }
            }
        }
    }
    remove_identity(&mut nested);
    nested
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
        let parent_index = row.get(PARENT_INDEX).and_then(Value::as_u64);
        let names = |candidate: usize| {
            let candidate = &rows[candidate];
            if let Some(parent_index) = parent_index {
                return candidate.get(DECLARATION_INDEX).and_then(Value::as_u64)
                    == Some(parent_index);
            }
            parent.is_some_and(|parent| name(candidate).as_deref() == Some(parent))
                && parent_line.is_none_or(|expected| candidate["line"].as_u64() == Some(expected))
        };
        match stack.iter().rposition(|&ancestor| names(ancestor)) {
            Some(position) => {
                stack.truncate(position + 1);
                enclosing.push(Some(stack[position]));
            }
            // A member declared inside a sibling's span (a Python attribute
            // that `__init__` assigns, a member of its class) took that
            // sibling off the chain: the parent is still a row of this
            // tree, so the chain is rebuilt from it.
            None if !stack.is_empty()
                && let Some(found) = (stack[0]..index).rev().find(|&row| names(row)) =>
            {
                stack.clear();
                let mut ancestor = Some(found);
                while let Some(row) = ancestor {
                    stack.push(row);
                    ancestor = enclosing[row];
                }
                stack.reverse();
                enclosing.push(Some(found));
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
pub fn top_level_spans(rows: &[Value]) -> Vec<std::ops::Range<usize>> {
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
    let children: Vec<Value> = members[index]
        .iter()
        .map(|&child| build_node(child, slots, members))
        .collect();
    let Some(fields) = node.as_object_mut() else {
        return node;
    };
    if children.is_empty() {
        return node;
    }
    fields.insert("members".into(), Value::Array(children));
    node
}

/// Flat declaration objects for outline rows: each member following its
/// parent with `parent` and `parentLine` restored. The inverse of [`declaration_rows`].
#[cfg(any(test, feature = "test-support"))]
pub fn flatten_members(rows: &[Value]) -> Vec<Value> {
    fn walk(rows: &[Value], parent: Option<(&Value, &Value)>, out: &mut Vec<Value>) {
        for row in rows {
            let Some(fields) = row.as_object() else {
                out.push(row.clone());
                continue;
            };
            let mut flat = fields.clone();
            let members = flat.shift_remove("members");
            if let Some((name, line)) = parent {
                flat.insert("parent".into(), name.clone());
                flat.insert("parentLine".into(), line.clone());
            }
            out.push(Value::Object(flat));
            let Some(Value::Array(members)) = members else {
                continue;
            };
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
