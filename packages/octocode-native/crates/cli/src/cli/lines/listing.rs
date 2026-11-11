use super::hits::heading;
use super::{annotation, hits, value_lines};
use serde_json::{Map, Value};

/// Whether `root` is `cwd`, the directory the command runs in.
pub(super) fn is_working_directory(root: &str, cwd: Option<&std::path::Path>) -> bool {
    let canonical = |path: &std::path::Path| std::fs::canonicalize(path).ok();
    cwd.and_then(canonical)
        .is_some_and(|cwd| canonical(std::path::Path::new(root)) == Some(cwd))
}

/// A listing: each `dir/` once, its entries indented under it exactly as
/// the result spells them, so a name is parsed for nothing. A group's other
/// fields follow its `dir/` in `[ … ]`. `None` unless `files` is such groups
/// (`{dir, files: [entry, …]}`), whichever tool lists them.
pub(super) fn listing(data: &Map<String, Value>, out: &mut Vec<String>) -> Option<()> {
    let groups = data.get("files")?.as_array()?;
    let mut lines = Vec::new();
    for group in groups {
        let mut rest = group.as_object()?.clone();
        let dir = rest.shift_remove("dir")?;
        let dir = dir.as_str()?;
        let Value::Array(entries) = rest.shift_remove("files")? else {
            return None;
        };
        let mut head = match dir {
            "" => "./".to_owned(),
            dir if dir.ends_with('/') => dir.to_owned(),
            dir => format!("{dir}/"),
        };
        if let Some(note) = annotation(&rest) {
            head.push_str(&format!("  {note}"));
        }
        lines.push(head);
        for entry in &entries {
            let entry = entry.as_str().filter(|entry| !entry.contains('\n'))?;
            lines.push(format!("  {entry}"));
        }
    }
    if lines.is_empty() {
        return None;
    }
    out.append(&mut lines);
    Some(())
}

/// The declarations of one file, as hits print: several, nested ones or a
/// file with fields of its own under the path once (`line: kind name`), a
/// lone declaration as `path:line: kind name`.
pub(super) fn outline_rows(
    path: &str,
    symbols: &[Value],
    file_note: Option<&str>,
    out: &mut Vec<String>,
) -> Option<()> {
    let nests = |symbol: &Value| {
        symbol
            .get("members")
            .is_some_and(|members| !members.is_null())
    };
    let grouped = file_note.is_some() || symbols.len() != 1 || symbols.iter().any(nests);
    if grouped {
        heading(path, file_note, out);
    }
    declarations(path, symbols, "", grouped, out)
}

/// `line: kind name` per declaration under its file's heading (`grouped`),
/// else `path:line: kind name`; members indented under their parent, as the
/// outline nests them. Span, doc comments and other fields follow.
/// A call row declared in another file (`path`, with `line` the
/// declaration's) names that file on its line, and each site the row's.
fn declarations(
    path: &str,
    symbols: &[Value],
    indent: &str,
    grouped: bool,
    out: &mut Vec<String>,
) -> Option<()> {
    for symbol in symbols {
        let mut symbol = symbol.as_object()?.clone();
        let line = symbol.shift_remove("line")?.as_u64()?;
        // A module-level caller has no name of its own: the file's stands in.
        let name = symbol.shift_remove("symbolName").unwrap_or_else(|| {
            Value::String(
                std::path::Path::new(path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_owned()),
            )
        });
        let kind = symbol.shift_remove("kind");
        let kind = kind.as_ref().and_then(Value::as_str).unwrap_or("symbol");
        let members = symbol.shift_remove("members");
        let declared = match symbol.get("path").and_then(Value::as_str) {
            Some(declared) => {
                let declared = declared.to_owned();
                symbol.shift_remove("path");
                Some(declared)
            }
            None => None,
        };
        let mut note = Vec::new();
        if let Some(end) = symbol.shift_remove("endLine").and_then(|end| end.as_u64()) {
            note.push(format!("{line}-{end}"));
        }
        if let Some(doc) = symbol
            .shift_remove("docStartLine")
            .and_then(|doc| doc.as_u64())
        {
            // Named like every other field: no abbreviation to look up.
            note.push(format!("docStartLine {doc}"));
        }
        // Call sites as `line:column`: `sites 83:36,90:4`; each names its
        // file (`sites a.ts:83:36`) when the declaration is elsewhere.
        if let Some(sites) = symbol
            .get("sites")
            .and_then(Value::as_array)
            .and_then(|sites| sites.iter().map(site).collect::<Option<Vec<_>>>())
        {
            symbol.shift_remove("sites");
            let sites: Vec<String> = sites
                .into_iter()
                .map(|site| {
                    if declared.is_some() {
                        format!("{path}:{site}")
                    } else {
                        site
                    }
                })
                .collect();
            note.push(format!("sites {}", sites.join(",")));
        }
        if !symbol.is_empty() {
            note.push(words(&symbol));
        }
        let name = name
            .as_str()
            .map_or_else(|| name.to_string(), str::to_owned);
        let mut entry = match (&declared, grouped) {
            (Some(declared), _) => format!("{declared}:{line}: {indent}{kind} {name}"),
            (None, true) => format!("{line}: {indent}{kind} {name}"),
            (None, false) => format!("{path}:{line}: {indent}{kind} {name}"),
        };
        if !note.is_empty() {
            entry.push_str(&format!("  [{}]", note.join(" ")));
        }
        out.push(entry);
        if let Some(Value::Array(members)) = members {
            declarations(path, &members, &format!("{indent}  "), grouped, out)?;
        }
    }
    Some(())
}

