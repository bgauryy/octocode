//! Public clasify end to end: JEV questions over a literal state or a source
//! captured through the authorized readers, answered by a mock provider.

use crate::support::{Run as _, Workspace, clasify_runtime, github_api, provider_env};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Answers every question by its own type, as the provider does: Noul 0.9,
/// Choice the first label, Score 1.5 over its levels with an echoed legend.
#[derive(Clone)]
struct Typed;

impl Respond for Typed {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).expect("provider body");
        let answers = body["questions"]
            .as_object()
            .expect("questions")
            .iter()
            .map(|(id, question)| {
                let answer = match question["type"].as_str() {
                    Some("choice") => {
                        let labels = question["criteria"]
                            .as_object()
                            .expect("labels")
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>();
                        let probabilities = labels
                            .iter()
                            .enumerate()
                            .map(|(index, label)| {
                                let share = if index == 0 { 1.0 } else { 0.0 };
                                (label.clone(), json!(share))
                            })
                            .collect::<serde_json::Map<_, _>>();
                        json!({"type":"choice","choice":labels[0],"confidence":1,"probabilities":probabilities})
                    }
                    Some("score") => {
                        let levels = question["criteria"].as_array().expect("levels");
                        let legend = levels
                            .iter()
                            .enumerate()
                            .map(|(index, level)| (index.to_string(), level.clone()))
                            .collect::<serde_json::Map<_, _>>();
                        let mut probabilities = serde_json::Map::new();
                        for index in 0..levels.len() {
                            let share = match index {
                                1 | 2 => 0.5,
                                _ => 0.0,
                            };
                            probabilities.insert(index.to_string(), json!(share));
                        }
                        json!({"type":"score","score":1.5,"confidence":0.5,"probabilities":probabilities,"legend":legend})
                    }
                    _ => json!({"type":"noul","noul":0.9}),
                };
                (id.clone(), answer)
            })
            .collect::<serde_json::Map<_, _>>();
        ResponseTemplate::new(200).set_body_json(json!({
            "model":"jev-1.13.0",
            "answers":answers,
            "usage":{"input_tokens":40,"output_tokens":7}
        }))
    }
}

async fn provider(expected: u64) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(Typed)
        .expect(expected)
        .mount(&server)
        .await;
    server
}

/// Provider request bodies, in arrival order.
pub(crate) async fn sent(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect()
}

fn noul(instructions: &str) -> Value {
    json!({"type":"noul","instructions":instructions})
}

fn state_row() -> Value {
    json!({
        "id":"rotation",
        "mainGoal":"Decide whether rotation needs a restart.",
        "reasoning":"Metadata only.",
        "state":{"document":{"text":"Update the configuration, then restart the service."}},
        "questions":{"restart":noul("Does `document.text` require a restart?")}
    })
}

/// One clasify call over `rows`: its structured `{"queries": [...]}` result.
pub(crate) async fn clasify(
    runtime: &octocode_native::runtime::ToolRuntime,
    rows: Vec<Value>,
) -> Value {
    runtime
        .run(
            "clasify-test".into(),
            "clasify".into(),
            json!({"queries":rows}),
        )
        .await
        .expect("clasify result")
        .structured_content
}

#[tokio::test]
async fn retired_jev_vendor_key_does_not_enable_clasify() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_JEV_KEY", "jev-native-secret".into())]);
    assert!(!runtime.is_available("clasify"));
    runtime.close().await;
}

#[tokio::test]
async fn public_clasify_requires_beta_and_a_nonblank_key_independently() {
    let workspace = Workspace::new();
    for (env, missing) in [
        (
            vec![("OCTOCODE_CLASSIFICATION_API", "secret".into())],
            "OCTOCODE_BETA",
        ),
        (
            vec![("OCTOCODE_BETA", "true".into())],
            "OCTOCODE_CLASSIFICATION_API",
        ),
        (
            vec![
                ("OCTOCODE_BETA", "true".into()),
                ("OCTOCODE_CLASSIFICATION_API", "  ".into()),
            ],
            "OCTOCODE_CLASSIFICATION_API",
        ),
    ] {
        let runtime = workspace.runtime(&env);
        assert!(!runtime.is_available("clasify"), "{env:?}");
        let error = runtime
            .run(
                "missing-gate".into(),
                "clasify".into(),
                json!({"queries":[state_row()]}),
            )
            .await
            .expect_err("an unmet prerequisite blocks public clasify");
        assert_eq!(error.code, "missingConfiguration");
        assert!(error.message.contains(missing), "{}", error.message);
        runtime.close().await;
    }
}

