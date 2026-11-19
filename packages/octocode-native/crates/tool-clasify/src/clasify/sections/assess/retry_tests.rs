//! A sections batch whose evaluation the default window refused and the
//! long-context window answered says so in its row warnings.
use super::*;
use octocode_test_support::json_fixture;
use octocode_tools_shared::judgment::LongContext;
use octocode_tools_shared::selection::long_window_fake::{self, BILLED, LongWindow};
use std::time::{Duration, Instant};

#[test]
fn classification_disclosures_keep_long_context_retry_on_the_sections_row() {
    let window = LongWindow::start();
    let evaluator = long_window_fake::evaluator(&window.server);
    let gate = evaluator.gate();
    let budget = evaluator.budget(
        Instant::now() + Duration::from_secs(20),
        octocode_io::cancellation::Cancellation::new(),
    );
    let state = json_fixture(r#"{"target": {"text": "text"}}"#);
    let questions = vec![json_fixture(r#"{"type": "noul", "instructions": "b"}"#)];
    let result = window.executor.block_on(evaluator.evaluate_questions(
        EvaluationInput {
            state: &state,
            questions: &questions,
            assets: None,
            long_context: LongContext::OnRefusal,
            companions: 0,
            cap: None,
        },
        None,
        &budget,
        &gate,
    ));
    // The session's one unit has its second question pending.
    let mut session = super::super::tests::session();
    let mut usage = ClassificationUsage::default();
    let (settled, deferred) = apply_results(
        &mut session,
        &[[0, 0]],
        vec![vec![1]],
        std::iter::once(result),
        &mut usage,
        true,
    );
    assert_eq!((settled, deferred), (1, 0));
    let output = emit(&mut session, &usage, None);
    let warnings = output["warnings"].as_array().expect("row warnings");
    assert_eq!(warnings.len(), 1, "{output}");
    let notice = warnings[0].as_str().expect("text");
    assert!(notice.starts_with("1 evaluation refused"), "{notice}");
    assert!(
        notice.contains(&format!("{BILLED} input tokens billed")),
        "{notice}"
    );
    assert!(
        notice.contains("longContext:false refuses instead"),
        "{notice}"
    );
    assert_eq!(window.long_requests(), 1);
}

/// A section answer a transient failure left unanswered stays pending once
/// on a plan a later batch continues, so the next batch asks it again; a
/// second transient failure is its error. A plan nothing continues settles
/// it as an error at once.
#[test]
fn a_transient_failure_is_asked_again_once_by_the_next_batch() {
    let failure = || {
        let error = octocode_tools_shared::judgment::ClassificationError::new(
            octocode_contract_types::ErrorCode::ClassificationProviderUnavailable,
            "down",
            "Retry later.",
        );
        QuestionOutcomes {
            answers: vec![Err(error.clone())],
            complete: Err(error),
            models: std::collections::BTreeSet::new(),
        }
    };
    let mut session = super::super::tests::session();
    let mut usage = ClassificationUsage::default();
    let apply = |session: &mut Session, usage: &mut ClassificationUsage, defer| {
        apply_results(
            session,
            &[[0, 0]],
            vec![vec![1]],
            std::iter::once(failure()),
            usage,
            defer,
        )
    };
    assert_eq!(apply(&mut session, &mut usage, true), (1, 1));
    let unit = &session.resources[0].units[0];
    assert!(unit.cells[1].is_none(), "pending for the next batch");
    assert!(unit.deferred.contains(&1));
    assert_eq!(apply(&mut session, &mut usage, true), (1, 0));
    let cell = session.resources[0].units[0].cells[1]
        .as_ref()
        .expect("settled");
    assert_eq!(cell["status"], "error", "{cell}");
    assert_eq!(
        cell["error"]["errorCode"], "classificationProviderUnavailable",
        "{cell}"
    );
    // Nothing continues the plan: an error at once.
    let mut once = super::super::tests::session();
    assert_eq!(apply(&mut once, &mut usage, false), (1, 0));
    assert!(once.resources[0].units[0].cells[1].is_some());
}
