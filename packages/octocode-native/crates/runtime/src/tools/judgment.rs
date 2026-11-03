//! The one classification evaluator: a captured state plus JEV questions in,
//! validated typed answers, the answering model and usage out. Retrieval
//! `classification` (through the selection judge) and public `clasify` both use
//! [`Evaluator`]; neither keeps its own provider client, retry loop or
//! accounting. Each evaluation reads the provider with its own request budget;
//! identical evaluations inside one request share one provider request
//! ([`shared`]), and nothing outlives the request.
pub(crate) mod batch;
pub(crate) mod shared;
pub(crate) mod stats;

#[cfg(test)]
mod tests;

use octocode_contract_types::ErrorCode;
pub(crate) use transport::ClassificationError;

use crate::providers::classification::{ClassificationProvider, gate::GateLease, transport};
use octocode_github::Partition;
use octocode_io::budget::RequestBudget;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
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

/// Whether `key` occurs in a value of `value` (a state or a question), never
/// a property name. An asset is sent only where a value names it: capture
/// refuses an unreferenced one before reading it, and a single-question
/// request carries only the assets that question or the state names. A key
/// is cited as a whole token: asset names are runs of `[A-Za-z0-9._-]`, so a
/// neighbouring letter, digit or `_` (or a `.`/`-` that joins one) makes the
/// occurrence part of a longer word or name, not a citation.
pub(crate) fn referenced(value: &Value, key: &str) -> bool {
    match value {
        Value::String(text) => cites(text, key),
        Value::Array(values) => values.iter().any(|value| referenced(value, key)),
        Value::Object(values) => values.values().any(|value| referenced(value, key)),
        _ => false,
    }
}

fn cites(text: &str, key: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    // Whether the characters next to an occurrence, nearest first, end the
    // token: none, a non-name character, or a `.`/`-` that closes a sentence
    // or phrase rather than joining a longer name.
    let ends = |next: &mut dyn Iterator<Item = char>| match next.next() {
        None => true,
        Some(c) if word(c) => false,
        Some('.' | '-') => !next.next().is_some_and(word),
        Some(_) => true,
    };
    !key.is_empty()
        && text.match_indices(key).any(|(start, matched)| {
            ends(&mut text[..start].chars().rev())
                && ends(&mut text[start + matched.len()..].chars())
        })
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
    /// Provider requests this call made.
    #[serde(skip)]
    pub provider_calls: u64,
    #[serde(skip)]
    pub known_usage_calls: u64,
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
            stats_home,
        }
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.key.is_some()
    }

    fn credential(&self) -> Result<&SecretString, ClassificationError> {
        self.key.as_ref().ok_or_else(|| {
            ClassificationError::new(
                ErrorCode::MissingConfiguration,
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
        cancellation: octocode_io::cancellation::Cancellation,
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
        transport::check_budget(budget)?;
        let connection = batch::Connection {
            key,
            base_url: &self.base_url,
            provider: self.provider,
            retries: self.retries,
        };
        Ok(batch::answer_questions(input, model, &connection, budget, gate).await)
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
