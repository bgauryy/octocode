//! Provider batching, single-flight judgment cache, and public answer projection.
use super::ProviderConfig;
use crate::providers::classification::gate::GateLease;
use crate::tools::clasify::locate::{
    LocatedPage, collapse_locate_answer, locate_provider_questions, located_state,
};
use crate::tools::clasify::transport::with_goal;
use crate::tools::clasify::{self, transport::ClassificationError};
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};

/// A page's answers and the usage of the request that produced them.
type PageOutcome = (Vec<Result<Value, ClassificationError>>, Option<Value>);

/// Judgments already made in this process for the exact same provider input;
/// a failed flight hands its answers to the identical pages queued behind it.
static JUDGMENTS: clasify::cache::JudgmentCache<Vec<Result<Value, ClassificationError>>> =
    clasify::cache::JudgmentCache::new();

/// One provider page, answered from [`JUDGMENTS`] when this exact state and
/// question set was judged before (a resumed `next.clasify`, a repeated
/// matrix, or an identical page judged concurrently in the same call). A
/// replay reports no usage because no request was made. Only a fully
/// successful answer set is stored; a failed one is shared only with the
/// identical pages already waiting, so a 429 is not re-sent once per page.
/// The key covers what the provider sees; correlation IDs are never sent, so
/// they do not split it.
async fn assess_provider_page(
    state: &Value,
    questions: &[Value],
    config: &ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
) -> PageOutcome {
    let endpoint = format!("{}/{}", config.base_url, config.endpoint_path);
    let provider_questions = questions
        .iter()
        .map(|question| question["question"].clone())
        .collect::<Vec<_>>();
    let key = clasify::cache::key(&endpoint, config.model, state, &provider_questions);
    let flight = JUDGMENTS.flight(&key);
    let mut turn = flight.turn().await;
    if let Some(failed) = turn.as_ref() {
        return (failed.clone(), None);
    }
    if let Some(answers) = JUDGMENTS.get(&key) {
        return (answers.into_iter().map(Ok).collect(), None);
    }
    let (answers, usage) = request_and_store(state, questions, config, budget, gate, key).await;
    if !answers.iter().all(Result::is_ok) {
        *turn = Some(answers.clone());
    }
    (answers, usage)
}

async fn request_and_store(
    state: &Value,
    questions: &[Value],
    config: &ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
    key: [u8; 32],
) -> PageOutcome {
    let (answers, usage) = request_provider_page(state, questions, config, budget, gate).await;
    if answers.iter().all(Result::is_ok) {
        let stored = answers
            .iter()
            .filter_map(|answer| answer.as_ref().ok().cloned())
            .map(|mut answer| {
                if let Some(fields) = answer.as_object_mut() {
                    fields.remove("usage");
                }
                answer
            })
            .collect();
        JUDGMENTS.put(key, stored);
    }
    (answers, usage)
}

async fn request_provider_page(
    state: &Value,
    questions: &[Value],
    config: &ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
) -> (Vec<Result<Value, ClassificationError>>, Option<Value>) {
    let indexed = questions
        .iter()
        .enumerate()
        .map(|(index, question)| (index, &question["question"]))
        .collect::<Vec<_>>();
    if indexed.len() > 1
        && let Some(prepared) =
            clasify::batch::prepare(state, &indexed, config.model, config.provider)
    {
        let result = clasify::batch::judge(
            prepared,
            &indexed,
            config.key,
            config.base_url,
            config.endpoint_path,
            config.model,
            config.provider,
            budget,
            config.retries,
            gate,
        )
        .await;
        return match result {
            Ok(mut response) => {
                for answer in response.answers.iter_mut().skip(1).flatten() {
                    if let Some(object) = answer.as_object_mut() {
                        object.remove("usage");
                    }
                }
                (response.answers, Some(response.usage))
            }
            Err(error) => {
                let usage = (error.provider_calls > 0)
                    .then(|| json!({"provider_calls":error.provider_calls}));
                (vec![Err(error); questions.len()], usage)
            }
        };
    }

    let assessed = stream::iter(questions.iter())
        .map(|question| {
            clasify::judge(
                state,
                &question["question"],
                config.key.clone(),
                config.base_url,
                config.endpoint_path,
                config.model,
                config.provider,
                budget.clone(),
                config.retries,
                gate,
            )
        })
        .buffered(questions.len().max(1))
        .collect::<Vec<_>>()
        .await;
    let usages = assessed
        .iter()
        .filter_map(|result| match result {
            Ok(data) => Some(data["usage"].clone()),
            Err(error) if error.provider_calls > 0 => {
                Some(json!({"provider_calls":error.provider_calls}))
            }
            Err(_) => None,
        })
        .collect::<Vec<_>>();
    (assessed, Some(json!({"calls":usages})))
}

