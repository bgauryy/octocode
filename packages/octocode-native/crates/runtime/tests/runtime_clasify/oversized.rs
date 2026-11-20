//! A text `source` larger than the provider's windows is answered by one
//! `evaluate` row: it is judged in sections, each question on every section,
//! and the row holds one answer per question with the deciding section's
//! lines, the coverage, and a read of the deciding lines.

use super::{clasify, noul};
use crate::support::{Workspace, clasify_runtime};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The fact one line of the source states.
const PLANTED: &str = "The worker opens a socket to the ledger.";
/// The largest request body the stand-in provider's window takes.
const WINDOW_BYTES: usize = 100_000;
/// A `maxRequests` that holds every section of these sources: a source's
/// sections are judged only within the row's bound.
const SECTIONS: u64 = 200;

/// The input tokens a taken request of `bytes` reports: the stated window's
/// tokens over [`WINDOW_BYTES`], so the window holds exactly that many bytes.
fn tokens(bytes: usize) -> usize {
    (bytes * crate::support::WINDOW_TOKENS).div_ceil(WINDOW_BYTES)
}

/// Refuses a body over [`WINDOW_BYTES`] in any window; else answers from the
/// section's own target: yes and the second label where it holds [`PLANTED`].
async fn windowed() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            if request.body.len() > WINDOW_BYTES {
                return ResponseTemplate::new(400)
                    .set_body_json(json_fixture(r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#));
            }
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let target = body["state"]["target"]["text"].as_str().unwrap_or_default();
            let hit = target.contains(PLANTED);
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .iter()
                .map(|(id, question)| {
                    let answer = if question["type"] == "choice" {
                        let labels: Vec<&String> =
                            question["criteria"].as_object().expect("labels").keys().collect();
                        let chosen = labels[usize::from(hit)];
                        let probabilities = labels
                            .iter()
                            .map(|label| ((*label).clone(), json!(if *label == chosen { 1.0 } else { 0.0 })))
                            .collect::<serde_json::Map<_, _>>();
                        json!({"type":"choice","choice":chosen,"confidence":1.0,"probabilities":probabilities})
                    } else {
                        json!({"type":"noul","noul":if hit { 0.96 } else { 0.03 }})
                    };
                    (id.clone(), answer)
                })
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-test","answers":answers,
                "usage":{"input_tokens":tokens(request.body.len()),"output_tokens":5},
            }))
        })
        .mount(&server)
        .await;
    server
}

/// A source of `lines` plain lines with [`PLANTED`] at line `planted`.
fn source(lines: usize, planted: usize) -> String {
    (1..=lines)
        .map(|line| {
            if line == planted {
                format!("{PLANTED}\n")
            } else {
                format!("Entry {line}: the nightly job copied {line} records and exited cleanly.\n")
            }
        })
        .collect()
}

#[tokio::test]
async fn an_oversized_source_is_judged_in_sections_and_answered_once_per_question() {
    let server = windowed().await;
    let workspace = Workspace::new();
    let (lines, planted) = (6_000, 4_321);
    let text = source(lines, planted);
    // Past both windows: the whole source is refused, and no long-window
    // retry can be shown to fit.
    assert!(text.len() > 262_000, "{}", text.len());
    workspace.write("big.txt", &text);
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = json!({"operation":"evaluate","maxRequests":SECTIONS,
    "source":{"tool":"localFetch","query":{"path":"big.txt"}},
    "questions":{
        "socket":noul("The text says the worker opens a socket."),
        "gpu":{"type":"choice","criteria":{"clean":"Only routine entries.","odd":"An entry that is not routine."}},
    }});
    let out = clasify(&runtime, vec![row]).await;
    let data = &out["results"][0]["data"];
    assert!(data.get("errorCode").is_none(), "{out}");

    // One answer per question, each naming its deciding section's lines.
    let socket = &data["answers"]["socket"];
    assert_eq!(socket["noul"], 0.96, "{out}");
    let (start, end) = (
        socket["lines"]["startLine"].as_u64().unwrap() as usize,
        socket["lines"]["endLine"].as_u64().unwrap() as usize,
    );
    assert!((start..=end).contains(&planted), "{out}");
    assert!(end - start < lines / 4, "a section, not the source: {out}");
    // Every section but one chose the first label: the majority, with the
    // minority visible as its share.
    let kind = &data["answers"]["gpu"];
    assert_eq!(kind["choice"], "clean", "{out}");
    assert!(
        kind["probabilities"]["odd"].as_f64().unwrap() > 0.0,
        "{out}"
    );

    // Coverage: every section answered both questions.
    let sections = data["coverage"]["totalSections"].as_u64().unwrap();
    assert!(sections > 1, "{out}");
    assert_eq!(data["coverage"]["answered"], json!(sections * 2), "{out}");
    // Spend: the refused whole-source request and one request per section;
    // `refused` says which of the calls was the whole source.
    assert_eq!(data["usage"]["calls"], json!(sections + 1), "{out}");
    assert_eq!(data["usage"]["refused"], 1, "{out}");
    assert_eq!(
        server.received_requests().await.unwrap().len() as u64,
        sections + 1
    );
    // The read holds the deciding lines, not the source.
    assert!(data["next"]["read"].is_object(), "{out}");
    crate::support::assert_cursor_only(&data["next"]["read"]["input"]);
    assert!(
        data.get("related").is_none(),
        "no optional fresh evaluation: {out}"
    );
    runtime.close().await;
}

