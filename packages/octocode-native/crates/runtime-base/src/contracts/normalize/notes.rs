//! What a leniently read call says: one warning naming each reading once,
//! with the rows it was made on.

use super::{Note, sketch};
use serde_json::Value;

/// How the value at `label` was read: from the JSON its text holds, as the
/// one-item list its type declares, or as the value it now holds. The
/// reading is stated, never an instruction: the call ran as read.
pub(super) fn reading(label: &str, received: &Value, meant: &Value) -> String {
    let from_text = received
        .as_str()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .is_some_and(|parsed| parsed == *meant);
    if from_text {
        return format!("{label} from JSON text");
    }
    if meant
        .as_array()
        .is_some_and(|items| items.len() == 1 && items[0] == *received)
    {
        return format!("{label} as a list");
    }
    format!("{label}: {}", sketch(meant))
}

/// `path` as a note names it: object keys joined by `.`, a list item by its
/// index in brackets (`resources.a`, `assets[0].source`).
pub(super) fn label(path: &[String]) -> String {
    let mut label = String::new();
    for segment in path {
        if segment.parse::<usize>().is_ok() {
            label.push('[');
            label.push_str(segment);
            label.push(']');
        } else {
            if !label.is_empty() {
                label.push('.');
            }
            label.push_str(segment);
        }
    }
    label
}

/// The call's one warning: `read ` and each distinct reading once, the
/// request's first, then each row's prefixed with the rows that share it
/// (`queries[0,2]: exclude from JSON text`), so a batch that sent one form
/// on every row names it once. No readings, no warning.
pub(super) fn per_call(notes: Vec<Note>) -> Vec<String> {
    let mut grouped: Vec<(Option<Vec<usize>>, String)> = Vec::new();
    for (index, note) in notes {
        let same = |(rows, said): &&mut (Option<Vec<usize>>, String)| {
            *said == note && rows.is_some() == index.is_some()
        };
        match (grouped.iter_mut().find(same), index) {
            (Some((Some(rows), _)), Some(index)) if !rows.contains(&index) => rows.push(index),
            (Some(_), _) => {}
            (None, index) => grouped.push((index.map(|index| vec![index]), note)),
        }
    }
    grouped.sort_by_key(|(rows, _)| rows.as_ref().map_or(0, |rows| rows[0] + 1));
    // Readings of the same rows share one prefix: `queries[0]: a, b`.
    let mut said: Vec<(String, Vec<String>)> = Vec::new();
    for (rows, note) in grouped {
        let prefix = rows.map(|rows| {
            let rows = rows
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!("queries[{rows}]: ")
        });
        let prefix = prefix.unwrap_or_default();
        match said.iter_mut().find(|(known, _)| *known == prefix) {
            Some((_, readings)) => readings.push(note),
            None => said.push((prefix, vec![note])),
        }
    }
    if said.is_empty() {
        return Vec::new();
    }
    let line = said
        .into_iter()
        .map(|(prefix, readings)| format!("{prefix}{}", shared_readings(readings).join(", ")))
        .collect::<Vec<_>>()
        .join("; ");
    vec![format!("read {line}")]
}

/// The ways a value is read that several fields can share, said once after
/// their labels (`include, rule from JSON text`).
const SHARED: [&str; 2] = [" from JSON text", " as a list"];

/// `readings` of one set of rows, the fields read the same way named
/// together, in the order each way was first made.
fn shared_readings(readings: Vec<String>) -> Vec<String> {
    let mut merged: Vec<(Option<&str>, Vec<String>)> = Vec::new();
    for reading in readings {
        let way = SHARED.into_iter().find(|way| reading.ends_with(way));
        let label = way
            .and_then(|way| reading.strip_suffix(way))
            .unwrap_or(&reading)
            .to_owned();
        match merged
            .iter_mut()
            .find(|(known, _)| way.is_some() && *known == way)
        {
            Some((_, labels)) => labels.push(label),
            None => merged.push((way, vec![label])),
        }
    }
    merged
        .into_iter()
        .map(|(way, labels)| format!("{}{}", labels.join(", "), way.unwrap_or("")))
        .collect()
}
