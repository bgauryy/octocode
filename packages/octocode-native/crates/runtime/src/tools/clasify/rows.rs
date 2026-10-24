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
        long_context: row["longContext"].as_bool() == Some(true),
    }
}

pub(super) fn parse_rows<R: std::borrow::Borrow<Value>>(rows: &[R]) -> Vec<Row> {
    rows.iter().map(|row| parse(row.borrow())).collect()
}

pub(super) fn contextual_questions<'a>(
    questions: &'a [Value],
    context: Option<&str>,
) -> std::borrow::Cow<'a, [Value]> {
    let Some(context) = context else {
        return std::borrow::Cow::Borrowed(questions);
    };
    std::borrow::Cow::Owned(
        questions
            .iter()
            .cloned()
            .map(|mut question| {
                let instructions = question
                    .as_object_mut()
                    .and_then(|fields| fields.remove("instructions"));
                let mut contextual = serde_json::json!({"context":context});
                if let Some(instructions) = instructions {
                    contextual["instructions"] = instructions;
                }
                question["instructions"] = contextual;
                question
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn optional_context_preserves_each_question_and_enters_provider_input() {
        let questions = vec![
            json!({"type":"choice","instructions":["literal", {"x":2}],"criteria":{"a":"A","b":"B"}}),
            json!({"type":"noul","criteria":{"true":"yes"}}),
        ];
        assert_eq!(&*contextual_questions(&questions, None), &questions);
        let contextual = contextual_questions(&questions, Some("  Research context\n"));
        for (before, after) in questions.iter().zip(contextual.iter()) {
            assert_eq!(after["instructions"]["context"], "  Research context\n");
            assert_eq!(
                after["instructions"]["instructions"],
                before["instructions"]
            );
            assert_eq!(after["criteria"], before["criteria"]);
            assert_eq!(after["type"], before["type"]);
        }
    }
}