#[tokio::test]
async fn configured_clasify_is_available_without_contacting_the_provider() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    assert!(runtime.is_available("clasify"));
    runtime.close().await;
}

#[tokio::test]
async fn a_state_row_sends_the_questions_unchanged_and_returns_provider_answers() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let mut row = state_row();
    row["questions"] = json!({
        "restart": {"type":"noul","instructions":{"question":"Requires `action`?","action":"restart"},
            "criteria":{"true":{"meaning":"yes"},"false":["no"]}},
        "route": {"type":"choice","instructions":"Which procedure?",
            "criteria":{"restart":"Restart.","hot":["No restart."],"unknown":null}},
        "support": {"type":"score","instructions":"How direct?",
            "criteria":[{"meaning":"Unrelated."},["Mentions it."],"Steps."]}
    });
    let out = clasify(&runtime, vec![row.clone()]).await;
    let request = &sent(&server).await[0];
    assert_eq!(request["state"], row["state"]);
    assert_eq!(request["model"], "jev-latest", "the configured default");
    // The questions reach the provider exactly as asked: no brief, preset or
    // renaming. Correlation keys are internal.
    let mut asked = request["questions"]
        .as_object()
        .unwrap()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    asked.sort_by_key(Value::to_string);
    let mut expected = row["questions"]
        .as_object()
        .unwrap()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    expected.sort_by_key(Value::to_string);
    assert_eq!(asked, expected);
    assert!(!request.to_string().contains("Metadata only"));
    assert!(!request.to_string().contains("Decide whether rotation"));

    let result = &out["queries"][0];
    assert_eq!(result["id"], "rotation");
    assert_eq!(result["model"], "jev-1.13.0");
    assert_eq!(
        result["usage"],
        json!({"input_tokens":40,"output_tokens":7})
    );
    assert_eq!(
        result["answers"]["restart"],
        json!({"type":"noul","noul":0.9})
    );
    assert_eq!(result["answers"]["route"]["type"], "choice");
    assert_eq!(result["answers"]["route"]["choice"], "restart");
    assert_eq!(
        result["answers"]["route"]["probabilities"],
        json!({"restart":1.0,"hot":0.0,"unknown":0.0})
    );
    assert_eq!(
        result["answers"]["support"]["score"], 1.5,
        "fractional, never rounded"
    );
    assert_eq!(
        result["answers"]["support"]["legend"],
        json!({"0":{"meaning":"Unrelated."},"1":["Mentions it."],"2":"Steps."})
    );
    assert!(
        result.get("hints").is_none(),
        "a state row has no source to read"
    );
    assert!(result.get("replayed").is_none());
    runtime.close().await;
}

