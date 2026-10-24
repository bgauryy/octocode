//! Text output for a terminal or a pipe. A single-row `localSearch`,
//! `astSearch`, `lspSearch` or `structureSearch files` result prints its
//! standard line format on stdout, as rg, ast-grep and fd do:
//! `path:line:text` hits, or with context `rg --heading` blocks (the path,
//! then `line:text` hits and `line-text` context, `--` between groups),
//! `path:line: kind name` declarations, one path per line, `path:line`, or
//! `path:count`. Every other field of
//! the result goes to stderr as `# key: value` notes, so stdout pipes cleanly
//! and no field is dropped. Every other result prints its JSON, as `--json`.
//!
//! Follow-up calls (`next.*`, call hints, `responsePagination.next`, a
//! repair) print as `octocode <tool> '<json>'` commands. They are read from
//! the structured result only, never from rendered text, so fetched content
//! that looks like a call is never rewritten.
mod follow;
mod hits;
mod listing;
pub(super) use follow::all_rows_failed;
use follow::field_notes;
use hits::{hits, value_lines};
use listing::{declarations, is_working_directory, listing, locations, single_row, words};
use octocode_native::id::ToolId;
use serde_json::{Map, Value};

/// A rendered result: data lines for stdout and notes for stderr.
pub(super) struct Lines {
    pub out: String,
    pub notes: String,
}

/// Explicit context determines whether hit values carry source-line prefixes.
fn context_requested(query: &Value) -> bool {
    let context = &query["queries"][0]["contextLines"];
    ["before", "after"]
        .into_iter()
        .any(|key| context[key].as_u64().is_some_and(|n| n > 0))
}

/// The line format of a single-row `localSearch`, `astSearch`, `lspSearch`
/// (sites, locations, call rows, symbols) or `structureSearch files` result;
/// `None` for any other result, which prints its JSON. `cwd` is the
/// caller's directory: a root equal to it needs no note.
pub(super) fn render(
    tool: ToolId,
    structured: &Value,
    query: &Value,
    cwd: Option<&std::path::Path>,
) -> Option<Lines> {
    let (display, calls) = follow::separate_calls(tool, structured)?;
    let mut lines = render_data(tool, &display, query, cwd)?;
    for call in calls {
        if !lines.notes.is_empty() {
            lines.notes.push('\n');
        }
        lines.notes.push_str(&format!("# {call}"));
    }
    Some(lines)
}

