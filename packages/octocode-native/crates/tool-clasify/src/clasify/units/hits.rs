//! The hits of a search row as units. A search tool lists a file's hits in
//! its `matches`, a match over several lines stating its last in `endLine`,
//! and may join the context windows of nearby hits into one entry that lists
//! them all in `matchedLines` (localSearch), or list one entry per shown line
//! with the hits named in the row's `matchedLines` (ghSearchCode). Either way
//! each hit is one unit, judged as the in-tool
//! `classification` filter judges it: its target line, its own window, and
//! what the row already says around it (the innermost of its `declarations`
//! holding the line, its key path in the row's `matchKeys`).
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The context lines the query that listed a page asked around each hit.
#[derive(Clone, Copy)]
pub(super) struct Reach {
    before: u64,
    after: u64,
}

impl Reach {
    pub(super) fn of(query: &Value) -> Self {
        let lines = |side: &str| query["contextLines"][side].as_u64().unwrap_or(0);
        Self {
            before: lines("before"),
            after: lines("after"),
        }
    }

    /// Whether `line` is in the window the query asked around `first..=last`.
    fn holds(self, (first, last): (u64, u64), line: u64) -> bool {
        line.saturating_add(self.before) >= first && line <= last.saturating_add(self.after)
    }
}

/// One hit: its first and last line, and the listed lines of its window.
struct Hit {
    line: u64,
    last: u64,
    window: Vec<(u64, String)>,
}

/// `<line>\t<text>`, a numbered window's record.
fn numbered(record: &str) -> Option<(u64, &str)> {
    let (digits, text) = record.split_once('\t')?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((digits.parse().ok()?, text))
}

/// The source lines an entry's `value` lists: a numbered window (two or more
/// `<line>\t<text>` records in order, the entry's `line` among them), else
/// the value's own lines from the entry's `line` on. `None`: not a hit entry.
fn listed(entry: &Value) -> Option<Vec<(u64, String)>> {
    let line = entry["line"].as_u64()?;
    let value = entry["value"].as_str()?;
    let records = value.split('\n').collect::<Vec<_>>();
    let window = records
        .iter()
        .copied()
        .map(numbered)
        .collect::<Option<Vec<_>>>()
        .filter(|window| {
            window.len() > 1
                && window.windows(2).all(|pair| pair[0].0 < pair[1].0)
                && window.iter().any(|(number, _)| *number == line)
        });
    Some(match window {
        Some(window) => window
            .into_iter()
            .map(|(number, text)| (number, text.to_owned()))
            .collect(),
        None => (line..)
            .zip(records)
            .map(|(number, text)| (number, text.to_owned()))
            .collect(),
    })
}

fn line_numbers(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .collect()
}

/// The hits of `row`, in page order; `None` when its `matches` are not hit
/// entries (each with a numeric `line` and a text `value`).
fn hits(row: &Value, reach: Reach) -> Option<Vec<Hit>> {
    let entries = row["matches"]
        .as_array()
        .filter(|entries| !entries.is_empty())?;
    let listed = entries
        .iter()
        .map(|entry| Some((entry, listed(entry)?)))
        .collect::<Option<Vec<_>>>()?;
    let named = line_numbers(&row["matchedLines"]);
    if !named.is_empty()
        && entries
            .iter()
            .all(|entry| entry.get("matchedLines").is_none())
    {
        // One entry per shown line; the row names which are hits.
        let lines = listed
            .into_iter()
            .flat_map(|(_, lines)| lines)
            .collect::<BTreeMap<_, _>>();
        return Some(
            named
                .into_iter()
                .map(|line| Hit {
                    line,
                    last: line,
                    window: within(&lines, reach, (line, line)),
                })
                .collect(),
        );
    }
    let mut hits = Vec::new();
    for (entry, lines) in listed {
        let line = entry["line"].as_u64()?;
        let matched = line_numbers(&entry["matchedLines"]);
        if !matched.is_empty() {
            // Windows joined into one entry: each hit keeps its own.
            let lines = lines.into_iter().collect::<BTreeMap<_, _>>();
            hits.extend(matched.into_iter().map(|hit| Hit {
                line: hit,
                last: hit,
                window: within(&lines, reach, (hit, hit)),
            }));
            continue;
        }
        let last = entry["endLine"].as_u64().unwrap_or(line).max(line);
        hits.push(Hit {
            line,
            last,
            window: lines,
        });
    }
    Some(hits)
}

fn within(lines: &BTreeMap<u64, String>, reach: Reach, span: (u64, u64)) -> Vec<(u64, String)> {
    lines
        .iter()
        .filter(|(line, _)| reach.holds(span, **line))
        .map(|(line, text)| (*line, text.clone()))
        .collect()
}

/// The innermost of `row`'s declarations holding `line`.
fn enclosing(row: &Value, line: u64) -> Option<&Value> {
    row["declarations"]
        .as_array()?
        .iter()
        .filter_map(|declared| {
            let first = declared["line"].as_u64()?;
            let last = declared["endLine"].as_u64().unwrap_or(first);
            (first <= line && line <= last).then(|| (last - first, declared))
        })
        .min_by_key(|(span, _)| *span)
        .map(|(_, declared)| declared)
}

/// What the judge reads of one hit of the file row `row` at `path`: the
/// target (its line or lines and their text, the declaration around it, its
/// key path) and, when the window shows more than the target, the window.
fn state(row: &Value, path: &str, hit: &Hit) -> Value {
    let span = hit.line..=hit.last;
    let text = hit
        .window
        .iter()
        .filter(|(line, _)| span.contains(line))
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>();
    let mut target = json!({"line": hit.line});
    if hit.last > hit.line {
        target["lastLine"] = json!(hit.last);
    }
    if !text.is_empty() {
        target["text"] = json!(text.join("\n"));
    }
    if let Some(declared) = enclosing(row, hit.line) {
        target["enclosing"] = declared.clone();
    }
    if let Some(key) = row["matchKeys"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|key| key["line"].as_u64() == Some(hit.line))
        .and_then(|key| key["keyPath"].as_str())
    {
        target["key"] = json!(key);
    }
    let mut state = json!({"path": path, "target": target});
    if hit.window.iter().any(|(line, _)| !span.contains(line)) {
        state["lines"] = hit
            .window
            .iter()
            .map(|(line, text)| json!([line, text]))
            .collect();
    }
    state
}

/// Each hit of the search row `row` named `own`: its key `<own>:<line>` and
/// what the judge reads of it. `None` when `row` names no path or lists no
/// hit entries.
pub(super) fn units(row: &Value, own: &str, reach: Reach) -> Option<Vec<(String, Value)>> {
    let path = row["path"].as_str()?;
    Some(
        hits(row, reach)?
            .iter()
            .map(|hit| (format!("{own}:{}", hit.line), state(row, path, hit)))
            .collect(),
    )
}
