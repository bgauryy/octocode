//! Units judged on their own (clasify's search hits, listed rows and
//! `perItem` items), sent in as few provider requests as keep each answer
//! about its unit alone. The units are packed as the selection judge packs
//! its candidates ([`requests::pack`]): the hits of one file share one state
//! and each hit is one question naming its target; whole units share a
//! request up to [`requests::GROUP_UNITS`]. A packed request states the
//! caller's question words once in its state, and each question names them
//! ([`shared_questions`]). A request that holds one unit is
//! that unit's own request, sent as it always was ([`Unit::alone`]).
//!
//! The answers come back per unit, whatever request carried them:
//!
//! - every answer is validated against its own question; a unit is answered
//!   only when each of its questions is;
//! - a packed request whose response left some answers missing or malformed
//!   asks only those units again, once; one still missing stays an error,
//!   never a guess;
//! - a packed request the provider refuses as too large is split, its units
//!   in two halves packed again at what the refusal taught; a unit left alone
//!   is sent on its own with the caller's long-context choice;
//! - each request's spend is stated once, with its identity, so identical
//!   requests of sibling rows are billed once ([`Evaluations`]).
use super::requests::{self, Asking, Caps, Request};
use super::{section_question, template};
use crate::judgment::shared::{Evaluations, Identity};
use crate::judgment::{ClassificationError, EvaluationInput, Evaluator, LongContext};
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::providers::classification::gate::{GateLease, call_cap};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// One unit: what the judge reads of it, a search hit naming its `target`,
/// and the state it is sent as when a request holds it alone.
pub struct Unit<'a> {
    pub state: &'a Value,
    pub alone: Value,
}

/// One row's units and how they are asked. `units[i]` is `None` when the
/// unit is withheld: nothing of it is sent.
pub struct UnitCall<'a> {
    pub units: &'a [Option<Unit<'a>>],
    /// The caller's questions, in order; each unit is asked every one.
    pub questions: &'a [Value],
    pub context: Option<&'a str>,
    pub model: Option<&'a str>,
    /// The caller's `longContext` choice.
    pub long_context: Option<bool>,
    /// The row's `maxRequests`, counted at each send.
    pub cap: Option<&'a crate::judgment::RequestCap>,
    /// Every unit in its own request, as each was judged before packing:
    /// exact isolation, one request per distinct unit.
    pub isolate: bool,
}

/// One provider request's spend, and its identity when an identical request
/// of a sibling row may share it.
pub struct Spend {
    pub identity: Option<Identity>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub calls: u64,
}

/// What a call's requests answered, unit by unit.
pub struct UnitAnswers {
    /// Per unit, in call order: `None` withheld, else one result per
    /// question.
    pub answers: Vec<Option<Vec<Result<Value, ClassificationError>>>>,
    /// The model that answered first.
    pub model: Option<String>,
    /// Every request sent, in the order the rounds sent them.
    pub spends: Vec<Spend>,
    /// Of the requests sent, those the window refused as too large.
    pub refused: u64,
}

/// The requests a call is planned as before any is sent, and their price:
/// the request bodies' bytes and the input tokens they cost at most
/// ([`Evaluator::tokens_at_most`]). With no refusal and no
/// missing answer, `requests` is the number sent; `requests_at_most` bounds
/// what the refusals, splits and re-asks can add (transient retries aside).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub requests: usize,
    pub requests_at_most: u64,
    pub body_bytes: usize,
    pub input_tokens_at_most: u64,
}

/// One request of a round.
enum Work {
    /// Several distinct units' questions over one shared state. `retry`: a
    /// second ask for missing answers, which is never shared, so an
    /// identical earlier request does not answer it from its own outcome.
    Packed { request: Request, retry: bool },
    /// One distinct unit and its identical twins (their ordinals), sent as
    /// the first one's own request.
    Alone { ordinals: Vec<usize> },
}

/// The caller's `question` about the target `label` names: its own words,
/// beside the template's target slot (an instructions object gains the slot;
/// any other instructions become its `question`).
pub(super) fn aimed(question: &Value, label: &str) -> Option<Value> {
    let slot = template()?["targetSlot"].as_str()?;
    let mut aimed = question.clone();
    let instructions = match question.get("instructions") {
        Some(Value::Object(fields)) if !fields.contains_key(slot) => {
            let mut fields = fields.clone();
            fields.insert(slot.to_owned(), Value::String(label.to_owned()));
            fields
        }
        other => {
            let mut fields = Map::new();
            if let Some(words) = other {
                fields.insert("question".into(), words.clone());
            }
            fields.insert(slot.to_owned(), Value::String(label.to_owned()));
            fields
        }
    };
    aimed["instructions"] = Value::Object(instructions);
    Some(aimed)
}