/// Sections fill the window: a source of N tokens takes about N / window
/// requests. The first call learns what a token costs from its first
/// section; a second call, which knows the window, sends exactly the
/// sections that fill it. Every answer still names its deciding lines.
#[tokio::test]
async fn sections_fill_the_window_the_provider_states() {
    let server = windowed().await;
    let workspace = Workspace::new();
    let (lines, planted) = (6_000, 4_321);
    let text = source(lines, planted);
    workspace.write("big.txt", &text);
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = json!({"operation":"evaluate","maxRequests":SECTIONS,
        "source":{"tool":"localFetch","query":{"path":"big.txt"}},
        "questions":{"socket":noul("The text says the worker opens a socket.")}});
    // The fewest requests that hold the source's tokens.
    let fewest = tokens(text.len()).div_ceil(crate::support::WINDOW_TOKENS) as u64;
    let requests = || async { server.received_requests().await.unwrap() };

    let first = clasify(&runtime, vec![row.clone()]).await;
    let data = &first["results"][0]["data"];
    let sections = data["coverage"]["totalSections"].as_u64().unwrap();
    eprintln!(
        "window fill: {} bytes, {fewest} windows; first call {sections} sections",
        text.len()
    );
    // One small section at one byte per token, then the rest at the window.
    assert!(
        sections <= fewest + 2,
        "{sections} sections for {fewest} windows: {first}"
    );
    // Every call is a request the provider received: the whole source, then
    // the sections. A section is sized by the window's average bytes per
    // character, so one denser than the average can pass the window by a few
    // bytes; it is refused and split (`sections::whole`), and its request
    // counts like any other.
    assert_eq!(
        data["usage"]["calls"],
        json!(requests().await.len()),
        "usage counts every request sent: {first}"
    );
    assert_eq!(data["usage"]["refused"], 1, "the whole source: {first}");

    let before = requests().await.len();
    let again = clasify(&runtime, vec![row]).await;
    let data = &again["results"][0]["data"];
    let sections = data["coverage"]["totalSections"].as_u64().unwrap();
    eprintln!("window fill: second call {sections} sections");
    assert!(
        sections <= fewest + 1,
        "{sections} sections for {fewest} windows: {again}"
    );
    assert_eq!(
        data["usage"]["calls"],
        json!(requests().await.len() - before),
        "{again}"
    );
    // The learned window sent no whole source to be refused.
    assert!(data["usage"].get("refused").is_none(), "{again}");
    let sent = requests().await;
    assert!(
        sent[before..]
            .iter()
            .all(|request| request.body.len() <= WINDOW_BYTES),
        "every section fits the window"
    );
    let socket = &data["answers"]["socket"];
    assert_eq!(socket["noul"], 0.96, "{again}");
    let (start, end) = (
        socket["lines"]["startLine"].as_u64().unwrap() as usize,
        socket["lines"]["endLine"].as_u64().unwrap() as usize,
    );
    assert!((start..=end).contains(&planted), "{again}");
    assert!(end - start < lines, "a section, not the source: {again}");
    runtime.close().await;
}

/// A source no section of which can be judged keeps the refusal as its error.
#[tokio::test]
async fn a_source_whose_sections_are_refused_too_keeps_the_refusal() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json_fixture(
            r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#,
        )))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write("big.txt", source(6_000, 1));
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = json!({"operation":"evaluate","maxRequests":SECTIONS,
        "source":{"tool":"localFetch","query":{"path":"big.txt"}},
        "questions":{"socket":noul("The text says the worker opens a socket.")}});
    let out = clasify(&runtime, vec![row]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["errorCode"], "classificationStateTooLarge", "{out}");
    assert!(data.get("answers").is_none(), "{out}");
    runtime.close().await;
}

