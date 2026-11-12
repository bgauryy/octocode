//! Text output for a terminal or a pipe. Stdout carries each row's evidence
//! and, after it, the facts that decide "am I done" and how to read the
//! answer; stderr carries the rest as `# key: value` notes. No field is
//! dropped.
//!
//! A row's content: `localSearch`, `astSearch` and `lspSearch` print the
//! line formats of rg and ast-grep, a file's rows under its path once
//! (`path`, then `line:text` hits and `line-text` context, `--` between
//! groups; `line: kind name` declarations), a lone row as `path:line:text` or
//! `path:line: kind name`, sites as `path:line,line`, and a file without rows
//! as its path. `path:N` is always a line. A file's other fields (counts, its
//! page) follow its path in `[ … ]`. A listing prints each `dir/` once with
//! its entries indented under it. Every other result prints as indented
//! `key: value` text ([`outline`]), a list of flat rows as a [`table`].
//!
//! The trailer follows the content, one `# ` line each, the same for every
//! tool: a `status` that is not ok, each page state (an object holding
//! `hasMore`) and incomplete mark, the row's `warnings` and `hints`, then the
//! commands that continue the answer: `responsePagination.next` first, then
//! `next.nextPage`, then any other. A call rejected before it ran prints as
//! a failed row does ([`rejected`]). A batch prints each row under `## results[N]` with its own trailer,
//! the envelope's after the last row, and names the row in its stderr notes
//! (`# results[N] key: value`).
//!
//! Counters and `related[]` leads are the stderr notes.
//! Calls print as `octocode <tool> '<json>'` commands. They are read from
//! the structured result only, never from rendered text, so fetched content
//! that looks like a call is never rewritten.
mod follow;
mod hits;
mod listing;
mod outline;
mod table;
use follow::{field_notes, split_stated, stated_notes};
use hits::{annotation, hits, value_lines};
use listing::{is_working_directory, listing, locations, outline_rows, words};
use octocode_native::id::ToolId;
use serde_json::{Map, Value, json};

/// A rendered result: stdout lines and stderr notes.
pub(super) struct Lines {
    pub out: String,
    pub notes: String,
}

/// One row's text: its content, its trailer (stdout, after the content) and
/// its notes (stderr), the last two without their `# `.
struct Row {
    body: Vec<String>,
    trailer: Vec<String>,
    notes: Vec<String>,
}

/// Explicit context determines whether hit values carry source-line prefixes.
/// `query` is the call as it ran ([`render`] repairs it first), so
/// `contextLines: 3` reads as the `{"before":3,"after":3}` it ran as.
fn context_requested(query: &Value) -> bool {
    let context = &query["queries"][0]["contextLines"];
    ["before", "after"]
        .into_iter()
        .any(|key| context[key].as_u64().is_some_and(|n| n > 0))
}

/// Each physical line of `notes` behind `prefix`, so a note that spans lines
/// stays a note on every one.
fn prefixed<'a>(prefix: &'a str, notes: &'a [String]) -> impl Iterator<Item = String> + 'a {
    notes
        .iter()
        .flat_map(|note| note.split('\n'))
        .map(move |line| format!("{prefix}{line}"))
}