/// The name of the caller's question `at` in a packed request's shared
/// question text ([`shared_questions`]).
fn question_name(at: usize) -> String {
    format!("Q{}", at + 1)
}

/// What names the caller's question `at` in a packed request's questions:
/// its entry in the state's shared question text (`questions.Q1`).
fn reference(at: usize) -> String {
    format!("{}.{}", requests::SHARED_QUESTIONS, question_name(at))
}

/// Whether the caller's words are stated once: only when they are longer
/// than twice the name that stands for them, so every packed request of two
/// or more hits is smaller for it.
fn shares(question: &Value, at: usize) -> bool {
    let words = serde_json::to_vec(&question["instructions"]).map_or(0, |bytes| bytes.len());
    words > 2 * (reference(at).len() + 2)
}

/// The caller's questions' own words that a packed request states once in
/// its state, `{"Q1": <instructions>, …}` ([`requests::SHARED_QUESTIONS`]);
/// `None` when no question's words are long enough to share ([`shares`]).
/// Measured live on flowD1 (wave 3, 2026-11-19): 8,171 → 5,642 input tokens
/// for the same 3 requests, and the yes set nearer each hit judged alone
/// (Jaccard 0.923 and 0.846 against the unbatched run, against 0.750 and
/// 0.833 with the words in every question).
fn shared_questions(questions: &[Value]) -> Option<Value> {
    let shared = questions
        .iter()
        .enumerate()
        .filter(|(at, question)| shares(question, *at))
        .map(|(at, question)| (question_name(at), question["instructions"].clone()))
        .collect::<Map<_, _>>();
    (!shared.is_empty()).then_some(Value::Object(shared))
}

/// The caller's question `at` about the target `label` names, in a packed
/// request: its type and criteria as asked; its words, when the state
/// states them once ([`shares`]), named by their entry (`questions.Q1`)
/// beside the template's target slot, else its own words ([`aimed`]).
fn named(question: &Value, at: usize, label: &str) -> Option<Value> {
    if !shares(question, at) {
        return aimed(question, label);
    }
    let slot = template()?["targetSlot"].as_str()?;
    let mut named = question.clone();
    let mut instructions = Map::new();
    instructions.insert("question".into(), Value::String(reference(at)));
    instructions.insert(slot.to_owned(), Value::String(label.to_owned()));
    named["instructions"] = Value::Object(instructions);
    Some(named)
}

impl Evaluator {
    /// Judge every unit of `call` ([`self`](super::units)).
    pub async fn judge_units(
        &self,
        call: &UnitCall<'_>,
        evaluations: &Evaluations,
        gate: &GateLease,
        context: &ExecutionContext,
    ) -> UnitAnswers {
        self.judge_units_at(call, evaluations, gate, context, requests::GROUP_UNITS)
            .await
    }

    /// The requests [`Self::judge_units`] would send, and their price.
    pub fn plan_units(&self, call: &UnitCall<'_>) -> Plan {
        self.plan_units_at(call, requests::GROUP_UNITS)
    }

    pub(super) fn plan_units_at(&self, call: &UnitCall<'_>, group_units: usize) -> Plan {
        let long = call.long_context == Some(true);
        let model = call.model.unwrap_or(&self.model);
        let mut body_bytes = 0_usize;
        let mut requests_at_most = 0_u64;
        let mut tokens = 0_u64;
        let works = self.works(call, &judged_ordinals(call), group_units, false);
        for work in &works {
            let (bytes, at_most, at_most_tokens) = match work {
                Work::Packed { request, .. } => {
                    let (all, one) = crate::judgment::batch::request_sizes(
                        &request.state,
                        &request.questions,
                        model,
                        self.provider,
                    );
                    (
                        all,
                        self.packed_at_most(call, request, long, all, one),
                        self.tokens_at_most(call.model, long, [(all, request.questions.len())]),
                    )
                }
                Work::Alone { ordinals } => {
                    let Some(sends) = self.alone_sends(call, ordinals[0]) else {
                        continue;
                    };
                    (
                        sends.bytes,
                        sends.at_most,
                        self.first_round_tokens_at_most(
                            call.model,
                            long,
                            &sends,
                            call.questions.len(),
                        ),
                    )
                }
            };
            body_bytes = body_bytes.saturating_add(bytes);
            requests_at_most = requests_at_most.saturating_add(at_most);
            tokens = tokens.saturating_add(at_most_tokens);
        }
        Plan {
            requests: works.len(),
            requests_at_most,
            body_bytes,
            input_tokens_at_most: tokens,
        }
    }