/// The provider's refusal is remembered for its model and window: a second
/// source at least that large is not uploaded whole to be refused again, it
/// goes straight to its sections. A source under every refused size is still
/// asked whole.
#[tokio::test]
async fn a_size_the_provider_refused_is_not_uploaded_whole_again() {
    let server = windowed().await;
    let workspace = Workspace::new();
    workspace.write("big.txt", source(6_000, 4_321));
    workspace.write("bigger.txt", source(7_000, 10));
    workspace.write("small.txt", source(600, 300));
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |file: &str| {
        json!({"operation":"evaluate","maxRequests":SECTIONS,
            "source":{"tool":"localFetch","query":{"path":file}},
            "questions":{"socket":noul("The text says the worker opens a socket.")}})
    };
    let requests = || async { server.received_requests().await.unwrap().len() as u64 };

    // First: the whole source is tried once and refused, then its sections.
    let first = clasify(&runtime, vec![row("big.txt")]).await;
    let data = &first["results"][0]["data"];
    // The whole source once, then its sections (a section denser than the
    // window's average may be refused and split: it counts as a request).
    assert_eq!(data["usage"]["calls"], json!(requests().await), "{first}");
    assert_eq!(data["usage"]["refused"], 1, "{first}");

    // Again, and a larger one: sections only, the same answer.
    for file in ["big.txt", "bigger.txt"] {
        let before = requests().await;
        let again = clasify(&runtime, vec![row(file)]).await;
        let data = &again["results"][0]["data"];
        let sections = data["coverage"]["totalSections"].as_u64().unwrap();
        assert_eq!(data["answers"]["socket"]["noul"], 0.96, "{again}");
        assert!(sections > 1, "{file}: {again}");
        assert_eq!(
            data["usage"]["calls"],
            json!(requests().await - before),
            "{file}: {again}"
        );
        assert!(data["usage"].get("refused").is_none(), "{file}: {again}");
        let largest = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .skip(before as usize)
            .map(|request| request.body.len())
            .max()
            .unwrap();
        assert!(
            largest <= WINDOW_BYTES,
            "{file}: no whole upload ({largest})"
        );
    }

    // Under the refused size nothing is known: the provider is asked whole.
    let before = requests().await;
    let small = clasify(&runtime, vec![row("small.txt")]).await;
    let data = &small["results"][0]["data"];
    assert!(data.get("coverage").is_none(), "{small}");
    assert!(data["answers"]["socket"].is_object(), "{small}");
    assert_eq!(requests().await - before, 1);
    runtime.close().await;
}

/// The call's time ends while sections are being judged: the row still
/// answers from the sections that finished, counts the rest in its coverage,
/// and offers the `sections` call that judges them.
#[tokio::test]
async fn a_deadline_mid_sections_answers_from_the_judged_ones_and_counts_the_rest() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            if request.body.len() > WINDOW_BYTES {
                return ResponseTemplate::new(400).set_body_json(json_fixture(
                    r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#,
                ));
            }
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let target = body["state"]["target"]["text"].as_str().unwrap_or_default();
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| {
                    let noul = if target.contains(PLANTED) { 0.96 } else { 0.03 };
                    (id.clone(), json!({"type":"noul","noul":noul}))
                })
                .collect::<serde_json::Map<_, _>>();
            let reply = ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-test","answers":answers,
                "usage":{"input_tokens":tokens(request.body.len()),"output_tokens":5},
            }));
            // The sections past line 3000 answer after the call's time.
            if target.contains("Entry 5000:") || target.contains("Entry 3500:") {
                reply.set_delay(std::time::Duration::from_secs(20))
            } else {
                reply
            }
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write("big.txt", source(6_000, 120));
    let limits = octocode_runtime_base::execution::RuntimeLimits {
        classification_timeout: Some(std::time::Duration::from_secs(3)),
        ..crate::support::PROVIDER_LIMITS
    };
    let runtime = workspace.runtime_with(&crate::support::provider_env(&server, &[]), limits);
    let row = json!({"operation":"evaluate","maxRequests":SECTIONS,
        "source":{"tool":"localFetch","query":{"path":"big.txt"}},
        "questions":{"socket":noul("The text says the worker opens a socket.")}});
    let out = clasify(&runtime, vec![row]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["answers"]["socket"]["noul"], 0.96, "{out}");
    let coverage = &data["coverage"];
    let total = coverage["totalSections"].as_u64().unwrap();
    let answered = coverage["answered"].as_u64().unwrap();
    let rest =
        coverage["unassessed"].as_u64().unwrap_or(0) + coverage["error"].as_u64().unwrap_or(0);
    assert!(answered > 0 && rest > 0, "{out}");
    assert_eq!(answered + rest, total, "{out}");
    // The deciding source remains readable; unassessed coverage above is not
    // turned into an absence claim or hidden by optional fresh-work advice.
    assert_eq!(data["next"]["read"]["tool"], "localFetch", "{out}");
    crate::support::assert_cursor_only(&data["next"]["read"]["input"]);
    runtime.close().await;
}

