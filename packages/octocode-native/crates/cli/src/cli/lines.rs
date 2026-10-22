//! Text output for a terminal or a pipe. A single-row `localSearch`,
//! `astSearch` or `structureSearch files` result prints its standard line
//! format on stdout, as rg, ast-grep and fd do: `path:line:text` hits, or
//! with context `rg --heading` blocks (the path, then `line:text` hits and
//! `line-text` context, `--` between groups), `path:line: kind name`
//! declarations, one path per line, or `path:count`. Every other field of
//! the result goes to stderr as `# key: value` notes, so stdout pipes cleanly
//! and no field is dropped. Other results keep the runtime's rendered text.
//!
//! Follow-up calls (`next.*`, call hints, `responsePagination.next`, a
//! repair) print as `octocode <tool> '<json>'` commands. They are read from
//! the structured result only, never from rendered text, so fetched content
//! that looks like a call is never rewritten.
use serde_json::{Map, Value};

/// A rendered result: data lines for stdout and notes for stderr.
pub(super) struct Lines {
    pub out: String,
    pub notes: String,
}

/// Whether hits carry context lines, so a hit's value is numbered source
/// (`N\ttext`): the query asked for context, or `clasifyContext` judges hits
/// with its default context. A batch never reaches the line format.
fn context_requested(query: &Value) -> bool {
    let row = query["queries"].get(0).unwrap_or(query);
    let keys = ["contextLines", "contextBefore", "contextAfter"];
    // `clasifyContext` judges hits with context unless a context field is set.
    keys.into_iter()
        .any(|key| row[key].as_u64().is_some_and(|n| n > 0))
        || (row.get("clasifyContext").is_some() && keys.iter().all(|key| row.get(*key).is_none()))
}