    /// What unit `ordinal` sends as its own request ([`Work::Alone`]).
    fn alone_sends(&self, call: &UnitCall<'_>, ordinal: usize) -> Option<crate::judgment::Sends> {
        let unit = call.units[ordinal].as_ref()?;
        let input = EvaluationInput {
            state: &unit.alone,
            questions: call.questions,
            assets: None,
            long_context: LongContext::from_field(call.long_context),
            companions: 0,
            cap: None,
        };
        Some(self.sends(input, call.model))
    }

    /// The most requests one packed request of `all` bytes (`one` with only
    /// its largest question) can lead to. One the window can never refuse
    /// sends itself and, when its response leaves answers missing, one
    /// re-ask of them: a subset of a request that fits fits too, and a lone
    /// unit re-asked is its own request. One the window may refuse is split
    /// in halves down to lone units: of `n` distinct units, at most `n - 1`
    /// packed requests split, at most `n - 2` more re-ask what their
    /// responses left missing (each unit is asked again once), and each unit
    /// is sent alone at most once.
    fn packed_at_most(
        &self,
        call: &UnitCall<'_>,
        request: &Request,
        long: bool,
        all: usize,
        one: usize,
    ) -> u64 {
        // Each distinct unit (its first question), by the first ordinal
        // reading it: what it sends when it goes alone.
        let mut seen = HashSet::new();
        let alone = request
            .slots
            .iter()
            .filter(|slot| seen.insert(slot.1))
            .map(|slot| {
                self.alone_sends(call, slot.0)
                    .map_or(0, |sends| sends.at_most)
            })
            .collect::<Vec<_>>();
        if crate::judgment::batch::never_refused(self.provider, long, all, one) {
            return alone
                .iter()
                .copied()
                .max()
                .unwrap_or(1)
                .max(1)
                .saturating_add(1);
        }
        let n = u64::try_from(alone.len()).unwrap_or(u64::MAX);
        n.saturating_sub(1)
            .saturating_add(n.saturating_sub(2))
            .saturating_add(alone.iter().sum::<u64>())
    }

    /// [`Self::judge_units`] with whole units sharing a request up to
    /// `group_units` of them.
    pub(super) async fn judge_units_at(
        &self,
        call: &UnitCall<'_>,
        evaluations: &Evaluations,
        gate: &GateLease,
        context: &ExecutionContext,
        group_units: usize,
    ) -> UnitAnswers {
        let mut judged = UnitAnswers {
            answers: vec![None; call.units.len()],
            model: None,
            spends: Vec::new(),
            refused: 0,
        };
        let mut retried = HashSet::new();
        let mut round = self.works(call, &judged_ordinals(call), group_units, false);
        let width = call_cap(self.max_concurrency).max(1);
        while !round.is_empty() {
            let sent = stream::iter(round)
                .map(|work| async move {
                    let outcome = self.send(call, &work, evaluations, gate, context).await;
                    (work, outcome)
                })
                .buffered(width)
                .collect::<Vec<_>>()
                .await;
            let mut next = Vec::new();
            for (work, outcome) in sent {
                next.extend(self.settle(
                    call,
                    work,
                    outcome,
                    &mut judged,
                    &mut retried,
                    group_units,
                ));
            }
            round = next;
        }
        judged
    }

    /// The requests `ordinals` (units with a state) are packed as, at the
    /// windows the provider has taught so far; a request of one distinct
    /// unit is that unit alone. A question the template cannot ask sends
    /// every unit alone.
    fn works(
        &self,
        call: &UnitCall<'_>,
        ordinals: &[usize],
        group_units: usize,
        retry: bool,
    ) -> Vec<Work> {
        let alone = || -> Vec<Work> {
            ordinals
                .iter()
                .map(|&ordinal| Work::Alone {
                    ordinals: vec![ordinal],
                })
                .collect()
        };
        if call.isolate {
            return isolated(call, ordinals);
        }
        let Some(scope) = section_question() else {
            return alone();
        };
        // A packed request states each question's text once
        // ([`Asking::shared`]); a question names its entry and its target.
        let shared = shared_questions(call.questions);
        let ask = |target: Option<&str>| -> Option<Vec<Value>> {
            match target {
                None => Some(call.questions.to_vec()),
                Some(label) => call
                    .questions
                    .iter()
                    .enumerate()
                    .map(|(at, question)| named(question, at, label))
                    .collect(),
            }
        };
        let asking = Asking {
            ask: &ask,
            hit_scope: scope,
            unit_scope: scope,
            context: call.context,
            own_windows: true,
            shared: shared.as_ref(),
        };
        let units = ordinals
            .iter()
            .filter_map(|&ordinal| Some((ordinal, call.units[ordinal].as_ref()?.state)));
        let Some(requests) = requests::pack(
            units,
            call.questions.len(),
            &asking,
            self.caps(call, group_units),
        ) else {
            return alone();
        };
        requests
            .into_iter()
            .map(|request| {
                let first = request.slots.first().map(|slot| slot.1);
                if request.slots.iter().all(|slot| Some(slot.1) == first) {
                    Work::Alone {
                        ordinals: request.slots.iter().map(|slot| slot.0).collect(),
                    }
                } else {
                    Work::Packed { request, retry }
                }
            })
            .collect()
    }

