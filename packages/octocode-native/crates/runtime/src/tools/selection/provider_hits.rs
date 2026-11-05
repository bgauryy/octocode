//! The hits of a provider page: every line a repo-scoped search resolved, or
//! every index fragment of a file it could not read, is its own candidate.
//! Selection then keeps those hits and their windows in the page's rows.
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// One candidate of a provider page.
pub(super) enum Hit {
    /// A resolved hit line and the listed lines of its window.
    Line { line: u64, window: Vec<u64> },
    /// One index fragment, which has no line number.
    Fragment(usize),
    /// A listing without line numbers: judged as one unit.
    Listing,
    /// A row with nothing to judge: withheld by name, or holding no text.
    Row,
}

/// `<line>\t<text>` as a resolved listing writes it.
fn numbered(entry: &Value) -> Option<(u64, &str)> {
    let (line, text) = entry.as_str()?.split_once('\t')?;
    Some((line.parse().ok()?, text))
}

/// The hits of `item`, with the context lines the query asked for around each
/// resolved one.
pub(super) fn hits(item: &Value, query: &Value, judged: bool) -> Vec<Hit> {
    if !judged {
        return vec![Hit::Row];
    }
    if let Some(lines) = item["lines"].as_array() {
        let listed = lines
            .iter()
            .filter_map(numbered)
            .map(|(line, _)| line)
            .collect::<BTreeSet<_>>();
        if listed.is_empty() {
            return vec![Hit::Listing];
        }
        let matched = item["matchedLines"].as_array().map_or_else(
            || listed.iter().copied().collect(),
            |lines| lines.iter().filter_map(Value::as_u64).collect::<Vec<_>>(),
        );
        let reach = |field: &str| query["contextLines"][field].as_u64().unwrap_or(0);
        let (before, after) = (reach("before"), reach("after"));
        return matched
            .into_iter()
            .map(|line| Hit::Line {
                line,
                window: listed
                    .range(line.saturating_sub(before)..=line.saturating_add(after))
                    .copied()
                    .collect(),
            })
            .collect();
    }
    let fragments = item["matches"].as_array().map_or(0, Vec::len);
    if fragments == 0 {
        return vec![Hit::Row];
    }
    (0..fragments).map(Hit::Fragment).collect()
}

/// What the judge sees of one hit: its target and window, or the fragment
/// itself, which is the whole unit.
pub(super) fn state(item: &Value, hit: &Hit) -> Option<Value> {
    let path = item.get("path");
    match hit {
        Hit::Row => None,
        Hit::Listing => Some(json!({"path":path, "evidence":item["lines"]})),
        Hit::Fragment(index) => {
            let value = item["matches"][*index]["value"].as_str()?;
            (!value.trim().is_empty()).then(|| json!({"path":path, "evidence":value}))
        }
        Hit::Line { line, window } => {
            let rows = item["lines"].as_array()?;
            let lines = rows
                .iter()
                .filter_map(numbered)
                .filter(|(number, _)| window.contains(number))
                .map(|(number, text)| json!([number, text]))
                .collect::<Vec<_>>();
            let text = rows
                .iter()
                .filter_map(numbered)
                .find(|(number, _)| number == line)?
                .1;
            (!text.trim().is_empty())
                .then(|| json!({"path":path, "target":{"line":line, "text":text}, "lines":lines}))
        }
    }
}

/// `item` holding only the hits at `kept` (indexes into `hits`), or `None`
/// when it keeps none.
pub(super) fn keep(mut item: Value, hits: &[Hit], kept: &[usize]) -> Option<Value> {
    if kept.len() == hits.len() {
        return Some(item);
    }
    if kept.is_empty() {
        return None;
    }
    let chosen = kept.iter().map(|&index| &hits[index]);
    if let Some(Hit::Fragment(_)) = hits.first() {
        let indices = chosen
            .filter_map(|hit| match hit {
                Hit::Fragment(index) => Some(*index),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let mut at = 0;
        if let Some(fragments) = item["matches"].as_array_mut() {
            fragments.retain(|_| {
                at += 1;
                indices.contains(&(at - 1))
            });
        }
        return Some(item);
    }
    let (mut lines, mut matched) = (BTreeSet::new(), Vec::new());
    for hit in chosen {
        if let Hit::Line { line, window } = hit {
            lines.extend(window.iter().copied());
            matched.push(*line);
        }
    }
    if let Some(rows) = item["lines"].as_array_mut() {
        rows.retain(|row| numbered(row).is_some_and(|(line, _)| lines.contains(&line)));
    }
    if item.get("matchedLines").is_some() {
        item["matchedLines"] = json!(matched);
    }
    if item.get("matchCount").is_some() {
        item["matchCount"] = json!(matched.len());
    }
    Some(item)
}
