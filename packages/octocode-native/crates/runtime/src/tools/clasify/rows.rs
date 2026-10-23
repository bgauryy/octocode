//! Rows as clasify reads them: one parsed row per admitted query, with
//! unique ids.
use super::{Input, Row, Target};
use serde_json::Value;

/// A row contract validation admitted. The provider receives the validated
/// row's own values: a typed form would reorder choice labels and map keys,
/// which are semantic input (the contract tests keep the shapes aligned).
fn parse(position: usize, row: &Value) -> Row {
    let questions = row["questions"].as_object();
    let input = if row["operation"] == "sections" {
        Input::Sections(row.clone())
    } else if row["operation"] == "models" {
        Input::Models
    } else if row.get("state").is_some() {
        Input::Judge(Target::State(row["state"].clone()))
    } else {
        Input::Judge(Target::Source(row["source"].clone()))
    };
    Row {
        id: row["id"]
            .as_str()
            .map_or_else(|| format!("q{position}"), str::to_owned),
        model: row["model"].as_str().map(str::to_owned),
        keys: questions
            .into_iter()
            .flat_map(|questions| questions.keys().cloned())
            .collect(),
        questions: questions
            .into_iter()
            .flat_map(|questions| questions.values().cloned())
            .collect(),
        input,
        assets: row.get("assets").cloned(),
        long_context: row["longContext"].as_bool() == Some(true),
    }
}

pub(super) fn parse_rows<R: std::borrow::Borrow<Value>>(rows: &[R]) -> Vec<Row> {
    let mut reserved = rows
        .iter()
        .filter_map(|row| row.borrow()["id"].as_str().map(str::to_owned))
        .collect::<std::collections::HashSet<_>>();
    rows.iter()
        .enumerate()
        .map(|(position, row)| {
            let row = row.borrow();
            let mut parsed = parse(position, row);
            if row.get("id").is_none() {
                let mut next = position;
                while !reserved.insert(parsed.id.clone()) {
                    next += 1;
                    parsed.id = format!("q{next}");
                }
            }
            parsed
        })
        .collect()
}
