//! Request-owned dedupe: identical evaluations inside one request send one
//! provider request and every asker receives its answer. This is not a cache:
//! an [`Evaluations`] lives for one request and is dropped with it, so a later
//! request always reaches the provider.
//!
//! Identity is the exact provider request payload ([`ClassificationProvider::build`]
//! over state, every question, resolved model and assets) plus the long-context
//! window, which changes the request limits and headers. Spend is counted once:
//! the asker that sent the request keeps the evaluation's provider calls; every
//! other asker receives the same answer, tokens and model but zero provider
//! calls, so summing askers' outcomes never double-counts.
use super::{ClassificationError, EvaluationInput, Evaluator, Judged};
use crate::providers::classification::gate::GateLease;
use octocode_io::budget::RequestBudget;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::OnceCell;

type Identity = [u8; 32];
type Slot = Arc<OnceCell<Result<Judged, ClassificationError>>>;

/// The evaluations one request has asked for, by identity.
#[derive(Default)]
pub(crate) struct Evaluations {
    slots: Mutex<HashMap<Identity, Slot>>,
}

impl Evaluations {
    fn slot(&self, identity: Identity) -> Slot {
        self.slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(identity)
            .or_default()
            .clone()
    }
}

impl Evaluator {
    /// [`Evaluator::evaluate_prepared`], sharing one provider request among
    /// identical evaluations of `evaluations`' request.
    pub(crate) async fn evaluate_once(
        &self,
        evaluations: &Evaluations,
        input: EvaluationInput<'_>,
        model: Option<&str>,
        budget: &RequestBudget,
        gate: &GateLease,
    ) -> Result<Judged, ClassificationError> {
        let Some(identity) = self.identity(input, model) else {
            return self.evaluate_prepared(input, model, budget, gate).await;
        };
        let mut sent = false;
        let shared = evaluations
            .slot(identity)
            .get_or_init(|| {
                sent = true;
                self.evaluate_prepared(input, model, budget, gate)
            })
            .await
            .clone();
        if sent {
            return shared;
        }
        match shared {
            Ok(judged) => Ok(Judged {
                provider_calls: 0,
                known_usage_calls: 0,
                ..judged
            }),
            Err(error) => Err(ClassificationError {
                provider_calls: 0,
                ..error
            }),
        }
    }

    /// The evaluation's identity, or `None` when it has no payload to compare
    /// (no questions: evaluation refuses it).
    fn identity(&self, input: EvaluationInput<'_>, model: Option<&str>) -> Option<Identity> {
        if input.questions.is_empty() {
            return None;
        }
        let questions: Vec<_> = input.questions.iter().collect();
        let request = self.provider.build(
            input.state,
            &questions,
            model.unwrap_or(&self.model),
            input.assets,
        );
        let mut hasher = octocode_engine::digest::HashWriter::default();
        hasher.update([u8::from(input.long_context)]);
        serde_json::to_writer(&mut hasher, &request).ok()?;
        Some(hasher.finalize())
    }
}