/// A call site `{line, column}` as `line:column`; `None` for any other value.
pub(super) fn site(value: &Value) -> Option<String> {
    let site = value.as_object()?;
    let line = site.get("line")?.as_u64()?;
    match (site.get("column").and_then(Value::as_u64), site.len()) {
        (Some(column), 2) => Some(format!("{line}:{column}")),
        (None, 1) => Some(line.to_string()),
        _ => None,
    }
}

/// `lspSearch` results in `data.matches`. A location (`definition`,
/// `references` with context, …) prints its numbered `content` as hits:
/// `path:N:text`, or `rg --heading` blocks when any spans lines (context or
/// a declaration body). A body's omission marker is a mark for the trailer.
/// A symbol without content (`workspaceSymbol`) prints as a declaration.
/// Consecutive rows of one file share its heading; hits on one line print
/// it once with each `column` ([`one_line_each`]), and a lone hit's column
/// is left to its text when the text spells `symbol` there and nowhere else.
pub(super) fn locations(
    matches: &[Value],
    symbol: Option<&str>,
    out: &mut Vec<String>,
    marks: &mut Vec<String>,
) -> Option<()> {
    // (path, whether the rows are hits, the rows)
    let mut groups: Vec<(String, bool, Vec<Value>)> = Vec::new();
    for item in matches {
        let mut item = item.as_object()?.clone();
        let path = item.shift_remove("path")?.as_str()?.to_owned();
        let range = item.shift_remove("displayRange")?;
        let line = range.get("startLine")?.as_u64()?;
        let mut entry = Map::new();
        entry.insert("line".into(), line.into());
        if let Some(end) = range
            .get("endLine")
            .and_then(Value::as_u64)
            .filter(|end| *end > line)
        {
            entry.insert("endLine".into(), end.into());
        }
        let is_hit = match item.shift_remove("content") {
            Some(content) => {
                let content = content.as_str()?.trim_end_matches('\n');
                let value = match content.rsplit_once('\n') {
                    Some((body, last)) if last.starts_with("… ") => {
                        marks.push(format!("file {path}:{line}: {last}"));
                        body
                    }
                    _ => content,
                };
                entry.insert("value".into(), value.into());
                true
            }
            // A symbol: `path:N: kind name  [column C …]`.
            None => {
                entry.insert("symbolName".into(), item.shift_remove("symbolName")?);
                if let Some(kind) = item.shift_remove("kind") {
                    entry.insert("kind".into(), kind);
                }
                false
            }
        };
        if let Some(column) = range.get("startCharacter") {
            entry.insert("column".into(), column.clone());
        }
        entry.append(&mut item);
        match groups.last_mut() {
            Some((last, hit, group)) if *last == path && *hit == is_hit => {
                group.push(Value::Object(entry));
            }
            _ => groups.push((path, is_hit, vec![Value::Object(entry)])),
        }
    }
    let numbered = groups
        .iter()
        .filter(|(_, is_hit, _)| *is_hit)
        .flat_map(|(_, _, group)| group)
        .any(|hit| {
            hit["value"]
                .as_str()
                .is_some_and(|value| value.contains('\n'))
        });
    for (path, is_hit, group) in groups {
        if !is_hit {
            outline_rows(&path, &group, None, out)?;
            continue;
        }
        let mut group = one_line_each(group);
        for hit in &mut group {
            let line = hit["line"].as_u64()?;
            let text = value_lines(hit["value"].as_str()?, line, true)?
                .into_iter()
                .find(|(n, _)| *n == line)?
                .1
                .to_owned();
            if let (Some(column), Some(symbol)) =
                (hit.get("column").and_then(Value::as_u64), symbol)
                && spelled_only_at(&text, symbol, column)
                && let Some(hit) = hit.as_object_mut()
            {
                hit.shift_remove("column");
            }
            // One source line each: `N\ttext` becomes the hit's text.
            if !numbered {
                hit["value"] = text.into();
            }
        }
        hits(&path, &group, numbered, None, out)?;
    }
    Some(())
}

/// Consecutive hits of one line that differ only in `column`, as one hit
/// with every column: the line prints once, `[column 19,88]`.
fn one_line_each(group: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::new();
    for hit in group {
        let same = |last: &Value| {
            let strip = |hit: &Value| {
                let mut hit = hit.as_object().cloned().unwrap_or_default();
                hit.shift_remove("column");
                hit
            };
            strip(last) == strip(&hit)
        };
        match merged.last_mut() {
            Some(last) if same(last) => {
                let mut columns = match last["column"].take() {
                    Value::Array(columns) => columns,
                    Value::Null => Vec::new(),
                    column => vec![column],
                };
                columns.extend(hit.get("column").cloned());
                last["column"] = Value::Array(columns);
            }
            _ => merged.push(hit),
        }
    }
    merged
}

/// Whether `text` spells `symbol` once, starting at the 1-based character
/// `column`: the column then says nothing the line does not.
fn spelled_only_at(text: &str, symbol: &str, column: u64) -> bool {
    let mut found = text.match_indices(symbol);
    match (found.next(), found.next()) {
        (Some((at, _)), None) => {
            text.char_indices().take_while(|(i, _)| *i < at).count() as u64 + 1 == column
        }
        _ => false,
    }
}

/// Fields as words: `key` for true, `key value` for a scalar or a list of
/// scalars, `key (words)` for a nested object; JSON only for a list of
/// objects or lists.
pub(super) fn words(fields: &Map<String, Value>) -> String {
    fields
        .iter()
        .map(|(key, value)| match value {
            Value::Bool(true) => key.clone(),
            Value::String(text) => format!("{key} {text}"),
            Value::Object(inner) => format!("{key} ({})", words(inner)),
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
