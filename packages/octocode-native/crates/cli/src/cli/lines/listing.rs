use super::{Lines, hits, notes, value_lines};
use serde_json::{Map, Value};

/// Whether `root` is `cwd`, the directory the command runs in.
pub(super) fn is_working_directory(root: &str, cwd: Option<&std::path::Path>) -> bool {
    let canonical = |path: &std::path::Path| std::fs::canonicalize(path).ok();
    cwd.and_then(canonical)
        .is_some_and(|cwd| canonical(std::path::Path::new(root)) == Some(cwd))
}

/// The only row of a single-query result, unless it failed.
pub(super) fn single_row(structured: &Value) -> Option<&Map<String, Value>> {
    let results = structured.get("results")?.as_array()?;
    let [row] = results.as_slice() else {
        return None;
    };
    let row = row.as_object()?;
    (row.get("status").and_then(Value::as_str) != Some("error")).then_some(row)
}

/// `structureSearch files` as fd prints it: one root-relative path per line,
/// a directory's ending in `/`. A `details` entry's fields (every one the
/// result carries, the size as `N B`) follow in `[ … ]`; a `paths` entry is
/// its bare name, parsed for nothing, so a name spelling ` (…)` stays whole.
/// The tree view keeps the rendered text.
pub(super) fn listing(
    structured: &Value,
    query: &Value,
    cwd: Option<&std::path::Path>,
) -> Option<Lines> {
    let asked = query["queries"].get(0).unwrap_or(query);
    if asked["operation"].as_str() != Some("files") {
        return None;
    }
    let detailed = asked["view"].as_str() != Some("paths");
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
            let entry = entry.as_str()?;
            let (name, fields) = if detailed {
                entry_fields(entry)
            } else {
                (entry, Vec::new())
            };
            let path = if dir.is_empty() || dir == "." {
                name.to_owned()
            } else {
                format!("{dir}/{name}")
            };
            let shown: Vec<String> = fields
                .iter()
                // The bare number is the size in bytes: say so.
                .map(|field| {
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
        notes: notes(structured, query, row, data.clone(), Vec::new(), cwd),
    })
}

/// A `details` entry's name and fields: the fields follow the last ` (` of
/// an entry ending in `)` (sizes, `symlink`, line counts,
/// `modified=<instant>`); they never spell ` (` themselves, whatever the
/// name holds.
pub(super) fn entry_fields(entry: &str) -> (&str, Vec<&str>) {
    match entry
        .strip_suffix(')')
        .and_then(|head| head.rsplit_once(" ("))
    {
        Some((name, fields)) => (name, fields.split(", ").collect()),
        None => (entry, Vec::new()),
    }
}

/// `path:line: kind name` per declaration, members indented under their
/// parent, as the outline nests them. Span, doc comments and other fields follow.
/// A call row declared in another file (`path`, with `line` the
/// declaration's) prints under that file, and each site under the row's.
pub(super) fn declarations(
    path: &str,
    symbols: &[Value],
    nested: bool,
    out: &mut Vec<String>,
) -> Option<()> {
    let indent = if nested { "  " } else { "" };
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
        let location = declared.as_deref().unwrap_or(path);
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
        let mut entry = format!("{location}:{line}: {indent}{kind} {name}");
        if !note.is_empty() {
            entry.push_str(&format!("  [{}]", note.join(" ")));
        }
        out.push(entry);
        if let Some(Value::Array(members)) = members {
            declarations(path, &members, true, out)?;
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
/// a declaration body). A body's omission marker is a file note. A symbol
/// without content (`workspaceSymbol`) prints as a declaration.
pub(super) fn locations(
    matches: &[Value],
    out: &mut Vec<String>,
    notes: &mut Vec<String>,
) -> Option<()> {
    let mut hits_by_path: Vec<(String, Vec<Value>)> = Vec::new();
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
        let Some(content) = item.shift_remove("content") else {
            // A symbol: `path:N: kind name  [column C …]`.
            entry.insert("symbolName".into(), item.shift_remove("symbolName")?);
            if let Some(kind) = item.shift_remove("kind") {
                entry.insert("kind".into(), kind);
            }
            if let Some(column) = range.get("startCharacter") {
                entry.insert("column".into(), column.clone());
            }
            entry.append(&mut item);
            declarations(&path, &[Value::Object(entry)], false, out)?;
            continue;
        };
        let content = content.as_str()?.trim_end_matches('\n');
        let value = match content.rsplit_once('\n') {
            Some((body, last)) if last.starts_with("… ") => {
                notes.push(format!("file {path}:{line}: {last}"));
                body
            }
            _ => content,
        };
        entry.insert("value".into(), value.into());
        if let Some(column) = range.get("startCharacter") {
            entry.insert("column".into(), column.clone());
        }
        entry.append(&mut item);
        match hits_by_path.last_mut() {
            Some((last, group)) if *last == path => group.push(Value::Object(entry)),
            _ => hits_by_path.push((path, vec![Value::Object(entry)])),
        }
    }
    let numbered = hits_by_path.iter().flat_map(|(_, group)| group).any(|hit| {
        hit["value"]
            .as_str()
            .is_some_and(|value| value.contains('\n'))
    });
    for (path, mut group) in hits_by_path {
        if !numbered {
            // One source line each: `N\ttext` becomes the hit's text.
            for hit in &mut group {
                let line = hit["line"].as_u64()?;
                let text = value_lines(hit["value"].as_str()?, line, true)?
                    .first()?
                    .1
                    .to_owned();
                hit["value"] = text.into();
            }
        }
        hits(&path, &group, numbered, out)?;
    }
    Some(())
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
