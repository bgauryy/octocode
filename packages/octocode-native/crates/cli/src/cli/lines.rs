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
//! page) follow its path in `[ … ]`. Paths that share a directory (the
//! files' and those the walk skipped) print relative to it, stated once
//! before them as `# root: dir` ([`once::relative_paths`]); counts by file
//! print as a `files[]: matchCount path` table, whichever count the files
//! share. A listing prints each `dir/` once with its entries indented under
//! it, a lone entry as `dir/entry`. Every other result prints as indented
//! `key: value` text ([`outline`]), a list of flat rows as a [`table`]; a
//! `graphQuery` row's paths print relative to the scope its query names,
//! stated once as its root ([`once::under_scope`]). A fraction prints to 4
//! significant digits ([`rounded`]).
//!
//! The trailer follows the content, one `# ` line each, the same for every
//! tool: a `status` that is not ok, each page state (an object holding
//! `hasMore`, in one form: its count first, `hasMore` last) and incomplete
//! mark, the row's `warnings` and `hints`, the
//! identity of what was read (`ref`, `commitSha`, `location`),
//! then the commands that continue the answer: `responsePagination.next`
//! first, then `next.nextPage`, then any other. A call rejected before it ran prints as
//! a failed row does ([`rejected`]). A batch prints each row under
//! `## results[N]`, `N` the place of its query, and names the row on every
//! `# ` line of its own, stdout and stderr alike (`# results[N] key: value`);
//! the envelope's trailer follows the last row. Objects and lists every
//! row of a batch shares print once before the first row
//! ([`once::shared_fields`]), a snapshot a later row names by its id alone
//! among them ([`once::with_batch_facts`]).
//!
//! Counters (one line, [`once::counters`]), what a walk skipped (one line of
//! names, [`once::skipped_line`]) and `related[]` leads are the stderr
//! notes; a note's nested value prints in the outline layout, one `# ` line
//! each.
//! Calls print as `octocode <tool> '<json>'` commands. They are read from
//! the structured result only, never from rendered text, so fetched content
//! that looks like a call is never rewritten.
//!
//! Every value prints as data ([`printable`](fn@printable)): its control characters
//! visible, and a line break in a value only where the layout puts its lines
//! (indented under its key, or behind their line numbers), so no value
//! prints a line of the renderer's own.
mod follow;
mod hits;
mod listing;
mod once;
mod outline;
mod printable;
mod table;
use follow::{field_notes, split_stated, stated_notes};
use hits::{annotation, hit_objects, hits, value_lines};
use listing::{is_working_directory, listing, locations, outline_rows, words};
use octocode_runtime_base::id::ToolId;
use printable::{one_line, printable};
use serde_json::{Map, Value, json};

/// A rendered result: stdout lines and stderr notes.
pub(super) struct Lines {
    pub out: String,
    pub notes: String,
}

