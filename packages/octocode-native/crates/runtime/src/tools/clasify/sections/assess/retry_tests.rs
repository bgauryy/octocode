//! A sections batch whose evaluation the default window refused and the
//! long-context window answered says so in its row hints.
use super::*;
use crate::tools::selection::long_window_fake::{self, BILLED, LongWindow};
use std::time::{Duration, Instant};

#[test]
fn a_long_context_retry_is_disclosed_on_the_sections_row() {
    let window = LongWindow::start();
    let evaluator = long_window_fake::evaluator(&window.server);
    let gate = evaluator.gate();
    let budget = evaluator.budget(
        Instant::now() + Duration::from_secs(20),
        octocode_io::cancellation::Cancellation::new(),
    );
    let state = json!({"target":{"text":"text"}});
    let questions = vec![json!({"type":"noul","instructions":"b"})];
    let result = window.executor.block_on(evaluator.evaluate_questions(
        EvaluationInput {
            state: &state,
            questions: &questions,
            assets: None,
            long_context: LongContext::OnRefusal,
        },
        None,
        &budget,
        &gate,
    ));
    // The session's one unit has its second question pending.
    let mut session = super::super::tests::session();
    let mut usage = ClassificationUsage::default();
    let settled = apply_results(
        &mut session,
        &[[0, 0]],
        vec![vec![1]],
        std::iter::once(result),
        &mut usage,
    );
    assert_eq!(settled, 1);
    let output = emit(&mut session, &usage, None);
    let hints = output["hints"].as_array().expect("row hints");
    assert_eq!(hints.len(), 1, "{output}");
    let notice = hints[0].as_str().expect("text");
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