    /// One request's room, from the provider's stated windows at what its
    /// answers taught of their tokens, never past the request body limit.
    fn caps(&self, call: &UnitCall<'_>, group_units: usize) -> Caps {
        let long = call.long_context == Some(true);
        let body = octocode_runtime_base::id::clasify_policy::REQUEST_BYTES;
        Caps {
            question_bytes: self.request_fit(call.model, long).bytes.min(body),
            request_bytes: self
                .fit(call.model, long, self.request_window(long))
                .bytes
                .min(body),
            group_units,
            hit_windows: true,
        }
    }

    async fn send(
        &self,
        call: &UnitCall<'_>,
        work: &Work,
        evaluations: &Evaluations,
        gate: &GateLease,
        context: &ExecutionContext,
    ) -> Sent {
        // One request's time starts when it runs, not when the row does;
        // the call's deadline bounds them all.
        let budget = self.budget(
            octocode_runtime_base::execution::cancel::deadline_of(context),
            context.cancellation.clone(),
        );
        match work {
            Work::Alone { ordinals } => {
                let Some(unit) = call.units[ordinals[0]].as_ref() else {
                    return Sent::Withheld;
                };
                let input = EvaluationInput {
                    state: &unit.alone,
                    questions: call.questions,
                    assets: None,
                    long_context: LongContext::from_field(call.long_context),
                    companions: 0,
                    cap: call.cap,
                };
                let (judged, identity) = self
                    .evaluate_once(evaluations, input, call.model, &budget, gate)
                    .await;
                Sent::Alone(judged, identity)
            }
            Work::Packed { request, retry } => {
                // A packed request is never retried in the long window: its
                // units are split instead, in the window they were packed for.
                let input = EvaluationInput {
                    state: &request.state,
                    questions: &request.questions,
                    assets: None,
                    long_context: if call.long_context == Some(true) {
                        LongContext::Always
                    } else {
                        LongContext::Never
                    },
                    companions: 0,
                    cap: call.cap,
                };
                if *retry {
                    let outcomes = self.evaluate_group(input, call.model, &budget, gate).await;
                    Sent::Packed(outcomes, None)
                } else {
                    let (outcomes, identity) = self
                        .evaluate_group_once(evaluations, input, call.model, &budget, gate)
                        .await;
                    Sent::Packed(outcomes, identity)
                }
            }
        }
    }

