//! Text output for a terminal or a pipe. A single-row `localSearch`,
//! `astSearch`, `lspSearch` or `structureSearch files` result prints its
//! standard line format on stdout, as rg, ast-grep and fd do:
//! `path:line:text` hits, or with context `rg --heading` blocks (the path,
//! then `line:text` hits and `line-text` context, `--` between groups),
//! `path:line: kind name` declarations, one path per line, `path:line`, or
//! `path:count`. Every other field of
//! the result goes to stderr as `# key: value` notes, so stdout pipes cleanly
//! and no field is dropped. Every other result, and each row of a batch
//! without a line format, prints as indented `key: value` text ([`outline`]).
//!
//! Follow-up calls (`next.*`, call hints, `responsePagination.next`, a
//! repair) print as `octocode <tool> '<json>'` commands. They are read from
//! the structured result only, never from rendered text, so fetched content
//! that looks like a call is never rewritten.
mod follow;
mod hits;
mod listing;
mod outline;
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

/// The text of a result: the line format of a single-row `localSearch`,
/// `astSearch`, `lspSearch` (sites, locations, call rows, symbols) or
/// `structureSearch files` result, `key: value` text for any other, each row
/// of a batch under its own heading; `None` only when the result does not
/// match its tool's output schema, which prints its JSON. `cwd` is the
/// caller's directory: a root equal to it needs no note.
pub(super) fn render(
    tool: ToolId,
    structured: &Value,
    query: &Value,
    cwd: Option<&std::path::Path>,
) -> Option<Lines> {
    let (display, calls) = follow::separate_calls(tool, structured)?;
    let mut lines = match display.get("results").and_then(Value::as_array) {
        Some(rows) if rows.len() == 1 => render_data(tool, &display, query, cwd)
            .unwrap_or_else(|| outline::generic(&display, query, cwd)),
        Some(_) => outline::batch(tool, &display, query, cwd),
        None => return None,
    };
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
    // documentSymbols`): the row names its path only when the tool changed it.
    if let (Some(Value::Array(symbols)), Some(path)) = (
        data.get("symbols"),
        data.get("path")
            .or_else(|| query["queries"].get(0)?.get("path"))
            .and_then(Value::as_str),
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
        let terms = term_counts(&rest, query);
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
            (None, Some((key, n)), None) if rest.len() == 1 + usize::from(terms.is_some()) => {
                rest.shift_remove(key);
                rest.shift_remove("termCounts");
                out.push(match &terms {
                    Some(terms) => format!("{path}:{n}  [{terms}]"),
                    None => format!("{path}:{n}"),
                });
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

/// A count row's `termCounts` as `term count` pairs in `matchString` order
/// (`planChunks 0 readGraph 10`), or `termCounts 0,10` when the query does
/// not name the terms (a continuation).
fn term_counts(rest: &Map<String, Value>, query: &Value) -> Option<String> {
    let counts = rest.get("termCounts")?.as_array()?.clone();
    let terms = query["queries"][0]["matchString"]
        .as_array()
        .filter(|terms| terms.len() == counts.len());
    Some(match terms {
        Some(terms) => terms
            .iter()
            .zip(&counts)
            .map(|(term, count)| format!("{} {count}", term.as_str().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join(" "),
        None => words(&Map::from_iter([(
            "termCounts".to_owned(),
            Value::Array(counts),
        )])),
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
    // Row fields beside `data` (`rowPart`, …), so a cut response is never
    // silent.
    let row_rest: Map<String, Value> = row
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "index" | "status" | "data"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    notes.extend(field_notes("", &row_rest));
    notes.extend(envelope_notes(structured, cwd));
    notes
        .iter()
        .map(|line| format!("# {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The envelope's fields beside `results` (`responsePagination`,
/// `warnings`, …), so a cut response is never silent, and the root unless
/// it is the working directory.
fn envelope_notes(structured: &Value, cwd: Option<&std::path::Path>) -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(envelope) = structured.as_object() {
        let rest: Map<String, Value> = envelope
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "results" | "root"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        notes.extend(field_notes("", &rest));
    }
    if let Some(root) = structured.get("root").and_then(Value::as_str)
        && !is_working_directory(root, cwd)
    {
        notes.push(format!("root: {root} (paths are relative to it)"));
    }
    notes
}

#[cfg(test)]
mod tests;
