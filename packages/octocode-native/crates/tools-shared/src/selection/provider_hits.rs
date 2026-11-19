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

/// One entry of a row's resolved `matches`: `{line, value}`, or the same
/// as one `"<line>\t<text>"` string.
pub(super) fn entry(entry: &Value) -> Option<(u64, &str)> {
    match entry.as_str() {
        Some(record) => {
            let (digits, text) = record.split_once('\t')?;
            if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            Some((digits.parse().ok()?, text))
        }
        None => Some((entry["line"].as_u64()?, entry["value"].as_str()?)),
    }
}

/// The entries of a row's resolved `matches`.
fn listed(item: &Value) -> impl Iterator<Item = (u64, &str)> {
    item["matches"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(entry)
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
                .or_else(|| entry(row).map(|(line, _)| line))
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

#[cfg(test)]
mod tests {
    use super::{Hit, hits, keep, state};
    use serde_json::json;

    /// A GitHub code row lists its lines as `{line, value}` or as
    /// `"<line>\t<text>"` strings: both read alike, and keeping a hit keeps
    /// its own lines in the form they came in.
    #[test]
    fn numbered_line_strings_read_as_entries() {
        let query = json!({"contextLines": {"before": 1, "after": 0}});
        for matches in [
            json!([{"line": 3, "value": "a"}, {"line": 4, "value": "needle"}, {"line": 9, "value": "needle(x)\tkept"}]),
            json!(["3\ta", "4\tneedle", "9\tneedle(x)\tkept"]),
        ] {
            let item = json!({"path": "src/x.rs", "matchedLines": [4, 9], "matches": matches});
            let found = hits(&item, &query, true);
            let windows = found
                .iter()
                .map(|hit| match hit {
                    Hit::Line { line, window } => (*line, window.clone()),
                    Hit::Row => (0, Vec::new()),
                })
                .collect::<Vec<_>>();
            assert_eq!(windows, [(4, vec![3, 4]), (9, vec![9])]);
            let judged = state(&item, &found[1]).expect("a state");
            assert_eq!(judged["target"]["text"], "needle(x)\tkept");
            let kept = keep(item.clone(), &found, &[0]).expect("kept");
            assert_eq!(kept["matches"], json!([matches[0], matches[1]]));
            assert_eq!(kept["matchedLines"], json!([4]));
        }
    }
}
