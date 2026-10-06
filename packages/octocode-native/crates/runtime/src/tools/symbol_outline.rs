//! Compact outline rows for astSearch `symbols`.
//!
//! Repeating every key per declaration made an outline several times larger
//! than the source it summarizes, so both response channels carry one string
//! row per declaration, in page order:
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

/// Outline rows: strings, or declaration objects carrying `name`, `kind` and
/// `line` (the earlier row shape).
fn is_declarations(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|rows| {
        !rows.is_empty()
            && rows.iter().all(|row| {
                row.is_string()
                    || (row.get("name").is_some()
                        && row.get("kind").is_some()
                        && row.get("line").is_some())
            })
    })
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
        let data = &mut row["data"];
        let path = data
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
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
            sections.push(section(
                &label,
                &path,
                rows.as_array().map_or(&[], Vec::as_slice),
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
            sections.push(section(&label, file_path, declarations));
        }
    }
    sections
}

fn section(label: &str, path: &str, rows: &[Value]) -> String {
    let separator = if path.is_empty() { "" } else { " " };
    let mut text = format!("=== symbols {label}{path}{separator}({LEGEND}) ===\n");
    let objects = rows.iter().any(Value::is_object);
    let converted;
    let rows = if objects {
        converted = outline_rows(rows);
        converted.as_slice()
    } else {
        rows
    };
    for row in rows {
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
        let name = row["name"].as_str().unwrap_or_default();
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
        if let Some(column) = row.get("character").and_then(Value::as_u64) {
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
            {"name":"Host","kind":"interface","line":500,"endLine":506,"exported":true,"docStartLine":498},
            {"name":"fileExists","kind":"method","line":501,"parent":"Host","docStartLine":500},
            {"name":"inner","kind":"function","line":502,"endLine":503,"parent":"fileExists"},
            {"name":"writeFile","kind":"property","line":505,"parent":"Host"},
            {"name":"orphan","kind":"method","line":600,"parent":"Gone","parentLine":7,"character":4},
            {"name":"main","kind":"function","line":700,"startLine":699,"exported":true,"exportedAs":["default"]}
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
        // Structured rows are already the outline: the text channel prints
        // them under the header unchanged.
        let mut response = json!({"results":[{"index":0,"data":{"path":"a.ts","symbols":rows}}]});
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

    #[test]
    fn rows_without_operation_render_with_shared_fields_restored() {
        let mut response = json!({"shared":{"parentLine":125},"results":[{"index":0,"data":{
        "symbols":[
            {"name":"get","kind":"function","line":277,"parent":"OnceCell"},
            {"name":"set","kind":"function","line":310,"parent":"OnceCell"}
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
                {"path":"src/a.rs","symbols":[{"name":"a","kind":"function","line":1}]},
                {"path":"src/b.rs","symbols":[{"name":"b","kind":"struct","line":3,"endLine":5}]}
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
}
