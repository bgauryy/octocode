//! Original batch row positions and placement of rejected rows on page one.

use crate::response::outcome::ToolOutcome;
use serde_json::{Value, json};

/// Original indices of the admitted rows: every input position not rejected.
pub(crate) fn admitted_indices(admitted: usize, rejected: &[usize]) -> Vec<usize> {
    (0..admitted + rejected.len())
        .filter(|position| !rejected.contains(position))
        .take(admitted)
        .collect()
}

/// The query each executed row answers. A row runs under its position in the
/// request it was admitted from and is published under this query's index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RowOrigin {
    /// A fresh request: a row's position is its index.
    Request,
    /// Saved work of one query, including fan-out reads.
    Query(usize),
    /// Saved work of several: the query of the row at each position.
    Rows(Vec<usize>),
}

impl RowOrigin {
    /// The index of the row at `position`.
    pub(crate) fn of(&self, position: usize) -> usize {
        match self {
            Self::Request => position,
            Self::Query(index) => *index,
            Self::Rows(indices) => indices.get(position).copied().unwrap_or(position),
        }
    }

    /// The indices of the rows at `positions`.
    pub(crate) fn indices(&self, positions: &[usize]) -> Vec<usize> {
        positions
            .iter()
            .map(|position| self.of(*position))
            .collect()
    }
}

/// Publish each row of an envelope under the query it answers.
pub(crate) fn inherit(structured: &mut Value, origin: &RowOrigin) {
    if *origin == RowOrigin::Request {
        return;
    }
    for row in structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let position = row["index"]
            .as_u64()
            .map_or(0, octocode_engine::num::saturating_usize);
        row["index"] = json!(origin.of(position));
    }
}

/// Take the rejected rows out of a settled envelope, leaving the body a page
/// continuation reproduces; returns the rejected rows. Positions refer to the
/// settled array, since fan-out continuations can share an originating query
/// index.
pub(crate) fn split_rejected(structured: &mut Value, rejected: &[usize]) -> Vec<Value> {
    let Some(rows) = structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
    else {
        return Vec::new();
    };
    let (mut kept, mut taken) = (Vec::new(), Vec::new());
    for (position, row) in rows.into_iter().enumerate() {
        if rejected.contains(&position) {
            taken.push(row);
        } else {
            kept.push(row);
        }
    }
    structured["results"] = Value::Array(kept);
    taken
}

/// Show the rejected rows on the first page, in `results` at their indices.
pub(crate) fn attach_rejected(outcome: &mut ToolOutcome, rejected: Vec<Value>) {
    if rejected.is_empty() {
        return;
    }
    outcome.structured_content.edit(|structured| {
        if !structured.get("results").is_some_and(Value::is_array) {
            structured["results"] = json!([]);
        }
        if let Some(rows) = structured["results"].as_array_mut() {
            rows.extend(rejected);
            rows.sort_by_key(|row| row["index"].as_u64());
        }
    });
}