enum PublicAnswerPlan {
    Direct(usize),
    Locate {
        choice: usize,
        exists: usize,
        page: LocatedPage,
    },
    Failed(ClassificationError),
}

/// Put the caller's goal, next-read reason, and the read that produced the
/// evidence on the state sent to Jev. An evidence object gains the missing
/// sibling keys. A string, array, or object that already uses any of those
/// names is wrapped so a judged field is kept.
pub(super) fn with_briefs(state: Value, reasoning: &str, goal: &str, read: Option<Value>) -> Value {
    let reasoning = reasoning.trim();
    let goal = goal.trim();
    if reasoning.is_empty() && goal.is_empty() && read.is_none() {
        return state;
    }
    let mut briefs = serde_json::Map::new();
    if !reasoning.is_empty() {
        briefs.insert("reasoning".into(), Value::String(reasoning.to_owned()));
    }
    if !goal.is_empty() {
        briefs.insert("goal".into(), Value::String(goal.to_owned()));
    }
    if let Some(read) = read {
        briefs.insert("read".into(), read);
    }
    match state {
        Value::Object(mut map) if briefs.keys().all(|key| !map.contains_key(key)) => {
            map.extend(briefs);
            Value::Object(map)
        }
        other => {
            briefs.insert("evidence".into(), other);
            Value::Object(briefs)
        }
    }
}

