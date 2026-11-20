//! Units packed into few requests against a mocked provider (no billed
//! call): the requests sent, each unit's answers read back against the
//! unbatched path, refusals split, missing answers asked again once, spend
//! summed, and the plan priced before sending.
use super::provider_tests::{context, evaluator_of};
use super::*;
use crate::judgment::shared::Evaluations;
use octocode_runtime_base::providers::classification::ClassificationProvider as _;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

/// The text a question is about: a packed question's target label (a hit's
/// quoted line, or a unit's line in the shared state), else the unit's own
/// state (a hit's target text when it names one).
fn judged_text(state: &Value, question: &Value) -> String {
    if let Some(label) = question["instructions"]["target"].as_str() {
        if let Some(rest) = label.strip_prefix("unit ") {
            let id = rest
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap_or_default();
            return state["units"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .find(|line| line.split('\t').next() == Some(&format!("unit {id}")))
                .unwrap_or_default()
                .to_owned();
        }
        return label
            .split_once(": ")
            .map_or(label, |(_, text)| text)
            .to_owned();
    }
    let content = state.get("content").unwrap_or(state);
    content["target"]["text"]
        .as_str()
        .map_or_else(|| content.to_string(), str::to_owned)
}

/// A judge reading only what each question is about: yes (0.94) when that
/// text holds `KEEP`, else no (0.03). A question about text holding `LOST`
/// gets no answer (only on its first ask when `once`). A request with more
/// than `refuse_over` questions is refused as too large.
#[derive(Clone)]
pub(super) struct Judge {
    pub(super) refuse_over: usize,
    pub(super) once: bool,
    pub(super) lost_seen: Arc<AtomicBool>,
    /// Refuse a request whose body is over these bytes (default window,
    /// long window), as a provider whose windows hold a token per byte.
    pub(super) refuse_bytes: Option<(usize, usize)>,
    /// The provider's own prompt, in input tokens per request and per
    /// question, reported beside a third of a token per body byte; `None`:
    /// every request reports 10.
    pub(super) prompt: Option<(u64, u64)>,
}

impl Judge {
    pub(super) fn new() -> Self {
        Self {
            refuse_over: usize::MAX,
            once: false,
            lost_seen: Arc::default(),
            refuse_bytes: None,
            prompt: None,
        }
    }

    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = request.body_json().expect("request");
        let questions = body["questions"].as_object().expect("questions");
        let long = request
            .headers
            .contains_key("typesafe-danger-zone-long-context");
        let over = self
            .refuse_bytes
            .is_some_and(|(default, wide)| request.body.len() > if long { wide } else { default });
        if questions.len() > self.refuse_over || over {
            return ResponseTemplate::new(400)
                .set_body_json(json!({"detail":{"error_type":"max_tokens_exceeded"}}));
        }
        let mut answers = serde_json::Map::new();
        for (id, question) in questions {
            let text = judged_text(&body["state"], question);
            if text.contains("LOST") && !(self.once && self.lost_seen.swap(true, Ordering::SeqCst))
            {
                continue;
            }
            let noul = if text.contains("KEEP") { 0.94 } else { 0.03 };
            answers.insert(id.clone(), json!({"type":"noul","noul":noul}));
        }
        let input_tokens = self.prompt.map_or(10, |(per_request, per_question)| {
            per_request
                + per_question * questions.len() as u64
                + (request.body.len() as u64).div_ceil(3)
        });
        ResponseTemplate::new(200).set_body_json(json!({
            "model":"jev-test","answers":answers,
            "usage":{"input_tokens":input_tokens,"output_tokens":2}
        }))
    }
}

/// One mocked provider with its executor, an evaluator of its own model (an
/// endpoint's windows are learned per model) and the call's shared state.
pub(super) struct Provider {
    executor: tokio::runtime::Runtime,
    server: MockServer,
    pub(super) evaluator: Evaluator,
    evaluations: Evaluations,
}

impl Provider {
    pub(super) fn start(judge: Judge) -> Self {
        Self::start_with(judge, |server, model| {
            evaluator_of("test-key", server, model)
        })
    }

