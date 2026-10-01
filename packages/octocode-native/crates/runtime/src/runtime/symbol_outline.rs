//! Compact text outline for astSearch `symbols` rows.
//!
//! The YAML text channel spends ~115 bytes per declaration on repeated keys.
//! The outline prints one line per declaration after the row metadata:
//!
//! ```text
//! === symbols program.ts (line[-endLine] kind name; + exported; indented = member) ===
//! 500-506 interface CompilerHostLikeForCache +
//!   501 method fileExists
//! ```
//!
//! Indentation encodes `parent`/`parentLine` whenever the parent row precedes
//! its member on the page; otherwise the member carries `(in Parent@line)`.
//! Optional facts follow as suffixes: `as a,b` (exportedAs), `doc` (a doc
//! block ends on the line above; `doc@N` names its first line otherwise),
//! `from@N` (startLine), `col N` (character). Structured
//! content keeps every row unchanged.

use serde_json::Value;

const LEGEND: &str = "line[-endLine] kind name; + exported; indented = member; doc = comment above";

/// Declaration rows: objects carrying `name`, `kind` and `line`.
fn is_declarations(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|rows| {
        !rows.is_empty()
            && rows.iter().all(|row| {
                row.get("name").is_some() && row.get("kind").is_some() && row.get("line").is_some()
            })
    })
}

/// Move each symbols row's declarations out of `response` into outline
/// sections, in row order. `label` prefixes sections of a batch (`[i] `).
/// Rows are recognized by their declaration arrays (the response stage drops
/// echoed fields such as `operation`), and envelope `shared` scalars hoisted
/// out of top-level declaration rows are restored before rendering.
pub(super) fn take_outlines(response: &mut Value) -> Vec<String> {
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
        if is_declarations(data.get("declarations"))
            && let Some(mut rows) = data
                .as_object_mut()
                .and_then(|fields| fields.remove("declarations"))
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
                        .all(|file| is_declarations(file.get("declarations")))
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
            let declarations = file["declarations"]
                .as_array()
                .map_or(&[][..], Vec::as_slice);
            sections.push(section(&label, file_path, declarations));
        }
    }
    sections
}

fn section(label: &str, path: &str, rows: &[Value]) -> String {
    let separator = if path.is_empty() { "" } else { " " };
    let mut text = format!("=== symbols {label}{path}{separator}({LEGEND}) ===\n");
    // Ancestors on this page: (name, line).
    let mut stack: Vec<(String, u64)> = Vec::new();
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
        for _ in 0..stack.len() {
            text.push_str("  ");
        }
        text.push_str(&line.to_string());
        if let Some(end) = row.get("endLine").and_then(Value::as_u64) {
            text.push_str(&format!("-{end}"));
        }
        text.push(' ');
        text.push_str(row["kind"].as_str().unwrap_or_default());
        text.push(' ');
        text.push_str(name);
        if row.get("exported").and_then(Value::as_bool) == Some(true) {
            text.push_str(" +");
        }
        if let Some(public) = row.get("exportedAs").and_then(Value::as_array) {
            let names = public
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(",");
            text.push_str(&format!(" as {names}"));
        }
        if let Some(doc) = row.get("docStartLine").and_then(Value::as_u64) {
            // A doc block ending right above the declaration is the norm.
            if doc + 1 == line {
                text.push_str(" doc");
            } else {
                text.push_str(&format!(" doc@{doc}"));
            }
        }
        if let Some(start) = row.get("startLine").and_then(Value::as_u64) {
            text.push_str(&format!(" from@{start}"));
        }
        if let Some(column) = row.get("character").and_then(Value::as_u64) {
            text.push_str(&format!(" col {column}"));
        }
        if let Some(detached) = detached {
            text.push_str(&detached);
        }
        text.push('\n');
        stack.push((name.to_owned(), line));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outline_indents_members_and_keeps_every_row_fact() {
        let mut response = json!({"results":[{"index":0,"data":{
        "operation":"symbols","path":"a.ts","totalDeclarations":5,
        "declarations":[
            {"name":"Host","kind":"interface","line":500,"endLine":506,"exported":true,"docStartLine":498},
            {"name":"fileExists","kind":"method","line":501,"parent":"Host","docStartLine":500},
            {"name":"inner","kind":"function","line":502,"endLine":503,"parent":"fileExists"},
            {"name":"writeFile","kind":"property","line":505,"parent":"Host"},
            {"name":"orphan","kind":"method","line":600,"parent":"Gone","parentLine":7,"character":4},
            {"name":"main","kind":"function","line":700,"startLine":699,"exported":true,"exportedAs":["default"]}
        ]}}]});
        let sections = take_outlines(&mut response);
        assert!(response["results"][0]["data"].get("declarations").is_none());
        assert_eq!(response["results"][0]["data"]["totalDeclarations"], 5);
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

    #[test]
    fn rows_without_operation_render_with_shared_fields_restored() {
        let mut response = json!({"shared":{"parentLine":125},"results":[{"index":0,"data":{
        "declarations":[
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
                {"path":"src/a.rs","declarations":[{"name":"a","kind":"function","line":1}]},
                {"path":"src/b.rs","declarations":[{"name":"b","kind":"struct","line":3,"endLine":5}]}
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
