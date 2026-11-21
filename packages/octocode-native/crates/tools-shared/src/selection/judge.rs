//! Inline evidence questions use the same typed evaluator as `clasify`.
//! Every admitted candidate receives independent questions; the core-owned
//! probability policy selects, excludes or leaves it unknown before host output.
use super::{Decision, JudgeCall, JudgeFailure, Judgments, ProviderError, SelectionJudge};
use crate::judgment::{
    ClassificationError, EvaluationInput, Evaluator, RequestCap, stats::ClassificationUsage,
};
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

mod requests;
mod units;

pub use units::{Plan, Spend, Unit, UnitAnswers, UnitCall};

const CONTROL_FIELDS: [&str; 8] = [
    "next",
    "related",
    "diagnostics",
    "hints",
    "selection",
    "snapshot",
    "pagination",
    "matchIndices",
];

fn template() -> Option<&'static Value> {
    octocode_runtime_base::contracts::parsed_contract()
        .ok()?
        .get("classifierTemplate")
}

/// A classifier fills the contract template's slot verbatim: the template
/// (core `classifierTemplate`) judges it as a predicate about the source, or
/// about `target` when a question names one line of a shared state. A target
/// question leaves out the question text every target shares: the shared
/// state carries it once ([`target_question`]).
fn compile(classifier: &str, target: Option<&str>) -> Option<Value> {
    let template = template()?;
    let slot = template["slot"].as_str()?;
    let mut question = template["question"].clone();
    let instructions = question["instructions"].as_object_mut()?;
    instructions.insert(slot.to_owned(), Value::String(classifier.to_owned()));
    if let Some(target) = target {
        instructions.shift_remove("question");
        instructions.insert(
            template["targetSlot"].as_str()?.to_owned(),
            Value::String(target.to_owned()),
        );
    }
    Some(question)
}

/// The template's question about a target line, which the state the targets
/// share holds once for all of them.
fn target_question() -> Option<&'static Value> {
    template()?
        .get("targetQuestion")
        .filter(|question| question.is_string())
}

/// The template's scope for a section question (core
/// `CLASSIFIER_TEMPLATE.sectionQuestion`): answer about the target section,
/// its context entries being background only. A section state carries it as
/// its `question`, as a shared hit state carries [`target_question`].
pub fn section_question() -> Option<&'static Value> {
    template()?
        .get("sectionQuestion")
        .filter(|question| question.is_string())
}

fn candidate_state(mut state: Value) -> Value {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                fields.retain(|key, _| !CONTROL_FIELDS.contains(&key.as_str()));
                fields.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut state);
    state
}

/// Compress only inline source coordinates, after candidate admission. Inclusive
/// runs preserve every coordinate, including omission markers and repeated lines.
fn provider_state(state: &Value) -> std::borrow::Cow<'_, Value> {
    let Some(lines) = state.get("sourceLines").and_then(Value::as_array) else {
        return std::borrow::Cow::Borrowed(state);
    };
    let mut ranges: Vec<[u64; 2]> = Vec::new();
    for value in lines {
        let Some(line) = value.as_u64() else {
            return std::borrow::Cow::Borrowed(state);
        };
        if let Some(last) = ranges.last_mut()
            && line > 0
            && last[1] > 0
            && last[1].checked_add(1) == Some(line)
        {
            last[1] = line;
        } else {
            ranges.push([line, line]);
        }
    }
    let mut compact = state.clone();
    if let Some(fields) = compact.as_object_mut() {
        fields.shift_remove("sourceLines");
        fields.insert("sourceLineRanges".into(), json!(ranges));
    }
    std::borrow::Cow::Owned(compact)
}

/// The level holding more than `threshold` of an answer's probability:
/// selected (direct), related (a confident near-miss) or excluded; an answer
/// with no such level is undecided.
#[derive(Clone, Copy)]
struct Policy {
    threshold: f64,
    selected: usize,
    related: usize,
    excluded: usize,
}

impl Policy {
    fn from_contract() -> Option<Self> {
        let template = template()?;
        let policy = &template["selectionPolicy"];
        if policy["version"].as_u64()? != 2 {
            return None;
        }
        let threshold = policy["threshold"].as_f64()?;
        let level = |key: &str| usize::try_from(policy[key].as_u64()?).ok();
        let (selected, related, excluded) = (
            level("selectedLevel")?,
            level("relatedLevel")?,
            level("excludedLevel")?,
        );
        let levels = template["question"]["criteria"].as_array()?.len();
        if !threshold.is_finite()
            || !(0.5..1.0).contains(&threshold)
            || [selected, related, excluded]
                .iter()
                .any(|&level| level >= levels)
            || selected == excluded
            || related == selected
            || related == excluded
        {
            return None;
        }
        Some(Self {
            threshold,
            selected,
            related,
            excluded,
        })
    }