    /// [`Self::start`] with the evaluator `evaluator` builds for the
    /// server's URL and a model of its own.
    pub(super) fn start_with(judge: Judge, evaluator: fn(String, &str) -> Evaluator) -> Self {
        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let server = executor.block_on(MockServer::start());
        executor.block_on(
            Mock::given(method("POST"))
                .respond_with(move |request: &Request| judge.respond(request))
                .mount(&server),
        );
        let model = crate::judgment::test_identity::unique("units");
        let evaluator = evaluator(server.uri(), &model);
        Self {
            executor,
            server,
            evaluator,
            evaluations: Evaluations::default(),
        }
    }

    pub(super) fn judge(&self, call: &UnitCall<'_>, group_units: usize) -> UnitAnswers {
        let gate = self.evaluator.gate();
        // Room for a refusal's rounds on a loaded test machine.
        let context = context(Duration::from_secs(120));
        self.executor.block_on(self.evaluator.judge_units_at(
            call,
            &self.evaluations,
            &gate,
            &context,
            group_units,
        ))
    }

    pub(super) fn bodies(&self) -> Vec<Value> {
        self.executor
            .block_on(self.server.received_requests())
            .expect("requests")
            .iter()
            .map(|request| request.body_json().expect("request"))
            .collect()
    }

    fn body_bytes(&self) -> usize {
        self.executor
            .block_on(self.server.received_requests())
            .expect("requests")
            .iter()
            .map(|request| request.body.len())
            .sum()
    }
}

pub(super) fn question() -> Vec<Value> {
    vec![json!({"type":"noul","instructions":"The entry is marked to keep."})]
}

/// A search page as clasify lists it: `files` (path, hit count), each hit a
/// state naming its target line; the hits in `marked` (page order) say KEEP.
pub(super) fn hits(files: &[(&str, usize)], marked: &[usize]) -> Vec<Value> {
    let mut states = Vec::new();
    for (path, count) in files {
        for line in 1..=*count {
            let at = states.len();
            let mark = if marked.contains(&at) { "KEEP" } else { "skip" };
            states.push(json!({"path":path,
                "target":{"line":line * 2,"text":format!("handoff {mark} entry {at}")}}));
        }
    }
    states
}

/// A unit's own request, as clasify sends a lone hit: its state with the
/// template's scope.
fn alone(state: &Value) -> Value {
    let mut alone = state.clone();
    if state.get("target").is_some() {
        alone["question"] = section_question().cloned().unwrap_or_default();
    }
    alone
}

pub(super) fn units(states: &[Value]) -> Vec<Option<Unit<'_>>> {
    states
        .iter()
        .map(|state| {
            Some(Unit {
                state,
                alone: alone(state),
            })
        })
        .collect()
}

pub(super) fn call<'a>(units: &'a [Option<Unit<'a>>], questions: &'a [Value]) -> UnitCall<'a> {
    UnitCall {
        units,
        questions,
        context: None,
        model: None,
        long_context: None,
        cap: None,
        isolate: false,
    }
}

/// Each unit's answers as plain values (an error as its code).
pub(super) fn values(judged: &UnitAnswers) -> Vec<Option<Vec<Value>>> {
    judged
        .answers
        .iter()
        .map(|answers| {
            answers.as_ref().map(|answers| {
                answers
                    .iter()
                    .map(|answer| match answer {
                        Ok(answer) => answer.clone(),
                        Err(error) => json!({"error": error.code}),
                    })
                    .collect()
            })
        })
        .collect()
}

pub(super) fn calls(judged: &UnitAnswers) -> u64 {
    judged.spends.iter().map(|spend| spend.calls).sum()
}