/// The text of a result (module docs); `None` only when the result does not
/// match its tool's output schema, which prints its JSON. `cwd` is the
/// caller's directory: a root equal to it needs no note.
///
/// `query` is the call as sent, or the request a cursor continued when the
/// runtime names it; it is read as the runtime ran it, so a shorthand the
/// runtime repairs (scalar `contextLines`, a bare row) prints exactly as its
/// contract form does. A bare cursor says nothing of the request, so its
/// `localSearch` hits print as `key: value` text.
pub(super) fn render(
    tool: ToolId,
    structured: &Value,
    query: &Value,
    cwd: Option<&std::path::Path>,
) -> Option<Lines> {
    let query = &octocode_native::contracts::input_as_run(tool.as_str(), query.clone());
    let (display, calls) = follow::separate_calls(tool, structured)?;
    let rows = display.get("results")?.as_array()?;
    let single = rows.len() == 1;
    let (mut envelope, envelope_notes) = envelope_tail(&display, cwd);
    let (commands, envelope_leads) = follow::notes_of(&calls, None);
    envelope.extend(commands);
    let mut out = Vec::new();
    let mut notes = Vec::new();
    let no_fields = Map::new();
    for (position, row) in rows.iter().enumerate() {
        let row = row.as_object().unwrap_or(&no_fields);
        let label = format!("results[{position}]");
        let index = row.get("index").and_then(Value::as_u64);
        if !single {
            out.push(match index {
                Some(index) if index != position as u64 => format!("## {label} index {index}"),
                _ => format!("## {label}"),
            });
        }
        // A row reads its own query (context lines, operation), whichever
        // rows share its page; a bare cursor names none, so it reads that.
        let asked = index
            .and_then(|index| query["queries"].get(usize::try_from(index).ok()?))
            .map(|asked| json!({"queries": [asked]}));
        let asked = asked.as_ref().unwrap_or(query);
        let mut text =
            render_data(tool, row, asked).unwrap_or_else(|| outline::generic(row, asked));
        let (commands, leads) = follow::notes_of(&calls, Some(position));
        out.append(&mut text.body);
        out.extend(prefixed("# ", &text.trailer));
        if single {
            out.extend(prefixed("# ", &envelope));
            // The heading names a batch row's index; a lone row's is a note.
            if let Some(index) = index.filter(|index| *index != 0) {
                text.notes.insert(0, format!("index: {index}"));
            }
        }
        out.extend(prefixed("# ", &commands));
        let prefix = if single {
            "# ".to_owned()
        } else {
            format!("# {label} ")
        };
        notes.extend(prefixed(&prefix, &text.notes));
        notes.extend(prefixed(&prefix, &leads));
    }
    if !single {
        out.extend(prefixed("# ", &envelope));
    }
    notes.extend(prefixed("# ", &envelope_notes));
    notes.extend(prefixed("# ", &envelope_leads));
    Some(Lines {
        out: out.join("\n"),
        notes: notes.join("\n"),
    })
}

/// An `lspSearch` answer's location rows, each with its path: `matches`, or
/// the `files[].matches` a location answer groups them in. Text hits (their
/// `value`), call rows (their `sites`) and the files view (no `line`) print
/// as their own shapes.
fn location_rows(data: &Map<String, Value>) -> Option<Vec<Value>> {
    if let Some(Value::Array(matches)) = data.get("matches") {
        return Some(matches.clone());
    }
    let files = data.get("files")?.as_array()?;
    let mut rows = Vec::new();
    for file in files {
        let path = file.get("path")?;
        for row in file.get("matches")?.as_array()? {
            row.get("line")?;
            if row.get("sites").is_some() || row.get("value").is_some() {
                return None;
            }
            let mut row = row.as_object()?.clone();
            row.insert("path".into(), path.clone());
            rows.push(Value::Object(row));
        }
    }
    Some(rows)
}

/// `matches` with each hit's `enclosing` index replaced by the declaration it
/// names, so a line can state it.
fn with_declarations(matches: Value, declarations: Option<&Value>) -> Value {
    let Value::Array(mut rows) = matches else {
        return matches;
    };
    for row in &mut rows {
        let named = row
            .get("enclosing")
            .and_then(Value::as_u64)
            .and_then(|index| declarations?.get(usize::try_from(index).ok()?))
            .cloned();
        if let (Some(named), Some(row)) = (named, row.as_object_mut()) {
            row.insert("enclosing".into(), named);
        }
    }
    Value::Array(rows)
}

