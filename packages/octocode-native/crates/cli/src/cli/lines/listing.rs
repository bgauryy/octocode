use super::hits::heading;
use super::{annotation, hits, one_line, value_lines};
use octocode_runtime_base::id::ToolId;
use serde_json::{Map, Value};

/// Whether `root` is `cwd`, the directory the command runs in.
pub(super) fn is_working_directory(root: &str, cwd: Option<&std::path::Path>) -> bool {
    let canonical = |path: &std::path::Path| std::fs::canonicalize(path).ok();
    cwd.and_then(canonical)
        .is_some_and(|cwd| canonical(std::path::Path::new(root)) == Some(cwd))
}

/// A listing: each `dir/` once, its entries indented under it exactly as
/// the result spells them, so a name is parsed for nothing; a group of one
/// file and no other field is one line, `dir/entry`. A group of one
/// directory keeps its heading: `dir/sub/` on one line would read as the
/// heading of `sub`, which a deeper listing prints next. A group's other
/// fields follow its `dir/` in `[ … ]`. A flat row (a string: an entry named
/// by its path, [`lists_flat_rows`]) is one line, as the result spells it,
/// file and directory alike: a ranked listing is such rows. Only on a page
/// that also lists groups does a flat directory row print as its group of
/// one does, since a `dir/` line there is a heading. `None` unless
/// `files` is such groups (`{dir, files: [entry, …]}`) and rows, whichever
/// tool lists them.
pub(super) fn listing(data: &Map<String, Value>, out: &mut Vec<String>) -> Option<()> {
    let groups = data.get("files")?.as_array()?;
    let mut lines = Vec::new();
    let headed = groups.iter().any(Value::is_object);
    for group in groups {
        if let Some(row) = group.as_str() {
            // Beside groups a line ending in `/` is a heading, so a flat
            // directory row there keeps the form its group of one has.
            match flat_directory(row).filter(|_| headed) {
                Some((dir, entry)) => {
                    lines.push(match one_line(dir) {
                        dir if dir.is_empty() => "./".to_owned(),
                        dir => format!("{dir}/"),
                    });
                    lines.push(format!("  {}", one_line(entry)));
                }
                None => lines.push(one_line(row).into_owned()),
            }
            continue;
        }
        let mut rest = group.as_object()?.clone();
        let dir = rest.shift_remove("dir")?;
        let dir = dir.as_str()?;
        let Value::Array(entries) = rest.shift_remove("files")? else {
            return None;
        };
        let mut head = match one_line(dir) {
            dir if dir.is_empty() => "./".to_owned(),
            dir if dir.ends_with('/') => dir.into_owned(),
            dir => format!("{dir}/"),
        };
        if let Some(note) = annotation(&rest) {
            head.push_str(&format!("  {note}"));
        } else if let [entry] = entries.as_slice()
            && let Some(entry) = entry.as_str().filter(|entry| !is_directory(entry))
        {
            lines.push(format!("{head}{}", one_line(entry)));
            continue;
        }
        lines.push(head);
        for entry in &entries {
            lines.push(format!("  {}", one_line(entry.as_str()?)));
        }
    }
    if lines.is_empty() {
        return None;
    }
    out.append(&mut lines);
    Some(())
}

/// The name of a listing entry (`<name>[/][ (<fields>)]`). The fields never
/// contain ` (`, so the last ` (` of an entry ending in `)` opens them.
pub(super) fn entry_name(entry: &str) -> &str {
    entry
        .strip_suffix(')')
        .and_then(|fields| fields.rsplit_once(" ("))
        .map_or(entry, |(name, _)| name)
}

/// Whether a listing entry names a directory: its name ends in `/`.
fn is_directory(entry: &str) -> bool {
    entry_name(entry).ends_with('/')
}

/// A flat row naming a directory, as the directory it lies in (empty: the
/// root's own entry) and its entry there; `None` for any other row.
fn flat_directory(row: &str) -> Option<(&str, &str)> {
    let path = entry_name(row).strip_suffix('/')?;
    Some(match path.rfind('/') {
        Some(at) => {
            let (dir, entry) = row.split_at_checked(at)?;
            (dir, entry.strip_prefix('/')?)
        }
        None => ("", row),
    })
}

