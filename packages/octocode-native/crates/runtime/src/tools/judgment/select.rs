//! Inline evidence questions use the same typed evaluator as `clasify`.
//! Every admitted candidate receives independent questions; the core-owned
//! probability policy selects, excludes or leaves it unknown before host output.
use super::{ClassificationError, EvaluationInput, Evaluator, stats::ClassificationUsage};
use crate::tools::selection::{
    Decision, JudgeCall, JudgeFailure, Judgments, ProviderError, SelectionJudge,
};
use futures_util::{StreamExt as _, stream};
use serde_json::{Value, json};
use std::time::Instant;

const CONTROL_FIELDS: [&str; 7] = [
    "next",
    "diagnostics",
    "hints",
    "selection",
    "snapshot",
    "pagination",
    "matchIndices",
];

fn template() -> Option<&'static Value> {
    crate::contracts::parsed_contract()
        .ok()?
        .get("classifierTemplate")
}

/// A classifier fills the contract template's slot verbatim: the template
/// (core `classifierTemplate`) judges it as a predicate about the source.
/// Questions are independent, so each receives the optional research context.
pub(crate) fn compile(
    classifier: &str,
    main_goal: Option<&str>,
    reasoning: Option<&str>,
) -> Option<Value> {
    let template = template()?;
    let slot = template["slot"].as_str()?;
    let mut question = template["question"].clone();
    let instructions = question["instructions"].as_object_mut()?;
    instructions.insert(slot.to_owned(), Value::String(classifier.to_owned()));
    for (key, value) in [("mainGoal", main_goal), ("reasoning", reasoning)] {
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            instructions.insert(key.to_owned(), json!(value));
        }
    }
    Some(question)
}

