//! A text `source` larger than the provider's windows is answered by one
//! `evaluate` row: it is judged in sections, each question on every section,
//! and the row holds one answer per question with the deciding section's
//! lines, the coverage, and a read of the deciding lines.

use super::{clasify, noul};
use crate::support::{Workspace, clasify_runtime};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The fact one line of the source states.
const PLANTED: &str = "The worker opens a socket to the ledger.";
/// The largest request body the stand-in provider's window takes.
const WINDOW_BYTES: usize = 100_000;

/// Refuses a body over [`WINDOW_BYTES`] in any window; else answers from the
/// section's own target: yes and the second label where it holds [`PLANTED`].
async fn windowed() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            if request.body.len() > WINDOW_BYTES {
                return ResponseTemplate::new(400)
                    .set_body_json(json!({"detail":{"error_type":"max_tokens_exceeded"}}));
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
                "usage":{"input_tokens":100,"output_tokens":5},
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
    let row = json!({"operation":"evaluate",
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
    assert!(kind["probabilities"]["odd"].as_f64().unwrap() > 0.0, "{out}");

    // Coverage: every section answered both questions.
    let sections = data["coverage"]["totalSections"].as_u64().unwrap();
    assert!(sections > 1, "{out}");
    assert_eq!(data["coverage"]["answered"], json!(sections * 2), "{out}");
    // Spend: the refused whole-source request and one request per section.
    assert_eq!(data["usage"]["calls"], json!(sections + 1), "{out}");
    assert_eq!(
        server.received_requests().await.unwrap().len() as u64,
        sections + 1
    );
    // The read holds the deciding lines, not the source.
    assert!(data["next"]["read"].is_object(), "{out}");
    // Every section's own verdict is one call away.
    let detail = data["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "clasify")
        .expect("sections call");
    assert_eq!(detail["input"]["queries"][0]["operation"], "sections", "{out}");
    runtime.close().await;
}

/// A source no section of which can be judged keeps the refusal as its error.
#[tokio::test]
async fn a_source_whose_sections_are_refused_too_keeps_the_refusal() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(json!({"detail":{"error_type":"max_tokens_exceeded"}})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write("big.txt", source(6_000, 1));
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = json!({"operation":"evaluate",
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
        json!({"operation":"evaluate",
            "source":{"tool":"localFetch","query":{"path":file}},
            "questions":{"socket":noul("The text says the worker opens a socket.")}})
    };
    let requests = || async { server.received_requests().await.unwrap().len() as u64 };

    // First: the whole source is tried once and refused, then its sections.
    let first = clasify(&runtime, vec![row("big.txt")]).await;
    let data = &first["results"][0]["data"];
    let sections = data["coverage"]["totalSections"].as_u64().unwrap();
    assert_eq!(data["usage"]["calls"], json!(sections + 1), "{first}");
    let after_first = requests().await;
    assert_eq!(after_first, sections + 1);

    // Again, and a larger one: sections only, the same answer.
    for file in ["big.txt", "bigger.txt"] {
        let before = requests().await;
        let again = clasify(&runtime, vec![row(file)]).await;
        let data = &again["results"][0]["data"];
        let sections = data["coverage"]["totalSections"].as_u64().unwrap();
        assert_eq!(data["answers"]["socket"]["noul"], 0.96, "{again}");
        assert_eq!(data["usage"]["calls"], json!(sections), "{file}: {again}");
        assert_eq!(requests().await - before, sections, "{file}");
        let largest = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .skip(before as usize)
            .map(|request| request.body.len())
            .max()
            .unwrap();
        assert!(largest <= WINDOW_BYTES, "{file}: no whole upload ({largest})");
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
                return ResponseTemplate::new(400)
                    .set_body_json(json!({"detail":{"error_type":"max_tokens_exceeded"}}));
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
                "usage":{"input_tokens":100,"output_tokens":5},
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
    let limits = octocode_native::execution::RuntimeLimits {
        classification_timeout: Some(std::time::Duration::from_secs(3)),
        ..crate::support::PROVIDER_LIMITS
    };
    let runtime = workspace.runtime_with(&crate::support::provider_env(&server, &[]), limits);
    let row = json!({"operation":"evaluate",
        "source":{"tool":"localFetch","query":{"path":"big.txt"}},
        "questions":{"socket":noul("The text says the worker opens a socket.")}});
    let out = clasify(&runtime, vec![row]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["answers"]["socket"]["noul"], 0.96, "{out}");
    let coverage = &data["coverage"];
    let total = coverage["totalSections"].as_u64().unwrap();
    let answered = coverage["answered"].as_u64().unwrap();
    let rest = coverage["unassessed"].as_u64().unwrap_or(0) + coverage["error"].as_u64().unwrap_or(0);
    assert!(answered > 0 && rest > 0, "{out}");
    assert_eq!(answered + rest, total, "{out}");
    // The way to the rest: the same questions as a sections call.
    let detail = data["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "clasify")
        .expect("sections call");
    assert_eq!(detail["input"]["queries"][0]["operation"], "sections", "{out}");
    runtime.close().await;
}
