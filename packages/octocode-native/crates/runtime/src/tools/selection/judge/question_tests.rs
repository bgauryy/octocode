//! The question and state shaping a judge receives, and the policy that
//! turns its answers into a decision.
use super::*;

#[test]
fn majority_policy_uses_named_probability_mass_and_any_all_question_semantics() {
    let policy = Policy::from_contract().expect("core selection policy");
    let answer = |zero, one, two| {
        json!({"type":"score",
        "probabilities":{"0":zero,"1":one,"2":two}})
    };
    let direct = answer(0.1, 0.2, 0.7);
    let absent = answer(0.8, 0.1, 0.1);
    let background = answer(0.1, 0.8, 0.1);
    let tied = answer(0.5, 0.0, 0.5);

    assert_eq!(
        policy.decide(std::slice::from_ref(&direct)),
        Decision::Selected
    );
    assert_eq!(
        policy.decide(std::slice::from_ref(&absent)),
        Decision::Excluded
    );
    // Most mass on "related": a confident near-miss, not undecided.
    assert_eq!(
        policy.decide(std::slice::from_ref(&background)),
        Decision::Related
    );
    assert_eq!(policy.decide(&[tied]), Decision::Unknown);
    let split = answer(0.3, 0.4, 0.3);
    assert_eq!(
        policy.decide(std::slice::from_ref(&split)),
        Decision::Unknown
    );
    assert_eq!(policy.decide(&[absent.clone(), direct]), Decision::Selected);
    assert_eq!(
        policy.decide(&[absent.clone(), absent.clone()]),
        Decision::Excluded
    );
    assert_eq!(
        policy.decide(&[absent.clone(), background.clone()]),
        Decision::Related
    );
    assert_eq!(policy.decide(&[background, split]), Decision::Unknown);
    assert_eq!(policy.decide(&[]), Decision::Unknown);
    assert_eq!(
        policy.decide(&[json!({"type":"score","score":2})]),
        Decision::Unknown
    );
}

#[test]
fn optional_research_context_is_in_each_independent_question() {
    for classifier in [
        "Does this name the stopping condition?",
        "Does this show the current default?",
    ] {
        let question =
            compile(classifier, Some(" Explain retry behavior\n")).expect("contextual question");
        assert_eq!(
            question["instructions"]["context"],
            " Explain retry behavior\n"
        );
    }
    let ordinary = compile("Does this name a limit?", None).expect("question");
    assert!(ordinary["instructions"].get("context").is_none());
}

#[test]
fn context_selection_routing_is_not_sent_as_source_evidence() {
    let candidate = json!({"evidence":{
        "path":"retry.rs",
        "selection":{"status":"partial","selectedItems":1,"unknownItems":3},
        "matches":[{"line":12,"value":"stop after three attempts"}]
    }});

    assert_eq!(
        candidate_state(candidate),
        json!({"evidence":{
            "path":"retry.rs",
            "matches":[{"line":12,"value":"stop after three attempts"}]
        }})
    );
}

#[test]
fn provider_coordinates_round_trip_without_changing_admission_evidence() {
    let lines = vec![0, 1, 2, 2, 3, 0, 0, 7, 8, 4, u64::MAX];
    let original = json!({"path":"src/λ.rs","sourceLines":lines,"content":"α\nβ\n…\n"});
    let compact = provider_state(&original);
    let expanded = compact["sourceLineRanges"]
        .as_array()
        .expect("runs")
        .iter()
        .flat_map(|pair| pair[0].as_u64().expect("start")..=pair[1].as_u64().expect("end"))
        .collect::<Vec<_>>();
    assert_eq!(expanded, lines);
    assert_eq!(compact["content"], original["content"]);
    assert_eq!(compact["path"], original["path"]);
    assert_eq!(original["sourceLines"], json!(lines));
    for state in [
        json!({"content":"source"}),
        json!({"sourceLines":["invalid"]}),
    ] {
        assert_eq!(provider_state(&state).as_ref(), &state);
    }
    let long = json!({"sourceLines":(1000..2000).collect::<Vec<_>>(),"content":"source"});
    assert!(provider_state(&long).to_string().len() < long.to_string().len());
}

#[test]
fn a_classifier_fills_the_contract_template_slot() {
    let template = template().expect("contract template");
    let question = compile("When do retries stop?", None).expect("question");
    assert_eq!(question["type"], "score");
    assert_eq!(
        question["instructions"][template["slot"].as_str().expect("slot")],
        "When do retries stop?",
        "the template, not native, frames the predicate"
    );
    assert_eq!(question["criteria"], template["question"]["criteria"]);
    assert_eq!(
        question["instructions"]["question"],
        template["question"]["instructions"]["question"]
    );
}

#[test]
fn a_candidate_exposes_only_returned_evidence_to_the_judge() {
    let public = json!({
        "evidence": {
            "path":"a.rs",
            "matches":[{"value":"needle","matchIndices":[{"line":1}],
                "next":{"read":{"query":{"selection":"secret-plan"}}}}],
            "pagination":{"hasMore":true},
            "hints":{"text":["follow"]},
            "diagnostics":{"requestId":"private"},
            "snapshot":"opaque"
        }
    });
    assert_eq!(
        candidate_state(public),
        json!({"evidence":{"path":"a.rs","matches":[{"value":"needle"}]}})
    );
}