pub(crate) fn candidate_state(mut state: Value) -> Value {
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

#[derive(Clone, Copy)]
struct Policy {
    threshold: f64,
    selected: usize,
    excluded: usize,
}

impl Policy {
    fn from_contract() -> Option<Self> {
        let template = template()?;
        let policy = &template["selectionPolicy"];
        if policy["version"].as_u64()? != 1 {
            return None;
        }
        let threshold = policy["threshold"].as_f64()?;
        let selected = usize::try_from(policy["selectedLevel"].as_u64()?).ok()?;
        let excluded = usize::try_from(policy["excludedLevel"].as_u64()?).ok()?;
        let levels = template["question"]["criteria"].as_array()?.len();
        if !threshold.is_finite()
            || !(0.5..1.0).contains(&threshold)
            || selected >= levels
            || excluded >= levels
            || selected == excluded
        {
            return None;
        }
        Some(Self {
            threshold,
            selected,
            excluded,
        })
    }

    fn decide(self, answers: &[Value]) -> Decision {
        if answers.is_empty() {
            return Decision::Unknown;
        }
        let mut selected = false;
        let mut all_excluded = true;
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
            selected |= direct > self.threshold;
            all_excluded &= irrelevant > self.threshold;
        }
        if selected {
            Decision::Selected
        } else if all_excluded {
            Decision::Excluded
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
        error_code: error.code.parse().ok()?,
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

impl SelectionJudge for Evaluator {
    fn identity(&self) -> Value {
        json!({
            "partition":self.partition().identity(), "model":self.model,
            "template":template(),
            "sourceCoordinates":1,
        })
    }

    fn project(&self, state: Value) -> Value {
        candidate_state(state)
    }

    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        if !self.is_configured() {
            return Err("classificationUnavailable".into());
        }
        let policy =
            Policy::from_contract().ok_or(JudgeFailure::from("invalidClassificationPolicy"))?;
        let questions = call
            .classifiers
            .iter()
            .map(|classifier| compile(classifier, call.main_goal, call.reasoning))
            .collect::<Option<Vec<_>>>()
            .ok_or(JudgeFailure::from("invalidClassificationRequest"))?;
        let deadline = (Instant::now() + call.limits.timeout).min(call.context.deadline);
        let budget = self.budget(deadline, call.context.cancellation.clone());
        let gate = self.gate();
        let outcomes = call.handle.block_on(async {
            stream::iter(call.admitted.iter().copied())
                .map(|ordinal| {
                    let state = call.candidates[ordinal].state.as_ref();
                    let (budget, gate, questions) = (&budget, &gate, &questions);
                    async move {
                        let Some(state) = state else {
                            return (ordinal, Err(ClassificationError::new(
                                "candidateIneligible", "The candidate has no safely prepared state.",
                                "Use the explicit unfiltered continuation to inspect this evidence.",
                            )));
                        };
                        let compact = provider_state(state);
                        let judged = self.evaluate_prepared(EvaluationInput {
                            state: &compact, questions, assets:None, long_context:call.long_context,
                        }, None, budget, gate).await;
                        (ordinal, judged)
                    }
                })
                .buffer_unordered(crate::providers::classification::gate::call_cap(self.max_concurrency))
                .collect::<Vec<_>>().await
        });
        let mut usage = ClassificationUsage::default();
        let mut decisions = Vec::with_capacity(outcomes.len());
        // The first provider failure in source order, reported when nothing
        // answered. A budget stop (the selection's own deadline) is not the
        // provider's answer: those candidates stay unknown. A paused
        // provider (the circuit opened by the failures of this very call) is
        // their consequence, so a failure that caused it comes first.
        let mut first_failure: Option<((bool, usize), ClassificationError)> = None;
        for (ordinal, outcome) in outcomes {
            usage.add(&outcome);
            let decision = match outcome {
                Ok(judged) if judged.answers.len() == questions.len() => {
                    policy.decide(&judged.answers)
                }
                Ok(_) => Decision::Unknown,
                Err(error) => {
                    let failure = selection_failure(ordinal, &error).ok_or_else(|| {
                        JudgeFailure::Provider(ProviderError {
                            code: "invalidClassificationResponse".into(),
                            message:
                                "Provider failure violates the canonical selection error contract."
                                    .into(),
                            hints: vec!["Inspect the provider response and retry.".into()],
                        })
                    })?;
                    call.failures.push(failure);
                    let rank = (error.paused, ordinal);
                    if !matches!(error.code.as_str(), "timeout" | "cancelled")
                        && first_failure
                            .as_ref()
                            .is_none_or(|(first, _)| rank < *first)
                    {
                        first_failure = Some((rank, error));
                    }
                    continue;
                }
            };
            decisions.push((ordinal, decision));
        }
        self.record(usage.clone());
        *call.usage = usage;
        // A provider that failed every candidate gave no answer at all: its
        // typed error (credential, quota, transport) is the row's outcome,
        // never an "unknown" judgment that looks like an honest unsure model.
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
mod tests {
    use super::*;

    #[test]
    fn majority_policy_uses_named_probability_mass_and_any_all_question_semantics() {
        let policy = Policy::from_contract().expect("core selection policy");
        let answer = |zero, one, two| {
            json!({"type":"score",
            "probabilities":{"0":zero,"1":one,"2":two}})
        };
        let direct = answer(0.1, 0.2, 0.7);
        let absent = answer(0.8, 0.1, 0.1);
        let background = answer(0.1, 0.8, 0.1);
        let tied = answer(0.5, 0.0, 0.5);

        assert_eq!(
            policy.decide(std::slice::from_ref(&direct)),
            Decision::Selected
        );
        assert_eq!(
            policy.decide(std::slice::from_ref(&absent)),
            Decision::Excluded
        );
        assert_eq!(
            policy.decide(std::slice::from_ref(&background)),
            Decision::Unknown
        );
        assert_eq!(policy.decide(&[tied]), Decision::Unknown);
        assert_eq!(policy.decide(&[absent.clone(), direct]), Decision::Selected);
        assert_eq!(
            policy.decide(&[absent.clone(), absent.clone()]),
            Decision::Excluded
        );
        assert_eq!(policy.decide(&[absent, background]), Decision::Unknown);
        assert_eq!(policy.decide(&[]), Decision::Unknown);
        assert_eq!(
            policy.decide(&[json!({"type":"score","score":2})]),
            Decision::Unknown
        );
    }

    #[test]
    fn optional_research_context_is_in_each_independent_question() {
        for classifier in [
            "Does this name the stopping condition?",
            "Does this show the current default?",
        ] {
            let question = compile(
                classifier,
                Some("Explain retry behavior"),
                Some("Locate termination evidence"),
            )
            .expect("contextual question");
            assert_eq!(
                question["instructions"]["mainGoal"],
                "Explain retry behavior"
            );
            assert_eq!(
                question["instructions"]["reasoning"],
                "Locate termination evidence"
            );
        }
        let ordinary = compile("Does this name a limit?", None, None).expect("question");
        assert!(ordinary["instructions"].get("mainGoal").is_none());
        assert!(ordinary["instructions"].get("reasoning").is_none());
    }

    #[test]
    fn evaluator_receives_context_and_long_context_and_keeps_success_after_failure() {
        use crate::execution::ExecutionContext;
        use crate::execution::SelectionLimits;
        use crate::tools::selection::Candidate;
        use std::time::Duration;
        use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let server = executor.block_on(MockServer::start());
        executor.block_on(async {
            Mock::given(method("POST"))
                .respond_with(|request: &Request| {
                    let body: Value = request.body_json().expect("request");
                    if body["state"]["text"] == "failed candidate" {
                        return ResponseTemplate::new(401);
                    }
                    let answers = body["questions"]
                        .as_object()
                        .expect("questions")
                        .iter()
                        .map(|(id, question)| {
                            let legend = question["criteria"]
                                .as_array()
                                .expect("criteria")
                                .iter()
                                .enumerate()
                                .map(|(i, criterion)| (i.to_string(), criterion.clone()))
                                .collect::<serde_json::Map<String, Value>>();
                            (
                                id.clone(),
                                json!({"type":"score","score":2.0,"confidence":1.0,
                        "legend":legend,"probabilities":{"0":0.0,"1":0.0,"2":1.0}}),
                            )
                        })
                        .collect::<serde_json::Map<String, Value>>();
                    ResponseTemplate::new(200).set_body_json(json!({
                        "model":"jev-resolved","answers":answers,
                        "usage":{"input_tokens":10,"output_tokens":2}
                    }))
                })
                .mount(&server)
                .await;
        });
        let evaluator = Evaluator::new(
            Some(secrecy::SecretString::from("test-key".to_owned())),
            server.uri(),
            "jev-test".into(),
            &crate::providers::classification::jev::JEV,
            Duration::from_secs(10),
            0,
            4,
            None,
            None,
        );
        let candidates = ["failed candidate", "selected candidate"]
            .map(|text| Candidate::new(Some(json!({"text":text,"sourceLines":[4,5,0,9]}))));
        let classifiers = vec!["Does this describe termination?".to_owned()];
        let context = ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: Instant::now() + Duration::from_secs(10),
            walk_threads: None,
            response_window: None,
            github_credential: None,
        };
        let run = |main_goal| {
            let mut usage = ClassificationUsage::default();
            let mut failures = Vec::new();
            let decisions = evaluator
                .select(JudgeCall {
                    failures: &mut failures,
                    candidates: &candidates,
                    admitted: &[0, 1],
                    classifiers: &classifiers,
                    main_goal: Some(main_goal),
                    reasoning: Some("Find the stopping rule"),
                    long_context: true,
                    limits: SelectionLimits {
                        max_candidates: 2,
                        max_chars: 10_000,
                        timeout: Duration::from_secs(10),
                    },
                    handle: executor.handle(),
                    context: &context,
                    usage: &mut usage,
                })
                .expect("independent candidate outcomes");
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].candidate, 0);
            assert_eq!(
                failures[0].error_code.to_string(),
                "classificationProviderError"
            );
            assert!(!failures[0].error.is_empty());
            assert!(
                failures[0]
                    .hints
                    .as_ref()
                    .is_some_and(|hints| !hints.is_empty())
            );
            assert_eq!(usage.calls, 2);
            assert_eq!(usage.known_usage_calls, 1);
            assert_eq!(usage.unknown_usage_calls, 1);
            assert_eq!((usage.input_tokens, usage.output_tokens), (10, 2));
            assert_eq!(
                usage.models.into_iter().collect::<Vec<_>>(),
                ["jev-resolved"]
            );
            decisions
        };
        let mut decisions = run("Explain retry behavior");
        decisions.sort_by_key(|(ordinal, _)| *ordinal);
        assert_eq!(
            decisions,
            [(1, Decision::Selected)],
            "a failed candidate stays undecided (unknown) beside a judged one"
        );
        let initial = executor
            .block_on(server.received_requests())
            .expect("requests");
        assert_eq!(initial.len(), 2);
        for request in initial {
            assert_eq!(
                request
                    .headers
                    .get("typesafe-danger-zone-long-context")
                    .and_then(|v| v.to_str().ok()),
                Some("true")
            );
            let body: Value = request.body_json().expect("request");
            assert!(body["state"].get("sourceLines").is_none());
            assert_eq!(
                body["state"]["sourceLineRanges"],
                json!([[4, 5], [0, 0], [9, 9]])
            );
            for question in body["questions"].as_object().expect("questions").values() {
                assert_eq!(
                    question["instructions"]["mainGoal"],
                    "Explain retry behavior"
                );
                assert_eq!(
                    question["instructions"]["reasoning"],
                    "Find the stopping rule"
                );
            }
        }
        run("Explain current defaults");
        assert_eq!(
            executor
                .block_on(server.received_requests())
                .expect("requests")
                .len(),
            4,
            "changing research context must not reuse the previous successful judgment"
        );
    }

    #[test]
    fn a_provider_failing_every_candidate_is_a_typed_error_not_unknown() {
        use crate::execution::{ExecutionContext, SelectionLimits};
        use crate::tools::selection::Candidate;
        use std::time::Duration;
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let server = executor.block_on(MockServer::start());
        executor.block_on(
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(401))
                .mount(&server),
        );
        let evaluator = Evaluator::new(
            Some(secrecy::SecretString::from("bad-key".to_owned())),
            server.uri(),
            "jev-test".into(),
            &crate::providers::classification::jev::JEV,
            Duration::from_secs(10),
            0,
            4,
            None,
            None,
        );
        let candidates = ["first", "second"].map(|text| Candidate::new(Some(json!({"text":text}))));
        let context = ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: Instant::now() + Duration::from_secs(10),
            walk_threads: None,
            response_window: None,
            github_credential: None,
        };
        let failure = evaluator
            .select(JudgeCall {
                failures: &mut Vec::new(),
                candidates: &candidates,
                admitted: &[0, 1],
                classifiers: &["Does this describe termination?".to_owned()],
                main_goal: None,
                reasoning: None,
                long_context: false,
                limits: SelectionLimits {
                    max_candidates: 2,
                    max_chars: 10_000,
                    timeout: Duration::from_secs(10),
                },
                handle: executor.handle(),
                context: &context,
                usage: &mut ClassificationUsage::default(),
            })
            .expect_err("no candidate was judged");
        let JudgeFailure::Provider(error) = failure else {
            panic!("a provider failure, not an unavailable judge: {failure:?}");
        };
        assert_eq!(error.code, "classificationProviderError");
        assert!(error.message.contains("401"), "{}", error.message);
        assert!(
            error
                .hints
                .iter()
                .any(|hint| hint.contains("OCTOCODE_CLASSIFICATION_API")),
            "credential hint: {:?}",
            error.hints
        );
    }

    /// A host that refuses connections fails every candidate; the circuit
    /// it opens within the same call is the consequence, so the row names
    /// the cause (no connection), not "failed repeatedly".
    #[test]
    fn a_refused_connection_is_named_not_the_circuit_it_opens() {
        use crate::execution::{ExecutionContext, SelectionLimits};
        use crate::tools::selection::Candidate;
        use std::time::Duration;

        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .expect("free port")
            .port();
        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let evaluator = Evaluator::new(
            Some(secrecy::SecretString::from("key".to_owned())),
            format!("http://127.0.0.1:{port}"),
            "jev-test".into(),
            &crate::providers::classification::jev::JEV,
            Duration::from_secs(10),
            0,
            4,
            None,
            None,
        );
        let candidates: Vec<_> = (0..12)
            .map(|n| Candidate::new(Some(json!({"text":format!("candidate {n}")}))))
            .collect();
        let admitted: Vec<usize> = (0..candidates.len()).collect();
        let context = ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: Instant::now() + Duration::from_secs(20),
            walk_threads: None,
            response_window: None,
            github_credential: None,
        };
        let failure = evaluator
            .select(JudgeCall {
                failures: &mut Vec::new(),
                candidates: &candidates,
                admitted: &admitted,
                classifiers: &["Does this describe termination?".to_owned()],
                main_goal: None,
                reasoning: None,
                long_context: false,
                limits: SelectionLimits {
                    max_candidates: candidates.len(),
                    max_chars: 100_000,
                    timeout: Duration::from_secs(20),
                },
                handle: executor.handle(),
                context: &context,
                usage: &mut ClassificationUsage::default(),
            })
            .expect_err("nothing was judged");
        let JudgeFailure::Provider(error) = failure else {
            panic!("a provider failure: {failure:?}");
        };
        assert_eq!(
            error.code,
            crate::providers::classification::CIRCUIT_OPEN,
            "the provider is unavailable: {error:?}"
        );
        assert!(error.message.contains("could not connect"), "{error:?}");
    }

    #[test]
    fn context_selection_routing_is_not_sent_as_source_evidence() {
        let candidate = json!({"evidence":{
            "path":"retry.rs",
            "selection":{"status":"partial","selectedItems":1,"unknownItems":3},
            "matches":[{"line":12,"value":"stop after three attempts"}]
        }});

        assert_eq!(
            candidate_state(candidate),
            json!({"evidence":{
                "path":"retry.rs",
                "matches":[{"line":12,"value":"stop after three attempts"}]
            }})
        );
    }

    #[test]
    fn provider_coordinates_round_trip_without_changing_admission_evidence() {
        let lines = vec![0, 1, 2, 2, 3, 0, 0, 7, 8, 4, u64::MAX];
        let original = json!({"path":"src/λ.rs","sourceLines":lines,"content":"α\nβ\n…\n"});
        let compact = provider_state(&original);
        let expanded = compact["sourceLineRanges"]
            .as_array()
            .expect("runs")
            .iter()
            .flat_map(|pair| pair[0].as_u64().expect("start")..=pair[1].as_u64().expect("end"))
            .collect::<Vec<_>>();
        assert_eq!(expanded, lines);
        assert_eq!(compact["content"], original["content"]);
        assert_eq!(compact["path"], original["path"]);
        assert_eq!(original["sourceLines"], json!(lines));
        for state in [
            json!({"content":"source"}),
            json!({"sourceLines":["invalid"]}),
        ] {
            assert_eq!(provider_state(&state).as_ref(), &state);
        }
        let long = json!({"sourceLines":(1000..2000).collect::<Vec<_>>(),"content":"source"});
        assert!(provider_state(&long).to_string().len() < long.to_string().len());
    }

    #[test]
    fn a_classifier_fills_the_contract_template_slot() {
        let template = template().expect("contract template");
        let question = compile("When do retries stop?", None, None).expect("question");
        assert_eq!(question["type"], "score");
        assert_eq!(
            question["instructions"][template["slot"].as_str().expect("slot")],
            "When do retries stop?",
            "the template, not native, frames the predicate"
        );
        assert_eq!(question["criteria"], template["question"]["criteria"]);
        assert_eq!(
            question["instructions"]["question"],
            template["question"]["instructions"]["question"]
        );
    }

    #[test]
    fn a_candidate_exposes_only_returned_evidence_to_the_judge() {
        let public = json!({
            "evidence": {
                "path":"a.rs",
                "matches":[{"value":"needle","matchIndices":[{"line":1}],
                    "next":{"read":{"query":{"selection":"secret-plan"}}}}],
                "pagination":{"hasMore":true},
                "hints":{"text":["follow"]},
                "diagnostics":{"requestId":"private"},
                "snapshot":"opaque"
            }
        });
        assert_eq!(
            candidate_state(public),
            json!({"evidence":{"path":"a.rs","matches":[{"value":"needle"}]}})
        );
    }
}
