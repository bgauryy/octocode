//! One evaluation row: capture its source when it names one, capture its
//! assets, and judge the state.
use super::{Row, RowOutcome, Target};
use crate::execution::ExecutionContext;
use crate::tools::judgment::{
    ClassificationError, EvaluationInput, Evaluator, LongContext, shared::Evaluations,
};
use crate::tools::source_capture::{self, Captured, SourceReader};
use octocode_contract_types::ErrorCode;

fn rejected(error: ClassificationError, captured: Option<Box<Captured>>) -> RowOutcome {
    RowOutcome::Judgment {
        judged: Err(error),
        captured,
        assets: None,
        identity: None,
    }
}

/// Whether literal `content` holds nothing to judge.
fn blank(state: &serde_json::Value) -> bool {
    match state {
        serde_json::Value::Null => true,
        serde_json::Value::String(text) => text.trim().is_empty(),
        serde_json::Value::Array(items) => items.is_empty(),
        serde_json::Value::Object(fields) => fields.is_empty(),
        _ => false,
    }
}

pub(super) fn evaluate_row(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    target: &Target,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &crate::providers::classification::gate::GateLease,
) -> RowOutcome {
    let (state, captured) = match target {
        // An empty literal is nothing to judge, as an empty source is.
        Target::Content(state) if blank(state) => {
            return rejected(
                ClassificationError::new(
                    ErrorCode::ClassificationContextEmpty,
                    "content is empty; nothing was judged.",
                    "Supply the text to judge.",
                ),
                None,
            );
        }
        Target::Content(state) => (state.clone(), None),
        Target::Source(source) => {
            // Only local memory bounds a capture: the provider's own window
            // decides what it judges, and its refusal says so.
            let limit = dispatcher.text_capture_limit();
            let mut captured = match source_capture::capture(source, dispatcher, context, limit) {
                Ok(captured) => captured,
                Err(error) => return rejected(error, None),
            };
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
    let state = super::rows::contextual_state(&state, row.context.as_deref());
    // One request's time starts when it runs, after the capture.
    let budget = evaluator.budget(
        crate::execution::cancel::deadline_of(context),
        context.cancellation.clone(),
    );
    let (mut judged, identity) = dispatcher.handle().block_on(evaluator.evaluate_once(
        evaluations,
        EvaluationInput {
            state: &state,
            questions: &row.questions,
            assets: assets.as_ref().map(|assets| &assets.provider),
            long_context: LongContext::from_field(row.long_context),
        },
        row.model.as_deref(),
        &budget,
        gate,
    ));
    // Jev's boxes are fractions of the fitted canvas; callers also get them
    // on the original image.
    if let (Ok(judged), Some(assets)) = (&mut judged, &assets) {
        assets.attach_source_boxes(&row.questions, &mut judged.answers);
    }
    RowOutcome::Judgment {
        judged,
        captured,
        assets: assets.map(|assets| assets.provenance),
        identity,
    }
}