/// The flowD1 shape: 63 hits of three files are three requests, one per
/// file, each hit one question naming its target and the question text the
/// state states once; every hit is answered about itself, exactly as the
/// unbatched path (one request per hit) answers it.
#[test]
fn the_hits_of_one_file_share_one_request_and_one_question_text_and_keep_their_own_answers() {
    let states = hits(
        &[
            ("docs/ref/extensions/handoff_prompt.md", 3),
            ("docs/ref/extensions/handoff_filters.md", 2),
            ("docs/handoffs.md", 58),
        ],
        &[1, 9, 30, 62],
    );
    let questions = question();
    let units = units(&states);
    let provider = Provider::start(Judge::new());
    let packed = provider.judge(&call(&units, &questions), 1);
    let bodies = provider.bodies();
    assert_eq!(bodies.len(), 3, "one request per file");
    assert_eq!(calls(&packed), 3);
    let mut held = bodies
        .iter()
        .map(|body| body["questions"].as_object().expect("questions").len())
        .collect::<Vec<_>>();
    held.sort_unstable();
    assert_eq!(held, [2, 3, 58], "one question per hit");
    for body in &bodies {
        let state = &body["state"];
        assert_eq!(state["question"], *section_question().expect("scope"));
        // No context was asked: each hit is its quoted label alone, as its
        // own request shows it, and no other hit's line informs it.
        assert!(state.get("lines").is_none(), "{state}");
        // The caller's words are stated once, in the state.
        assert_eq!(
            state["questions"],
            json!({"Q1": "The entry is marked to keep."}),
            "{state}"
        );
        for question in body["questions"].as_object().expect("questions").values() {
            let target = question["instructions"]["target"].as_str().expect("target");
            assert!(target.starts_with("line "), "{target}");
            assert_eq!(question["instructions"]["question"], "questions.Q1");
            assert_eq!(question["type"], "noul");
        }
    }

    // The unbatched path: each hit its own call, its own request.
    let unbatched = Provider::start(Judge::new());
    let alone = (0..states.len())
        .map(|at| {
            let one = &units[at..=at];
            values(&unbatched.judge(&call(one, &questions), 1))
                .pop()
                .flatten()
        })
        .collect::<Vec<_>>();
    assert_eq!(unbatched.bodies().len(), 63);
    assert_eq!(values(&packed), alone, "the same answer for every hit");
    let kept = values(&packed)
        .iter()
        .enumerate()
        .filter(|(_, answers)| {
            answers
                .as_ref()
                .is_some_and(|answers| answers[0]["noul"] == json!(0.94))
        })
        .map(|(at, _)| at)
        .collect::<Vec<_>>();
    assert_eq!(kept, [1, 9, 30, 62]);
}