#[tokio::test]
async fn a_row_model_and_the_configured_model_reach_the_provider() {
    let server = provider(2).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(
        &workspace,
        &server,
        &[("OCTOCODE_CLASSIFICATION_MODEL", "jev-1.13.0".into())],
    );
    let mut pinned = state_row();
    pinned["id"] = json!("pinned");
    pinned["model"] = json!("jev-1.14.0");
    clasify(&runtime, vec![state_row(), pinned]).await;
    let mut models = sent(&server)
        .await
        .iter()
        .map(|request| request["model"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    models.sort();
    assert_eq!(models, ["jev-1.13.0", "jev-1.14.0"]);
    runtime.close().await;
}

#[tokio::test]
async fn an_identical_evaluation_replays_without_a_provider_request() {
    let server = provider(2).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let first = clasify(&runtime, vec![state_row()]).await;
    let again = clasify(&runtime, vec![state_row()]).await;
    assert!(first["queries"][0].get("replayed").is_none());
    assert_eq!(again["queries"][0]["replayed"], true);
    assert_eq!(
        again["queries"][0]["answers"],
        first["queries"][0]["answers"]
    );
    assert_eq!(again["queries"][0]["usage"], first["queries"][0]["usage"]);
    // A different question is a different evaluation.
    let mut other = state_row();
    other["questions"] = json!({"restart":noul("Does `document.text` forbid a restart?")});
    let changed = clasify(&runtime, vec![other]).await;
    assert!(changed["queries"][0].get("replayed").is_none());
    assert_eq!(sent(&server).await.len(), 2);
    runtime.close().await;
}

#[tokio::test]
async fn identical_rows_in_one_call_share_one_provider_request() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let mut second = state_row();
    second["id"] = json!("again");
    let out = clasify(&runtime, vec![state_row(), second]).await;
    let rows = out["queries"].as_array().unwrap();
    assert_eq!(rows[0]["answers"], rows[1]["answers"]);
    assert_eq!(
        rows.iter()
            .filter(|row| row.get("replayed").is_some())
            .count(),
        1,
        "{out}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn persistent_judgments_serve_a_fresh_runtime_and_memory_mode_does_not() {
    let server = provider(3).await;
    let workspace = Workspace::new();
    let env = |mode: &str| [("OCTOCODE_STORAGE_MODE", mode.to_owned())];
    for (mode, replayed) in [
        ("persistent", None),
        ("persistent", Some(true)),
        ("memory", None),
        ("memory", None),
    ] {
        let runtime = clasify_runtime(&workspace, &server, &env(mode));
        let mut row = state_row();
        row["state"] = json!(format!(
            "judged under {}",
            if mode == "memory" { "memory" } else { "disk" }
        ));
        let out = clasify(&runtime, vec![row]).await;
        assert_eq!(
            out["queries"][0].get("replayed").cloned(),
            replayed.map(Value::from),
            "{mode}"
        );
        runtime.close().await;
    }
}

#[tokio::test]
async fn a_local_source_judges_exactly_its_ranges_and_leads_to_the_same_read() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let body = (1..=60)
        .map(|n| format!("line {n} {}\n", if n == 30 { "OUTSIDE" } else { "inside" }))
        .collect::<String>();
    workspace.write("docs/guide.md", &body);
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![json!({
            "source":{"tool":"localFetch","query":{"path":"docs/guide.md","ranges":["2-4","40-41"]}},
            "questions":{"inside":noul("Does the selection say inside?")}
        })],
    )
    .await;
    let state = &sent(&server).await[0]["state"];
    let text = state["text"].as_str().unwrap();
    for line in [2, 3, 4, 40, 41] {
        assert!(text.contains(&format!("line {line} inside")), "{text}");
    }
    for line in [1, 5, 30, 39, 42] {
        assert!(
            !text.contains(&format!("line {line} ")),
            "line {line} is outside: {text}"
        );
    }
    let result = &out["queries"][0];
    assert_eq!(result["answers"]["inside"]["noul"], 0.9);
    let read = &crate::support::checks::hint(result, "read");
    assert_eq!(read["tool"], "localFetch");
    let row = &read["query"]["queries"][0];
    assert_eq!(row["path"], "docs/guide.md");
    assert_eq!(row["ranges"], json!(["2-4", "40-41"]));
    assert!(row.get("minify").is_none(), "{row}");
    assert_eq!(row["snapshot"].as_str().map(str::len), Some(64), "{row}");
    assert!(
        !out.to_string().contains("line 3 inside"),
        "no source body is returned"
    );
    runtime.close().await;
}