/// A row in its line format: a listing of any tool, or the hits, sites and
/// declarations of a search tool. `None` for a failed row and for any shape
/// without one, which print as `key: value` text.
fn render_data(tool: ToolId, row: &Map<String, Value>, query: &Value) -> Option<Row> {
    if row.get("status").and_then(Value::as_str) == Some("error") {
        return None;
    }
    let data = row.get("data")?.as_object()?;
    let mut rest = data.clone();
    let mut body = Vec::new();
    let mut marks = Vec::new();
    if listing(data, &mut body).is_some() {
        rest.shift_remove("files");
        return Some(tail(query, row, rest, marks, body));
    }
    if !matches!(
        tool,
        ToolId::LocalSearch | ToolId::AstSearch | ToolId::LspSearch
    ) || (tool == ToolId::LocalSearch && query.get("cursor").is_some())
    {
        return None;
    }
    if tool == ToolId::LspSearch
        && let Some(matches) = location_rows(data)
    {
        let symbol = query["queries"][0]["symbolName"].as_str();
        locations(&matches, symbol, &mut body, &mut marks)?;
        rest.shift_remove("matches");
        rest.shift_remove("files");
        return Some(tail(query, row, rest, marks, body));
    }
    // A single-file outline (`astSearch symbols`, `lspSearch
    // documentSymbols`): the row names its path only when the tool changed it.
    if let (Some(Value::Array(symbols)), Some(path)) = (
        data.get("symbols"),
        data.get("path")
            .or_else(|| query["queries"].get(0)?.get("path"))
            .and_then(Value::as_str),
    ) {
        outline_rows(path, symbols, None, &mut body)?;
        rest.shift_remove("symbols");
        return Some(tail(query, row, rest, marks, body));
    }
    let empty = row.get("status").and_then(Value::as_str) == Some("empty");
    let files = match rest.shift_remove("files") {
        Some(Value::Array(files)) => files,
        Some(_) => return None,
        // An empty result has no content; its trailer says so.
        None if empty => Vec::new(),
        None => return None,
    };
    let numbered = tool == ToolId::LocalSearch && context_requested(query);
    for file in &files {
        file_rows(tool, file.as_object()?, numbered, &mut body)?;
    }
    Some(tail(query, row, rest, marks, body))
}

/// One file of a search result: its hits, declarations or sites under its
/// path, or the path alone; its other fields follow the path in `[ … ]`.
fn file_rows(
    tool: ToolId,
    file: &Map<String, Value>,
    numbered: bool,
    out: &mut Vec<String>,
) -> Option<()> {
    let path = file.get("path")?.as_str()?;
    let mut rest = file.clone();
    rest.shift_remove("path");
    if let Some(Value::Array(symbols)) = rest.shift_remove("symbols") {
        return outline_rows(path, &symbols, annotation(&rest).as_deref(), out);
    }
    let declarations_of_file = rest.shift_remove("declarations");
    match rest.shift_remove("matches") {
        Some(Value::Array(matches)) if matches.iter().all(|item| item.get("value").is_some()) => {
            let Value::Array(matches) =
                with_declarations(Value::Array(matches), declarations_of_file.as_ref())
            else {
                return None;
            };
            return hits(path, &matches, numbered, annotation(&rest).as_deref(), out);
        }
        // `lspSearch` call rows (`callers`, `callees`): the calling or
        // called declaration, its call sites in `[ … ]`.
        Some(Value::Array(matches)) if tool == ToolId::LspSearch => {
            return outline_rows(path, &matches, annotation(&rest).as_deref(), out);
        }
        Some(_) => return None,
        None => {}
    }
    if let Some(declarations) = declarations_of_file {
        rest.insert("declarations".into(), declarations);
    }
    // `view:"files"` sites: `path:5,32`, every start line in the file. A
    // count that only repeats their number is not restated.
    let sites = rest
        .get("matchedLines")
        .and_then(|lines| serde_json::from_value::<Vec<u64>>(lines.clone()).ok())
        .filter(|lines| !lines.is_empty());
    let mut entry = path.to_owned();
    if let Some(lines) = sites {
        rest.shift_remove("matchedLines");
        if rest.get("matchCount").and_then(Value::as_u64) == Some(lines.len() as u64) {
            rest.shift_remove("matchCount");
        }
        let lines: Vec<String> = lines.iter().map(u64::to_string).collect();
        entry.push_str(&format!(":{}", lines.join(",")));
    }
    // Any other field is named: `path  [matchCount 2]`, never `path:2`.
    if let Some(note) = annotation(&rest) {
        entry.push_str(&format!("  {note}"));
    }
    out.push(entry);
    Some(())
}

