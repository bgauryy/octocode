use super::words;
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

/// The hit lines of one file. Hit lines are `matchedLines` when present,
/// else `line..=endLine`; the rest are context. Fields the line does not
/// show follow the first hit line in `[ … ]`.
pub(super) fn hits(
    path: &str,
    matches: &[Value],
    numbered: bool,
    out: &mut Vec<String>,
) -> Option<()> {
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
pub(super) fn match_note(extra: &mut Map<String, Value>) -> Option<String> {
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