/// R02: disjoint ranges stay intersected through every content page the
/// capture follows; no line outside them reaches the provider.
#[tokio::test]
async fn a_multi_page_capture_keeps_disjoint_ranges_on_every_page() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let body = (1..=1200)
        .map(|n| {
            let tag = if (401..600).contains(&n) || n > 1000 {
                "OUT"
            } else {
                "in"
            };
            format!("row {n:04} {tag} {}\n", "x".repeat(16))
        })
        .collect::<String>();
    workspace.write("data/rows.txt", &body);
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![json!({
            "source":{"tool":"localFetch","query":{"path":"data/rows.txt","ranges":["1-400","600-1000"]}},
            "questions":{"q":noul("Does every selected row say in?")}
        })],
    )
    .await;
    assert!(out["queries"][0].get("answers").is_some(), "{out}");
    let text = sent(&server).await[0]["state"]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!text.contains("OUT"), "an outside line was captured");
    for row in [1, 400, 600, 1000] {
        assert!(
            text.contains(&format!("row {row:04} in")),
            "row {row} missing"
        );
    }
    assert!(
        text.len() > 16 * 1024,
        "the selection spans more than one page"
    );
    runtime.close().await;
}

#[tokio::test]
async fn the_read_lead_returns_the_classified_capture() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    workspace.write("src/a.rs", "fn a() {}\nfn b() {}\nfn c() {}\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![json!({
            "source":{"tool":"localFetch","query":{"path":"src/a.rs","fullContent":true}},
            "questions":{"b":noul("Does the file define b?")}
        })],
    )
    .await;
    let read = crate::support::checks::hint(&out["queries"][0], "read").clone();
    let replay = runtime
        .run(
            "read-lead".into(),
            read["tool"].as_str().unwrap().into(),
            read["query"].clone(),
        )
        .await
        .expect("lead runs");
    let content = replay.structured_content["results"][0]["data"]["content"]
        .as_str()
        .unwrap();
    assert!(content.contains("fn b() {}"), "{content}");
    // A changed file no longer matches the captured snapshot.
    workspace.write("src/a.rs", "fn changed() {}\n");
    let stale = runtime
        .run(
            "stale-lead".into(),
            "localFetch".into(),
            read["query"].clone(),
        )
        .await
        .expect("typed row");
    assert_eq!(
        stale.structured_content["results"][0]["data"]["errorCode"], "staleSnapshot",
        "{}",
        stale.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn an_oversized_or_missing_source_fails_its_row_without_provider_work() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    workspace.write(
        "big.txt",
        "x".repeat(80) + &"\nfiller line of text".repeat(3000),
    );
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![
            json!({"id":"big","source":{"tool":"localFetch","query":{"path":"big.txt","fullContent":true}},
                "questions":{"q":noul("Is it big?")}}),
            json!({"id":"gone","source":{"tool":"localFetch","query":{"path":"missing.txt","ranges":["1-2"]}},
                "questions":{"q":noul("Is it there?")}}),
            state_row(),
        ],
    )
    .await;
    let rows = out["queries"].as_array().unwrap();
    assert_eq!(
        rows[0]["error"]["errorCode"], "classificationContextTooLarge",
        "{out}"
    );
    assert!(rows[0].get("answers").is_none());
    assert_eq!(rows[1]["error"]["errorCode"], "pathNotFound", "{out}");
    assert_eq!(
        rows[2]["answers"]["restart"]["noul"], 0.9,
        "a sibling keeps its answer"
    );
    assert_eq!(
        sent(&server).await.len(),
        1,
        "only the state row reached the provider"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_github_source_reads_its_requested_ref_and_pins_the_lead_to_that_commit() {
    let provider = provider(2).await;
    let github = MockServer::builder().start().await;
    let shas = [
        (
            "v1",
            "1111111111111111111111111111111111111111",
            "old body\n",
        ),
        (
            "v2",
            "2222222222222222222222222222222222222222",
            "new body\n",
        ),
    ];
    for (reference, sha, body) in shas {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/commits/{reference}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
            .mount(&github)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/contents/src%2Flib.rs"))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body),
                "size":body.len(),"sha":format!("{:0>40}", reference),"path":"src/lib.rs"
            })))
            .mount(&github)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &provider, &github_api(&github));
    let row = |reference: &str| {
        json!({"id":reference,"source":{"tool":"ghGetFileContent","query":{
            "owner":"a","repo":"b","path":"src/lib.rs","ref":reference,"fullContent":true}},
            "questions":{"q":noul("Does the file say body?")}})
    };
    let out = clasify(&runtime, vec![row("v1"), row("v2")]).await;
    let states = sent(&provider)
        .await
        .into_iter()
        .map(|request| request["state"].to_string())
        .collect::<Vec<_>>();
    assert!(
        states.iter().any(|state| state.contains("old body")),
        "{states:?}"
    );
    assert!(
        states.iter().any(|state| state.contains("new body")),
        "{states:?}"
    );
    for (index, (_, sha, _)) in shas.iter().enumerate() {
        let lead = &crate::support::checks::hint(&out["queries"][index], "read");
        assert_eq!(lead["tool"], "ghGetFileContent");
        assert_eq!(lead["query"]["queries"][0]["ref"], *sha, "{lead}");
        assert!(
            lead["query"]["queries"][0].get("minify").is_none(),
            "{lead}"
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_disabled_reader_is_rejected_at_validation_before_any_work() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&provider_env(
        &server,
        &[("DISABLE_TOOLS", "localFetch".into())],
    ));
    let error = runtime
        .run(
            "disabled-reader".into(),
            "clasify".into(),
            json!({"queries":[{"source":{"tool":"localFetch","query":{"path":"a.rs","ranges":["1-2"]}},
                "questions":{"q":noul("Anything?")}}]}),
        )
        .await
        .expect_err("rejected at validation");
    assert_eq!(error.code, "invalidInput", "{error:?}");
    let issues = error.validation_issues.expect("field issues");
    assert_eq!(issues[0].rule_id, "clasify.source-reader-available");
    assert_eq!(issues[0].path, ["queries", "0", "source"]);
    assert!(
        issues[0].message.contains("localFetch"),
        "{}",
        issues[0].message
    );
    runtime.close().await;
}

