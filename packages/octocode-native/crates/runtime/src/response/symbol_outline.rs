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
//! its parent. [`flatten_members`] restores the flat rows. The YAML text
//! channel prints the same rows as a compact outline, one line per
//! declaration:
//!
//! ```text
//! 500-506 interface CompilerHostLikeForCache +
//!   501 method fileExists
//! ```
//!
//! A row is `line[-endLine] kind name`; consecutive childless siblings of
//! one kind share a row (`; <range> <name>` each), and adjacent blocks of one
//! name (a type's `impl` blocks) share a row listing every range, with all
//! their members under it. Indentation encodes `parent` and
//! `parentLine` whenever the parent row precedes its member on the page;
//! otherwise the member carries `(in Parent@line)`. Optional facts follow as
//! suffixes: `+` (exported), `as a,b` (exportedAs), `doc` (a doc block ends
//! on the line above; `doc@N` names its first line otherwise), `from@N`
//! (startLine), `col N` (character). The text channel prints the rows under
//! one `=== symbols path (legend) ===` header per outline.

use serde_json::Value;

const LEGEND: &str = "line[-endLine] kind name, + exported, indented = member, doc = comment above, \"; \" joins same-kind siblings, a-b,c-d = one name's blocks";

/// Outline rows: declaration objects carrying `symbolName`, `kind` and
/// `line`.
fn is_declarations(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|rows| {
        !rows.is_empty()
            && rows.iter().all(|row| {
                row.get("symbolName").is_some()
                    && row.get("kind").is_some()
                    && row.get("line").is_some()
            })
    })
}

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

