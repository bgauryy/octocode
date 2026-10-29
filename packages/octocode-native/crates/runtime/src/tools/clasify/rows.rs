//! Rows as clasify reads them, preserving question order and literal values.
use super::{Input, Row, Target};
use serde_json::Value;

/// A row contract validation admitted. The provider receives the validated
/// row's own values: a typed form would reorder choice labels and map keys,
/// which are semantic input (the contract tests keep the shapes aligned).
fn parse(row: &Value) -> Row {
    let questions = row["questions"].as_object();
    let input = if row["operation"] == "sections" {
        Input::Sections(row.clone())
    } else if row["operation"] == "models" {
        Input::Models
    } else if row.get("content").is_some() {
        Input::Judge(Target::Content(row["content"].clone()))
    } else {
        Input::Judge(Target::Source(row["source"].clone()))
    };
    Row {
        context: row["context"].as_str().map(str::to_owned),
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
        long_context: row["longContext"].as_bool(),
    }
}

pub(super) fn parse_rows<R: std::borrow::Borrow<Value>>(rows: &[R]) -> Vec<Row> {
    rows.iter().map(|row| parse(row.borrow())).collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn optional_context_wraps_the_state_once_and_keeps_it_literal() {
        let state = json!({"target":{"text":"fn a() {}"},"context":[]});
        assert_eq!(&*contextual_state(&state, None), &state);
        assert_eq!(
            &*contextual_state(&state, Some("  Research context\n")),
            &json!({"context":"  Research context\n","content":state})
        );
    }
}
