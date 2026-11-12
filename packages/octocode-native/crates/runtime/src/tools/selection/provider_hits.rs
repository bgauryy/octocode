//! The hits of a provider page: every line a repo-scoped search resolved is
//! its own candidate. Selection then keeps those hits and their windows in
//! the page's rows.
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// One candidate of a provider page.
pub(super) enum Hit {
    /// A resolved hit line and the listed lines of its window.
    Line { line: u64, window: Vec<u64> },
    /// A row with nothing to judge: withheld by name, or holding no text.
    Row,
}

/// The `{line, value}` entries of a row's resolved `matches`.
fn listed(item: &Value) -> impl Iterator<Item = (u64, &str)> {
    item["matches"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| Some((entry["line"].as_u64()?, entry["value"].as_str()?)))
}

/// The hits of `item`, with the context lines the query asked for around each
/// resolved one.
pub(super) fn hits(item: &Value, query: &Value, judged: bool) -> Vec<Hit> {
    let listed = listed(item).map(|(line, _)| line).collect::<BTreeSet<_>>();
    if !judged || listed.is_empty() {
        return vec![Hit::Row];
    }
    let matched = item["matchedLines"].as_array().map_or_else(
        || listed.iter().copied().collect(),
        |lines| lines.iter().filter_map(Value::as_u64).collect::<Vec<_>>(),
    );
    let reach = |field: &str| query["contextLines"][field].as_u64().unwrap_or(0);
    let (before, after) = (reach("before"), reach("after"));
    matched
        .into_iter()
        .map(|line| Hit::Line {
            line,
            window: listed
                .range(line.saturating_sub(before)..=line.saturating_add(after))
                .copied()
                .collect(),
        })
        .collect()
}

/// What the judge sees of one hit: its target and window.
pub(super) fn state(item: &Value, hit: &Hit) -> Option<Value> {
    let Hit::Line { line, window } = hit else {
        return None;
    };
    let lines = listed(item)
        .filter(|(number, _)| window.contains(number))
        .map(|(number, text)| json!([number, text]))
        .collect::<Vec<_>>();
    let text = listed(item).find(|(number, _)| number == line)?.1;
    (!text.trim().is_empty()).then(
        || json!({"path":item.get("path"), "target":{"line":line, "text":text}, "lines":lines}),
    )
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
    let (mut lines, mut matched) = (BTreeSet::new(), Vec::new());
    for hit in chosen {
        if let Hit::Line { line, window } = hit {
            lines.extend(window.iter().copied());
            matched.push(*line);
        }
    }
    if let Some(rows) = item["matches"].as_array_mut() {
        rows.retain(|row| {
            row["line"]
                .as_u64()
                .is_some_and(|line| lines.contains(&line))
        });
    }
    if item.get("matchedLines").is_some() {
        item["matchedLines"] = json!(matched);
    }
    if item.get("matchCount").is_some() {
        item["matchCount"] = json!(matched.len());
    }
    Some(item)
}