#[tokio::test]
async fn removed_matrix_input_is_rejected_not_converted() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    for legacy in [
        json!({"resources":[{"value":"x"}],"questions":[{"type":"yesno","ask":"Relevant?"}]}),
        json!({"state":"x","questions":{"q":{"type":"locate","instructions":"Where?"}}}),
        json!({"state":"x","questions":{"q":noul("x")},"resources":[{"value":"x"}]}),
    ] {
        let error = runtime
            .run(
                "legacy".into(),
                "clasify".into(),
                json!({"queries":[legacy.clone()]}),
            )
            .await
            .expect_err("legacy shape");
        assert_eq!(error.code, "invalidInput", "{legacy}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_provider_failure_fails_the_row_and_the_runtime_recovers() {
    let server = MockServer::builder().start().await;
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = attempts.clone();
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(move |request: &Request| {
            if counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                ResponseTemplate::new(401)
            } else {
                Typed.respond(request)
            }
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let failed = clasify(&runtime, vec![state_row()]).await;
    assert_eq!(
        failed["queries"][0]["error"]["errorCode"], "classificationProviderError",
        "{failed}"
    );
    assert!(failed["queries"][0].get("answers").is_none());
    let recovered = clasify(&runtime, vec![state_row()]).await;
    assert_eq!(recovered["queries"][0]["answers"]["restart"]["noul"], 0.9);
    assert!(
        recovered["queries"][0].get("replayed").is_none(),
        "a failure is never cached"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_provider_failure_keeps_the_pinned_source_inspection_call() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write(
        "src/evidence.txt",
        "RECOVERY_SOURCE_SENTINEL applies after restart.\n",
    );
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![json!({
            "source":{"tool":"localFetch","query":{"path":"src/evidence.txt","ranges":["1-1"]}},
            "questions":{"restart":noul("Does this state a restart requirement?")}
        })],
    )
    .await;
    let row = &out["queries"][0];
    assert_eq!(row["error"]["errorCode"], "classificationProviderError");
    assert!(row.get("answers").is_none());
    assert!(!out.to_string().contains("RECOVERY_SOURCE_SENTINEL"));
    let lead = &crate::support::checks::hint(&row["error"], "read");
    assert_eq!(lead["tool"], "localFetch", "{out}");
    assert_eq!(lead["query"]["queries"][0]["ranges"], json!(["1-1"]));
    assert!(lead["query"]["queries"][0]["snapshot"].as_str().is_some());
    assert!(
        crate::support::checks::hint(&row["error"], "text")
            .to_string()
            .contains("if still needed")
    );
    let read = runtime
        .run("inspect".into(), "localFetch".into(), lead["query"].clone())
        .await
        .expect("inspection call");
    assert!(
        read.structured_content
            .to_string()
            .contains("RECOVERY_SOURCE_SENTINEL")
    );
    runtime.close().await;
}

#[tokio::test]
async fn clasify_pages_complete_answers_once_and_replays_without_inference() {
    let server = provider(3).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let labels: serde_json::Map<_, _> = (0..64)
        .map(|index| (format!("label{index:02}"), json!("criterion")))
        .collect();
    let questions: serde_json::Map<_, _> = (0..12)
        .map(|index| {
            (
                format!("question{index:02}"),
                json!({"type":"choice","criteria":labels,"instructions":"Choose one label."}),
            )
        })
        .collect();
    let rows: Vec<_> = (0..3)
        .map(|index| {
            json!({"id":format!("row{index}"),
        "state":{"text":format!("Evidence {index}")},"questions":questions})
        })
        .collect();
    let first = runtime
        .run(
            "clasify-pages".into(),
            "clasify".into(),
            json!({"queries":rows,"responseLength":1800}),
        )
        .await
        .expect("first page");
    let mut page = first.structured_content;
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        assert!(pages < 60, "page chain must progress");
        for row in page["queries"].as_array().expect("query rows") {
            for (question, answer) in row["answers"].as_object().expect("answers") {
                assert_eq!(answer["probabilities"].as_object().unwrap().len(), 64);
                assert!(
                    seen.insert((row["id"].as_str().unwrap().to_owned(), question.clone())),
                    "duplicate answer"
                );
            }
        }
        pages += 1;
        let pagination = &page["responsePagination"];
        if pagination["hasMore"] != true {
            break;
        }
        let next = pagination["next"]["query"].clone();
        assert!(
            next.get("queries").is_none(),
            "large inputs must not be echoed"
        );
        let next_page = runtime
            .run(format!("page{pages}"), "clasify".into(), next.clone())
            .await
            .expect("page");
        let repeated = runtime
            .run(format!("repeat{pages}"), "clasify".into(), next)
            .await
            .expect("repeat");
        assert_eq!(next_page.structured_content, repeated.structured_content);
        assert_eq!(
            next_page.structured_content["responsePagination"]["replayed"],
            true
        );
        page = next_page.structured_content;
    }
    assert_eq!(seen.len(), 36);
    assert!(pages > 1);
    assert_eq!(sent(&server).await.len(), 3);
    runtime.close().await;
}

/// A context selection that outlives its deadline (the constant
/// `RuntimeLimits::selection.timeout`, here 100 ms through the test seam)
/// discloses the gap instead of returning raw evidence: the receipt is
/// partial or unavailable, the unjudged evidence stays unknown with no body,
/// and `next.unfiltered` recovers it without the provider. Replaces the
/// read-tool-selection harness case that set OCTOCODE_CLASSIFICATION_TIMEOUT_MS.
#[tokio::test]
async fn a_selection_past_its_deadline_withholds_bodies_and_recovers() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            // A decisive answer: every question at its top level.
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .iter()
                .map(|(id, question)| {
                    let levels = question["criteria"].as_array().expect("levels");
                    let top = levels.len() - 1;
                    let legend = levels
                        .iter()
                        .enumerate()
                        .map(|(index, level)| (index.to_string(), level.clone()))
                        .collect::<serde_json::Map<_, _>>();
                    let probabilities = (0..levels.len())
                        .map(|index| (index.to_string(), json!(if index == top { 1.0 } else { 0.0 })))
                        .collect::<serde_json::Map<_, _>>();
                    (id.clone(), json!({"type":"score","score":top as f64,"confidence":1.0,
                        "legend":legend,"probabilities":probabilities}))
                })
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200)
                .set_body_json(json!({"model":"m","answers":answers,"usage":{"input_tokens":1,"output_tokens":1}}))
                .set_delay(std::time::Duration::from_millis(400))
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write(
        "src/rule.txt",
        "UNKNOWN_SELECTION_SENTINEL cancellation removes a queued request.\n",
    );
    let read = |timeout: std::time::Duration| {
        let limits = octocode_native::execution::RuntimeLimits {
            selection: octocode_native::execution::SelectionLimits {
                timeout,
                ..crate::support::PROVIDER_LIMITS.selection
            },
            ..crate::support::PROVIDER_LIMITS
        };
        let runtime = workspace.runtime_with(&provider_env(&server, &[]), limits);
        let query = json!({"path": file, "clasifyContext":{"classifiers":["Does this state the cancellation rule?"]}});
        async move {
            let row = crate::support::run(&runtime, "localFetch", query, &[]).await;
            runtime.close().await;
            row.data().clone()
        }
    };
    let late = read(std::time::Duration::from_millis(100)).await;
    let selection = &late["selection"];
    assert_eq!(selection["status"], "partial", "{late}");
    assert_eq!(selection["unknownItems"], 1, "{late}");
    assert_eq!(selection["selectedItems"], 0, "{late}");
    let warnings = late["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "one notice: {late}");
    assert!(
        warnings[0]
            .as_str()
            .is_some_and(|notice| notice.contains("next.unfiltered")),
        "{late}"
    );
    assert!(
        !late.to_string().contains("UNKNOWN_SELECTION_SENTINEL"),
        "{late}"
    );
    let unfiltered = late["next"]["unfiltered"].clone();
    assert_eq!(unfiltered["tool"], "localFetch", "{late}");
    let recovery = workspace.runtime(&[]);
    let recovered = recovery
        .run(
            "unfiltered".into(),
            "localFetch".into(),
            unfiltered["query"].clone(),
        )
        .await
        .expect("unfiltered recovery");
    assert!(
        recovered
            .structured_content
            .to_string()
            .contains("UNKNOWN_SELECTION_SENTINEL"),
        "{}",
        recovered.structured_content
    );
    recovery.close().await;
    // The same read under the constant deadline is judged and selected: the
    // deadline, not the evidence, left it unknown.
    let judged = read(
        octocode_native::execution::RuntimeLimits::DEFAULT
            .selection
            .timeout,
    )
    .await;
    assert_eq!(judged["selection"]["status"], "complete", "{judged}");
    assert_eq!(judged["selection"]["selectedItems"], 1, "{judged}");
    assert!(
        judged["content"]
            .as_str()
            .unwrap()
            .contains("UNKNOWN_SELECTION_SENTINEL"),
        "{judged}"
    );
}

