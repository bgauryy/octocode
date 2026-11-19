//! Request-owned dedupe: identical evaluations inside one request send one
//! provider request and every asker receives its answer. This is not a cache:
//! an [`Evaluations`] lives for one request and is dropped with it, so a later
//! request always reaches the provider.
//!
//! Identity is the exact provider request payload ([`ClassificationProvider::build`](octocode_runtime_base::providers::classification::ClassificationProvider::build)
//! over state, every question, resolved model and assets) plus the long-context
//! window, which changes the request limits and headers. Spend is counted once:
//! the asker that sent the request keeps the evaluation's provider calls; every
//! other asker receives the same answer, tokens and model but zero provider
//! calls, so summing askers' outcomes never double-counts. A packed request
//! (several units' questions over one state, sent once) is shared the same
//! way under an identity of its own ([`Evaluator::evaluate_group_once`]).
use super::{ClassificationError, EvaluationInput, Evaluator, Judged, QuestionOutcomes};
use octocode_io::budget::RequestBudget;
use octocode_runtime_base::providers::classification::gate::GateLease;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::OnceCell;

/// An evaluation's identity: the hash of its exact provider request.
pub type Identity = [u8; 32];
type Slot = Arc<OnceCell<Result<Judged, ClassificationError>>>;
type GroupSlot = Arc<OnceCell<QuestionOutcomes>>;

/// The evaluations one request has asked for, by identity.
#[derive(Default)]
pub struct Evaluations {
    slots: Mutex<HashMap<Identity, Slot>>,
    groups: Mutex<HashMap<Identity, GroupSlot>>,
}

fn slot<T>(
    slots: &Mutex<HashMap<Identity, Arc<OnceCell<T>>>>,
    identity: Identity,
) -> Arc<OnceCell<T>> {
    slots
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(identity)
        .or_default()
        .clone()
}

impl Evaluations {
    fn slot(&self, identity: Identity) -> Slot {
        slot(&self.slots, identity)
    }

    fn group(&self, identity: Identity) -> GroupSlot {
        slot(&self.groups, identity)
    }
}

/// A twin's copy of a shared outcome: the same answers and tokens, no
/// provider calls.
fn unbilled(mut outcomes: QuestionOutcomes) -> QuestionOutcomes {
    match &mut outcomes.complete {
        Ok(judged) => (judged.provider_calls, judged.known_usage_calls) = (0, 0),
        Err(error) => error.provider_calls = 0,
    }
    for error in outcomes
        .answers
        .iter_mut()
        .filter_map(|answer| answer.as_mut().err())
    {
        error.provider_calls = 0;
    }
    outcomes
}

impl Evaluator {
    /// [`Evaluator::evaluate_prepared`], sharing one provider request among
    /// identical evaluations of `evaluations`' request.
    /// The outcome comes with the evaluation's identity (`None` when it has
    /// none to share), so a caller holding every asker can settle which one
    /// carries the spend.
    pub async fn evaluate_once(
        &self,
        evaluations: &Evaluations,
        input: EvaluationInput<'_>,
        model: Option<&str>,
        budget: &RequestBudget,
        gate: &GateLease,
    ) -> (Result<Judged, ClassificationError>, Option<Identity>) {
        let Some(identity) = self.identity(input, model) else {
            return (
                self.evaluate_prepared(input, model, budget, gate).await,
                None,
            );
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
            return (shared, Some(identity));
        }
        let shared = match shared {
            Ok(judged) => Ok(Judged {
                provider_calls: 0,
                known_usage_calls: 0,
                ..judged
            }),
            Err(error) => Err(ClassificationError {
                provider_calls: 0,
                ..error
            }),
        };
        (shared, Some(identity))
    }

    /// [`Evaluator::evaluate_group`], sharing one provider request among
    /// identical packed requests of `evaluations`' request, with the packed
    /// request's identity (`None`: it has none to share).
    pub(crate) async fn evaluate_group_once(
        &self,
        evaluations: &Evaluations,
        input: EvaluationInput<'_>,
        model: Option<&str>,
        budget: &RequestBudget,
        gate: &GateLease,
    ) -> (QuestionOutcomes, Option<Identity>) {
        let Some(identity) = self.identity_of(input, model, true) else {
            return (self.evaluate_group(input, model, budget, gate).await, None);
        };
        let mut sent = false;
        let shared = evaluations
            .group(identity)
            .get_or_init(|| {
                sent = true;
                self.evaluate_group(input, model, budget, gate)
            })
            .await
            .clone();
        (if sent { shared } else { unbilled(shared) }, Some(identity))
    }

    /// The evaluation's identity, or `None` when it has no payload to compare
    /// (no questions: evaluation refuses it).
    fn identity(&self, input: EvaluationInput<'_>, model: Option<&str>) -> Option<Identity> {
        self.identity_of(input, model, false)
    }

    /// The identity of the request `input` is, `packed` when it is sent
    /// once and never split ([`Evaluator::evaluate_group`]).
    fn identity_of(
        &self,
        input: EvaluationInput<'_>,
        model: Option<&str>,
        packed: bool,
    ) -> Option<Identity> {
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
        if packed {
            hasher.update(*b"packed");
        }
        hasher.update([input.long_context as u8]);
        serde_json::to_writer(&mut hasher, &request).ok()?;
        Some(hasher.finalize())
    }
}
