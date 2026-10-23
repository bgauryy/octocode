use super::*;
use serde_json::json;

fn jev() -> &'static dyn ClassificationProvider {
    &crate::providers::classification::jev::JEV
}

fn text_input<'a>(state: &'a Value, questions: &'a [Value]) -> EvaluationInput<'a> {
    EvaluationInput {
        state,
        questions,
        assets: None,
        long_context: false,
    }
}

#[test]
fn partial_failure_keeps_successful_question_and_all_attempt_usage() {
    let result = fold_singles(
        vec![
            Ok((
                Response {
                    answers: vec![json!({"type":"noul","noul":0.8})],
                    model: "jev-1.13.0".into(),
                    input_tokens: 10,
                    output_tokens: 2,
                },
                1,
            )),
            Err(ClassificationError {
                provider_calls: 2,
                ..request_error("second question failed")
            }),
        ],
        2,
    );
    assert_eq!(
        result.answers[0].as_ref().expect("first answer survives")["noul"],
        json!(0.8)
    );
    assert!(result.answers[1].is_err());
    let error = result
        .complete
        .expect_err("ordinary evaluation still reports incomplete");
    assert_eq!(error.provider_calls, 3);
    assert_eq!(error.usage().input_tokens, 10);
    assert_eq!(error.usage().output_tokens, 2);
    assert_eq!(error.usage().known_usage_calls, 1);
}

#[test]
fn grouped_partial_answers_preserve_siblings_and_count_the_post_once() {
    let questions = vec![json!({"type":"noul","instructions":"question"}); 12];
    let request = jev().build(
        &json!("state"),
        &questions.iter().collect::<Vec<_>>(),
        "m",
        None,
    );
    let mut answers = serde_json::Map::new();
    for index in 0..questions.len() {
        answers.insert(
            format!("answer_{index}"),
            json!({"type":"noul","noul":index as f64 / 20.0}),
        );
    }
    let complete =
        json!({"model":"m","usage":{"input_tokens":10,"output_tokens":2},"answers":answers});
    for missing in [false, true] {
        let mut response = complete.clone();
        if missing {
            response["answers"]
                .as_object_mut()
                .expect("answers")
                .remove("answer_1");
        } else {
            response["answers"]["answer_1"]["noul"] = json!(2);
        }
        let outcomes =
            grouped_outcomes(&request, &response, 3, jev()).expect("valid global response");
        assert_eq!(outcomes.answers.len(), 12);
        assert!(outcomes.answers[1].is_err());
        for (index, answer) in outcomes
            .answers
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 1)
        {
            assert_eq!(
                answer.as_ref().expect("valid sibling")["noul"],
                json!(index as f64 / 20.0)
            );
        }
        let mut usage = crate::tools::judgment::stats::ClassificationUsage::default();
        usage.add(&outcomes.complete);
        assert_eq!(
            (
                usage.calls,
                usage.known_usage_calls,
                usage.unknown_usage_calls,
                usage.input_tokens,
                usage.output_tokens
            ),
            (3, 1, 2, 10, 2)
        );
        assert_eq!(
            outcomes.models,
            std::collections::BTreeSet::from(["m".into()])
        );
    }
    let mut unexpected = complete.clone();
    unexpected["answers"]["other"] = json!({"type":"noul","noul":0.2});
    assert!(grouped_outcomes(&request, &unexpected, 1, jev()).is_err());
    let mut invalid_model = complete;
    invalid_model["model"] = json!("");
    assert!(grouped_outcomes(&request, &invalid_model, 1, jev()).is_err());
}

#[test]
fn questions_group_only_within_the_headroom() {
    let questions = vec![json!({"type":"noul","instructions":"q"}); 3];
    assert!(grouped(text_input(&json!("small"), &questions), "m", jev()).is_some());
    let large = json!("x".repeat(MAX_STATE_AND_QUESTION_BYTES));
    assert!(grouped(text_input(&large, &questions), "m", jev()).is_none());
    // Each request alone fits; the five long questions together do not.
    let long = vec![json!({"type":"noul","instructions":"q".repeat(20 * 1024)}); 5];
    assert!(
        grouped(text_input(&json!("x".repeat(30 * 1024)), &long), "m", jev()).is_none(),
        "each fits alone but the group does not"
    );
}

#[test]
fn images_share_one_request_without_counting_base64_as_text_context() {
    let state = json!("opaque");
    let questions = [json!({"type":"box"}), json!({"type":"noul"})];
    let assets = json!({"opaque":{"type":"image","format":"png","data_base64":"A".repeat(MAX_GROUP_BYTES + 1)}});
    let input = EvaluationInput {
        assets: Some(&assets),
        ..text_input(&state, &questions)
    };
    let (request, body) =
        grouped(input, "vision", jev()).expect("one image shared by both questions");
    assert_eq!(request["assets"], assets);
    assert!(body.len() > MAX_GROUP_BYTES);
    assert_eq!(request["questions"].as_object().unwrap().len(), 2);
    assert!(max_request_bytes(input) > MAX_TEXT_REQUEST_BYTES);
}

#[test]
fn long_context_opt_in_keeps_questions_batched_over_large_text() {
    let state = json!("x".repeat(MAX_STATE_AND_QUESTION_BYTES + 1));
    let questions = [json!({"type":"noul"}), json!({"type":"noul"})];
    let input = text_input(&state, &questions);
    assert!(grouped(input, "m", jev()).is_none());
    assert!(
        grouped(
            EvaluationInput {
                long_context: true,
                ..input
            },
            "m",
            jev()
        )
        .is_some()
    );
}

#[test]
fn a_split_evaluation_sums_usage_and_keeps_question_order() {
    let part = |answer: &str, tokens: u64| {
        (
            Response {
                answers: vec![json!({"type":"noul","noul":answer})],
                model: "jev-1.13.0".into(),
                input_tokens: tokens,
                output_tokens: 1,
            },
            1,
        )
    };
    let judged = judged(vec![part("a", 5), part("b", 7)]).unwrap();
    assert_eq!(judged.answers[0]["noul"], "a");
    assert_eq!(judged.answers[1]["noul"], "b");
    assert_eq!(
        (
            judged.input_tokens,
            judged.output_tokens,
            judged.provider_calls
        ),
        (12, 2, 2)
    );
    assert_eq!(judged.model, "jev-1.13.0");
}

#[test]
fn different_resolved_models_do_not_get_labeled_as_one_model() {
    let parts = ["jev-a", "jev-b"]
        .into_iter()
        .map(|model| {
            (
                Response {
                    answers: vec![json!({"type":"noul","noul":0.5})],
                    model: model.into(),
                    input_tokens: 3,
                    output_tokens: 1,
                },
                1,
            )
        })
        .collect();
    let error = judged(parts).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidClassificationResponse);
    assert_eq!(
        (
            error.provider_calls,
            error.usage().known_usage_calls,
            error.usage().input_tokens,
            error.usage().output_tokens
        ),
        (2, 2, 6, 2)
    );
}