/// A provider that is down (503 without a retry delay) cannot judge the
/// search, but the search succeeded: its hits come back unfiltered under an
/// `unavailable` receipt and a warning naming the failure, unlike a refused
/// credential (below), which the caller must fix and so stays the row's
/// error.
#[tokio::test]
async fn a_provider_that_is_down_under_clasify_context_returns_the_unfiltered_hits() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("src/a.rs", "fn recovery() {}\nfn other() { recovery(); }\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = crate::support::run(
        &runtime,
        "localSearch",
        json!({"path": file.parent().unwrap(), "matchString": "recovery",
            "clasifyContext": {"classifiers": ["Does it add hints?"]}}),
        &[],
    )
    .await;
    let data = row.data();
    assert_ne!(row.status(), "error", "{data}");
    assert_eq!(data["selection"]["status"], "unavailable", "{data}");
    assert_eq!(
        data["selection"]["reason"], "classificationProviderUnavailable",
        "{data}"
    );
    let text = data.to_string();
    assert!(
        text.contains("fn recovery() {}"),
        "every hit is shown: {data}"
    );
    assert!(text.contains("other() { recovery(); }"), "{data}");
    assert!(
        text.contains("HTTP 503"),
        "the warning names the failure: {data}"
    );
    runtime.close().await;
}