/// Whether the flat rows of `data`'s `files` (strings: entries named by
/// their path, `<path>[/][ (<fields>)]`) print as a listing: those of the
/// tools whose flat row is a row path, spelled from where a group's `dir`
/// is spelled from (`ghStructure`: repository-relative; `structureSearch`:
/// workspace-relative, or from the row's `root`), so [`listing`] prints
/// each row as its one line. `false` for flat rows of any other tool: such a row prints as
/// `key: value` text, not as a listing, so no line names a path nothing
/// stated.
pub(super) fn lists_flat_rows(tool: ToolId, data: &Map<String, Value>) -> bool {
    let Some(Value::Array(rows)) = data.get("files") else {
        return true;
    };
    !rows.iter().any(Value::is_string)
        || matches!(tool, ToolId::GhStructure | ToolId::StructureSearch)
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
    // Line-first outline strings (`"15\tfunction name (end 67)"`) already
    // are the line each declaration prints as: text states them as they
    // are (`15: function name (end 67)`), adding and renaming nothing.
    if !symbols.is_empty() && symbols.iter().all(Value::is_string) {
        let rows: Vec<&str> = symbols.iter().filter_map(Value::as_str).collect();
        let grouped = file_note.is_some() || rows.len() != 1;
        if grouped {
            heading(path, file_note, out);
        }
        let shown = one_line(path);
        for row in rows {
            let line = match row.split_once('\t') {
                Some((number, text)) => format!("{}: {}", one_line(number), one_line(text)),
                None => one_line(row).into_owned(),
            };
            out.push(if grouped {
                line
            } else {
                format!("{shown}:{line}")
            });
        }
        return Some(());
    }
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
    let shown = one_line(path);
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
                        format!("{shown}:{site}")
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
            .map_or_else(|| name.to_string(), |name| one_line(name).into_owned());
        let kind = one_line(kind);
        let declared = declared.as_deref().map(one_line);
        let mut entry = match (&declared, grouped) {
            (Some(declared), _) => format!("{declared}:{line}: {indent}{kind} {name}"),
            (None, true) => format!("{line}: {indent}{kind} {name}"),
            (None, false) => format!("{shown}:{line}: {indent}{kind} {name}"),
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

/// How a location's `value` starts when its file could not be read: the
/// reason, not source.
const UNREAD_CONTENT: &str = "[content unavailable";

/// `lspSearch` results in `data.matches`. A location (`definition`,
/// `references`, …) prints its `value` as hits: `path:N:text`, or
/// `rg --heading` blocks when any spans lines (context or a declaration
/// body). Content spanning lines is numbered; one line at the row's own
/// `line` is served unnumbered. A body's omission marker is a mark for the trailer.
/// A symbol without a `value` (`workspaceSymbol`) prints as a declaration.
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
        let line = item.shift_remove("line")?.as_u64()?;
        let end = item.shift_remove("endLine");
        let column = item.shift_remove("column");
        let mut entry = Map::new();
        entry.insert("line".into(), line.into());
        if let Some(end) = end
            .as_ref()
            .and_then(Value::as_u64)
            .filter(|end| *end > line)
        {
            entry.insert("endLine".into(), end.into());
        }
        let is_hit = match item.shift_remove("value") {
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
        if let Some(column) = column {
            entry.insert("column".into(), column);
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
            let value = hit["value"].as_str()?;
            // One line at the row's own `line` is served as that line's text,
            // with no number; any other content is numbered. A location whose
            // file could not be read carries the reason instead: no line.
            let numbered_value = value_lines(value, line, true).is_some();
            if !numbered_value
                && (value.is_empty() || value.contains('\n') || value.starts_with(UNREAD_CONTENT))
            {
                return None;
            }
            let text = value_lines(value, line, numbered_value)?
                .into_iter()
                .find(|(n, _)| *n == line)?
                .1
                .to_owned();
            if let (Some(column), Some(symbol)) =
                (hit.get("column").and_then(Value::as_u64), symbol)
                && hit.get("importRole").is_none()
                && spelled_only_at(&text, symbol, column)
                && let Some(hit) = hit.as_object_mut()
            {
                hit.shift_remove("column");
            }
            // One source line each: `N\ttext` becomes the hit's text. Beside
            // numbered bodies, an unnumbered line takes its number.
            if !numbered {
                hit["value"] = text.into();
            } else if !numbered_value {
                hit["value"] = format!("{line}\t{text}").into();
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

/// Whether `text` spells `symbol` once, starting at the 1-based UTF-16
/// `column`: the column then says nothing the line does not.
fn spelled_only_at(text: &str, symbol: &str, column: u64) -> bool {
    text.split_once(symbol).is_some_and(|(prefix, suffix)| {
        !suffix.contains(symbol) && prefix.encode_utf16().count() as u64 + 1 == column
    })
}

/// A page state ([`super::follow::is_page`]) in the one form every page
/// prints in, whichever fields it states. A page that states where it
/// starts prints its place: `lines 166-247 of 247`, or `lines 166-247`
/// without a total, so no number reads as "delivered so far". A page that
/// states no offset prints its count (`10 of 37 lines`, `10 lines this
/// page` without a total, `total 37 lines` without a length), and an empty
/// page `0 of 37 lines after 20`. Any other field follows as words. A unit
/// names the numbers, so without one they print bare. Whether more follows
/// is the reply's `next` to say. `None` for any other object.
fn page(fields: &Map<String, Value>) -> Option<String> {
    if !super::follow::is_page(fields) {
        return None;
    }
    let mut rest = fields.clone();
    let length = take_count(&mut rest, "length");
    let total = take_count(&mut rest, "total");
    let offset = take_count(&mut rest, "offset");
    let unit = match rest.get("unit") {
        Some(Value::String(unit)) => Some(one_line(unit).to_string()),
        _ => None,
    };
    if unit.is_some() {
        rest.shift_remove("unit");
    }
    let mut parts = vec![page_place(unit.as_deref(), offset, length, total)];
    parts.retain(|part| !part.is_empty());
    if !rest.is_empty() {
        parts.push(words(&rest));
    }
    Some(parts.join(" "))
}

/// The numbers of a page state as one phrase (see [`page`]): the place form
/// when `offset` and a non-empty `length` are stated, else the count form.
pub(super) fn page_place(
    unit: Option<&str>,
    offset: Option<u64>,
    length: Option<u64>,
    total: Option<u64>,
) -> String {
    let named = |numbers: String| match unit {
        Some(unit) => format!("{numbers} {unit}"),
        None => numbers,
    };
    if let (Some(offset), Some(length)) = (offset, length.filter(|length| *length > 0)) {
        let range = format!("{}-{}", offset + 1, offset + length);
        let range = match unit {
            Some(unit) => format!("{unit} {range}"),
            None => range,
        };
        return match total {
            Some(total) => format!("{range} of {total}"),
            None => range,
        };
    }
    let mut out = match (length, total) {
        (Some(length), Some(total)) => named(format!("{length} of {total}")),
        // A length with no total is this page's size, said so.
        (Some(length), None) => format!("{} this page", named(length.to_string())),
        (None, Some(total)) => named(format!("total {total}")),
        (None, None) => String::new(),
    };
    // Offset 0 is the reading without `after`.
    if let Some(offset) = offset.filter(|offset| *offset > 0) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&format!("after {offset}"));
    }
    out
}

/// The whole number `fields` states under `key`, taken out of them; any
/// other value stays, to print as words.
fn take_count(fields: &mut Map<String, Value>, key: &str) -> Option<u64> {
    let count = fields.get(key)?.as_u64()?;
    fields.shift_remove(key);
    Some(count)
}

/// Fields as words: `key value` for a scalar (`key true`, `key false`) or a list of
/// scalars, `key (words)` for a nested object, a page block as its count
/// ([`page`]); JSON only for a list of objects or lists.
pub(super) fn words(fields: &Map<String, Value>) -> String {
    if let Some(page) = page(fields) {
        return page;
    }
    fields
        .iter()
        .map(|(key, value)| match value {
            Value::String(text) => format!("{key} {}", one_line(text)),
            Value::Object(inner) => match page(inner) {
                Some(page) => format!("{key} {page}"),
                None => format!("{key} ({})", words(inner)),
            },
            Value::Array(items)
                if items
                    .iter()
                    .all(|item| !item.is_object() && !item.is_array()) =>
            {
                let items: Vec<String> = items
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .map_or_else(|| item.to_string(), |item| one_line(item).into_owned())
                    })
                    .collect();
                format!("{key} {}", items.join(","))
            }
            other => format!("{key} {other}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}