fn render_data(
    tool: ToolId,
    structured: &Value,
    query: &Value,
    cwd: Option<&std::path::Path>,
) -> Option<Lines> {
    match tool {
        ToolId::StructureSearch => return listing(structured, query, cwd),
        ToolId::LocalSearch | ToolId::AstSearch | ToolId::LspSearch => {}
        _ => return None,
    }
    if tool == ToolId::LocalSearch && query.get("cursor").is_some() {
        return None;
    }
    let row = single_row(structured)?;
    let data = row.get("data")?.as_object()?;
    if tool == ToolId::LspSearch
        && let Some(Value::Array(matches)) = data.get("matches")
    {
        let mut out = Vec::new();
        let mut file_notes = Vec::new();
        locations(matches, &mut out, &mut file_notes)?;
        let mut data = data.clone();
        data.shift_remove("matches");
        return Some(Lines {
            out: out.join("\n"),
            notes: notes(structured, query, row, data, file_notes, cwd),
        });
    }
    // A single-file outline (`astSearch symbols`, `lspSearch
    // documentSymbols`): the declarations sit on the row beside its path.
    if let (Some(Value::Array(symbols)), Some(path)) = (
        data.get("symbols"),
        data.get("path").and_then(Value::as_str),
    ) {
        let mut out = Vec::new();
        declarations(path, symbols, false, &mut out)?;
        let mut data = data.clone();
        data.shift_remove("symbols");
        return Some(Lines {
            out: out.join("\n"),
            notes: notes(structured, query, row, data, Vec::new(), cwd),
        });
    }
    let empty = row.get("status").and_then(Value::as_str) == Some("empty");
    let no_files = Vec::new();
    let files = match data.get("files") {
        Some(files) => files.as_array()?,
        // An empty result prints nothing on stdout, as rg does; its notes say why.
        None if empty => &no_files,
        None => return None,
    };
    let numbered = tool == ToolId::LocalSearch && context_requested(query);
    let mut out = Vec::new();
    let mut file_notes = Vec::new();
    for file in files {
        let file = file.as_object()?;
        let path = file.get("path")?.as_str()?;
        let mut rest = file.clone();
        rest.shift_remove("path");
        if let Some(Value::Array(symbols)) = rest.shift_remove("symbols") {
            declarations(path, &symbols, false, &mut out)?;
            if !rest.is_empty() {
                file_notes.push(format!("file {path}: {}", words(&rest)));
            }
            continue;
        }
        let matches = rest.shift_remove("matches");
        let count = ["matchCount", "matchedLineCount"]
            .into_iter()
            .find_map(|key| rest.get(key).and_then(Value::as_u64).map(|n| (key, n)));
        // `lspSearch view:files`: the start line of every site in the file.
        let site_lines = rest
            .get("lines")
            .filter(|_| tool == ToolId::LspSearch)
            .and_then(|lines| serde_json::from_value::<Vec<u64>>(lines.clone()).ok());
        match (matches, count, site_lines) {
            (Some(Value::Array(matches)), _, _)
                if matches.iter().all(|item| item.get("value").is_some()) =>
            {
                hits(path, &matches, numbered, &mut out)?;
            }
            // `lspSearch` call rows (`callers`, `callees`): the calling or
            // called declaration, its call sites in `[ … ]`.
            (Some(Value::Array(matches)), _, _) if tool == ToolId::LspSearch => {
                declarations(path, &matches, false, &mut out)?;
            }
            // A `path:N` line per site, as sites print without grouping; the
            // count is the number of lines.
            (None, count, Some(lines)) => {
                rest.shift_remove("lines");
                if count.is_some_and(|(key, n)| key == "matchCount" && n == lines.len() as u64) {
                    rest.shift_remove("matchCount");
                }
                out.extend(lines.iter().map(|n| format!("{path}:{n}")));
            }
            (None, Some((key, n)), None) if rest.len() == 1 => {
                rest.shift_remove(key);
                out.push(format!("{path}:{n}"));
            }
            (None, _, None) => out.push(path.to_owned()),
            _ => return None,
        }
        if !rest.is_empty() {
            file_notes.push(format!("file {path}: {}", words(&rest)));
        }
    }
    Some(Lines {
        out: out.join("\n"),
        notes: notes(structured, query, row, data.clone(), file_notes, cwd),
    })
}

/// The stderr notes of a single-row result: its status, the fields of
/// `data` beside the printed ones (less the searched path it echoes),
/// `file_notes`, the row and envelope fields, the root, and the follow-up
/// calls.
fn notes(
    structured: &Value,
    query: &Value,
    row: &Map<String, Value>,
    mut data: Map<String, Value>,
    file_notes: Vec<String>,
    cwd: Option<&std::path::Path>,
) -> String {
    let mut notes = Vec::new();
    // The searched path echoed back: every printed line already starts with it.
    let asked = query["queries"].get(0).unwrap_or(query);
    if data.get("path").is_some_and(|path| *path == asked["path"]) {
        data.shift_remove("path");
    }
    if let Some(status) = row.get("status").and_then(Value::as_str) {
        notes.push(format!("status: {status}"));
    }
    data.shift_remove("files");
    notes.extend(field_notes("", &data));
    notes.extend(file_notes);
    // Row and envelope fields beside `data` (`rowPart`, `responsePagination`,
    // `warnings`, …), so a cut response is never silent.
    let row_rest: Map<String, Value> = row
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "index" | "status" | "data"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    notes.extend(field_notes("", &row_rest));
    if let Some(envelope) = structured.as_object() {
        let rest: Map<String, Value> = envelope
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "results" | "root"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        notes.extend(field_notes("", &rest));
    }
    // Paths relative to the working directory need no root note.
    if let Some(root) = structured.get("root").and_then(Value::as_str)
        && !is_working_directory(root, cwd)
    {
        notes.push(format!("root: {root} (paths are relative to it)"));
    }
    notes
        .iter()
        .map(|line| format!("# {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests;