/// The same for a read and a provider page: localFetch, ghGetFileContent
/// and ghSearchCode show their evidence unfiltered under an `unavailable`
/// receipt and a warning naming the failure, with no unfiltered lead that
/// repeats it.
#[tokio::test]
async fn a_provider_that_is_down_under_clasify_context_shows_reads_and_code_pages_unfiltered() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let github = MockServer::builder().start().await;
    let sha = "1111111111111111111111111111111111111111";
    let body = "fn recovery() {}\nfn other() { recovery(); }\n";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fa.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode(body),
            "size":body.len(),"sha":"2".repeat(40),"path":"src/a.rs"
        })))
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"a.rs","path":"src/a.rs","sha":"2","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"fn recovery() {}","matches":[{"text":"recovery","indices":[3,11]}]}]}
            ]
        })))
        .mount(&github)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("src/a.rs", body);
    let runtime = clasify_runtime(
        &workspace,
        &server,
        &[("GITHUB_API_URL", format!("{}/api/v3", github.uri()))],
    );
    let context = json!({"classifiers": ["Does it add hints?"]});
    for (tool, query, shown) in [
        (
            "localFetch",
            json!({"path": file, "fullContent": true, "clasifyContext": context}),
            "other() { recovery(); }",
        ),
        (
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"src/a.rs","ref":"main",
                "fullContent": true, "clasifyContext": context}),
            "other() { recovery(); }",
        ),
        (
            "ghSearchCode",
            json!({"owner":"a","repo":"b","keywords":["recovery"], "clasifyContext": context}),
            "src/a.rs",
        ),
    ] {
        let row = crate::support::run(&runtime, tool, query, &[]).await;
        let data = row.data();
        assert_ne!(row.status(), "error", "{tool}: {data}");
        assert_eq!(data["selection"]["status"], "unavailable", "{tool}: {data}");
        assert_eq!(
            data["selection"]["reason"], "classificationProviderUnavailable",
            "{tool}: {data}"
        );
        assert!(
            data.to_string().contains(shown),
            "{tool}: the evidence is shown: {data}"
        );
        assert!(
            data["warnings"].to_string().contains("HTTP 503"),
            "{tool}: the warning names the failure: {data}"
        );
        assert!(
            data["next"].get("unfiltered").is_none(),
            "{tool}: no lead repeats the page: {data}"
        );
    }
    runtime.close().await;
}