    fn decide(self, answers: &[Value]) -> Decision {
        if answers.is_empty() {
            return Decision::Unknown;
        }
        let mut selected = false;
        let mut all_excluded = true;
        // Every answer decided (related or excluded), at least one related.
        let mut all_decided = true;
        for answer in answers {
            if answer["type"] != "score" {
                return Decision::Unknown;
            }
            let Some(probabilities) = answer["probabilities"].as_object() else {
                return Decision::Unknown;
            };
            // The shared provider validator owns distribution shape, totals and
            // Score consistency. Selection uses its validated named outcomes.
            let Some(direct) = probabilities
                .get(&self.selected.to_string())
                .and_then(Value::as_f64)
            else {
                return Decision::Unknown;
            };
            let Some(irrelevant) = probabilities
                .get(&self.excluded.to_string())
                .and_then(Value::as_f64)
            else {
                return Decision::Unknown;
            };
            let Some(related) = probabilities
                .get(&self.related.to_string())
                .and_then(Value::as_f64)
            else {
                return Decision::Unknown;
            };
            selected |= direct > self.threshold;
            all_excluded &= irrelevant > self.threshold;
            all_decided &= related > self.threshold || irrelevant > self.threshold;
        }
        if selected {
            Decision::Selected
        } else if all_excluded {
            Decision::Excluded
        } else if all_decided {
            Decision::Related
        } else {
            Decision::Unknown
        }
    }
}

/// A candidate's provider failure as its receipt records it, or `None` when
/// the error does not fit the selection error contract.
fn selection_failure(
    ordinal: usize,
    error: &ClassificationError,
) -> Option<octocode_contract_types::SelectionFailure> {
    Some(octocode_contract_types::SelectionFailure {
        candidate: i64::try_from(ordinal).ok()?,
        error: error.message.parse().ok()?,
        error_class: Some(octocode_runtime_base::id::error_codes::class(error.code)),
        error_code: error.code,
        hints: if error.hints.is_empty() {
            None
        } else {
            Some(octocode_contract_types::RowHints(
                error
                    .hints
                    .iter()
                    .map(|hint| hint.parse().ok())
                    .collect::<Option<_>>()?,
            ))
        },
    })
}

/// A page whose first round needs `planned` requests, over the caller's
/// `max`: nothing was sent.
fn over_budget(planned: u64, max: u64) -> ProviderError {
    ProviderError {
        code: ErrorCode::ClassificationOverBudget,
        message: format!(
            "This page's classification needs {planned} provider requests, over classification.maxRequests {max}; none was sent."
        ),
        hints: vec![
            "next.confirm sends it with maxRequests at selection.usage.requestsCeiling (billed); next.unfiltered shows every unit, unbilled.".into(),
        ],
    }
}

impl SelectionJudge for Evaluator {
    fn identity(&self) -> Value {
        json!({
            "partition":self.partition().identity(), "model":self.model,
            "template":template(),
            "sourceCoordinates":1,
            // Whole units of a page share requests (unit groups) up to this
            // many: a plan judged at another size is not replayed.
            "unitGroups":requests::GROUP_UNITS,
        })
    }

    fn project(&self, state: Value) -> Value {
        candidate_state(state)
    }

    fn question_chars(&self, predicates: &[String]) -> usize {
        predicates
            .iter()
            .filter_map(|classifier| compile(classifier, Some("line 1")))
            .map(|question| crate::selection::serialized_chars(&question))
            .sum()
    }

    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        self.select_grouped(call, requests::GROUP_UNITS)
    }
}

