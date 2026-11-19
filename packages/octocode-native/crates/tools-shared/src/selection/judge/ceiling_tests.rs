//! A units call's stated price against what it sends (mocked provider, no
//! billed call): the first round is what a call sends when nothing is
//! refused or missing, and no run sends more than the stated ceiling,
//! whatever the provider refuses (within its windows) or leaves unanswered.
use super::units_tests::{Judge, Provider, call, calls, hits, question, units, values};
use crate::judgment::Evaluator;
use octocode_runtime_base::providers::classification::{
    ClassificationProvider, ProviderContractError, ProviderDefaults, ProviderResponse, jev::JEV,
};
use serde_json::{Value, json};
use std::time::Duration;

/// Jev's wire with windows small enough that a packed request of a few
/// hits is over them: a request the window may refuse.
struct Narrow;
static NARROW: Narrow = Narrow;
static NARROW_DEFAULTS: ProviderDefaults = ProviderDefaults {
    host: "https://api.typesafe.ai",
    model: "jev-latest",
    endpoint_path: "v1/systemone",
    models_path: "v1/models",
    context_tokens: 700,
    request_tokens: 1_400,
    long_context_tokens: 2_800,
};

impl ClassificationProvider for Narrow {
    fn defaults(&self) -> &'static ProviderDefaults {
        &NARROW_DEFAULTS
    }

    fn headers(&self, images: bool, long_context: bool) -> reqwest::header::HeaderMap {
        JEV.headers(images, long_context)
    }

    fn build(
        &self,
        state: &Value,
        questions: &[&Value],
        model: &str,
        assets: Option<&Value>,
    ) -> Value {
        JEV.build(state, questions, model, assets)
    }

    fn read(&self, request: &Value, response: &Value) -> ProviderResponse {
        JEV.read(request, response)
    }

    fn models(&self, response: &Value) -> Result<Value, ProviderContractError> {
        JEV.models(response)
    }
}

fn narrow(base_url: String, model: &str) -> Evaluator {
    Evaluator::new(
        Some(secrecy::SecretString::from(
            crate::judgment::test_identity::unique("narrow"),
        )),
        base_url,
        model.into(),
        &NARROW,
        Some(Duration::from_secs(10)),
        0,
        4,
    )
}

/// A judge refusing what a token-per-byte window could not hold: the most
/// any real window refuses.
fn windowed() -> Judge {
    Judge {
        refuse_bytes: Some((1_400, 2_800)),
        ..Judge::new()
    }
}

/// flowD1's shape at Jev's windows: three packed requests, each one the
/// window can never refuse, so the ceiling is the first round and one
/// re-ask each. A clean run sends the first round; a run whose responses
/// leave answers missing sends the re-asks and stays under the ceiling.
#[test]
fn a_packed_request_the_window_cannot_refuse_is_priced_at_one_re_ask() {
    let marked = [1, 9, 30, 62];
    let states = hits(
        &[
            ("docs/ref/extensions/handoff_prompt.md", 3),
            ("docs/ref/extensions/handoff_filters.md", 2),
            ("docs/handoffs.md", 58),
        ],
        &marked,
    );
    let questions = question();
    let units = units(&states);
    let clean = Provider::start(Judge::new());
    let plan = clean.evaluator.plan_units_at(&call(&units, &questions), 1);
    assert_eq!((plan.requests, plan.requests_at_most), (3, 6));
    let judged = clean.judge(&call(&units, &questions), 1);
    assert_eq!(
        calls(&judged),
        3,
        "nothing refused or missing: the first round"
    );

    // Two hits of the big file come back unanswered on the first ask.
    let mut lost = states.clone();
    for at in [20, 40] {
        lost[at]["target"]["text"] = json!(format!("handoff LOST entry {at}"));
    }
    let units = super::units_tests::units(&lost);
    let flaky = Provider::start(Judge {
        once: true,
        ..Judge::new()
    });
    let plan = flaky.evaluator.plan_units_at(&call(&units, &questions), 1);
    let judged = flaky.judge(&call(&units, &questions), 1);
    assert!(
        calls(&judged) > 3 && calls(&judged) <= plan.requests_at_most,
        "{} of {}",
        calls(&judged),
        plan.requests_at_most
    );
    assert!(values(&judged).iter().all(Option::is_some));
}

