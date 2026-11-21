use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn optional_context_wraps_the_state_once_and_keeps_it_literal() {
    let state = json_fixture(r#"{"target": {"text": "fn a() {}"}, "context": []}"#);
    assert_eq!(&*contextual_state(&state, None), &state);
    assert_eq!(
        &*contextual_state(&state, Some("  Research context\n")),
        &json!({"context":"  Research context\n","content":state})
    );
}

/// The judge reads the row's goal before its background; the caller's
/// reasoning is a belief about the answer and never reaches it.
#[test]
fn the_goal_and_background_reach_the_judge_and_reasoning_never_does() {
    let row = json!({
        "operation": "evaluate",
        "mainGoal": "find the retry loop",
        "reasoning": "probably in cache.rs",
        "background": "Rust sources of a CLI",
        "content": "fn a() {}",
        "questions": {"q": {"type": "noul", "instructions": "Retries."}},
    });
    let parsed = parse(&row);
    let context = parsed.context.as_deref().expect("a judge context");
    let (goal, background) = (
        context.find("find the retry loop").expect("the goal"),
        context
            .find("Rust sources of a CLI")
            .expect("the background"),
    );
    assert!(goal < background, "{context}");
    assert!(!context.contains("probably in cache.rs"), "{context}");
    let state = contextual_state(&row["content"], parsed.context.as_deref());
    assert_eq!(state["context"], json!(context));
    assert_eq!(state["content"], row["content"]);
}

/// A row with neither sends its state as it is, and either alone is the
/// whole context.
#[test]
fn a_row_without_a_goal_or_background_sends_no_context() {
    assert_eq!(judge_context(None, None), None);
    assert_eq!(judge_context(None, Some("b")).as_deref(), Some("b"));
    let goal = judge_context(Some("g"), None).expect("the goal alone");
    assert!(goal.contains('g') && !goal.contains('\n'), "{goal}");
    let blank = json!({"operation": "evaluate", "mainGoal": "  ", "content": "x"});
    assert_eq!(parse(&blank).context, None);
}

/// A call emitted for a row judges with the row's own goal and background,
/// each under its own field, never the joined context.
#[test]
fn an_emitted_call_carries_the_rows_goal_and_background_apart() {
    let row = parse(&json!({
        "operation": "evaluate",
        "mainGoal": "find the retry loop",
        "background": "Rust sources of a CLI",
        "content": "x",
    }));
    let mut call = json!({"operation": "evaluate"});
    carry_brief(&row, &mut call);
    assert_eq!(call["mainGoal"], json!("find the retry loop"));
    assert_eq!(call["background"], json!("Rust sources of a CLI"));
    assert_eq!(
        parse(&call).context,
        row.context,
        "the emitted call judges with the same context"
    );
}

/// `predicates` are yes/no questions asked after the row's own, each under
/// its own key, in order; a key a question took is never reused.
#[test]
fn predicates_are_yes_no_questions_keyed_in_order() {
    let row = parse(&json!({
        "operation": "evaluate",
        "source": {"tool": "localSearch", "query": {"matchString": ["retry"]}},
        "questions": {"p1": {"type": "choice", "criteria": {"a": null, "b": null}}},
        "predicates": ["Implements a retry loop.", "Is test code."],
    }));
    assert_eq!(row.keys.len(), row.questions.len());
    let unique = row.keys.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(unique.len(), row.keys.len(), "{:?}", row.keys);
    assert_eq!(
        row.questions[0]["type"], "choice",
        "the row's own question is first"
    );
    for (question, fact) in row.questions[1..]
        .iter()
        .zip(["Implements a retry loop.", "Is test code."])
    {
        assert_eq!(question["type"], "noul");
        assert_eq!(question["instructions"], fact);
    }
}