/// A row from its `body`: the trailer and notes are its status, `marks`
/// (omissions the content states), the fields of `data` the body does not
/// show (less the searched path it echoes) and the row's own fields. Those
/// that decide "am I done" are the trailer, then its warnings and hints; the
/// rest are notes.
fn tail(
    query: &Value,
    row: &Map<String, Value>,
    mut data: Map<String, Value>,
    marks: Vec<String>,
    body: Vec<String>,
) -> Row {
    // The searched path echoed back: the printed lines already name it.
    let asked = query["queries"].get(0).unwrap_or(query);
    if data.get("path").is_some_and(|path| *path == asked["path"]) {
        data.shift_remove("path");
    }
    let mut trailer = Vec::new();
    if let Some(status) = row.get("status").and_then(Value::as_str) {
        trailer.push(format!("status: {status}"));
    }
    trailer.extend(marks);
    let (stated, info) = split_stated(data);
    let (done, mut advice) = stated_notes(stated);
    trailer.extend(done);
    let mut notes = field_notes("", &info);
    // Row fields beside `data` (`rowPart`, …), so a cut response is never
    // silent.
    if let Some(part) = row.get("rowPart") {
        trailer.push(row_part_note(part));
    }
    let row_rest: Map<String, Value> = row
        .iter()
        .filter(|(key, value)| match key.as_str() {
            "index" | "data" | "rowPart" => false,
            "status" => !value.is_string(),
            _ => true,
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let (stated, info) = split_stated(row_rest);
    let (done, row_advice) = stated_notes(stated);
    trailer.extend(done);
    advice.extend(row_advice);
    trailer.extend(advice);
    notes.extend(field_notes("", &info));
    Row {
        body,
        trailer,
        notes,
    }
}

/// The `rowPart` note, one form for every part: `rowPart: part N of M`, then
/// any other fact the part carries (`continues`) as words.
fn row_part_note(part: &Value) -> String {
    let Some(fields) = part.as_object() else {
        return format!("rowPart: {part}");
    };
    let mut rest = fields.clone();
    let position = rest.shift_remove("part");
    let of = rest.shift_remove("of");
    let mut note = match (position, of) {
        (Some(position), Some(of)) => format!("rowPart: part {position} of {of}"),
        (position, of) => {
            let mut named = Map::new();
            named.extend(position.map(|value| ("part".to_owned(), value)));
            named.extend(of.map(|value| ("of".to_owned(), value)));
            format!("rowPart: {}", words(&named))
        }
    };
    if !rest.is_empty() {
        note.push(' ');
        note.push_str(&words(&rest));
    }
    note
}

/// The envelope's fields beside `results`: its page state and its warnings
/// and hints as trailer lines, the rest as notes, and the root unless it is
/// the working directory.
fn envelope_tail(structured: &Value, cwd: Option<&std::path::Path>) -> (Vec<String>, Vec<String>) {
    let rest: Map<String, Value> = structured
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| !matches!(key.as_str(), "results" | "root"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let (stated, info) = split_stated(rest);
    let mut notes = field_notes("", &info);
    if let Some(root) = structured.get("root").and_then(Value::as_str)
        && !is_working_directory(root, cwd)
    {
        notes.push(format!("root: {root} (paths are relative to it)"));
    }
    let (mut trailer, advice) = stated_notes(stated);
    trailer.extend(advice);
    (trailer, notes)
}

/// A call the runtime rejected before it ran (`envelope`: its `errorCode`,
/// `error` and any detail), as a failed row prints: its fields, `errorCode`
/// first, then `# status: error` and its hints.
pub(super) fn rejected(envelope: &Value) -> String {
    let fields = envelope.as_object().cloned().unwrap_or_default();
    let mut data: Map<String, Value> = ["errorCode", "error"]
        .into_iter()
        .filter_map(|key| Some((key.to_owned(), fields.get(key)?.clone())))
        .collect();
    data.extend(fields);
    let row = json!({"status": "error", "data": data});
    let row = outline::generic(row.as_object().unwrap_or(&Map::new()), &Value::Null);
    row.body
        .into_iter()
        .chain(prefixed("# ", &row.trailer))
        .chain(prefixed("# ", &row.notes))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests;