pub(super) async fn assess_page(
    state: &Value,
    read: Option<Value>,
    questions: &[Value],
    goal: &str,
    reasoning: &str,
    config: &ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
) -> (Vec<Result<Value, ClassificationError>>, Option<Value>) {
    let has_locate = questions
        .iter()
        .any(|question| clasify::is_locate(&question["question"]));
    let located = has_locate.then(|| located_state(state));
    let (provider_state, page, locate_error) = match located {
        Some(Ok((state, page))) => (state, page, None),
        Some(Err(error)) => (state.clone(), LocatedPage::default(), Some(error)),
        None => (state.clone(), LocatedPage::default(), None),
    };
    let provider_state = with_briefs(provider_state, reasoning, goal, read);
    let mut provider_questions = Vec::new();
    let mut plans = Vec::with_capacity(questions.len());
    for question in questions {
        if clasify::is_locate(&question["question"]) {
            if let Some(error) = &locate_error {
                plans.push(PublicAnswerPlan::Failed(error.clone()));
                continue;
            }
            let target = question["question"]["ask"].as_str().unwrap_or_default();
            let [choice, exists] = locate_provider_questions(target, &page);
            let choice_index = provider_questions.len();
            provider_questions
                .push(json!({"id":question["id"],"question":with_goal(choice, goal)}));
            let exists_index = provider_questions.len();
            provider_questions
                .push(json!({"id":question["id"],"question":with_goal(exists, goal)}));
            plans.push(PublicAnswerPlan::Locate {
                choice: choice_index,
                exists: exists_index,
                page: page.clone(),
            });
        } else {
            let index = provider_questions.len();
            let mut cloned = question.clone();
            if let Some(provider_question) = cloned.get_mut("question") {
                *provider_question = with_goal(provider_question.take(), goal);
            }
            provider_questions.push(cloned);
            plans.push(PublicAnswerPlan::Direct(index));
        }
    }
    let (answers, usage) = if provider_questions.is_empty() {
        (Vec::new(), None)
    } else {
        assess_provider_page(&provider_state, &provider_questions, config, budget, gate).await
    };
    let projected = plans
        .into_iter()
        .map(|plan| match plan {
            PublicAnswerPlan::Direct(index) => answers.get(index).cloned().unwrap_or_else(|| {
                Err(ClassificationError::invalid_response(
                    "Provider answer count did not match the requested questions.",
                ))
            }),
            PublicAnswerPlan::Locate {
                choice,
                exists,
                page,
            } => match (answers.get(choice), answers.get(exists)) {
                (Some(choice), Some(exists)) => collapse_locate_answer(choice, exists, &page),
                _ => Err(ClassificationError::invalid_response(
                    "Provider answer count did not match the locate questions.",
                )),
            },
            PublicAnswerPlan::Failed(error) => Err(error),
        })
        .collect();
    (projected, usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn briefs_reach_the_provider_state_beside_the_evidence() {
        let file = json!({"path":"a.rs","lines":[1,1],"content":"fn a() {}\n"});
        let tagged = with_briefs(
            file,
            "  The next read is the writer.  ",
            "  The function that writes the continuation.  ",
            None,
        );
        assert_eq!(tagged["reasoning"], "The next read is the writer.");
        assert_eq!(tagged["goal"], "The function that writes the continuation.");
        assert_eq!(tagged["path"], "a.rs");
        assert_eq!(tagged["content"], "fn a() {}\n");
        assert_eq!(
            with_briefs(json!("plain"), "why", "what", None),
            json!({"reasoning":"why","goal":"what","evidence":"plain"})
        );
        assert_eq!(
            with_briefs(json!(["a", "b"]), "why", "what", None)["evidence"],
            json!(["a", "b"])
        );
        let existing = with_briefs(
            json!({"goal": "test", "reasoning":"field","content":"x"}),
            "why",
            "what",
            None,
        );
        assert_eq!(existing["reasoning"], "why");
        assert_eq!(existing["goal"], "what");
        assert_eq!(existing["evidence"]["reasoning"], "field");
        assert_eq!(
            with_briefs(json!({"a":1}), "   ", "   ", None),
            json!({"a":1})
        );
    }

    #[tokio::test]
    async fn usage_counts_failed_and_retried_requests_without_inventing_tokens() {
        use secrecy::SecretString;
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        for batched in [false, true] {
            for scenario in ["rejected", "malformed", "known", "unknown", "retried"] {
                let server = MockServer::start().await;
                let attempts = Arc::new(AtomicUsize::new(0));
                let seen = attempts.clone();
                Mock::given(method("POST"))
                    .respond_with(move |request: &wiremock::Request| {
                        let attempt = seen.fetch_add(1, Ordering::SeqCst);
                        if scenario == "rejected" {
                            return ResponseTemplate::new(400);
                        }
                        if scenario == "malformed" {
                            return ResponseTemplate::new(200)
                                .set_body_json(json!({"malformed":true}));
                        }
                        if scenario == "retried" && attempt == 0 {
                            return ResponseTemplate::new(503);
                        }
                        let request: Value = request.body_json().expect("request");
                        let answers = request["questions"]
                            .as_object()
                            .expect("questions")
                            .keys()
                            .map(|key| (key.clone(), json!({"type":"noul","noul":0.9})))
                            .collect::<serde_json::Map<_, _>>();
                        let usage = if scenario == "known" || scenario == "retried" {
                            json!({"input_tokens":10,"output_tokens":3})
                        } else {
                            json!({})
                        };
                        ResponseTemplate::new(200)
                            .set_body_json(json!({"model":"m","answers":answers,"usage":usage}))
                    })
                    .mount(&server)
                    .await;
                let key = SecretString::from("fake-audit-only");
                let base_url = server.uri();
                let config = ProviderConfig {
                    key: &key,
                    base_url: &base_url,
                    endpoint_path: "v1/systemone",
                    model: "m",
                    provider: &crate::providers::classification::jev::JEV,
                    retries: 1,
                    max_concurrency: 1,
                };
                let budget = crate::providers::RequestBudget::with_timeout(
                    std::time::Duration::from_secs(10),
                    1_000_000,
                );
                let gate = crate::providers::classification::gate::lease(
                    &format!("{base_url}/v1/systemone"),
                    1,
                );
                let questions = (0..if batched {2} else {1}).map(|i| json!({"id":format!("q{i}"),"question":{"type":"noul","instructions":"Judge supplied fixture."}})).collect::<Vec<_>>();
                let (_, usage) = assess_provider_page(
                    &json!({"fixture":scenario}),
                    &questions,
                    &config,
                    &budget,
                    &gate,
                )
                .await;
                let records = match usage.expect("attempt receipt") {
                    Value::Object(mut fields)
                        if fields.get("calls").is_some_and(Value::is_array) =>
                    {
                        fields
                            .remove("calls")
                            .expect("calls")
                            .as_array()
                            .expect("records")
                            .clone()
                    }
                    receipt => vec![receipt],
                };
                let calls = records
                    .iter()
                    .map(|r| r.get("provider_calls").and_then(Value::as_u64).unwrap_or(1))
                    .sum::<u64>();
                assert_eq!(
                    calls,
                    attempts.load(Ordering::SeqCst) as u64,
                    "{scenario}, batched={batched}"
                );
                assert_eq!(calls, if scenario == "retried" { 2 } else { 1 });
                if scenario == "rejected" || scenario == "malformed" || scenario == "unknown" {
                    assert!(records.iter().all(
                        |r| r.get("input_tokens").is_none() && r.get("output_tokens").is_none()
                    ));
                } else {
                    assert_eq!(records[0]["input_tokens"], 10);
                    assert_eq!(records[0]["output_tokens"], 3);
                }
            }
        }
    }
}
