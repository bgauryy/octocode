//! The one classification evaluator: a captured state plus JEV questions in,
//! validated typed answers, the answering model and usage out. Retrieval
//! `clasifyContext` (through [`select`]) and public `clasify` both use
//! [`Evaluator`]; neither keeps its own provider client, cache,
//! retry loop or accounting.
//!
//! Identical prepared input (account partition, model, state, questions,
//! media bytes and capability options)
//! reuses one validated answer set from either entry point. Only complete
//! successful evaluations are stored; concurrent identical evaluations share
//! one provider request, and a provider failure is shared only with the
//! requests already waiting on it.
pub(crate) mod batch;
mod cache;
pub(crate) mod select;
pub(crate) mod stats;

#[cfg(test)]
mod tests;

pub(crate) use transport::ClassificationError;

use crate::providers::classification::{ClassificationProvider, gate::GateLease, transport};
use octocode_github::Partition;
use octocode_io::budget::RequestBudget;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Source capture resolves assets before evaluation; this view never owns or
/// renders source bytes. The same evaluator handles ordinary text calls.
#[derive(Clone, Copy)]
pub(crate) struct EvaluationInput<'a> {
    pub state: &'a Value,
    pub questions: &'a [Value],
    pub assets: Option<&'a Value>,
    pub(crate) long_context: bool,
}

impl EvaluationInput<'_> {
    pub(super) fn images(&self) -> bool {
        self.assets.is_some()
            || self
                .questions
                .iter()
                .any(|question| question["type"] == "box")
    }
}

/// A validated evaluation: one provider answer per question, in question
/// order, exactly as the provider returned it (`type` included).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Judged {
    pub answers: Vec<Value>,
    /// The model that answered (the provider's response), not the alias asked.
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Provider requests this call made; zero for a replay.
    #[serde(skip)]
    pub provider_calls: u64,
    #[serde(skip)]
    pub known_usage_calls: u64,
    /// Answers saved from an identical earlier evaluation; the tokens are
    /// that evaluation's, not new spend.
    #[serde(skip)]
    pub replayed: bool,
}

/// Per-question results retain successful siblings when another request fails.
/// `complete` preserves the all-or-error contract of ordinary evaluation, and
/// carries the usage of every attempted request exactly once.
#[derive(Clone, Debug)]
pub(crate) struct QuestionOutcomes {
    pub answers: Vec<Result<Value, ClassificationError>>,
    pub complete: Result<Judged, ClassificationError>,
    pub models: std::collections::BTreeSet<String>,
}

impl QuestionOutcomes {
    fn from_complete(result: Result<Judged, ClassificationError>, count: usize) -> Self {
        let answers = match &result {
            Ok(judged) => judged.answers.iter().cloned().map(Ok).collect(),
            Err(error) => (0..count).map(|_| Err(error.clone())).collect(),
        };
        let models = result
            .as_ref()
            .ok()
            .map(|judged| judged.model.clone())
            .into_iter()
            .collect();
        Self {
            answers,
            complete: result,
            models,
        }
    }

    fn replayed(mut self) -> Self {
        match &mut self.complete {
            Ok(judged) => {
                judged.replayed = true;
                judged.provider_calls = 0;
                judged.known_usage_calls = 0;
            }
            Err(error) => {
                error.provider_calls = 0;
                error.reported_usage = None;
            }
        }
        for error in self
            .answers
            .iter_mut()
            .filter_map(|answer| answer.as_mut().err())
        {
            error.provider_calls = 0;
            error.reported_usage = None;
        }
        self
    }
}

/// Provider connection and policy shared by every evaluation of a runtime.
#[derive(Clone)]
pub(crate) struct Evaluator {
    pub(crate) key: Option<SecretString>,
    pub(crate) base_url: String,
    /// The configured default model (`classification.model`, else the
    /// provider's default).
    pub(crate) model: String,
    pub(crate) provider: &'static dyn ClassificationProvider,
    pub(crate) timeout: Duration,
    pub(crate) retries: u32,
    pub(crate) max_concurrency: usize,
    judgments: Arc<cache::Judgments>,
    /// `<home>` when session statistics are recorded.
    stats_home: Option<PathBuf>,
}

impl Evaluator {
    pub(crate) fn new(
        key: Option<SecretString>,
        base_url: String,
        model: String,
        provider: &'static dyn ClassificationProvider,
        timeout: Duration,
        retries: u32,
        max_concurrency: usize,
        cache_dir: Option<PathBuf>,
        stats_home: Option<PathBuf>,
    ) -> Self {
        Self {
            key,
            base_url,
            model,
            provider,
            timeout,
            retries,
            max_concurrency,
            judgments: Arc::new(cache::Judgments::new(cache_dir)),
            stats_home,
        }
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.key.is_some()
    }