/// Move each symbols row's declarations out of `response` into outline
/// sections, in row order. `label` prefixes sections of a batch (`[i] `).
/// Rows are recognized by their declaration arrays (the response stage drops
/// echoed fields such as `operation`), and envelope `shared` scalars hoisted
/// out of object declaration rows are restored before rendering.
pub(crate) fn take_outlines(response: &mut Value) -> Vec<String> {
    let shared = response
        .get("shared")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let rows = response
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .map(|rows| rows.as_mut_slice())
        .unwrap_or_default();
    let labelled = rows.len() > 1;
    let mut sections = Vec::new();
    for row in rows {
        let label = if labelled {
            format!("[{}] ", row["index"])
        } else {
            String::new()
        };
        let path = row["data"]
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let data = &mut row["data"];
        if is_declarations(data.get("symbols"))
            && let Some(mut rows) = data
                .as_object_mut()
                .and_then(|fields| fields.remove("symbols"))
        {
            for declaration in rows.as_array_mut().into_iter().flatten() {
                if let Some(fields) = declaration.as_object_mut() {
                    for (key, value) in &shared {
                        fields.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
            }
            let legend = sections.is_empty();
            sections.push(section(
                &label,
                &path,
                rows.as_array().map_or(&[], Vec::as_slice),
                legend,
            ));
        }
        let grouped = data
            .get("files")
            .and_then(Value::as_array)
            .is_some_and(|files| {
                !files.is_empty()
                    && files
                        .iter()
                        .all(|file| is_declarations(file.get("symbols")))
            });
        if !grouped {
            continue;
        }
        let Some(files) = data
            .as_object_mut()
            .and_then(|fields| fields.remove("files"))
        else {
            continue;
        };
        for file in files.as_array().map_or(&[][..], Vec::as_slice) {
            let file_path = file["path"].as_str().unwrap_or_default();
            let declarations = file["symbols"].as_array().map_or(&[][..], Vec::as_slice);
            let legend = sections.is_empty();
            sections.push(section(&label, file_path, declarations, legend));
        }
    }
    sections
}

/// One file's outline section; the first section of a response states the
/// row legend, later ones only their path.
fn section(label: &str, path: &str, rows: &[Value], legend: bool) -> String {
    let mut text = if legend {
        let separator = if path.is_empty() { "" } else { " " };
        format!("=== symbols {label}{path}{separator}({LEGEND}) ===\n")
    } else {
        format!("=== symbols {label}{path} ===\n")
    };
    for row in outline_rows(&flatten_members(rows)) {
        text.push_str(row.as_str().unwrap_or_default());
        text.push('\n');
    }
    text
}

/// One outline entry per declaration: its nesting depth among the rows, its
/// line range(s), kind, and the name with its suffixes.
#[derive(Clone)]
struct Entry {
    depth: usize,
    ranges: Vec<String>,
    kind: String,
    label: String,
    /// The member names a parent outside the rows: `(in Parent@line)`.
    detached: bool,
}

/// An entry with the entries nested under it.
struct Node {
    entry: Entry,
    children: Vec<Node>,
}

/// Outline strings for declaration objects, in order. Nesting is computed
/// over `rows` alone, so a member whose parent is not among them names it.
///
/// Rows are grouped losslessly: adjacent siblings with the same kind, name
/// and suffixes (a Rust type's several `impl` blocks) share one row listing
/// each range (`109-891,893-901 impl Server`) with all their members under
/// it, and consecutive childless siblings of one kind share a row, each
/// further one as `; <range> <name>` (`130-136 function a; 159-162 b`).
pub(crate) fn outline_rows(rows: &[Value]) -> Vec<Value> {
    let entries = outline_entries(rows);
    let mut index = 0;
    let forest = merge_blocks(nest(&entries, &mut index, 0));
    let mut out = Vec::with_capacity(entries.len());
    emit(&forest, &mut out);
    out
}

fn outline_entries(rows: &[Value]) -> Vec<Entry> {
    // Ancestors among `rows`: (name, line).
    let mut stack: Vec<(String, u64)> = Vec::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let name = row["symbolName"]
            .as_str()
            .or_else(|| row["name"].as_str())
            .unwrap_or_default();
        let line = row["line"].as_u64().unwrap_or(0);
        let parent = row.get("parent").and_then(Value::as_str);
        let parent_line = row.get("parentLine").and_then(Value::as_u64);
        let mut detached = None;
        match parent {
            None => stack.clear(),
            Some(parent) => {
                let position = stack.iter().rposition(|(ancestor, at)| {
                    ancestor == parent && parent_line.is_none_or(|expected| expected == *at)
                });
                match position {
                    Some(index) => stack.truncate(index + 1),
                    None => {
                        stack.clear();
                        detached = Some(match parent_line {
                            Some(at) => format!(" (in {parent}@{at})"),
                            None => format!(" (in {parent})"),
                        });
                    }
                }
            }
        }
        let mut range = line.to_string();
        if let Some(end) = row.get("endLine").and_then(Value::as_u64) {
            range.push_str(&format!("-{end}"));
        }
        let mut label = name.to_owned();
        if row.get("exported").and_then(Value::as_bool) == Some(true) {
            label.push_str(" +");
        }
        if let Some(public) = row.get("exportedAs").and_then(Value::as_array) {
            let names = public
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(",");
            label.push_str(&format!(" as {names}"));
        }
        if let Some(doc) = row.get("docStartLine").and_then(Value::as_u64) {
            // A doc block ending right above the declaration is the norm.
            if doc + 1 == line {
                label.push_str(" doc");
            } else {
                label.push_str(&format!(" doc@{doc}"));
            }
        }
        if let Some(start) = row.get("startLine").and_then(Value::as_u64) {
            label.push_str(&format!(" from@{start}"));
        }
        if let Some(column) = row
            .get("column")
            .or_else(|| row.get("character"))
            .and_then(Value::as_u64)
        {
            label.push_str(&format!(" col {column}"));
        }
        let is_detached = detached.is_some();
        if let Some(detached) = detached {
            label.push_str(&detached);
        }
        out.push(Entry {
            depth: stack.len(),
            ranges: vec![range],
            kind: row["kind"].as_str().unwrap_or_default().to_owned(),
            label,
            detached: is_detached,
        });
        stack.push((name.to_owned(), line));
    }
    out
}

/// The entries from `index` on at `depth`, each with its deeper entries.
fn nest(entries: &[Entry], index: &mut usize, depth: usize) -> Vec<Node> {
    let mut nodes: Vec<Node> = Vec::new();
    while let Some(entry) = entries.get(*index) {
        if entry.depth < depth {
            break;
        }
        if entry.depth > depth {
            let children = nest(entries, index, depth + 1);
            match nodes.last_mut() {
                Some(last) => last.children.extend(children),
                None => nodes.extend(children),
            }
            continue;
        }
        *index += 1;
        nodes.push(Node {
            entry: entry.clone(),
            children: Vec::new(),
        });
    }
    nodes
}

/// Merge adjacent siblings that declare the same kind and name with the
/// same suffixes into one node listing every range.
fn merge_blocks(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        node.children = merge_blocks(std::mem::take(&mut node.children));
        if let Some(last) = out.last_mut()
            && !node.entry.detached
            && !last.entry.detached
            && last.entry.kind == node.entry.kind
            && last.entry.label == node.entry.label
        {
            last.entry.ranges.extend(node.entry.ranges);
            last.children.extend(node.children);
            continue;
        }
        out.push(node);
    }
    out
}

