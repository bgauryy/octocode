use super::{one_line, words};
use serde_json::{Map, Value};

/// A hit value's lines with their numbers: numbered source when the query
/// asked for context (`None` when the value is not, such as a clipped
/// snippet), else the value's lines from `line`. An empty value is one empty
/// line; a `\r` stays, as rg prints it.
pub(super) fn value_lines(value: &str, line: u64, numbered: bool) -> Option<Vec<(u64, &str)>> {
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

/// Fields a line does not show, as its `[ … ]` note; `None` when there are
/// none.
pub(super) fn annotation(fields: &Map<String, Value>) -> Option<String> {
    (!fields.is_empty()).then(|| format!("[{}]", words(fields)))
}

/// A file's heading: its path once, its own fields (`file_note`) after it,
/// a blank line before it unless it opens the output.
pub(super) fn heading(path: &str, file_note: Option<&str>, out: &mut Vec<String>) {
    if !out.is_empty() {
        out.push(String::new());
    }
    let path = one_line(path);
    out.push(match file_note {
        Some(note) => format!("{path}  {note}"),
        None => path.into_owned(),
    });
}

/// The fields every hit of a file holds as a text value (`role call`): a
/// run of hits with the same value states it once, on the first of the run.
fn run_labels(matches: &[Value]) -> Vec<String> {
    let Some(first) = matches.first().and_then(Value::as_object) else {
        return Vec::new();
    };
    first
        .keys()
        .filter(|key| !matches!(key.as_str(), "value" | "enclosing"))
        .filter(|key| {
            matches
                .iter()
                .all(|item| item.get(key.as_str()).is_some_and(Value::is_string))
        })
        .cloned()
        .collect()
}

/// The hit lines of one file. Hit lines are `matchedLines` when present,
/// else `line..=endLine`; the rest are context. Fields the line does not
/// show follow the first hit line in `[ … ]`; a text field every hit holds
/// only where its value changes ([`run_labels`]).
pub(super) fn hits(
    path: &str,
    matches: &[Value],
    numbered: bool,
    file_note: Option<&str>,
    out: &mut Vec<String>,
) -> Option<()> {
    // With context, several hits or fields of its own, as `rg --heading`:
    // the path once, then `N:text` hits and `N-text` context, a blank line
    // between files. One hit stays one `path:N:text` line.
    let heading = numbered || matches.len() != 1 || file_note.is_some();
    if heading {
        self::heading(path, file_note, out);
    }
    let path = one_line(path);
    let mut last_line: Option<u64> = None;
    let mut last_owner: Option<String> = None;
    let labels = run_labels(matches);
    let mut last_labels = Map::new();
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
        for label in &labels {
            let value = extra.get(label).cloned();
            if last_labels.get(label) == value.as_ref() {
                extra.shift_remove(label);
            }
            last_labels.extend(value.map(|value| (label.clone(), value)));
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
            let mut entry = if heading {
                format!("{n}{sep}{text}")
            } else {
                format!("{path}{sep}{n}{sep}{text}")
            };
            if hit && !annotated {
                annotated = true;
                if let Some(note) = match_note(&mut extra, &mut last_owner) {
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

/// `[function build_searcher 992-1016]` when a hit's
/// enclosing declaration differs from the previous hit's in this file, then any other match field
/// as words (`declaration`).
pub(super) fn match_note(
    extra: &mut Map<String, Value>,
    last_owner: &mut Option<String>,
) -> Option<String> {
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
        let owner = format!("{} {}{span}", one_line(kind), one_line(name));
        if last_owner.as_deref() != Some(owner.as_str()) {
            parts.push(owner.clone());
        }
        *last_owner = Some(owner);
        extra.shift_remove("enclosing");
    }
    if !extra.is_empty() {
        parts.push(words(&std::mem::take(extra)));
    }
    (!parts.is_empty()).then(|| format!("[{}]", parts.join(" ")))
}
