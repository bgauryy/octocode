//! Whole units of a page (rows, paths, entries, chunks) share provider
//! requests: one state listing the units by id, one question per unit and
//! predicate naming its unit. A counting provider checks the requests sent,
//! the per-unit answers read back, and the windows the groups close at.
use super::provider_tests::{context, evaluator_of};
use super::requests::GROUP_UNITS;

/// The group size the batching tests judge at: the shipped size is one unit
/// per request, so these name theirs.
const GROUP: usize = 4;
use super::*;
use crate::selection::Candidate;
use octocode_runtime_base::providers::classification::ClassificationProvider as _;
use octocode_test_support::json_fixture;
use std::time::Duration;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

/// What the provider received for one judged page.
struct Judged {
    decisions: Vec<(usize, Decision)>,
    failures: Vec<octocode_contract_types::SelectionFailure>,
    usage: ClassificationUsage,
    bodies: Vec<Value>,
}

/// A provider that answers each question about the unit its label names:
/// direct evidence when the unit's own text holds `MARK`, absence otherwise.
/// A question about `omit` (a unit id) gets no answer.
fn by_unit(omit: Option<usize>) -> impl Fn(&Request) -> ResponseTemplate {
    move |request| {
        let body: Value = request.body_json().expect("request");
        let state = &body["state"];
        let answers = body["questions"]
            .as_object()
            .expect("questions")
            .iter()
            .filter_map(|(id, question)| {
                let label = question["instructions"]["target"].as_str();
                let unit = label.and_then(|label| {
                    label
                        .strip_prefix("unit ")?
                        .split(|c: char| !c.is_ascii_digit())
                        .next()?
                        .parse::<usize>()
                        .ok()
                });
                if unit.is_some() && unit == omit {
                    return None;
                }
                let text = match unit {
                    Some(unit) => state["units"]
                        .as_str()
                        .expect("a unit question needs the units state")
                        .lines()
                        .find(|line| line.split('\t').next() == Some(&format!("unit {unit}")))
                        .expect("the unit's line")
                        .to_owned(),
                    None => state.to_string(),
                };
                let (score, probabilities) = if text.contains("MARK") {
                    (2.0, json_fixture(r#"{"0": 0.0, "1": 0.0, "2": 1.0}"#))
                } else {
                    (0.0, json_fixture(r#"{"0": 1.0, "1": 0.0, "2": 0.0}"#))
                };
                let legend = question["criteria"]
                    .as_array()
                    .expect("criteria")
                    .iter()
                    .enumerate()
                    .map(|(i, criterion)| (i.to_string(), criterion.clone()))
                    .collect::<serde_json::Map<String, Value>>();
                Some((
                    id.clone(),
                    json!({"type":"score","score":score,"confidence":1.0,
                    "legend":legend,"probabilities":probabilities}),
                ))
            })
            .collect::<serde_json::Map<String, Value>>();
        ResponseTemplate::new(200).set_body_json(json!({
            "model":"jev-resolved","answers":answers,
            "usage":{"input_tokens":10,"output_tokens":2}
        }))
    }
}

fn judge(
    states: &[Value],
    respond: impl Fn(&Request) -> ResponseTemplate + Send + Sync + 'static,
    long_context: crate::judgment::LongContext,
) -> Judged {
    judge_at(states, respond, long_context, GROUP)
}

fn judge_at(
    states: &[Value],
    respond: impl Fn(&Request) -> ResponseTemplate + Send + Sync + 'static,
    long_context: crate::judgment::LongContext,
    group_units: usize,
) -> Judged {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let server = executor.block_on(MockServer::start());
    executor.block_on(
        Mock::given(method("POST"))
            .respond_with(respond)
            .mount(&server),
    );
    static MODELS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let model = format!(
        "unit-group-{}",
        MODELS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let evaluator = evaluator_of("test-key", server.uri(), &model);
    let candidates = states
        .iter()
        .map(|state| Candidate::new(Some(state.clone())))
        .collect::<Vec<_>>();
    let admitted = (0..candidates.len()).collect::<Vec<_>>();
    let context = context(Duration::from_secs(10));
    let (mut usage, mut failures) = (ClassificationUsage::default(), Vec::new());
    let decisions = evaluator.select_grouped(
        JudgeCall {
            failures: &mut failures,
            candidates: &candidates,
            admitted: &admitted,
            predicates: &["Does this concern authentication?".to_owned()],
            research_context: None,
            long_context,
            handle: executor.handle(),
            context: &context,
            usage: &mut usage,
        },
        group_units,
    );
    let mut decisions = decisions.unwrap_or_default();
    decisions.sort_by_key(|(ordinal, _)| *ordinal);
    let mut bodies: Vec<Value> = executor
        .block_on(server.received_requests())
        .expect("requests")
        .iter()
        .map(|request| request.body_json().expect("request"))
        .collect();
    // Groups are sent concurrently, so arrival order is scheduling. Order
    // them by the first candidate each holds, which is the order they were
    // cut in: its line in the shared state is `unit 1`, a tab, its compact
    // state. A request of no units keeps its arrival place after them.
    let first_candidate = |body: &Value| {
        let units = body["state"]["units"].as_str()?;
        let (_, first) = units.lines().next()?.split_once('\t')?;
        states
            .iter()
            .position(|state| provider_state(state).to_string() == first)
    };
    bodies.sort_by_key(|body| first_candidate(body).unwrap_or(usize::MAX));
    Judged {
        decisions,
        failures,
        usage,
        bodies,
    }
}

const NEVER: crate::judgment::LongContext = crate::judgment::LongContext::Never;

fn selected(judged: &Judged) -> Vec<usize> {
    judged
        .decisions
        .iter()
        .filter(|(_, decision)| *decision == Decision::Selected)
        .map(|(ordinal, _)| *ordinal)
        .collect()
}

fn paths(count: usize, marked: &[usize]) -> Vec<Value> {
    (0..count)
        .map(|n| {
            let mark = if marked.contains(&n) { "MARK/" } else { "" };
            json!({"path": format!("src/{mark}file{n}.rs")})
        })
        .collect()
}

/// Thirteen listed paths are a request per `GROUP` units, not thirteen;
/// each unit is answered on its own, named by the id its line opens with.
#[test]
fn thirteen_path_units_share_requests_with_one_answer_each() {
    let judged = judge(&paths(13, &[2, 6]), by_unit(None), NEVER);
    assert_eq!(judged.bodies.len(), 13usize.div_ceil(GROUP));
    assert_eq!(judged.usage.calls as usize, judged.bodies.len());
    let held = judged.bodies[0]["questions"]
        .as_object()
        .expect("questions")
        .len();
    assert_eq!(held, 13.min(GROUP));
    if GROUP > 1 {
        let units = judged.bodies[0]["state"]["units"].as_str().expect("units");
        assert_eq!(units.lines().count(), held);
        assert!(
            units.starts_with("unit 1\t{\"path\":\"src/file0.rs\"}\nunit 2\t"),
            "{units}"
        );
        assert!(judged.bodies[0]["state"]["question"].is_string());
    }
    assert_eq!(judged.decisions.len(), 13, "every unit has its own answer");
    assert_eq!(selected(&judged), [2, 6]);
    assert!(judged.failures.is_empty());
    // Each question quotes the unit it is about beside its id, and that id is
    // the one its line in the state opens with.
    let labels = judged.bodies[2 / GROUP]["questions"]
        .as_object()
        .expect("questions")
        .values()
        .map(|question| question["instructions"]["target"].as_str().expect("label"))
        .collect::<Vec<_>>();
    assert!(
        labels.contains(&format!("unit {}: src/MARK/file2.rs", 2 % GROUP + 1).as_str()),
        "{labels:?}"
    );
}

/// A page of forty units is `ceil(40 / N)` requests at any group size N, and
/// the same units are kept.
#[test]
fn a_forty_unit_page_is_ceil_forty_over_the_group_size_requests() {
    for size in [1, 4, 8] {
        let judged = judge_at(&paths(40, &[7, 31]), by_unit(None), NEVER, size);
        assert_eq!(judged.bodies.len(), 40usize.div_ceil(size), "size {size}");
        assert_eq!(selected(&judged), [7, 31], "size {size}");
        assert_eq!(judged.decisions.len(), 40, "size {size}");
    }
}

/// One unit per request is each unit's own state and question, never a
/// shared state of one.
#[test]
fn a_group_size_of_one_judges_each_unit_on_its_own_state() {
    let judged = judge_at(&paths(3, &[1]), by_unit(None), NEVER, 1);
    assert_eq!(judged.bodies.len(), 3);
    assert!(
        judged
            .bodies
            .iter()
            .all(|body| body["state"].get("units").is_none())
    );
    assert_eq!(selected(&judged), [1]);
}

/// A page of one unit is its own state and question, as it always was.
#[test]
fn a_one_unit_page_is_its_own_state() {
    let judged = judge(&paths(1, &[0]), by_unit(None), NEVER);
    assert_eq!(judged.bodies.len(), 1);
    assert!(judged.bodies[0]["state"].get("units").is_none());
    assert_eq!(judged.bodies[0]["state"]["path"], "src/MARK/file0.rs");
    assert_eq!(selected(&judged), [0]);
}

/// A page over the provider's per-question window is several requests, and
/// no request's state is past the window derived from the provider's stated
/// tokens; a unit over it goes alone.
#[test]
fn a_group_closes_at_the_derived_window_and_an_oversize_unit_goes_alone() {
    let window = usize::try_from(
        octocode_runtime_base::providers::classification::jev::JEV
            .defaults()
            .question_window(false),
    )
    .expect("window");
    // Unseen, the window holds one byte per token.
    let unit = |name: &str, bytes: usize| json!({"path": name, "content": "x".repeat(bytes)});
    let states = [
        unit("a", window / 3),
        unit("b", window / 3),
        unit("c", window / 3),
        unit("huge", window + 1_000),
        unit("d", 100),
    ];
    let judged = judge(&states, by_unit(None), NEVER);
    assert!(judged.bodies.len() >= 2, "{} requests", judged.bodies.len());
    let mut seen = Vec::new();
    for body in &judged.bodies {
        let units = body["state"]["units"].as_str();
        let state_bytes = serde_json::to_vec(&body["state"]).expect("state").len();
        let held = units.map_or(1, |units| units.lines().count());
        if held > 1 {
            assert!(state_bytes <= window, "{state_bytes} over the window");
        }
        let text = body["state"].to_string();
        seen.push((held, text.contains("huge")));
    }
    assert!(
        seen.iter().any(|(held, _)| *held > 1),
        "units below the window share a request: {seen:?}"
    );
    assert!(
        seen.contains(&(1, true)),
        "the oversize unit is alone: {seen:?}"
    );
    assert_eq!(judged.decisions.len(), states.len(), "nothing is lost");
}

/// One question that comes back without an answer leaves only its unit
/// unknown.
#[test]
fn one_unanswered_question_leaves_only_its_unit_unknown() {
    let judged = judge(&paths(4, &[0, 2]), by_unit(Some(2)), NEVER);
    assert_eq!(judged.bodies.len(), 4usize.div_ceil(GROUP));
    assert_eq!(
        judged
            .failures
            .iter()
            .map(|failure| failure.candidate)
            .collect::<Vec<_>>(),
        [1],
        "only unit 2 (candidate 1) failed"
    );
    assert_eq!(
        judged.decisions,
        [
            (0, Decision::Selected),
            (2, Decision::Selected),
            (3, Decision::Excluded)
        ]
    );
}

/// A provider that cannot answer fails every unit of the request, each one
/// retryable by a later page.
#[test]
fn a_transient_provider_failure_leaves_every_unit_retryable() {
    let judged = judge(
        &paths(5, &[]),
        |_: &Request| ResponseTemplate::new(503),
        NEVER,
    );
    assert!(judged.decisions.is_empty());
    let mut failed = judged
        .failures
        .iter()
        .map(|failure| failure.candidate)
        .collect::<Vec<_>>();
    failed.sort_unstable();
    assert_eq!(failed, [0, 1, 2, 3, 4]);
    assert!(
        judged.failures.iter().all(|failure| {
            octocode_runtime_base::providers::classification::is_transient(failure.error_code)
        }),
        "{:?}",
        judged.failures
    );
}

/// Identical units are one entry: one question, every candidate reads it.
#[test]
fn identical_units_share_one_entry() {
    let states = [
        json_fixture(r#"{"path": "MARK/same.rs"}"#),
        json_fixture(r#"{"path": "other.rs"}"#),
        json_fixture(r#"{"path": "MARK/same.rs"}"#),
    ];
    let judged = judge(&states, by_unit(None), NEVER);
    assert_eq!(judged.bodies.len(), 1);
    assert_eq!(
        judged.bodies[0]["questions"]
            .as_object()
            .expect("questions")
            .len(),
        2,
        "two distinct units, two questions"
    );
    assert_eq!(
        judged.decisions,
        [
            (0, Decision::Selected),
            (1, Decision::Excluded),
            (2, Decision::Selected)
        ]
    );
}

/// A credential-named path is withheld before it becomes a unit: it is in no
/// state the provider receives, and stays unknown.
#[test]
fn a_credential_path_never_enters_a_state() {
    let data = json_fixture(
        r#"{
            "files": [
                {"path": "config/.env"},
                {
                    "dir": "src",
                    "files": ["a.rs (1)", ".env (2)", "b.rs (3)"]
                }
            ]
        }"#,
    );
    let states = crate::selection::row_units::Rows::of(
        octocode_runtime_base::id::ToolId::StructureSearch,
        &data,
    )
    .states();
    assert!(states.iter().any(Option::is_none), "{states:?}");
    let judged = judge(
        &states.into_iter().flatten().collect::<Vec<_>>(),
        by_unit(None),
        NEVER,
    );
    assert_eq!(judged.bodies.len(), 1);
    let sent = judged.bodies[0].to_string();
    assert!(!sent.contains(".env"), "{sent}");
    assert!(
        sent.contains("src/a.rs") && sent.contains("src/b.rs"),
        "{sent}"
    );
}

/// Twenty-six chunks of a file (about 2 KB each) take the group size or the
/// window, whichever closes a request first.
#[test]
fn twenty_six_chunks_close_at_the_group_size_or_the_window() {
    let states = (0..26)
        .map(|n| {
            let mark = if n % 9 == 4 { "MARK " } else { "" };
            json!({"path":"src/big.rs",
                "sourceLines": (n * 40 + 1..=n * 40 + 40).collect::<Vec<_>>(),
                "content": format!("{mark}{}", "let value = compute(input);\n".repeat(70))})
        })
        .collect::<Vec<_>>();
    let judged = judge(&states, by_unit(None), NEVER);
    assert!(
        judged.bodies.len() >= 26usize.div_ceil(GROUP),
        "{}",
        judged.bodies.len()
    );
    assert_eq!(judged.decisions.len(), 26);
    assert_eq!(selected(&judged), [4, 13, 22]);
    let labels = judged
        .bodies
        .iter()
        .flat_map(|body| body["questions"].as_object().expect("questions").values())
        .map(|question| question["instructions"]["target"].as_str().expect("label"))
        .collect::<Vec<_>>();
    assert!(
        labels.contains(&"unit 1: src/big.rs lines 1-40"),
        "{labels:?}"
    );
}

/// Ten pull request rows with bodies: a request per group size, each row
/// judged on its own title and body.
#[test]
fn ten_pull_request_rows_with_bodies_share_requests() {
    let states = (0..10)
        .map(|n| {
            let body = format!(
                "{}Refactors the module layout and updates the call sites. {}",
                if n == 3 || n == 8 { "MARK " } else { "" },
                "Details of the change and its tests follow. ".repeat(12)
            );
            json!({"title": format!("Change number {n}"), "body": body})
        })
        .collect::<Vec<_>>();
    let judged = judge(&states, by_unit(None), NEVER);
    assert_eq!(judged.bodies.len(), 10usize.div_ceil(GROUP));
    assert_eq!(selected(&judged), [3, 8]);
    assert_eq!(judged.decisions.len(), 10);
    let labels = judged.bodies[3 / GROUP]["questions"]
        .as_object()
        .expect("questions")
        .values()
        .map(|question| question["instructions"]["target"].as_str().expect("label"))
        .collect::<Vec<_>>();
    assert!(
        labels.contains(&format!("unit {}: Change number 3", 3 % GROUP + 1).as_str()),
        "{labels:?}"
    );
}

/// A plan judged at another group size is bound to another judge, so it is
/// not replayed as if batched at this one.
#[test]
fn the_judge_identity_names_the_group_size() {
    let judge = evaluator_of("test-key", "http://localhost:1".into(), "m");
    assert_eq!(
        judge.identity()["unitGroups"].as_u64(),
        Some(GROUP_UNITS as u64)
    );
}