/// A hit whose query asked no context lists no window: its file's shared
/// state carries no line, so no other hit's line informs it, and each
/// question quotes its own line and no other (live flowD1: with every hit
/// line in the state, six lines beside the deciding ones crossed 0.5).
#[test]
fn a_hit_without_a_window_reads_only_its_own_line() {
    let states = hits(&[("a.md", 4)], &[2]);
    let questions = question();
    let units = units(&states);
    let provider = Provider::start(Judge::new());
    let judged = provider.judge(&call(&units, &questions), 1);
    let bodies = provider.bodies();
    assert_eq!(bodies.len(), 1);
    let state = &bodies[0]["state"];
    assert!(state.get("lines").is_none(), "{state}");
    for text in states
        .iter()
        .map(|state| state["target"]["text"].as_str().expect("text"))
    {
        assert!(!state.to_string().contains(text), "{state}");
    }
    let labels = bodies[0]["questions"]
        .as_object()
        .expect("questions")
        .values()
        .map(|question| {
            question["instructions"]["target"]
                .as_str()
                .expect("target")
                .to_owned()
        })
        .collect::<Vec<_>>();
    let own = states
        .iter()
        .map(|state| {
            format!(
                "line {}: {}",
                state["target"]["line"],
                state["target"]["text"].as_str().expect("text")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, own, "each question quotes its own line only");
    assert_eq!(
        values(&judged)[2],
        Some(vec![json!({"type":"noul","noul":0.94})])
    );
}

/// Hits whose query asked context each carry their own window in their own
/// question, never in the file's shared state: no question shows a line of
/// another hit's window, exactly as each hit's own request shows it (flowD1's
/// hits at contextLines 2 would otherwise each read 138 lines on average
/// beyond their own 5).
#[test]
fn hits_with_context_read_only_their_own_window() {
    let text = |line: u64| format!("step {line}");
    let hit = |line: u64, window: std::ops::RangeInclusive<u64>| {
        json!({"path":"w.rs","target":{"line":line,"text":text(line)},
            "lines": window.map(|at| json!([at, text(at)])).collect::<Vec<_>>()})
    };
    let states = vec![
        hit(4, 2..=5),
        hit(6, 4..=7),
        hit(12, 10..=13),
        hit(20, 20..=20),
    ];
    let questions = question();
    let units = units(&states);
    let provider = Provider::start(Judge::new());
    provider.judge(&call(&units, &questions), 1);
    let bodies = provider.bodies();
    assert_eq!(bodies.len(), 1, "one request for the file");
    assert!(
        bodies[0]["state"].get("lines").is_none(),
        "{}",
        bodies[0]["state"]
    );
    let labels = bodies[0]["questions"]
        .as_object()
        .expect("questions")
        .values()
        .map(|question| {
            question["instructions"]["target"]
                .as_str()
                .expect("target")
                .to_owned()
        })
        .collect::<Vec<_>>();
    let window = |line: u64, lines: std::ops::RangeInclusive<u64>| {
        let shown = lines
            .map(|at| format!("{at}\t{}", text(at)))
            .collect::<Vec<_>>()
            .join("\n");
        format!("line {line}: {}\n{shown}", text(line))
    };
    assert_eq!(
        labels,
        [
            window(4, 2..=5),
            window(6, 4..=7),
            window(12, 10..=13),
            "line 20: step 20".to_owned()
        ],
        "each question its own window, a window of only the hit adds nothing"
    );
}

/// Whole units (items, rows, paths) are `ceil(N / k)` requests at a group
/// size `k`, with the same answers; the shipped size is one.
#[test]
fn whole_units_take_ceil_n_over_the_group_size_requests() {
    let states = (0..13)
        .map(|at| {
            let mark = if at == 2 || at == 11 { "KEEP" } else { "skip" };
            json!({"path": format!("src/{mark}/file{at}.rs")})
        })
        .collect::<Vec<_>>();
    let questions = question();
    let units = units(&states);
    let mut answers = Vec::new();
    for size in [1, 4, 8] {
        let provider = Provider::start(Judge::new());
        let judged = provider.judge(&call(&units, &questions), size);
        assert_eq!(
            provider.bodies().len(),
            13usize.div_ceil(size),
            "size {size}"
        );
        assert_eq!(calls(&judged), 13usize.div_ceil(size) as u64, "size {size}");
        answers.push(values(&judged));
    }
    assert!(answers.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(
        super::requests::GROUP_UNITS,
        1,
        "the size the fx-k2size A/B kept"
    );
}

/// Identical units share one entry: one question, every unit reads it.
#[test]
fn identical_units_collapse_into_one_question() {
    let states = vec![json!("x KEEP"), json!("y"), json!("x KEEP"), json!("y")];
    let questions = question();
    let units = units(&states);
    let provider = Provider::start(Judge::new());
    let judged = provider.judge(&call(&units, &questions), 1);
    assert_eq!(provider.bodies().len(), 2, "two distinct units");
    let answered = values(&judged);
    assert_eq!(answered[0], answered[2]);
    assert_eq!(answered[1], answered[3]);
    assert_eq!(calls(&judged), 2);
}

/// A refused packed request is split: its units are asked again in halves,
/// the sibling file's request is not sent again, and every hit is answered.
#[test]
fn a_refusal_splits_and_retries_only_the_refused_units() {
    let states = hits(&[("big.md", 8), ("small.md", 2)], &[0, 5, 9]);
    let questions = question();
    let units = units(&states);
    let provider = Provider::start(Judge {
        refuse_over: 4,
        ..Judge::new()
    });
    let judged = provider.judge(&call(&units, &questions), 1);
    let bodies = provider.bodies();
    let of = |path: &str| {
        bodies
            .iter()
            .filter(|body| body["state"]["path"] == path)
            .map(|body| body["questions"].as_object().expect("questions").len())
            .collect::<Vec<_>>()
    };
    let mut big = of("big.md");
    big.sort_unstable();
    assert_eq!(big, [4, 4, 8], "refused whole, then two halves");
    assert_eq!(of("small.md"), [2], "the sibling is sent once");
    assert_eq!(judged.refused, 1);
    assert_eq!(calls(&judged), 4, "the refused request counts");
    let answered = values(&judged);
    for (at, answers) in answered.iter().enumerate() {
        let expected = if [0, 5, 9].contains(&at) { 0.94 } else { 0.03 };
        assert_eq!(
            answers.as_ref().expect("judged")[0]["noul"],
            expected,
            "{at}"
        );
    }
}

/// An answer missing from a packed response is asked again, once and alone;
/// still missing, it is that unit's error, never a guess. One missing only
/// on its first ask is answered by the re-ask.
#[test]
fn a_missing_answer_is_asked_again_once_then_reported() {
    let mut states = hits(&[("a.md", 5)], &[1]);
    states[3]["target"]["text"] = json!("handoff LOST entry 3");
    let questions = question();
    let units = units(&states);

    let provider = Provider::start(Judge::new());
    let judged = provider.judge(&call(&units, &questions), 1);
    let bodies = provider.bodies();
    assert_eq!(bodies.len(), 2, "the packed request and one re-ask");
    assert!(
        bodies
            .iter()
            .any(|body| body["state"]["target"]["text"] == "handoff LOST entry 3"),
        "the re-ask holds only the missing unit, as its own request"
    );
    let answered = values(&judged);
    assert_eq!(
        answered[3],
        Some(vec![
            json!({"error": ErrorCode::InvalidClassificationResponse})
        ])
    );
    assert_eq!(answered[1], Some(vec![json!({"type":"noul","noul":0.94})]));
    assert!(
        [0, 2, 4]
            .iter()
            .all(|&at| answered[at] == Some(vec![json!({"type":"noul","noul":0.03})]))
    );
    assert_eq!(calls(&judged), 2);

    let flaky = Provider::start(Judge {
        once: true,
        ..Judge::new()
    });
    let judged = flaky.judge(&call(&units, &questions), 1);
    assert_eq!(flaky.bodies().len(), 2);
    assert_eq!(
        values(&judged)[3],
        Some(vec![json!({"type":"noul","noul":0.03})])
    );
}

/// The call's spend is every request's, summed once; an identical packed
/// request of a sibling row is not sent again and carries no calls.
#[test]
fn usage_sums_across_requests_and_a_twin_request_is_sent_once() {
    let states = hits(&[("a.md", 4), ("b.md", 3)], &[2]);
    let questions = question();
    let units = units(&states);
    let provider = Provider::start(Judge::new());
    let first = provider.judge(&call(&units, &questions), 1);
    let twin = provider.judge(&call(&units, &questions), 1);
    assert_eq!(provider.bodies().len(), 2);
    let sum = |judged: &UnitAnswers, field: fn(&Spend) -> u64| -> u64 {
        judged.spends.iter().map(field).sum()
    };
    assert_eq!(sum(&first, |spend| spend.calls), 2);
    assert_eq!(sum(&first, |spend| spend.input_tokens), 20);
    assert_eq!(sum(&first, |spend| spend.output_tokens), 4);
    assert_eq!(sum(&twin, |spend| spend.calls), 0);
    assert!(
        first
            .spends
            .iter()
            .zip(&twin.spends)
            .all(|(a, b)| a.identity.is_some() && a.identity == b.identity)
    );
    assert_eq!(values(&first), values(&twin));
}

/// The plan prices the call before any request: its request count and body
/// bytes are the ones sent.
#[test]
fn the_plan_matches_the_requests_sent() {
    let states = hits(&[("a.md", 6), ("b.md", 1), ("c.md", 9)], &[3]);
    let questions = question();
    let units = units(&states);
    let provider = Provider::start(Judge::new());
    let plan = provider
        .evaluator
        .plan_units_at(&call(&units, &questions), 1);
    assert!(provider.bodies().is_empty(), "planning sends nothing");
    provider.judge(&call(&units, &questions), 1);
    assert_eq!(plan.requests, 3);
    assert_eq!(plan.requests, provider.bodies().len());
    assert_eq!(plan.body_bytes, provider.body_bytes());
    // Before the provider answered here, only its window bounds a request.
    assert!(plan.input_tokens_at_most > 0);
    assert!(plan.input_tokens_at_most <= provider.evaluator.request_window(false) * plan.requests_at_most);
    let learned = provider
        .evaluator
        .plan_units_at(&call(&units, &questions), 1);
    assert!(learned.input_tokens_at_most >= learned.body_bytes as u64);
}

/// A price's input tokens are a bound a real run cannot pass, small
/// requests included, where the provider's own prompt outweighs the body:
/// before the provider answered a request here the stated window bounds it;
/// after one, every plan (one item, packed items, hits of several files; one question
/// or three) states at most what its run then reports.
#[test]
fn a_planned_token_price_bounds_what_the_run_reports() {
    let mut judge = Judge::new();
    judge.prompt = Some((600, 300));
    let provider = Provider::start(judge);
    let one = question();
    let three = vec![
        json!({"type":"noul","instructions":"The entry is marked to keep."}),
        json!({"type":"noul","instructions":"The entry is short."}),
        json!({"type":"noul","instructions":"The entry names a file."}),
    ];
    let item = vec![json!("fix: keep the KEEP entry")];
    let items = (0..5)
        .map(|at| json!(format!("item {at}: a short line")))
        .collect::<Vec<_>>();
    let page = hits(&[("a.md", 3), ("b.md", 2)], &[1]);
    let cases: [(&[Value], &[Value]); 4] = [
        (&item, &one),
        (&items, &three),
        (&page, &one),
        (&item, &three),
    ];
    let first = units(cases[0].0);
    // Before any answer a request is bounded by the window alone, which a
    // small request's real cost stays under.
    assert!(
        provider
            .evaluator
            .plan_units(&call(&first, cases[0].1))
            .input_tokens_at_most
            > 0
    );
    provider.judge(&call(&first, cases[0].1), super::requests::GROUP_UNITS);
    for (states, questions) in cases {
        let units = units(states);
        let plan = provider.evaluator.plan_units(&call(&units, questions));
        let judged = provider.judge(&call(&units, questions), super::requests::GROUP_UNITS);
        let reported = judged
            .spends
            .iter()
            .map(|spend| spend.input_tokens)
            .sum::<u64>();
        let bound = plan.input_tokens_at_most;
        assert!(
            reported <= bound,
            "{} units, {} questions: reported {reported} over {bound}",
            states.len(),
            questions.len()
        );
        // A token per body byte alone would not bound it: the prompt
        // outweighs the body.
        assert!(reported > plan.body_bytes as u64, "{reported}");
    }
}

/// Many questions close a file's group at the provider's request window:
/// no request is over it, and every hit is still answered. Each question's
/// words are too short to be stated once, so every hit's questions carry
/// them.
#[test]
fn a_hit_group_closes_at_the_request_window() {
    let states = hits(&[("a.md", 40)], &[7]);
    let questions = (0..20)
        .map(|n| json!({"type":"noul","instructions":format!("Kept {n}?")}))
        .collect::<Vec<_>>();
    let units = units(&states);
    let provider = Provider::start(Judge::new());
    let judged = provider.judge(&call(&units, &questions), 1);
    let bodies = provider.bodies();
    assert!(bodies.len() >= 2, "{} requests", bodies.len());
    let window = usize::try_from(
        octocode_runtime_base::providers::classification::jev::JEV
            .defaults()
            .request_window(false),
    )
    .expect("window");
    for body in &bodies {
        let sent =
            serde_json::to_vec(&json!({"state":body["state"],"questions":body["questions"]}))
                .expect("body")
                .len();
        assert!(
            sent <= window,
            "{sent} over the {window}-byte request window"
        );
    }
    assert!(values(&judged).iter().all(Option::is_some));
    assert_eq!(
        values(&judged)[7],
        Some(vec![json!({"type":"noul","noul":0.94}); 20])
    );
}

/// A packed request states the caller's long words once, whatever form they
/// take, and each question names them and its target, keeping its type and
/// criteria; words shorter than twice their name stay in each question. The
/// request is smaller than one whose every question carries the words.
#[test]
fn a_packed_question_names_the_callers_words_stated_once() {
    let long = json!({"type":"noul","instructions":
        "Does this line show the entry marked to keep, and not a passing mention of it?"});
    let short = json!({"type":"noul","instructions":{"ask":"Kept?"}});
    let questions = vec![long.clone(), short.clone()];
    let states = hits(&[("a.md", 4)], &[]);
    let units = units(&states);
    let provider = Provider::start(Judge::new());
    let judged = provider.judge(&call(&units, &questions), 1);
    let bodies = provider.bodies();
    assert_eq!(bodies.len(), 1, "one file, one request");
    let body = &bodies[0];
    assert_eq!(
        body["state"]["questions"],
        json!({"Q1": long["instructions"]}),
        "only the long words are shared"
    );
    let asked = body["questions"].as_object().expect("questions");
    assert_eq!(asked.len(), 8, "every hit is asked both questions");
    let mut repeated = body.clone();
    for (at, (key, question)) in asked.iter().enumerate() {
        let label = question["instructions"]["target"]
            .as_str()
            .expect("target")
            .to_owned();
        assert!(label.starts_with("line "), "{question}");
        let caller = &questions[at % 2];
        assert_eq!(question["type"], caller["type"]);
        assert_eq!(question.get("criteria"), caller.get("criteria"));
        if at % 2 == 0 {
            assert_eq!(question["instructions"]["question"], "questions.Q1");
        } else {
            assert_eq!(
                question["instructions"],
                json!({"ask":"Kept?","target":label})
            );
        }
        repeated["questions"][key] = units::aimed(caller, &label).expect("aimed");
    }
    repeated["state"]
        .as_object_mut()
        .expect("state")
        .remove("questions");
    let size = |body: &Value| serde_json::to_vec(body).expect("body").len();
    assert!(
        size(body) < size(&repeated),
        "{} vs {}",
        size(body),
        size(&repeated)
    );
    assert!(values(&judged).iter().all(Option::is_some));
}

/// A caller's question names its target beside its own words, whatever form
/// the words take.
#[test]
fn a_targeted_question_keeps_the_callers_words() {
    let string = json!({"type":"noul","instructions":"Is it set?"});
    assert_eq!(
        units::aimed(&string, "line 3: x").expect("aimed")["instructions"],
        json!({"question":"Is it set?","target":"line 3: x"})
    );
    let object =
        json!({"type":"choice","instructions":{"ask":"Kind?"},"criteria":{"a":"A.","b":"B."}});
    let aimed = units::aimed(&object, "line 3: x").expect("aimed");
    assert_eq!(
        aimed["instructions"],
        json!({"ask":"Kind?","target":"line 3: x"})
    );
    assert_eq!(aimed["criteria"], object["criteria"]);
    let taken = json!({"type":"noul","instructions":{"target":"mine"}});
    assert_eq!(
        units::aimed(&taken, "line 3: x").expect("aimed")["instructions"],
        json!({"question":{"target":"mine"},"target":"line 3: x"})
    );
}

/// Opted in, every hit is its own request, as before units shared requests:
/// N hits are N requests, each answered exactly as the unbatched path
/// answers it, and the plan prices N before anything is sent.
#[test]
fn isolated_hits_take_one_request_each_priced_before_sending() {
    let states = hits(&[("a.md", 3), ("b.md", 2), ("c.md", 7)], &[1, 4, 9]);
    let questions = question();
    let units = units(&states);
    let isolated = UnitCall {
        isolate: true,
        ..call(&units, &questions)
    };
    let provider = Provider::start(Judge::new());
    let plan = provider.evaluator.plan_units_at(&isolated, 1);
    assert!(provider.bodies().is_empty(), "planning sends nothing");
    assert_eq!(plan.requests, 12, "one request per hit");
    assert_eq!(
        plan.requests_at_most, 12,
        "a small hit alone is never refused"
    );
    let judged = provider.judge(&isolated, 1);
    let bodies = provider.bodies();
    assert_eq!(bodies.len(), 12);
    assert_eq!(calls(&judged), 12);
    for body in &bodies {
        assert_eq!(
            body["questions"].as_object().map(serde_json::Map::len),
            Some(1),
            "a hit's own request asks only its questions"
        );
        assert!(body["state"].get("target").is_some(), "{body}");
    }
    // The unbatched path: each hit judged in a call of its own.
    let unbatched = Provider::start(Judge::new());
    let alone = (0..states.len())
        .map(|at| {
            let one = &units[at..=at];
            values(&unbatched.judge(&call(one, &questions), 1))
                .pop()
                .flatten()
        })
        .collect::<Vec<_>>();
    assert_eq!(values(&judged), alone, "the same answer for every hit");
    // Packed, the same hits are one request per file.
    let packed = Provider::start(Judge::new());
    packed.judge(&call(&units, &questions), 1);
    assert_eq!(packed.bodies().len(), 3);
}