/// The line format of a single-row `localSearch`, `astSearch`, `lspSearch`
/// (rows of sites) or `structureSearch files` result; `None` for any other
/// result, which keeps the rendered text.
pub(super) fn render(tool: &str, structured: &Value, query: &Value) -> Option<Lines> {
    if tool == "structureSearch" {
        return listing(structured, query);
    }
    if !matches!(tool, "localSearch" | "astSearch" | "lspSearch") {
        return None;
    }
    let row = single_row(structured)?;
    let data = row.get("data")?.as_object()?;
    let empty = row.get("status").and_then(Value::as_str) == Some("empty");
    let no_files = Vec::new();
    let files = match data.get("files") {
        Some(files) => files.as_array()?,
        // An empty result prints nothing on stdout, as rg does; its notes say why.
        None if empty => &no_files,
        None => return None,
    };
    let numbered = tool == "localSearch" && context_requested(query);
    let mut out = Vec::new();
    let mut file_notes = Vec::new();
    for file in files {
        let file = file.as_object()?;
        let path = file.get("path")?.as_str()?;
        let mut rest = file.clone();
        rest.shift_remove("path");
        if let Some(Value::Array(symbols)) = rest.shift_remove("symbols") {
            declarations(path, &symbols, None, &mut out)?;
            if !rest.is_empty() {
                file_notes.push(format!("file {path}: {}", words(&rest)));
            }
            continue;
        }
        let matches = rest.shift_remove("matches");
        let count = ["matchCount", "matchedLineCount"]
            .into_iter()
            .find_map(|key| rest.get(key).and_then(Value::as_u64).map(|n| (key, n)));
        match (matches, count) {
            (Some(Value::Array(matches)), _) => hits(path, &matches, numbered, &mut out)?,
            (None, Some((key, n))) if rest.len() == 1 => {
                rest.shift_remove(key);
                out.push(format!("{path}:{n}"));
            }
            (None, _) => out.push(path.to_owned()),
            _ => return None,
        }
        if !rest.is_empty() {
            file_notes.push(format!("file {path}: {}", words(&rest)));
        }
    }
    Some(Lines {
        out: out.join("\n"),
        notes: notes(structured, query, row, data.clone(), file_notes),
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
    // `warnings`, `shared`, …), so a cut response is never silent.
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
        && !is_working_directory(root)
    {
        notes.push(format!("root: {root} (paths are relative to it)"));
    }
    notes.extend(follow_ups(structured));
    notes
        .iter()
        .map(|line| format!("# {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `root` is the directory the command runs in.
fn is_working_directory(root: &str) -> bool {
    let canonical = |path: &std::path::Path| std::fs::canonicalize(path).ok();
    std::env::current_dir()
        .ok()
        .and_then(|cwd| canonical(&cwd))
        .is_some_and(|cwd| canonical(std::path::Path::new(root)) == Some(cwd))
}

/// The only row of a single-query result, unless it failed.
fn single_row(structured: &Value) -> Option<&Map<String, Value>> {
    let results = structured.get("results")?.as_array()?;
    let [row] = results.as_slice() else {
        return None;
    };
    let row = row.as_object()?;
    (row.get("status").and_then(Value::as_str) != Some("error")).then_some(row)
}

/// `structureSearch files` as fd prints it: one root-relative path per line,
/// a directory's ending in `/`. An entry's fields follow in `[ … ]`, its
/// size only when the query sorts or filters by size. The tree view keeps
/// the rendered text.
fn listing(structured: &Value, query: &Value) -> Option<Lines> {
    let asked = query["queries"].get(0).unwrap_or(query);
    if asked["operation"].as_str() != Some("files") {
        return None;
    }
    let sized = asked["sort"].as_str() == Some("size") || asked.get("size").is_some();
    let row = single_row(structured)?;
    let data = row.get("data")?.as_object()?;
    let mut out = Vec::new();
    for group in data
        .get("files")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
    {
        let dir = group.get("dir")?.as_str()?;
        for entry in group.get("files")?.as_array()? {
            let (name, fields) = entry_fields(entry.as_str()?);
            let path = if dir.is_empty() || dir == "." {
                name.to_owned()
            } else {
                format!("{dir}/{name}")
            };
            let shown: Vec<String> = fields
                .iter()
                .enumerate()
                .filter(|(index, field)| {
                    sized || *index > 0 || !field.bytes().all(|byte| byte.is_ascii_digit())
                })
                // The bare number is the size in bytes: say so.
                .map(|(_, field)| {
                    if field.bytes().all(|byte| byte.is_ascii_digit()) {
                        format!("{field} B")
                    } else {
                        (*field).to_owned()
                    }
                })
                .collect();
            out.push(if shown.is_empty() {
                path
            } else {
                format!("{path}  [{}]", shown.join(", "))
            });
        }
    }
    Some(Lines {
        out: out.join("\n"),
        notes: notes(structured, query, row, data.clone(), Vec::new()),
    })
}

/// A listed entry's name and fields: the fields follow the last ` (` of an
/// entry ending in `)` (sizes, `symlink`, line counts, `modifiedMs=N`).
fn entry_fields(entry: &str) -> (&str, Vec<&str>) {
    match entry
        .strip_suffix(')')
        .and_then(|head| head.rsplit_once(" ("))
    {
        Some((name, fields)) => (name, fields.split(", ").collect()),
        None => (entry, Vec::new()),
    }
}

/// `path:line: kind name` per declaration, members indented under their
/// parent, as the outline nests them. A member takes its parent's `shared`
/// fields. Span, doc comment and other fields follow in `[ … ]`.
fn declarations(
    path: &str,
    symbols: &[Value],
    shared: Option<&Map<String, Value>>,
    out: &mut Vec<String>,
) -> Option<()> {
    let indent = if shared.is_some() { "  " } else { "" };
    for symbol in symbols {
        let mut symbol = symbol.as_object()?.clone();
        for (key, value) in shared.into_iter().flatten() {
            symbol.entry(key.clone()).or_insert_with(|| value.clone());
        }
        let line = symbol.shift_remove("line")?.as_u64()?;
        let name = symbol.shift_remove("symbolName")?;
        let kind = symbol.shift_remove("kind");
        let kind = kind.as_ref().and_then(Value::as_str).unwrap_or("symbol");
        let members = symbol.shift_remove("members");
        let own_shared = symbol.shift_remove("shared");
        let mut note = Vec::new();
        if let Some(end) = symbol.shift_remove("endLine").and_then(|end| end.as_u64()) {
            note.push(format!("{line}-{end}"));
        }
        if let Some(doc) = symbol
            .shift_remove("docStartLine")
            .and_then(|doc| doc.as_u64())
        {
            note.push(format!("doc {doc}"));
        }
        if !symbol.is_empty() {
            note.push(words(&symbol));
        }
        let name = name
            .as_str()
            .map_or_else(|| name.to_string(), str::to_owned);
        let mut entry = format!("{path}:{line}: {indent}{kind} {name}");
        if !note.is_empty() {
            entry.push_str(&format!("  [{}]", note.join(" ")));
        }
        out.push(entry);
        if let Some(Value::Array(members)) = members {
            let own_shared = own_shared.as_ref().and_then(Value::as_object);
            declarations(path, &members, Some(own_shared.unwrap_or(&Map::new())), out)?;
        }
    }
    Some(())
}

/// Fields as words: `key` for true, `key value` for a scalar or a list of
/// scalars; JSON only for a nested object.
fn words(fields: &Map<String, Value>) -> String {
    fields
        .iter()
        .map(|(key, value)| match value {
            Value::Bool(true) => key.clone(),
            Value::String(text) => format!("{key} {text}"),
            Value::Array(items)
                if items
                    .iter()
                    .all(|item| !item.is_object() && !item.is_array()) =>
            {
                let items: Vec<String> = items
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .map_or_else(|| item.to_string(), str::to_owned)
                    })
                    .collect();
                format!("{key} {}", items.join(","))
            }
            other => format!("{key} {other}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A hit value's lines with their numbers: numbered source when the query
/// asked for context (`None` when the value is not, such as a clipped
/// snippet), else the value's lines from `line`. An empty value is one empty
/// line; a `\r` stays, as rg prints it.
fn value_lines(value: &str, line: u64, numbered: bool) -> Option<Vec<(u64, &str)>> {
    let parts = value.split('\n');
    if !numbered {
        return Some(parts.zip(line..).map(|(text, n)| (n, text)).collect());
    }
    let lines = parts
        .map(|text| {
            let (n, text) = text.split_once('\t')?;
            (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                .then(|| n.parse().ok().map(|n| (n, text)))
                .flatten()
        })
        .collect::<Option<Vec<_>>>()?;
    lines.iter().any(|(n, _)| *n == line).then_some(lines)
}

/// The hit lines of one file. Hit lines are `matchedLines` when present,
/// else `line..=endLine`; the rest are context. Fields the line does not
/// show follow the first hit line in `[ … ]`.
fn hits(path: &str, matches: &[Value], numbered: bool, out: &mut Vec<String>) -> Option<()> {
    // With context, as `rg --heading`: the path once, then `N:text` hits and
    // `N-text` context, a blank line between files; the path is not repeated
    // on every line of a window.
    if numbered {
        if !out.is_empty() {
            out.push(String::new());
        }
        out.push(path.to_owned());
    }
    let mut last_line: Option<u64> = None;
    for item in matches {
        let item = item.as_object()?;
        let line = item.get("line")?.as_u64()?;
        let end = item
            .get("endLine")
            .and_then(Value::as_u64)
            .unwrap_or(line)
            .max(line);
        let value = item.get("value")?.as_str()?;
        let lines = value_lines(value, line, numbered)?;
        let mut extra = item.clone();
        for key in ["line", "value"] {
            extra.shift_remove(key);
        }
        let matched: Option<Vec<u64>> = extra
            .shift_remove("matchedLines")
            .and_then(|lines| serde_json::from_value(lines).ok());
        let is_hit = |n: u64| match &matched {
            Some(lines) => lines.contains(&n),
            None => (line..=end).contains(&n),
        };
        // `endLine` stays a note when the printed lines stop short of it.
        if lines.last().is_some_and(|(n, _)| *n >= end) {
            extra.shift_remove("endLine");
        }
        let first = lines.first().map_or(line, |(n, _)| *n);
        // As rg: `--` separates groups only when there are context lines.
        if numbered && last_line.is_some_and(|last| first > last + 1) {
            out.push("--".into());
        }
        let mut annotated = false;
        for (n, text) in &lines {
            let hit = !numbered || is_hit(*n);
            let sep = if hit { ':' } else { '-' };
            let mut entry = if numbered {
                format!("{n}{sep}{text}")
            } else {
                format!("{path}{sep}{n}{sep}{text}")
            };
            if hit && !annotated {
                annotated = true;
                if let Some(note) = match_note(&mut extra) {
                    entry.push_str("  ");
                    entry.push_str(&note);
                }
            }
            out.push(entry);
        }
        last_line = numbered.then(|| lines.last().map_or(end, |(n, _)| *n));
    }
    Some(())
}

/// `[function build_searcher 992-1016]` for the usual enclosing declaration,
/// then any other match field as words (`declaration`, `enclosingLine 935`).
fn match_note(extra: &mut Map<String, Value>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(Value::Object(enclosing)) = extra.get("enclosing")
        && let (Some(kind), Some(name)) = (
            enclosing.get("kind").and_then(Value::as_str),
            enclosing.get("symbolName").and_then(Value::as_str),
        )
        && enclosing
            .keys()
            .all(|key| matches!(key.as_str(), "kind" | "symbolName" | "line" | "endLine"))
    {
        let span = match (enclosing.get("line"), enclosing.get("endLine")) {
            (Some(start), Some(end)) => format!(" {start}-{end}"),
            (Some(start), None) => format!(" {start}"),
            _ => String::new(),
        };
        parts.push(format!("{kind} {name}{span}"));
        extra.shift_remove("enclosing");
    }
    if !extra.is_empty() {
        parts.push(words(&std::mem::take(extra)));
    }
    (!parts.is_empty()).then(|| format!("[{}]", parts.join(" ")))
}

/// A call object: `{tool, query}`.
fn is_call(value: &Value) -> bool {
    value.get("tool").and_then(Value::as_str).is_some() && value.get("query").is_some()
}

/// A hint string the runtime prints for a call, `[name: ]<tool> {json}`,
/// as `[name: ]octocode <tool> '<json>'`; `None` for any other text.
fn call_string(text: &str) -> Option<String> {
    let brace = text.find(" {")?;
    let (head, json) = text.split_at(brace);
    let json = json.trim_start();
    serde_json::from_str::<Value>(json).ok()?.as_object()?;
    let tool_start = head.rfind(' ').map_or(0, |i| i + 1);
    let (prefix, tool) = head.split_at(tool_start);
    octocode_native::id::ToolId::from_name(tool)?;
    Some(format!("{prefix}{}", command(tool, json)))
}

/// `value` without its calls, or `None` when nothing else is left.
fn without_calls(value: &Value) -> Option<Value> {
    match value {
        call if is_call(call) => None,
        Value::String(text) if call_string(text).is_some() => None,
        Value::Object(map) => {
            let kept: Map<String, Value> = map
                .iter()
                .filter_map(|(key, value)| without_calls(value).map(|value| (key.clone(), value)))
                .collect();
            (!kept.is_empty()).then_some(Value::Object(kept))
        }
        Value::Array(items) => {
            let kept: Vec<Value> = items.iter().filter_map(without_calls).collect();
            (!kept.is_empty()).then_some(Value::Array(kept))
        }
        other => Some(other.clone()),
    }
}

/// `key: value` notes for every field; calls are left to [`follow_ups`].
fn field_notes(prefix: &str, fields: &Map<String, Value>) -> Vec<String> {
    let mut lines = Vec::new();
    for (key, value) in fields {
        let Some(value) = without_calls(value) else {
            continue;
        };
        match value {
            Value::Array(items) if matches!(key.as_str(), "hints" | "warnings") => {
                for item in items {
                    let text = item
                        .as_str()
                        .map_or_else(|| item.to_string(), str::to_owned);
                    lines.push(format!("{prefix}{key}: {text}"));
                }
            }
            Value::String(text) => lines.push(format!("{prefix}{key}: {text}")),
            // A flat object reads as words: `summary: files 4 dirs 0`.
            Value::Object(fields) if fields.values().all(|value| !value.is_object()) => {
                lines.push(format!("{prefix}{key}: {}", words(&fields)));
            }
            other => lines.push(format!("{prefix}{key}: {other}")),
        }
    }
    lines
}

/// Keys whose values are content (source, matches, diffs), never calls.
const CONTENT_KEYS: [&str; 6] = ["files", "content", "matches", "value", "patch", "body"];

/// Every follow-up call in a structured result, as `label: octocode …`
/// lines: `next.*`, call hints, `responsePagination.next`, a repair. A
/// single row's calls drop the `results[0].data.` prefix.
pub(super) fn follow_ups(structured: &Value) -> Vec<String> {
    fn walk(value: &Value, label: &str, out: &mut Vec<String>) {
        if is_call(value) {
            let tool = value["tool"].as_str().unwrap_or_default();
            let mut line = format!("{label}: {}", command(tool, &value["query"].to_string()));
            if let Some(reason) = value.get("reason").and_then(Value::as_str) {
                line.push_str(&format!("  # {reason}"));
            }
            out.push(line);
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    if CONTENT_KEYS.contains(&key.as_str()) {
                        continue;
                    }
                    let label = if label.is_empty() {
                        key.clone()
                    } else {
                        format!("{label}.{key}")
                    };
                    walk(value, &label, out);
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    match item.as_str().and_then(call_string) {
                        Some(call) if label.ends_with("hints") => {
                            out.push(format!("{label}: {call}"))
                        }
                        _ => walk(item, &format!("{label}[{index}]"), out),
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(structured, "", &mut out);
    let single = structured["results"]
        .as_array()
        .is_some_and(|rows| rows.len() == 1);
    out.into_iter()
        .map(|line| match line.strip_prefix("results[0].data.") {
            Some(rest) if single => rest.to_owned(),
            _ => line,
        })
        .collect()
}

/// `structured` without the calls [`follow_ups`] prints as commands (the
/// same walk: content keys are never touched), and without a `next` or
/// `hints` left empty by that, so a rendering of it shows each follow-up
/// once, on stderr.
pub(super) fn without_follow_ups(structured: &Value) -> Value {
    fn strip(value: &mut Value, hints: bool) {
        match value {
            Value::Object(map) => {
                map.retain(|key, child| {
                    if CONTENT_KEYS.contains(&key.as_str()) {
                        return true;
                    }
                    if is_call(child) {
                        return false;
                    }
                    strip(child, key == "hints");
                    let emptied = match child {
                        Value::Object(inner) => inner.is_empty(),
                        Value::Array(items) => items.is_empty(),
                        _ => false,
                    };
                    !(emptied && matches!(key.as_str(), "next" | "hints"))
                });
            }
            Value::Array(items) => {
                items.retain(|item| {
                    !(hints && item.as_str().and_then(call_string).is_some()) && !is_call(item)
                });
                for item in items {
                    strip(item, false);
                }
            }
            _ => {}
        }
    }
    let mut value = structured.clone();
    strip(&mut value, false);
    value
}

/// `octocode <tool> '<json>'`, single-quoted for the shell that runs it:
/// POSIX, or PowerShell ([`powershell`]).
pub(super) fn command(tool: &str, json: &str) -> String {
    if powershell() {
        // PowerShell doubles a quote inside single quotes; the JSON's own
        // double quotes reach the program intact from PowerShell 7.3.
        format!("octocode {tool} '{}'", json.replace('\'', "''"))
    } else {
        format!("octocode {tool} '{}'", json.replace('\'', r"'\''"))
    }
}

/// Whether printed commands are for PowerShell: on Windows outside a POSIX
/// shell (Git Bash, MSYS and Cygwin set `MSYSTEM` or `SHELL`).
fn powershell() -> bool {
    cfg!(windows) && std::env::var_os("MSYSTEM").is_none() && std::env::var_os("SHELL").is_none()
}

/// Whether every row of a result failed: its text then belongs on stderr.
pub(super) fn all_rows_failed(structured: &Value) -> bool {
    structured["results"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty() && rows.iter().all(|row| row["status"] == "error"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn leaves(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => map.values().for_each(|v| leaves(v, out)),
            Value::Array(items) => items.iter().for_each(|v| leaves(v, out)),
            Value::String(text) => out.extend(
                text.lines()
                    .map(|l| l.split_once('\t').map_or(l, |(_, t)| t).to_owned()),
            ),
            other => out.push(other.to_string()),
        }
    }

    fn sample() -> Value {
        json!({"results":[{"index":0,"rowPart":{"part":1,"of":2},"data":{
            "matchCount":2,"matchedLineCount":2,
            "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"path":"s","matchString":"it's","page":2}]}}},
            "files":[{"path":"a.rs","matches":[
                {"line":26,"value":"25\tuse a;\n26\tuse SearcherBuilder;\n27\tuse b;"},
                {"line":997,"value":"996\t) {\n997\t  let sb = SearcherBuilder::new();",
                 "enclosing":{"symbolName":"build","kind":"function","line":992,"endLine":1016}}]}]}}],
            "warnings":["incomplete"],
            "responsePagination":{"hasMore":true,"next":{"tool":"localSearch","query":{"responseSnapshot":"s1"}}},
            "root":"/nowhere"})
    }

    fn context() -> Value {
        json!({"path":"s","matchString":"x","contextLines":1})
    }

    #[test]
    fn hits_print_like_rg_and_lose_no_field() {
        let lines = render("localSearch", &sample(), &context()).expect("lines");
        assert_eq!(
            lines.out,
            "a.rs\n25-use a;\n26:use SearcherBuilder;\n27-use b;\n--\n996-) {\n997:  let sb = SearcherBuilder::new();  [function build 992-1016]"
        );
        assert!(lines.notes.contains("# next.nextPage: octocode localSearch '{\"queries\":[{\"path\":\"s\",\"matchString\":\"it'\\''s\",\"page\":2}]}'"), "{}", lines.notes);
        assert!(
            lines.notes.contains(
                "# responsePagination.next: octocode localSearch '{\"responseSnapshot\":\"s1\"}'"
            ),
            "{}",
            lines.notes
        );
        assert!(
            lines.notes.contains("# rowPart: part 1 of 2"),
            "{}",
            lines.notes
        );
        assert!(
            lines.notes.contains("# warnings: incomplete"),
            "{}",
            lines.notes
        );
        // Shell quoting aside, every leaf value is printed.
        let both = format!("{}\n{}", lines.out, lines.notes).replace(r"'\''", "'");
        let mut all = Vec::new();
        leaves(&sample(), &mut all);
        // A true flag prints as its key alone (`hasMore`), as words do.
        for leaf in all.into_iter().filter(|leaf| leaf != "true") {
            assert!(both.contains(leaf.trim_matches('"')), "missing {leaf}");
        }
    }

    #[test]
    fn numbering_follows_the_query_and_matched_lines() {
        // Without context, a `N\t` prefix is the line's own text.
        let tsv = json!({"results":[{"index":0,"data":{"files":[{"path":"t","matches":[{"line":3,"value":"3\tfoo"}]}]}}]});
        assert_eq!(
            render("localSearch", &tsv, &json!({})).expect("lines").out,
            "t:3:3\tfoo"
        );
        // With context, `matchedLines` marks every hit.
        let merged = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[
            {"line":2,"matchedLines":[2,3],"value":"1\tx\n2\tfoo\n3\tfoo"}]}]}}]});
        assert_eq!(
            render("localSearch", &merged, &context())
                .expect("lines")
                .out,
            "a\n1-x\n2:foo\n3:foo"
        );
        // A clipped snippet that is not numbered keeps the rendered text.
        let clipped = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[
            {"line":6,"value":"xxx…","clipped":true}]}]}}]});
        assert!(render("localSearch", &clipped, &context()).is_none());
    }

    #[test]
    fn empty_values_crlf_and_clipped_structural_matches() {
        let blank = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[{"line":4,"value":""},{"line":5,"value":"x\r"}]}]}}]});
        assert_eq!(
            render("localSearch", &blank, &json!({}))
                .expect("lines")
                .out,
            "a:4:\na:5:x\r"
        );
        let ast = json!({"results":[{"index":0,"data":{"files":[{"path":"m.ts","matches":[
            {"line":1,"endLine":3,"column":1,"value":"function foo(a) …","clipped":true}]}]}}]});
        assert_eq!(
            render("astSearch", &ast, &json!({})).expect("lines").out,
            "m.ts:1:function foo(a) …  [endLine 3 column 1 clipped]"
        );
    }

    #[test]
    fn lists_counts_and_empty_results() {
        let files = json!({"results":[{"index":0,"data":{"files":[{"path":"a"},{"path":"b"}]}}]});
        assert_eq!(
            render("localSearch", &files, &json!({}))
                .expect("lines")
                .out,
            "a\nb"
        );
        let counts = json!({"results":[{"index":0,"data":{"matchCount":3,"files":[{"path":"a","matchCount":3}]}}]});
        let lines = render("localSearch", &counts, &json!({})).expect("lines");
        assert_eq!(lines.out, "a:3");
        assert_eq!(lines.notes, "# matchCount: 3");
        let empty = json!({"results":[{"index":0,"status":"empty","data":{"matchCount":0,
            "hints":["No matches.","includeSkipped: localSearch {\"queries\":[{\"path\":\".\"}]}"]}}]});
        let lines = render("localSearch", &empty, &json!({})).expect("lines");
        assert_eq!(lines.out, "");
        assert!(lines.notes.contains("# status: empty\n"), "{}", lines.notes);
        assert!(
            lines.notes.contains("# hints: No matches."),
            "{}",
            lines.notes
        );
        assert!(
            lines.notes.contains(
                "# hints: includeSkipped: octocode localSearch '{\"queries\":[{\"path\":\".\"}]}'"
            ),
            "{}",
            lines.notes
        );
    }

    #[test]
    fn declarations_and_listings_print_one_line_each() {
        let symbols = json!({"results":[{"index":0,"data":{"filesScanned":2,"files":[
            {"path":"a.rs","symbols":[
                {"symbolName":"render","kind":"function","line":31,"endLine":113,"docStartLine":29},
                {"symbolName":"Visit","kind":"impl","line":34,"endLine":43,"shared":{"kind":"method"},
                 "members":[{"symbolName":"fresh","line":37,"exported":true}]}]}]}}]});
        let lines = render("astSearch", &symbols, &json!({})).expect("lines");
        assert_eq!(
            lines.out,
            "a.rs:31: function render  [31-113 doc 29]\na.rs:34: impl Visit  [34-43]\na.rs:37:   method fresh  [exported]"
        );
        assert!(!lines.notes.contains('{'), "{}", lines.notes);
        let listed = json!({"results":[{"index":0,"data":{"path":"src","files":[
            {"dir":"src","files":["a (1).rs (12)","b.rs (34, symlink)","sub/"]}],
            "summary":{"files":2,"dirs":1,"bytes":46}}}]});
        let files = json!({"operation":"files","path":"src"});
        let lines = render("structureSearch", &listed, &files).expect("lines");
        assert_eq!(lines.out, "src/a (1).rs\nsrc/b.rs  [symlink]\nsrc/sub/");
        let sized = json!({"operation":"files","path":"src","sort":"size"});
        let lines = render("structureSearch", &listed, &sized).expect("lines");
        assert_eq!(
            lines.out,
            "src/a (1).rs  [12 B]\nsrc/b.rs  [34 B, symlink]\nsrc/sub/"
        );
        assert!(render("structureSearch", &listed, &json!({"path":"src"})).is_none());
    }

    #[test]
    fn follow_ups_leave_the_rendering_and_content_stays() {
        let call = json!({"tool":"ghSearchCode","query":{"page":2}});
        let result = json!({"results":[{"index":0,"data":{
            "next":{"nextPage":call},
            "hints":["No more.","broaden: ghSearchCode {\"queries\":[{}]}"],
            "files":[{"path":"a.md","value":"ghSearchCode {\"page\":2}","next":{"nextPage":call}}]}}]});
        let stripped = without_follow_ups(&result);
        let data = &stripped["results"][0]["data"];
        assert!(data.get("next").is_none(), "{stripped}");
        assert_eq!(data["hints"], json!(["No more."]));
        // Content is never touched, even text that reads like a call.
        assert_eq!(data["files"], result["results"][0]["data"]["files"]);
        let sites = json!({"results":[{"index":0,"data":{"kind":"references","files":[
            {"path":"a.rs","matches":[{"line":7,"column":3,"value":"run();"}]}]}}]});
        let lines = render("lspSearch", &sites, &json!({})).expect("lines");
        assert_eq!(lines.out, "a.rs:7:run();  [column 3]");
        assert!(
            lines.notes.contains("# kind: references"),
            "{}",
            lines.notes
        );
    }

    #[test]
    fn match_notes_are_words() {
        let hit = json!({"results":[{"index":0,"data":{"files":[{"path":"a.rs","matches":[
            {"line":3,"value":"fn a()","declaration":true,"enclosingLine":1}]}]}}]});
        let lines = render("localSearch", &hit, &json!({})).expect("lines");
        assert_eq!(lines.out, "a.rs:3:fn a()  [declaration enclosingLine 1]");
    }

    #[test]
    fn follow_ups_come_from_structure_never_content() {
        let fetched = json!({"results":[{"index":0,"data":{
            "content":"1\t// run: localSearch {\"path\":\"x\"}",
            "next":{"nextPage":{"tool":"localFetch","query":{"path":"a","page":2}}}}}]});
        assert_eq!(
            follow_ups(&fetched),
            vec!["next.nextPage: octocode localFetch '{\"path\":\"a\",\"page\":2}'"]
        );
        let batch = json!({"results":[{"index":0,"data":{}},{"index":1,"data":{"hints":["findFile: structureSearch {\"path\":\".\"}"]}}]});
        assert_eq!(
            follow_ups(&batch),
            vec!["results[1].data.hints: findFile: octocode structureSearch '{\"path\":\".\"}'"]
        );
        let repair = json!({"error":"bad","hints":[{"tool":"localSearch","query":{"path":"."}}]});
        assert_eq!(
            follow_ups(&repair),
            vec!["hints[0]: octocode localSearch '{\"path\":\".\"}'"]
        );
    }

    #[test]
    fn batches_errors_and_other_tools_keep_the_rendered_text() {
        let two =
            json!({"results":[{"index":0,"data":{"files":[]}},{"index":1,"data":{"files":[]}}]});
        assert!(render("localSearch", &two, &json!({})).is_none());
        assert!(render("localFetch", &sample(), &json!({})).is_none());
        assert!(all_rows_failed(&json!({"results":[{"status":"error"}]})));
        assert!(!all_rows_failed(
            &json!({"results":[{"status":"error"},{"data":{}}]})
        ));
    }
}