/// One row's text: the directory its paths are relative to, its content,
/// its trailer (stdout, after the content) and its notes (stderr), the root,
/// trailer and notes without their `# `.
struct Row {
    root: Option<String>,
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

/// Each note on one line behind `prefix`: a line break in a note's value
/// prints visibly ([`one_line`]), so a value never starts a note of its own.
fn prefixed<'a>(prefix: &'a str, notes: &'a [String]) -> impl Iterator<Item = String> + 'a {
    notes
        .iter()
        .map(move |note| format!("{prefix}{}", one_line(note)))
}

/// `text` as printed for a person beside a JSON answer: its control
/// characters visible, its lines kept.
pub(super) fn printable_text(text: &str) -> String {
    printable::lines(text).into_owned()
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
    let query = &printable(octocode_runtime_base::contracts::input_as_run(
        tool.as_str(),
        query.clone(),
    ));
    let (display, calls) = follow::separate_calls(tool, structured)?;
    let mut display = rounded(printable(display));
    hit_objects(&mut display);
    let rows = display.get("results")?.as_array()?;
    let batch = is_batch(rows, query);
    let (mut envelope, envelope_notes) = envelope_tail(&display, cwd);
    let (commands, envelope_leads) = follow::notes_of(&calls, None);
    envelope.extend(commands);
    let mut out = Vec::new();
    let mut notes = Vec::new();
    let no_fields = Map::new();
    // A row reads its own query (context lines, operation), whichever rows
    // share its page; a bare cursor names none, so it reads that.
    let asked_by = |row: &Map<String, Value>| {
        row.get("index")
            .and_then(Value::as_u64)
            .and_then(|index| query["queries"].get(usize::try_from(index).ok()?))
            .map_or_else(|| query.clone(), |asked| json!({"queries": [asked]}))
    };
    // What the response states once for every row (`shared.data`, the
    // reading rule's step after a row's own data) prints once, first.
    if let Some(Value::Object(stated)) = display.pointer("/shared/data") {
        outline::object(stated, "", &mut out);
    }
    let whole = once::with_batch_facts(tool, rows);
    let mut shared = once::shared_fields(&whole, |row| {
        render_data(tool, row, &asked_by(row)).is_none()
    });
    // A field holding a row's own call stays with that row.
    shared.retain(|key, _| {
        !calls.iter().any(|call| {
            call.row.is_some() && call.label.split(['.', '[']).next() == Some(key.as_str())
        })
    });
    outline::object(&shared, "", &mut out);
    for (position, row) in rows.iter().enumerate() {
        let mut row = row.as_object().unwrap_or(&no_fields).clone();
        if let Some(Value::Object(data)) = row.get_mut("data") {
            data.retain(|key, _| !shared.contains_key(key));
        }
        let index = row.get("index").and_then(Value::as_u64);
        let label = format!("results[{}]", index.unwrap_or(position as u64));
        if batch {
            out.push(format!("## {label}"));
        }
        let asked = asked_by(&row);
        let scope = if tool == ToolId::GraphQuery {
            once::under_scope(&mut row, &asked)
        } else {
            None
        };
        let mut text =
            render_data(tool, &row, &asked).unwrap_or_else(|| outline::generic(&row, &asked));
        if text.root.is_none() {
            text.root = scope.map(|scope| format!("root: {scope}"));
        }
        let (commands, leads) = follow::notes_of(&calls, Some(position));
        let prefix = if batch {
            format!("# {label} ")
        } else {
            "# ".to_owned()
        };
        out.extend(prefixed(&prefix, text.root.as_slice()));
        out.append(&mut text.body);
        out.extend(prefixed(&prefix, &text.trailer));
        if !batch {
            out.extend(prefixed("# ", &envelope));
        }
        out.extend(prefixed(&prefix, &commands));
        notes.extend(prefixed(&prefix, &text.notes));
        notes.extend(prefixed(&prefix, &leads));
    }
    if batch {
        out.extend(prefixed("# ", &envelope));
    }
    notes.extend(prefixed("# ", &envelope_notes));
    notes.extend(prefixed("# ", &envelope_leads));
    Some(Lines {
        out: out.join("\n"),
        notes: notes.join("\n"),
    })
}

/// Whether rows print as a batch, each under `## results[N]`: a page of
/// several rows, a call of several queries, or a row of a later query (a
/// page that holds one row of a batch).
fn is_batch(rows: &[Value], query: &Value) -> bool {
    rows.len() > 1
        || query["queries"]
            .as_array()
            .is_some_and(|asked| asked.len() > 1)
        || rows
            .iter()
            .any(|row| row["index"].as_u64().is_some_and(|index| index > 0))
}

/// `value` with each fraction rounded to 4 significant digits, an integer
/// part never cut (`0.04051045341394517` prints `0.04051`, `12345.67`
/// `12346.0`); `--json` keeps the exact value.
fn rounded(value: Value) -> Value {
    match value {
        Value::Number(number) => {
            let fraction = number
                .as_f64()
                .filter(|x| number.is_f64() && x.is_finite() && x.fract() != 0.0);
            let Some(x) = fraction else {
                return Value::Number(number);
            };
            let short = if x.abs() >= 1e4 {
                x.round()
            } else {
                format!("{x:.3e}").parse().unwrap_or(x)
            };
            serde_json::Number::from_f64(short).map_or(Value::Number(number), Value::Number)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(rounded).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key, rounded(value)))
                .collect(),
        ),
        other => other,
    }
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

