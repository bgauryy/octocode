//! Rows as clasify reads them, preserving question order and literal values.
use super::{Input, Row, Target};
use serde_json::Value;

/// The evaluate field that sends each search hit in its own request.
pub(super) const PER_HIT: &str = "perHit";

/// The private field of a units row's `next.retry`: the unit keys it asks
/// again.
pub(super) const RETRY_UNITS: &str = "retryUnits";

/// A row contract validation admitted. The provider receives the validated
/// row's own values: a typed form would reorder choice labels and map keys,
/// which are semantic input (the contract tests keep the shapes aligned).
/// The questions a row asks, in order: its `questions`, then each of its
/// `predicates` as a yes/no question keyed `p1`, `p2`, … (a key a question
/// already took keeps the question, and the predicate takes the next free
/// `p<n>`).
fn asked(row: &Value) -> (Vec<String>, Vec<Value>) {
    let mut keys = Vec::new();
    let mut questions = Vec::new();
    for (key, question) in row["questions"].as_object().into_iter().flatten() {
        keys.push(key.clone());
        questions.push(question.clone());
    }
    let mut next = 1usize;
    for predicate in row["predicates"].as_array().into_iter().flatten() {
        let key = loop {
            let key = format!("p{next}");
            next += 1;
            if !keys.contains(&key) {
                break key;
            }
        };
        keys.push(key);
        questions.push(serde_json::json!({"type": "noul", "instructions": predicate}));
    }
    (keys, questions)
}

fn parse(row: &Value) -> Row {
    let (keys, questions) = asked(row);
    let input = if row["operation"] == "sections" {
        Input::Sections(row.clone())
    } else if row["operation"] == "models" {
        Input::Models
    } else if row.get("resources").is_some() {
        Input::Resources(row["resources"].clone())
    } else if row.get("content").is_some() {
        Input::Judge(Target::Content(row["content"].clone()))
    } else {
        Input::Judge(Target::Source(row["source"].clone()))
    };
    let text = |field: &str| {
        row[field]
            .as_str()
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned)
    };
    let (goal, background) = (text("mainGoal"), text("background"));
    Row {
        context: judge_context(goal.as_deref(), background.as_deref()),
        goal,
        background,
        model: row["model"].as_str().map(str::to_owned),
        keys,
        questions,
        input,
        assets: row.get("assets").cloned(),
        long_context: row["longContext"].as_bool(),
        details: row["view"] == "details",
        per_item: row["perItem"] == true,
        source_progress: row["sourceProgress"].as_object().cloned(),
        max_requests: row["maxRequests"].as_u64(),
        walk_requests: row["walkRequests"].as_u64().unwrap_or(0),
        saved_page: row
            .get("savedPage")
            .filter(|page| page.is_object())
            .cloned(),
        per_hit: row[PER_HIT] == true,
        collapse: row["collapse"] == true,
        retry_units: row[RETRY_UNITS].as_array().map(|keys| {
            keys.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        }),
        query: row.clone(),
    }
}

pub(super) fn parse_rows<R: std::borrow::Borrow<Value>>(rows: &[R]) -> Vec<Row> {
    rows.iter().map(|row| parse(row.borrow())).collect()
}

/// What the judge reads beside every unit: the row's `mainGoal` as the goal
/// its questions serve, then its `background`. `reasoning` never goes: it is
/// the caller's belief about the answer, and a stated belief moved the
/// judge's answers toward it (measured 2026-11-21: a wrong belief raised a
/// false unit from 0.05 to 0.21 and lowered the true one from 0.91 to 0.86,
/// while a goal left a precise question's answers within 0.05 and cut a
/// vague question's false positive from 0.77 to 0.30).
pub(super) fn judge_context(goal: Option<&str>, background: Option<&str>) -> Option<String> {
    match (goal, background) {
        (Some(goal), Some(background)) => Some(format!("Goal: {goal}\n{background}")),
        (Some(goal), None) => Some(format!("Goal: {goal}")),
        (None, background) => background.map(str::to_owned),
    }
}

/// `row`'s `mainGoal` and `background` on a `call` emitted for it, so the
/// call it leads to judges with the same context.
pub(super) fn carry_brief(row: &Row, call: &mut Value) {
    if let Some(goal) = &row.goal {
        call["mainGoal"] = serde_json::json!(goal);
    }
    if let Some(background) = &row.background {
        call["background"] = serde_json::json!(background);
    }
}

/// The provider state with the row's task context: sent once beside the
/// judged state, never copied into each question (measured 2026-10-29: per
/// question it cost more image tokens and lowered true-fact confidence).
pub(super) fn contextual_state<'a>(
    state: &'a Value,
    context: Option<&str>,
) -> std::borrow::Cow<'a, Value> {
    match context {
        None => std::borrow::Cow::Borrowed(state),
        Some(context) => {
            std::borrow::Cow::Owned(serde_json::json!({"context":context,"content":state}))
        }
    }
}

/// A provider state with the template's target scope as its `question`: the
/// judge answers about the state's `target` (a section, or a search hit),
/// never from the context beside it. A section gets it when its batch runs,
/// so a saved plan gets it too.
pub(super) fn scoped(state: std::borrow::Cow<'_, Value>) -> Value {
    let mut state = state.into_owned();
    if let (Some(question), Some(fields)) = (
        octocode_tools_shared::selection::section_question(),
        state.as_object_mut(),
    ) {
        fields.insert("question".into(), question.clone());
    }
    state
}

#[cfg(test)]
mod tests;

impl Row {
    /// The requests this page may send: `maxRequests` less what the earlier
    /// pages of its walk sent.
    pub(super) fn budget(&self) -> Option<u64> {
        self.max_requests
            .map(|max| max.saturating_sub(self.walk_requests))
    }
}