/// Refuses a body over [`WINDOW_BYTES`]; else answers `a` as a noul and `b`
/// with a label its question does not offer, and reports the tokens of every
/// taken request through `spent`.
async fn half_invalid(spent: std::sync::Arc<std::sync::atomic::AtomicU64>) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(move |request: &wiremock::Request| {
            if request.body.len() > WINDOW_BYTES {
                return ResponseTemplate::new(400).set_body_json(json_fixture(
                    r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#,
                ));
            }
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .iter()
                .map(|(id, question)| {
                    let answer = if question["type"] == "choice" {
                        json_fixture(
                            r#"{
                                "type": "choice",
                                "choice": "not-a-label",
                                "confidence": 1.0,
                                "probabilities": {"not-a-label": 1.0}
                            }"#,
                        )
                    } else {
                        json_fixture(r#"{"type": "noul", "noul": 0.2}"#)
                    };
                    (id.clone(), answer)
                })
                .collect::<serde_json::Map<_, _>>();
            let input = tokens(request.body.len()) as u64;
            spent.fetch_add(input, std::sync::atomic::Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-test","answers":answers,
                "usage":{"input_tokens":input,"output_tokens":5},
            }))
        })
        .mount(&server)
        .await;
    server
}

fn two_kinds() -> Value {
    json!({
        "a":noul("The text says the worker opens a socket."),
        "b":{"type":"choice","criteria":{"clean":"Only routine entries.","odd":"An entry that is not routine."}},
    })
}

/// One question every section answers and one every section's reply is
/// invalid for: the row keeps the answered one, the other is an error cell
/// with its sections' code, and the usage counts every request.
#[tokio::test]
async fn an_unanswered_question_is_an_error_cell_beside_the_answered_ones() {
    let spent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let server = half_invalid(spent.clone()).await;
    let workspace = Workspace::new();
    workspace.write("big.txt", source(6_000, 4_321));
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |view: &str| {
        json!({"operation":"evaluate","maxRequests":SECTIONS,"view":view,
            "source":{"tool":"localFetch","query":{"path":"big.txt"}},
            "questions":two_kinds()})
    };
    let summary = clasify(&runtime, vec![row("summary")]).await;
    let data = &summary["results"][0]["data"];
    assert_eq!(data["answers"]["a"]["noul"], 0.2, "{summary}");
    assert_eq!(data["answers"]["b"]["status"], "error", "{summary}");
    assert_eq!(
        data["answers"]["b"]["error"]["errorCode"], "invalidClassificationResponse",
        "{summary}"
    );
    assert!(data.get("errorCode").is_none(), "{summary}");
    let requests = server.received_requests().await.unwrap().len() as u64;
    assert_eq!(data["usage"]["calls"], json!(requests), "{summary}");

    spent.store(0, std::sync::atomic::Ordering::SeqCst);
    let before = server.received_requests().await.unwrap().len() as u64;
    let details = clasify(&runtime, vec![row("details")]).await;
    let data = &details["results"][0]["data"];
    // The window refusals report no usage; every taken request did.
    assert_eq!(
        data["usage"]["inputTokens"],
        json!(spent.load(std::sync::atomic::Ordering::SeqCst)),
        "{details}"
    );
    let sent = server.received_requests().await.unwrap().len() as u64 - before;
    assert_eq!(data["usage"]["calls"], json!(sent), "{details}");
    runtime.close().await;
}