/// `matches` with each hit's `enclosing` replaced by the declaration it
/// names, so a line can state it. A merged window's `enclosing` may be one
/// index per `matchedLines` entry; each resolves to its own declaration.
fn with_declarations(matches: Value, declarations: Option<&Value>) -> Value {
    let Value::Array(mut rows) = matches else {
        return matches;
    };
    let declaration = |index: &Value| {
        let index = usize::try_from(index.as_u64()?).ok()?;
        declarations?.get(index).cloned()
    };
    for row in &mut rows {
        let named = match row.get("enclosing") {
            Some(Value::Array(indexes)) => indexes
                .iter()
                .map(&declaration)
                .collect::<Option<Vec<_>>>()
                .map(Value::Array),
            Some(index) => declaration(index),
            None => None,
        };
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
    let mut body = Vec::new();
    let mut marks = Vec::new();
    // A listing's groups print relative to the directory they share, as hits do.
    let mut relative = data.clone();
    let root = once::relative_paths(&mut relative);
    if listing(&relative, &mut body).is_some() {
        let mut rest = relative;
        rest.shift_remove("files");
        // A stated root prints as the listing's own root line.
        rest.shift_remove("root");
        let mut text = tail(query, row, rest, marks, body);
        text.root = root.map(|root| format!("root: {root}"));
        return Some(text);
    }
    if !matches!(
        tool,
        ToolId::LocalSearch | ToolId::AstSearch | ToolId::LspSearch
    ) || (tool == ToolId::LocalSearch && query.get("cursor").is_some())
    {
        return None;
    }
    let mut relative = data.clone();
    let root = once::relative_paths(&mut relative);
    let data = &relative;
    let rooted = |mut text: Row| {
        text.root = root.as_ref().map(|root| format!("root: {root}"));
        text
    };
    let mut rest = data.clone();
    if tool == ToolId::LspSearch
        && let Some(matches) = location_rows(data)
    {
        let symbol = query["queries"][0]["symbolName"].as_str();
        locations(&matches, symbol, &mut body, &mut marks)?;
        rest.shift_remove("matches");
        rest.shift_remove("files");
        return Some(rooted(tail(query, row, rest, marks, body)));
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
    let term = (tool == ToolId::LocalSearch)
        .then(|| one_term(query))
        .flatten();
    if let Some(mut counts) = table::count_table(&files) {
        body.append(&mut counts);
    } else {
        for file in &files {
            file_rows(tool, file.as_object()?, (numbered, term), &mut body)?;
        }
    }
    Some(rooted(tail(query, row, rest, marks, body)))
}

/// The query's one `matchString` term, which a localSearch span equal to it
/// leaves out of its `value`; `None` for several terms or none.
fn one_term(query: &Value) -> Option<&str> {
    match &query["queries"][0]["matchString"] {
        Value::String(term) => Some(term),
        Value::Array(terms) if terms.len() == 1 => terms[0].as_str(),
        _ => None,
    }
}

/// `matches` with each span that left out its `value` (a localSearch span
/// equal to the query's one term) holding that term, so it prints as the
/// others do.
fn with_term(mut matches: Option<Value>, term: Option<&str>) -> Option<Value> {
    if let (Some(Value::Array(items)), Some(term)) = (matches.as_mut(), term) {
        for item in items.iter_mut().filter_map(Value::as_object_mut) {
            if item.get("value").is_none() && item.get("column").is_some() {
                item.insert("value".into(), term.into());
            }
        }
    }
    matches
}

/// One file of a search result: its hits, declarations or sites under its
/// path, or the path alone; its other fields follow the path in `[ … ]`.
/// `term` is the value a localSearch span without one holds ([`one_term`]).
fn file_rows(
    tool: ToolId,
    file: &Map<String, Value>,
    (numbered, term): (bool, Option<&str>),
    out: &mut Vec<String>,
) -> Option<()> {
    let path = file.get("path")?.as_str()?;
    let mut rest = file.clone();
    rest.shift_remove("path");
    if let Some(Value::Array(symbols)) = rest.shift_remove("symbols") {
        return outline_rows(path, &symbols, annotation(&rest).as_deref(), out);
    }
    let declarations_of_file = rest.shift_remove("declarations");
    match with_term(rest.shift_remove("matches"), term) {
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
    let mut entry = one_line(path).into_owned();
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
    let (stated, mut info) = split_stated(data);
    let (done, mut advice) = stated_notes(stated);
    trailer.extend(done);
    // What a walk skipped reads as one line of names, beside content or not.
    once::skipped_line(&mut info);
    let (counts, info) = once::counters(info);
    let mut notes: Vec<String> = counts.into_iter().collect();
    notes.extend(field_notes("", &info));
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
        root: None,
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
        .filter(|(key, _)| !matches!(key.as_str(), "results" | "root" | "shared"))
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
    let envelope = printable(envelope.clone());
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
