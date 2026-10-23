//! One evaluation row: capture its source when it names one, capture its
//! assets, and judge the state.
use super::{Row, RowOutcome, Target, max_state_chars};
use crate::execution::ExecutionContext;
use crate::tools::judgment::{ClassificationError, EvaluationInput, Evaluator};
use crate::tools::source_capture::{self, Captured, SourceReader};

fn rejected(error: ClassificationError, captured: Option<Box<Captured>>) -> RowOutcome {
    RowOutcome::Judgment {
        judged: Err(error),
        captured,
        assets: None,
    }
}

pub(super) fn evaluate_row(
    evaluator: &Evaluator,
    row: &Row,
    target: &Target,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    budget: &octocode_io::budget::RequestBudget,
    gate: &crate::providers::classification::gate::GateLease,
) -> RowOutcome {
    let max_chars = max_state_chars(row.long_context);
    let (state, captured) = match target {
        // Admission measured a literal state against the same budget.
        Target::State(state) => (state.clone(), None),
        Target::Source(source) => {
            let mut captured = match source_capture::capture(source, dispatcher, context, max_chars)
            {
                Ok(captured) => captured,
                Err(error) => return rejected(error, None),
            };
            // The admission budget in serialized Unicode scalars: captured
            // identities, line labels and JSON escapes count too; no source
            // is judged from a silently shortened state.
            if crate::tools::selection::serialized_chars(&captured.state) > max_chars {
                return rejected(
                    source_capture::too_large(
                        max_chars,
                        "The serialized state",
                        source_capture::IN_PARTS_HINT,
                    ),
                    Some(Box::new(captured)),
                );
            }
            // The answer row reports the capture's read and provenance, never
            // its state.
            (
                std::mem::take(&mut captured.state),
                Some(Box::new(captured)),
            )
        }
    };
    let assets = match row
        .assets
        .as_ref()
        .map(|assets| {
            source_capture::assets::capture(assets, &state, &row.questions, dispatcher, context)
        })
        .transpose()
    {
        Ok(assets) => assets,
        Err(error) => return rejected(error, captured),
    };
    let judged = dispatcher.handle().block_on(evaluator.evaluate_prepared(
        EvaluationInput {
            state: &state,
            questions: &row.questions,
            assets: assets.as_ref().map(|assets| &assets.provider),
            long_context: row.long_context,
        },
        row.model.as_deref(),
        budget,
        gate,
    ));
    RowOutcome::Judgment {
        judged,
        captured,
        assets: assets.map(|assets| assets.provenance),
    }
}