    /// Records one request's answers and spend; returns the requests it
    /// leads to: the halves of a refused one, the re-ask of missing answers.
    fn settle(
        &self,
        call: &UnitCall<'_>,
        work: Work,
        sent: Sent,
        judged: &mut UnitAnswers,
        retried: &mut HashSet<usize>,
        group_units: usize,
    ) -> Vec<Work> {
        let count = call.questions.len();
        match (work, sent) {
            (_, Sent::Withheld) => Vec::new(),
            (Work::Alone { ordinals }, Sent::Alone(outcome, identity)) => {
                let answers: Vec<Result<Value, ClassificationError>> = match outcome {
                    Ok(answered) => {
                        judged.model.get_or_insert_with(|| answered.model.clone());
                        judged.refused = judged.refused.saturating_add(
                            answered
                                .long_context_retry
                                .map_or(0, |retry| retry.refused_calls),
                        );
                        judged.spends.push(Spend {
                            identity,
                            input_tokens: answered.input_tokens,
                            output_tokens: answered.output_tokens,
                            calls: answered.provider_calls,
                        });
                        answered.answers.into_iter().map(Ok).collect()
                    }
                    Err(error) => {
                        let usage = error.usage();
                        judged.spends.push(Spend {
                            identity,
                            input_tokens: usage.input_tokens,
                            output_tokens: usage.output_tokens,
                            calls: error.provider_calls,
                        });
                        (0..count).map(|_| Err(error.clone())).collect()
                    }
                };
                for ordinal in ordinals {
                    merge(&mut judged.answers[ordinal], answers.clone());
                }
                Vec::new()
            }
            (Work::Packed { request, .. }, Sent::Packed(outcomes, identity)) => {
                let (input_tokens, output_tokens, calls, refused) = match &outcomes.complete {
                    Ok(answered) => (
                        answered.input_tokens,
                        answered.output_tokens,
                        answered.provider_calls,
                        None,
                    ),
                    Err(error) => {
                        let usage = error.usage();
                        let refused = (error.code == ErrorCode::ClassificationStateTooLarge)
                            .then_some(error.provider_calls);
                        (
                            usage.input_tokens,
                            usage.output_tokens,
                            error.provider_calls,
                            refused,
                        )
                    }
                };
                judged.spends.push(Spend {
                    identity,
                    input_tokens,
                    output_tokens,
                    calls,
                });
                if judged.model.is_none() {
                    judged.model = outcomes.models.iter().next().cloned();
                }
                // Each distinct unit's first question, and the ordinals
                // reading it.
                let mut blocks = BTreeMap::<usize, Vec<usize>>::new();
                for &(ordinal, first) in &request.slots {
                    blocks.entry(first).or_default().push(ordinal);
                }
                if let Some(refused) = refused {
                    judged.refused = judged.refused.saturating_add(refused);
                    let blocks = blocks.into_values().collect::<Vec<_>>();
                    let (front, back) = blocks.split_at(blocks.len() / 2);
                    return [front, back]
                        .into_iter()
                        .flat_map(|half| {
                            let ordinals = half.iter().flatten().copied().collect::<Vec<_>>();
                            self.works(call, &ordinals, group_units, false)
                        })
                        .collect();
                }
                let read_back = outcomes.answers.iter().any(Result::is_ok);
                let mut again = Vec::new();
                for (first, ordinals) in blocks {
                    let answers = (first..first + count)
                        .map(|at| {
                            outcomes.answers.get(at).cloned().unwrap_or_else(|| {
                                Err(ClassificationError::invalid_response(
                                    "Classification provider response is missing the expected answer.",
                                ))
                            })
                        })
                        .collect::<Vec<_>>();
                    let missing = answers.iter().any(|answer| {
                        answer.as_ref().is_err_and(|error| {
                            error.code == ErrorCode::InvalidClassificationResponse
                        })
                    });
                    for &ordinal in &ordinals {
                        merge(&mut judged.answers[ordinal], answers.clone());
                    }
                    // Only a response that answered something is asked again,
                    // and each unit once.
                    if missing && read_back && retried.insert(ordinals[0]) {
                        again.extend(ordinals);
                    }
                }
                if again.is_empty() {
                    Vec::new()
                } else {
                    self.works(call, &again, group_units, true)
                }
            }
            (_, _) => Vec::new(),
        }
    }
}

/// What one request answered.
enum Sent {
    Alone(
        Result<crate::judgment::Judged, ClassificationError>,
        Option<Identity>,
    ),
    Packed(crate::judgment::QuestionOutcomes, Option<Identity>),
    Withheld,
}

/// Each distinct unit of `ordinals` as its own request, identical ones (the
/// same state sent alone) reading one: what every unit was sent as before
/// units shared requests.
fn isolated(call: &UnitCall<'_>, ordinals: &[usize]) -> Vec<Work> {
    let mut works: Vec<Work> = Vec::new();
    let mut first = HashMap::<String, usize>::new();
    for &ordinal in ordinals {
        let Some(unit) = call.units[ordinal].as_ref() else {
            continue;
        };
        match first.get(&unit.alone.to_string()) {
            Some(&at) => {
                if let Some(Work::Alone { ordinals }) = works.get_mut(at) {
                    ordinals.push(ordinal);
                }
            }
            None => {
                first.insert(unit.alone.to_string(), works.len());
                works.push(Work::Alone {
                    ordinals: vec![ordinal],
                });
            }
        }
    }
    works
}

/// The units with a state, in call order.
fn judged_ordinals(call: &UnitCall<'_>) -> Vec<usize> {
    (0..call.units.len())
        .filter(|&ordinal| call.units[ordinal].is_some())
        .collect()
}

/// A unit's answers from one more request: the first ever, or, after a
/// re-ask, each question still failing takes the new result.
fn merge(
    known: &mut Option<Vec<Result<Value, ClassificationError>>>,
    answers: Vec<Result<Value, ClassificationError>>,
) {
    match known {
        None => *known = Some(answers),
        Some(known) => {
            for (old, new) in known.iter_mut().zip(answers) {
                if old.is_err() {
                    *old = new;
                }
            }
        }
    }
}