/// Once the endpoint's answers taught a sparse density, a file's hits pack
/// past what a token per byte holds: the window may refuse that request, so
/// its ceiling is a full split down to lone units plus their re-asks. Every
/// run, refusing at the windows and dropping answers, stays under it.
#[test]
fn a_packed_request_the_window_may_refuse_is_priced_at_its_full_split() {
    let questions = question();
    // One lone hit answered first teaches the endpoint its density.
    let probe = hits(&[("probe.md", 1)], &[]);
    let probe_units = units(&probe);
    for (judge, lose) in [
        (windowed(), false),
        (
            Judge {
                once: true,
                ..windowed()
            },
            true,
        ),
    ] {
        let provider = Provider::start_with(judge, narrow);
        provider.judge(&call(&probe_units, &questions), 1);
        let mut states = hits(&[("big.md", 12)], &[3, 8]);
        if lose {
            states[5]["target"]["text"] = json!("handoff LOST entry 5");
        }
        let units = units(&states);
        let plan = provider
            .evaluator
            .plan_units_at(&call(&units, &questions), 1);
        let before = provider.bodies().len();
        let judged = provider.judge(&call(&units, &questions), 1);
        let sent = provider.bodies().len() - before;
        assert_eq!(
            plan.requests, 1,
            "the taught density packs the file in one request"
        );
        assert!(
            plan.requests_at_most >= 12,
            "a refusable request is priced past its lone units: {}",
            plan.requests_at_most
        );
        assert!(sent > 1, "the window refused the packed request");
        assert!(
            u64::try_from(sent).expect("count") <= plan.requests_at_most,
            "{sent} sent, ceiling {}",
            plan.requests_at_most
        );
        assert_eq!(calls(&judged), u64::try_from(sent).expect("count"));
        let kept = values(&judged)
            .iter()
            .enumerate()
            .filter(|(_, answers)| {
                answers
                    .as_ref()
                    .is_some_and(|answers| answers[0]["noul"] == json!(0.94))
            })
            .map(|(at, _)| at)
            .collect::<Vec<_>>();
        assert_eq!(kept, [3, 8]);
    }
}

/// A lone unit the default window may refuse and the long one cannot is
/// priced at both, and a refusal there sends exactly those two.
#[test]
fn a_lone_unit_is_priced_at_its_long_window_retry() {
    let questions = question();
    let states = vec![
        json!({"path":"big.md","target":{"line":2,"text":format!("KEEP {}", "x".repeat(1_600))}}),
    ];
    let units = units(&states);
    let provider = Provider::start_with(windowed(), narrow);
    let plan = provider
        .evaluator
        .plan_units_at(&call(&units, &questions), 1);
    assert_eq!((plan.requests, plan.requests_at_most), (1, 2));
    let judged = provider.judge(&call(&units, &questions), 1);
    assert_eq!(
        calls(&judged),
        2,
        "refused, then answered in the long window"
    );
    assert_eq!(
        values(&judged)[0],
        Some(vec![json!({"type":"noul","noul":0.94})])
    );
}

/// A row held to its ceiling shares its transient retries: the first
/// failing request takes the one retry, the second fails at once, and every
/// unit a transient failure left unanswered says so, while the units that
/// answered keep their answers. The row never sends past its ceiling.
#[test]
fn spent_shared_retries_leave_the_rest_unanswered_and_transient() {
    let provider = crate::selection::transient_fake::Transient::start();
    // One transient retry per request, shared by the row.
    let evaluator = provider.evaluator(1);
    let states = [
        json!({"text":"a fine entry"}),
        json!({"text":"b FAIL entry"}),
        json!({"text":"c FAIL entry"}),
    ];
    let units = states
        .iter()
        .map(|state| {
            Some(super::Unit {
                state,
                alone: state.clone(),
            })
        })
        .collect::<Vec<_>>();
    let questions = question();
    let plan = evaluator.plan_units(&call(&units, &questions));
    assert_eq!(plan.requests_at_most, 3, "each whole unit is one request");
    let room = evaluator.retry_room();
    assert_eq!(room, 1);
    let ceiling = crate::judgment::RequestCap::ceiling(plan.requests_at_most, room);
    let cap = crate::judgment::RequestCap::held(plan.requests_at_most, room);
    let held = super::UnitCall {
        cap: Some(&cap),
        ..call(&units, &questions)
    };
    let gate = evaluator.gate();
    let context = super::provider_tests::context(Duration::from_secs(60));
    let judged = provider.executor.block_on(evaluator.judge_units(
        &held,
        &crate::judgment::shared::Evaluations::default(),
        &gate,
        &context,
    ));
    let sent = provider.bodies().len();
    assert_eq!(
        u64::try_from(sent).expect("count"),
        ceiling,
        "three planned requests and the one shared retry"
    );
    assert_eq!(calls(&judged), ceiling);
    assert_eq!(
        values(&judged)[0],
        Some(vec![json!({"type":"noul","noul":0.9})]),
        "an answered unit keeps its answer"
    );
    let failures = judged.answers[1..]
        .iter()
        .map(|answers| {
            let answers = answers.as_ref().expect("judged");
            answers[0]
                .as_ref()
                .expect_err("a transient failure")
                .clone()
        })
        .collect::<Vec<_>>();
    for error in &failures {
        assert_eq!(
            octocode_runtime_base::id::error_codes::class(error.code),
            octocode_runtime_base::id::error_codes::ErrorClass::Transient,
            "{error:?}"
        );
    }
    let spent = failures
        .iter()
        .filter(|error| {
            error
                .hints
                .iter()
                .any(|hint| hint == crate::judgment::cap::RETRIES_SPENT)
        })
        .count();
    assert_eq!(
        spent, 1,
        "the request the room left without its retry says so"
    );
    assert_eq!(cap.left(), 0, "the ceiling is spent");
}