impl Evaluator {
    /// [`SelectionJudge::select`] with whole units sharing a request up to
    /// `group_units` of them.
    fn select_grouped(&self, call: JudgeCall<'_>, group_units: usize) -> Judgments {
        if !self.is_configured() {
            return Err("classificationUnavailable".into());
        }
        let policy =
            Policy::from_contract().ok_or(JudgeFailure::from("invalidClassificationPolicy"))?;
        let long = call.long_context == crate::judgment::LongContext::Always;
        // One request's room, from the provider's stated windows at what its
        // answers taught of their tokens, never past the request body limit.
        let body = octocode_runtime_base::id::clasify_policy::REQUEST_BYTES;
        let caps = requests::Caps {
            question_bytes: self.request_fit(None, long).bytes.min(body),
            request_bytes: self
                .fit(None, long, self.request_window(long))
                .bytes
                .min(body),
            group_units,
            hit_windows: true,
        };
        let requests = requests::requests(&call, caps)
            .ok_or(JudgeFailure::from("invalidClassificationRequest"))?;
        // The page's price before any request is sent: each request's first
        // round and the most its refusals can add ([`Self::sends`]).
        let (planned, ceiling) =
            requests
                .iter()
                .fold((0_u64, 0_u64), |(planned, ceiling), request| {
                    let sends = self.sends(
                        EvaluationInput {
                            state: &request.state,
                            questions: &request.questions,
                            assets: None,
                            long_context: call.long_context,
                            companions: 0,
                            cap: None,
                        },
                        None,
                    );
                    (
                        planned.saturating_add(sends.first),
                        ceiling.saturating_add(sends.at_most),
                    )
                });
        // Priced before any request: a plan over the caller's cap sends
        // nothing and answers with its price; within it, every send, re-ask,
        // split and retry is held to the cap (clasify's admission rule).
        let retries = self.retry_room();
        let max = call.max_requests;
        if planned > max {
            *call.usage = ClassificationUsage {
                requests_planned: Some(planned),
                requests_ceiling: Some(RequestCap::ceiling(ceiling, retries)),
                ..ClassificationUsage::default()
            };
            return Err(JudgeFailure::Provider(over_budget(planned, max)));
        }
        // The first round's sends, then what the cap leaves for re-asks,
        // splits and transient retries: never past the caller's cap.
        let at_most = ceiling.min(max);
        let room = max - at_most;
        let cap = RequestCap::held(at_most, room);
        let cap = &cap;
        let gate = self.gate();
        let long_context = call.long_context;
        let context = call.context;
        let outcomes = call.handle.block_on(async {
            stream::iter(requests.iter())
                .map(|request| {
                    let gate = &gate;
                    async move {
                        // One request's time starts when it runs, not when
                        // the page does; the call's deadline bounds them all.
                        let budget = self.budget(
                            octocode_runtime_base::execution::cancel::deadline_of(context),
                            context.cancellation.clone(),
                        );
                        let outcome = self
                            .evaluate_questions(
                                EvaluationInput {
                                    state: &request.state,
                                    questions: &request.questions,
                                    assets: None,
                                    long_context,
                                    companions: 0,
                                    cap: Some(cap),
                                },
                                None,
                                &budget,
                                gate,
                            )
                            .await;
                        (request, outcome)
                    }
                })
                .buffer_unordered(
                    octocode_runtime_base::providers::classification::gate::call_cap(
                        self.max_concurrency,
                    ),
                )
                .collect::<Vec<_>>()
                .await
        });
        let mut usage = ClassificationUsage::default();
        let mut decisions = Vec::new();
        // The first provider failure in source order, reported when nothing
        // answered. A paused provider (the circuit opened by the failures of
        // this very call) is their consequence, so a failure that caused it
        // comes first. A cancelled call is no answer of the provider's.
        let mut first_failure: Option<((bool, usize), ClassificationError)> = None;
        let predicates = call.predicates.len();
        for (request, outcome) in outcomes {
            usage.add(&outcome.complete);
            usage.models.extend(outcome.models.iter().cloned());
            for &(ordinal, first) in &request.slots {
                let mut answers = Vec::with_capacity(predicates);
                let mut failure = None;
                for answer in outcome.answers.iter().skip(first).take(predicates) {
                    match answer {
                        Ok(answer) => answers.push(answer.clone()),
                        Err(error) => failure = failure.or(Some(error)),
                    }
                }
                let Some(error) = failure else {
                    let decision = if answers.len() == predicates {
                        policy.decide(&answers)
                    } else {
                        Decision::Unknown
                    };
                    decisions.push((ordinal, decision));
                    continue;
                };
                let recorded = selection_failure(ordinal, error).ok_or_else(|| {
                    JudgeFailure::Provider(ProviderError {
                        code: ErrorCode::InvalidClassificationResponse,
                        message:
                            "Provider failure violates the canonical selection error contract."
                                .into(),
                        hints: vec!["Inspect the provider response and retry.".into()],
                    })
                })?;
                call.failures.push(recorded);
                let rank = (error.paused, ordinal);
                if error.code != ErrorCode::Cancelled
                    && first_failure
                        .as_ref()
                        .is_none_or(|(first, _)| rank < *first)
                {
                    first_failure = Some((rank, error.clone()));
                }
            }
        }
        usage.requests_planned = Some(planned);
        // The plan's own ceiling, its retries included, held to the cap.
        usage.requests_ceiling = Some(RequestCap::ceiling(ceiling, retries).min(max));
        *call.usage = usage;
        // A provider that failed every candidate gave no answer at all: its
        // typed error (credential, quota, transport, too slow) is the row's
        // outcome, never an "unknown" judgment that looks like an honest
        // unsure model.
        match first_failure {
            Some((_, error)) if decisions.is_empty() => {
                Err(JudgeFailure::Provider(ProviderError {
                    code: error.code,
                    message: error.message,
                    hints: error.hints,
                }))
            }
            _ => Ok(decisions),
        }
    }
}

#[cfg(test)]
mod ceiling_tests;
#[cfg(test)]
mod provider_tests;
#[cfg(test)]
mod question_tests;
#[cfg(test)]
mod unit_group_tests;
#[cfg(test)]
mod units_tests;