/// A provider that refuses the credential is not an unsure model: under
/// clasifyContext the row fails with the provider's typed error, its
/// credential hint, an `unavailable` receipt, and the unfiltered lead, and
/// the call exits as a failure instead of success.
#[tokio::test]
async fn a_refused_credential_under_clasify_context_fails_the_row_with_its_error() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("src/a.rs", "fn recovery() {}\nfn other() { recovery(); }\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = crate::support::run(
        &runtime,
        "localSearch",
        json!({"path": file.parent().unwrap(), "matchString": "recovery",
            "clasifyContext": {"classifiers": ["Does it add hints?"]}}),
        &[],
    )
    .await;
    assert_eq!(row.status(), "error", "{}", row.data());
    let data = row.data();
    assert_eq!(data["errorCode"], "classificationProviderError", "{data}");
    assert!(
        data["hints"]
            .to_string()
            .contains("OCTOCODE_CLASSIFICATION_API"),
        "{data}"
    );
    assert_eq!(data["selection"]["status"], "unavailable", "{data}");
    assert_eq!(
        data["selection"]["reason"], "classificationProviderError",
        "{data}"
    );
    assert!(data["next"]["unfiltered"].is_object(), "{data}");
    assert_ne!(
        row.outcome.exit_class(),
        octocode_native::response::exit::ExitClass::Success,
        "{data}"
    );
    runtime.close().await;
}