/// Every question failed for one reason that is not the window: the row
/// fails with that reason, not the refusal, and still counts the spend.
#[tokio::test]
async fn a_row_whose_every_question_failed_alike_reports_that_reason_and_the_spend() {
    let spent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let server = half_invalid(spent.clone()).await;
    let workspace = Workspace::new();
    workspace.write("big.txt", source(6_000, 4_321));
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![
            json!({"operation":"evaluate","maxRequests":SECTIONS,"view":"details",
            "source":{"tool":"localFetch","query":{"path":"big.txt"}},
            "questions":{"b":two_kinds()["b"].clone()}}),
        ],
    )
    .await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["errorCode"], "invalidClassificationResponse", "{out}");
    let requests = server.received_requests().await.unwrap().len() as u64;
    assert_eq!(data["usage"]["calls"], json!(requests), "{out}");
    assert_eq!(
        data["usage"]["inputTokens"],
        json!(spent.load(std::sync::atomic::Ordering::SeqCst)),
        "{out}"
    );
    runtime.close().await;
}

/// C1: a source over the window sends its one named request whole; its
/// sections are its own expansion, so without `maxRequests` none is sent:
/// the row states their price and `next.confirm` runs them. C3: in the same
/// process the refused size is known, and the same row sends nothing.
#[tokio::test]
async fn an_oversized_sources_sections_wait_for_a_bound_that_holds_them() {
    let server = windowed().await;
    let workspace = Workspace::new();
    workspace.write("big.txt", source(6_000, 4_321));
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = json!({"operation":"evaluate",
        "source":{"tool":"localFetch","query":{"path":"big.txt"}},
        "questions":{"socket":noul("The text says the worker opens a socket.")}});
    let requests = || async { server.received_requests().await.unwrap().len() as u64 };

    let out = clasify(&runtime, vec![row.clone()]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["errorCode"], "classificationOverBudget", "{out}");
    let spent = requests().await;
    assert!(spent >= 1, "the whole source was asked: {out}");
    assert_eq!(data["usage"]["calls"], json!(spent), "{out}");
    let ceiling = data["usage"]["requestsCeiling"]
        .as_u64()
        .expect("a ceiling");
    assert!(
        data["usage"]["requestsPlanned"].as_u64().expect("a plan") > 1,
        "{out}"
    );
    let confirm = &data["next"]["confirm"];
    assert_eq!(
        confirm["input"]["queries"][0]["maxRequests"], ceiling,
        "{out}"
    );

    let again = clasify(&runtime, vec![row]).await;
    let data = &again["results"][0]["data"];
    assert_eq!(data["errorCode"], "classificationOverBudget", "{again}");
    assert_eq!(data["usage"]["calls"], 0, "{again}");
    assert_eq!(requests().await, spent, "a known refusal sends nothing");

    let confirmed = crate::support::Run::run(
        &runtime,
        "confirm".into(),
        "clasify".into(),
        confirm["input"].clone(),
    )
    .await
    .expect("the confirm runs")
    .structured_content
    .into_value();
    let data = &confirmed["results"][0]["data"];
    assert!(data.get("errorCode").is_none(), "{confirmed}");
    assert_eq!(data["answers"]["socket"]["noul"], 0.96, "{confirmed}");
    assert!(
        data["coverage"]["totalSections"].as_u64().unwrap() > 1,
        "{confirmed}"
    );
    let calls = data["usage"]["calls"].as_u64().expect("calls");
    assert!(calls <= ceiling, "{calls} past the confirmed {ceiling}");
    assert_eq!(requests().await - spent, calls, "{confirmed}");
    runtime.close().await;
}

/// C3: literal content has no sections; once the window refused its size in
/// this process, content at least that large is refused without a request.
#[tokio::test]
async fn content_the_window_refused_is_not_sent_again() {
    let server = windowed().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = json!({"operation":"evaluate","content":"entry ".repeat(WINDOW_BYTES / 4),
        "questions":{"socket":noul("The text says the worker opens a socket.")}});
    let requests = || async { server.received_requests().await.unwrap().len() };
    let first = clasify(&runtime, vec![row.clone()]).await;
    assert_eq!(
        first["results"][0]["data"]["errorCode"], "classificationStateTooLarge",
        "{first}"
    );
    let spent = requests().await;
    assert!(spent >= 1, "{first}");
    let again = clasify(&runtime, vec![row]).await;
    let data = &again["results"][0]["data"];
    assert_eq!(data["errorCode"], "classificationStateTooLarge", "{again}");
    assert!(data.get("usage").is_none(), "nothing was sent: {again}");
    assert_eq!(requests().await, spent);
    runtime.close().await;
}