fn emit(nodes: &[Node], out: &mut Vec<Value>) {
    let mut row: Option<(String, String)> = None;
    for node in nodes {
        let entry = &node.entry;
        let ranges = entry.ranges.join(",");
        let groupable = node.children.is_empty() && !entry.detached;
        if groupable
            && let Some((kind, text)) = row.as_mut()
            && *kind == entry.kind
        {
            text.push_str(&format!("; {ranges} {}", entry.label));
            continue;
        }
        if let Some((_, text)) = row.take() {
            out.push(Value::String(text));
        }
        let text = format!(
            "{}{ranges} {} {}",
            "  ".repeat(entry.depth),
            entry.kind,
            entry.label
        );
        if groupable {
            row = Some((entry.kind.clone(), text));
        } else {
            out.push(Value::String(text));
            emit(&node.children, out);
        }
    }
    if let Some((_, text)) = row {
        out.push(Value::String(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outline_indents_members_and_keeps_every_row_fact() {
        let mut response = json!({"results":[{"index":0,"data":{
        "operation":"symbols","path":"a.ts","pagination":{"totalItems":5},
        "symbols":[
            {"symbolName":"Host","kind":"interface","line":500,"endLine":506,"exported":true,"docStartLine":498},
            {"symbolName":"fileExists","kind":"method","line":501,"parent":"Host","docStartLine":500},
            {"symbolName":"inner","kind":"function","line":502,"endLine":503,"parent":"fileExists"},
            {"symbolName":"writeFile","kind":"property","line":505,"parent":"Host"},
            {"symbolName":"orphan","kind":"method","line":600,"parent":"Gone","parentLine":7,"column":4},
            {"symbolName":"main","kind":"function","line":700,"startLine":699,"exported":true,"exportedAs":["default"]}
        ]}}]});
        let sections = take_outlines(&mut response);
        assert!(response["results"][0]["data"].get("symbols").is_none());
        assert_eq!(
            response["results"][0]["data"]["pagination"]["totalItems"],
            5
        );
        assert_eq!(
            sections,
            vec![format!(
                "=== symbols a.ts ({LEGEND}) ===\n\
                 500-506 interface Host + doc@498\n\
                 \x20 501 method fileExists doc\n\
                 \x20   502-503 function inner\n\
                 \x20 505 property writeFile\n\
                 600 method orphan col 4 (in Gone@7)\n\
                 700 function main + as default from@699\n"
            )]
        );
    }

    /// Same-kind leaf siblings share a row and a name's adjacent blocks share
    /// one parent row; every declaration's range, kind, name and suffixes
    /// stay readable.
    #[test]
    fn outline_rows_group_siblings_and_merge_blocks_losslessly() {
        let objects = [
            json!({"name":"Server","kind":"struct","line":105,"endLine":107,"exported":true}),
            json!({"name":"Server","kind":"impl","line":109,"endLine":891}),
            json!({"name":"connect","kind":"function","line":194,"endLine":204,"exported":true,"docStartLine":164}),
            json!({"name":"ready","kind":"function","line":307,"endLine":310,"exported":true,"docStartLine":306}),
            json!({"name":"MAX","kind":"constant","line":320,"parent":"Server"}),
            json!({"name":"Server","kind":"impl","line":893,"endLine":901}),
            json!({"name":"poll_read","kind":"function","line":894,"endLine":900,"parent":"Server"}),
            json!({"name":"Client","kind":"impl","line":984,"endLine":990}),
            json!({"name":"open","kind":"function","line":985,"endLine":989,"parent":"Client"}),
            json!({"name":"orphan","kind":"function","line":2000,"parent":"Gone","parentLine":7}),
            json!({"name":"lone","kind":"function","line":2001}),
        ];
        let mut objects = objects.to_vec();
        for object in &mut objects[2..4] {
            object["parent"] = json!("Server");
        }
        assert_eq!(
            outline_rows(&objects),
            [
                json!("105-107 struct Server +"),
                json!("109-891,893-901 impl Server"),
                json!("  194-204 function connect + doc@164; 307-310 ready + doc"),
                json!("  320 constant MAX"),
                json!("  894-900 function poll_read"),
                json!("984-990 impl Client"),
                json!("  985-989 function open"),
                json!("2000 function orphan (in Gone@7)"),
                json!("2001 function lone"),
            ]
        );
    }

    #[test]
    fn outline_rows_are_the_rendered_lines_and_render_verbatim() {
        let objects = [
            json!({"name":"Host","kind":"interface","line":500,"endLine":506,"exported":true,"docStartLine":498}),
            json!({"name":"fileExists","kind":"method","line":501,"parent":"Host","docStartLine":500}),
            json!({"name":"orphan","kind":"method","line":600,"parent":"Gone","parentLine":7}),
        ];
        let rows = outline_rows(&objects);
        assert_eq!(
            rows,
            [
                json!("500-506 interface Host + doc@498"),
                json!("  501 method fileExists doc"),
                json!("600 method orphan (in Gone@7)")
            ]
        );
        // Structured rows are declaration objects; the text channel prints
        // them as these outline lines under the header.
        let public = declaration_rows(&objects);
        assert_eq!(public[0]["symbolName"], "Host", "{public:?}");
        assert_eq!(
            public[1],
            json!({"symbolName":"orphan","kind":"method","line":600,"parent":"Gone","parentLine":7})
        );
        let mut response = json!({"results":[{"index":0,"data":{"path":"a.ts","symbols":public}}]});
        assert_eq!(
            take_outlines(&mut response),
            vec![format!(
                "=== symbols a.ts ({LEGEND}) ===\n\
                 500-506 interface Host + doc@498\n\
                 \x20 501 method fileExists doc\n\
                 600 method orphan (in Gone@7)\n"
            )]
        );
        assert!(response["results"][0]["data"].get("symbols").is_none());
    }

    /// Members nest under the parent row they follow: no member repeats
    /// `parent`/`parentLine`, a kind or `exported` every member shares is
    /// stated once, a member whose parent is off the page stays top level
    /// naming it, and a declaration without members is a plain object.
    /// Flattening restores every flat fact.
    #[test]
    fn declaration_rows_nest_members_under_their_parent() {
        let flat = [
            json!({"name":"Server","kind":"impl","line":109,"endLine":891}),
            json!({"name":"connect","kind":"method","line":194,"endLine":204,"exported":true,"docStartLine":164,"parent":"Server","parentLine":109}),
            json!({"name":"ready","kind":"method","line":307,"endLine":310,"exported":true,"parent":"Server","parentLine":109}),
            json!({"name":"inner","kind":"function","line":308,"parent":"ready"}),
            json!({"name":"Client","kind":"impl","line":984,"endLine":990}),
            json!({"name":"open","kind":"method","line":985,"endLine":989,"parent":"Client"}),
            json!({"name":"orphan","kind":"method","line":2000,"parent":"Gone","parentLine":7}),
        ];
        let rows = declaration_rows(&flat);
        assert_eq!(
            json!(rows),
            json!([
                {"symbolName":"Server","kind":"impl","line":109,"endLine":891,
                 "shared":{"kind":"method","exported":true},
                 "members":[
                    {"symbolName":"connect","line":194,"endLine":204,"docStartLine":164},
                    {"symbolName":"ready","line":307,"endLine":310,
                     "members":[{"symbolName":"inner","kind":"function","line":308}]}
                 ]},
                {"symbolName":"Client","kind":"impl","line":984,"endLine":990,
                 "members":[{"symbolName":"open","kind":"method","line":985,"endLine":989}]},
                {"symbolName":"orphan","kind":"method","line":2000,"parent":"Gone","parentLine":7}
            ])
        );
        let restored = flatten_members(&rows);
        assert_eq!(restored.len(), flat.len());
        for (row, original) in restored.iter().zip(&flat) {
            assert_eq!(row["symbolName"], original["name"], "{row}");
            for key in [
                "kind",
                "line",
                "endLine",
                "exported",
                "docStartLine",
                "parent",
            ] {
                assert_eq!(row.get(key), original.get(key), "{key}: {row}");
            }
        }
        // Same text outline as the flat rows.
        assert_eq!(outline_rows(&restored), outline_rows(&flat), "{restored:?}");
    }

    /// Byte budget for a large Rust file outline (tokio named_pipe.rs: two
    /// types, many impl blocks of exported, documented methods): the nested
    /// rows cost at most 65% of the flat ones and lose no declaration.
    #[test]
    fn nested_symbols_fit_the_byte_budget_of_a_large_outline() {
        let mut flat = Vec::new();
        for (type_index, name) in ["NamedPipeServer", "NamedPipeClient"].iter().enumerate() {
            let base = 100 + type_index as u64 * 900;
            flat.push(json!({"name":name,"kind":"struct","line":base,"endLine":base + 2,"exported":true,"docStartLine":base - 60}));
            flat.push(json!({"name":name,"kind":"impl","line":base + 4,"endLine":base + 790}));
            for method in 0..20u64 {
                let line = base + 25 + method * 35;
                flat.push(json!({"name":format!("method_{method}"),"kind":"method","line":line,"endLine":line + 4,
                    "exported":true,"docStartLine":line - 20,"parent":name,"parentLine":base + 4}));
            }
            for block in 0..5u64 {
                let line = base + 800 + block * 10;
                flat.push(json!({"name":name,"kind":"impl","line":line,"endLine":line + 8}));
                flat.push(json!({"name":format!("poll_{block}"),"kind":"method","line":line + 1,"endLine":line + 7,
                    "parent":name,"parentLine":line}));
            }
        }
        let bytes = |rows: &[Value]| serde_json::to_string(rows).expect("json").len();
        let before = {
            // The flat public rows (one object per declaration, parent named).
            let mut rows = Vec::new();
            for row in &flat {
                let mut row = row.clone();
                let name = row.as_object_mut().and_then(|f| f.shift_remove("name"));
                row["symbolName"] = name.unwrap_or_default();
                rows.push(row);
            }
            bytes(&rows)
        };
        let nested = declaration_rows(&flat);
        let after = bytes(&nested);
        assert!(
            after * 100 <= before * 65,
            "nested {after} B vs flat {before} B"
        );
        assert_eq!(flatten_members(&nested).len(), flat.len());
    }

    #[test]
    fn rows_without_operation_render_with_shared_fields_restored() {
        let mut response = json!({"shared":{"parentLine":125},"results":[{"index":0,"data":{
        "symbols":[
            {"symbolName":"get","kind":"function","line":277,"parent":"OnceCell"},
            {"symbolName":"set","kind":"function","line":310,"parent":"OnceCell"}
        ]}}]});
        let sections = take_outlines(&mut response);
        assert_eq!(
            sections,
            vec![format!(
                "=== symbols ({LEGEND}) ===\n\
                 277 function get (in OnceCell@125)\n\
                 310 function set (in OnceCell@125)\n"
            )]
        );
        // Match rows are not declarations.
        let mut matches = json!({"results":[{"index":0,"data":{"files":[{"path":"a.rs","matches":[{"line":1,"value":"x","column":0}]}]}}]});
        assert!(take_outlines(&mut matches).is_empty());
        assert!(matches["results"][0]["data"]["files"].is_array());
    }

    #[test]
    fn directory_outlines_one_section_per_file_and_label_batches() {
        let mut response = json!({"results":[
            {"index":0,"data":{"operation":"symbols","path":"src","files":[
                {"path":"src/a.rs","symbols":[{"symbolName":"a","kind":"function","line":1}]},
                {"path":"src/b.rs","symbols":[{"symbolName":"b","kind":"struct","line":3,"endLine":5}]}
            ]}},
            {"index":1,"data":{"operation":"match","files":[{"path":"x.rs","matches":[]}]}}
        ]});
        let sections = take_outlines(&mut response);
        assert_eq!(sections.len(), 2);
        assert!(
            sections[0].starts_with("=== symbols [0] src/a.rs ("),
            "{sections:?}"
        );
        assert!(sections[1].ends_with("3-5 struct b\n"), "{sections:?}");
        assert!(response["results"][0]["data"].get("files").is_none());
        // Other operations keep their rows.
        assert!(response["results"][1]["data"]["files"].is_array());
    }

    /// Canonical facts of flat internal rows, as [`flatten_members`]
    /// restores them: public key names, keys sorted.
    fn facts(rows: &[Value]) -> Vec<Value> {
        rows.iter()
            .map(|row| {
                let mut fields: Vec<(String, Value)> = row
                    .as_object()
                    .expect("row")
                    .iter()
                    .map(|(key, value)| {
                        let key = match key.as_str() {
                            "name" => "symbolName",
                            "character" => "column",
                            other => other,
                        };
                        (key.to_owned(), value.clone())
                    })
                    .collect();
                fields.sort_by(|a, b| a.0.cmp(&b.0));
                Value::Object(fields.into_iter().collect())
            })
            .collect()
    }

    /// Every outline row and every nested member is an object, whatever
    /// facts it carries: a member-less declaration is never a string.
    #[test]
    fn every_row_and_member_is_an_object() {
        fn all_objects(rows: &[Value]) -> bool {
            rows.iter().all(|row| {
                row.is_object()
                    && row
                        .get("members")
                        .and_then(Value::as_array)
                        .is_none_or(|members| all_objects(members))
            })
        }
        let flat = [
            json!({"name":"lone","kind":"function","line":1}),
            json!({"name":"Host","kind":"class","line":3,"endLine":9}),
            json!({"name":"get","kind":"method","line":4,"parent":"Host"}),
            json!({"name":"orphan","kind":"method","line":20,"parent":"Gone"}),
        ];
        let rows = declaration_rows(&flat);
        assert!(all_objects(&rows), "{rows:?}");
        assert_eq!(
            rows[0],
            json!({"symbolName":"lone","kind":"function","line":1})
        );
        assert!(!is_declarations(Some(&json!(["a (3-5, function)"]))));
        assert!(is_declarations(Some(
            &json!([{"symbolName":"a","kind":"function","line":3}])
        )));
    }

    /// One span per top-level row: a container's span holds its members,
    /// an orphan (its parent filtered out) is its own row, and a new file
    /// starts a new row even when a name repeats across files.
    #[test]
    fn top_level_spans_cover_each_tree_once_and_split_files() {
        let flat = [
            json!({"name":"Host","kind":"class","line":3,"endLine":9,"path":"a.ts"}),
            json!({"name":"get","kind":"method","line":4,"parent":"Host","path":"a.ts"}),
            json!({"name":"inner","kind":"function","line":5,"parent":"get","path":"a.ts"}),
            json!({"name":"orphan","kind":"method","line":20,"parent":"Gone","path":"a.ts"}),
            json!({"name":"Host","kind":"class","line":1,"endLine":4,"path":"a.ts"}),
            json!({"name":"set","kind":"method","line":2,"parent":"Host","path":"b.ts"}),
        ];
        assert_eq!(top_level_spans(&flat), vec![0..3, 3..4, 4..5, 5..6]);
        // The spans tile the rows, so pages over spans reach each row once.
        let without_paths: Vec<Value> = flat[..4].to_vec();
        let spans = top_level_spans(&without_paths);
        assert_eq!(spans.len(), declaration_rows(&without_paths).len());
        assert!(top_level_spans(&[]).is_empty());
    }

    /// Round trip: every object row flattens back to its declaration's
    /// facts, in source order, for awkward names and values, also after a
    /// JSON round trip (the wire).
    #[test]
    fn object_rows_round_trip_every_fact_including_awkward_names() {
        let flat = [
            json!({"name":"operator()","kind":"method","line":3,"endLine":5,"exported":true}),
            json!({"name":"From<(A, B)> for X","kind":"impl","line":7,"endLine":20,"exported":true}),
            json!({"name":"from","kind":"function","line":8,"endLine":10,"docStartLine":7,"exported":true,
                "parent":"From<(A, B)> for X","parentLine":7}),
            json!({"name":"into (x)","kind":"function","line":11,"exported":true,"exportedAs":["a","b c","d (e)"],
                "character":4,"parent":"From<(A, B)> for X","parentLine":7}),
            json!({"name":"orphan","kind":"method","line":30,"parent":"Outer, Inc","parentLine":2}),
            json!({"name":"slice","kind":"method","line":33,"parent":"Foo for [T]","startLine":32}),
            json!({"name":"a, b","kind":"field","line":35}),
        ];
        let rows = declaration_rows(&flat);
        assert_eq!(
            rows[1]["shared"],
            json!({"kind":"function","exported":true})
        );
        assert_eq!(
            rows[1]["members"][1],
            json!({"symbolName":"into (x)","line":11,"exportedAs":["a","b c","d (e)"],"column":4})
        );
        assert_eq!(facts(&flatten_members(&rows)), facts(&flat));
        let wire: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&rows).expect("json")).expect("parse");
        assert_eq!(facts(&flatten_members(&wire)), facts(&flat));
    }
}