    fn credential(&self) -> Result<&SecretString, ClassificationError> {
        self.key.as_ref().ok_or_else(|| {
            ClassificationError::new(
                "missingConfiguration",
                "No classification API credential is configured.",
                "Set OCTOCODE_CLASSIFICATION_API.",
            )
        })
    }

    /// The endpoint and credential this evaluator's answers belong to.
    pub(crate) fn partition(&self) -> Partition {
        let path = self.provider.defaults().endpoint_path;
        let origin = transport::endpoint(&self.base_url, path).map_or_else(
            |_| format!("{}|{path}", self.base_url),
            |endpoint| crate::providers::classification::gate::endpoint_key(&endpoint),
        );
        Partition::new(origin, self.key.as_ref())
    }

    /// A request budget ending at `deadline` or the evaluator timeout,
    /// whichever comes first.
    pub(crate) fn budget(
        &self,
        deadline: Instant,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> RequestBudget {
        transport::budget(deadline.min(Instant::now() + self.timeout), cancellation)
    }

    /// One call's share of the process-wide endpoint gate.
    pub(crate) fn gate(&self) -> GateLease {
        crate::providers::classification::gate::lease(
            self.partition().identity(),
            self.max_concurrency,
        )
    }

    pub(crate) async fn evaluate_prepared(
        &self,
        input: EvaluationInput<'_>,
        model: Option<&str>,
        budget: &RequestBudget,
        gate: &GateLease,
    ) -> Result<Judged, ClassificationError> {
        self.evaluate_questions(input, model, budget, gate)
            .await
            .complete
    }

    pub(crate) async fn evaluate_questions(
        &self,
        input: EvaluationInput<'_>,
        model: Option<&str>,
        budget: &RequestBudget,
        gate: &GateLease,
    ) -> QuestionOutcomes {
        match self.evaluate_shared(input, model, budget, gate).await {
            Ok(outcomes) => outcomes,
            Err(error) => QuestionOutcomes::from_complete(Err(error), input.questions.len()),
        }
    }

    async fn evaluate_shared(
        &self,
        input: EvaluationInput<'_>,
        model: Option<&str>,
        budget: &RequestBudget,
        gate: &GateLease,
    ) -> Result<QuestionOutcomes, ClassificationError> {
        let key = self.credential()?;
        let model = model.unwrap_or(&self.model);
        let partition = self.partition();
        let entry = cache::Judgments::key(&partition, model, input);
        let ticket = self.judgments.flight.join(entry.resource.clone());
        let mut turn = transport::wait(budget, ticket.turn()).await?;
        transport::check_budget(budget)?;
        if let Some(outcomes) = turn.as_ref() {
            return Ok(outcomes.clone().replayed());
        }
        if let Some(saved) = self.judgments.get(&entry) {
            return Ok(
                QuestionOutcomes::from_complete(Ok(saved), input.questions.len()).replayed(),
            );
        }
        let connection = batch::Connection {
            key,
            base_url: &self.base_url,
            provider: self.provider,
            retries: self.retries,
        };
        let outcomes = batch::answer_questions(input, model, &connection, budget, gate).await;
        match &outcomes.complete {
            Ok(judged) => self.judgments.put(entry, judged),
            Err(error) => {
                // Deadline/cancellation belongs to this caller, not its waiters.
                if !matches!(error.code.as_str(), "cancelled" | "timeout") {
                    *turn = Some(outcomes.clone());
                }
            }
        }
        Ok(outcomes)
    }

    /// The model catalog (clasify `models` output shape), read through the
    /// same authenticated transport and gate.
    pub(crate) async fn models(
        &self,
        budget: &RequestBudget,
        gate: &GateLease,
    ) -> Result<Value, ClassificationError> {
        let key = self.credential()?;
        transport::check_key(key)?;
        let result = transport::get(
            key,
            transport::endpoint(&self.base_url, self.provider.defaults().models_path)?,
            budget,
            self.retries,
            gate,
        )
        .await;
        let calls = match &result {
            Ok((_, calls)) => *calls,
            Err(error) => error.provider_calls,
        };
        self.record(stats::ClassificationUsage {
            discovery_calls: calls,
            ..stats::ClassificationUsage::default()
        });
        let (response, calls) = result?;
        self.provider
            .models(&response)
            .map_err(|error| ClassificationError {
                provider_calls: calls,
                ..ClassificationError::invalid_response(error.message)
            })
    }

    /// Add one call's provider usage to the session statistics.
    pub(crate) fn record(&self, usage: stats::ClassificationUsage) {
        if let Some(home) = &self.stats_home {
            stats::record_classification(home, usage);
        }
    }
}

pub(crate) fn retention() -> Duration {
    cache::TTL
}
