//! Interpreter for the `clasify_box_image` opcode: a box question locates
//! something in an image, so a row without an image asset cannot answer it.
use super::{ContractValidationError, ValidationIssue, query_values};
use serde_json::Value;

pub(super) fn validate_box_image(input: &Value) -> Result<(), ContractValidationError> {
    let mut issues = Vec::new();
    for (index, query) in query_values(input) {
        if query
            .get("assets")
            .and_then(Value::as_object)
            .is_some_and(|assets| !assets.is_empty())
        {
            continue;
        }
        for (key, question) in query
            .get("questions")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if question.get("type").and_then(Value::as_str) == Some("box") {
                issues.push(ValidationIssue {
                    rule_id: "clasify.box-image".into(),
                    path: vec![
                        "queries".into(),
                        index.to_string(),
                        "questions".into(),
                        key.clone(),
                    ],
                    message: "box questions need an image in assets.".into(),
                    schema: None,
                    received: None,
                });
            }
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    /// A box question without an image is refused before any provider call;
    /// with an image asset it is admitted.
    #[test]
    fn a_box_question_needs_an_image_asset() {
        let box_question = json!({"where":{"type":"box","instructions":"Where is the cat?"}});
        let error = super::validate_box_image(
            &json!({"queries":[{"state":"a cat","questions":box_question}]}),
        )
        .expect_err("no image");
        assert_eq!(error.issues[0].path, ["queries", "0", "questions", "where"]);
        assert_eq!(
            error.issues[0].message,
            "box questions need an image in assets."
        );
        super::validate_box_image(&json!({"queries":[{
            "state":"photo","questions":box_question,
            "assets":{"photo":{"type":"image","source":{"url":"https://example.org/cat.png"}}}
        }]}))
        .expect("an image asset");
        super::validate_box_image(&json!({"queries":[{
            "state":"a cat","questions":{"q":{"type":"noul","instructions":"Is it a cat?"}}
        }]}))
        .expect("no box question");
    }
}
