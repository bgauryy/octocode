#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use crate::support;

use serde_json::json;
use std::time::{Duration, Instant};
use support::{LocateTopPassage, MOCK_PROVIDER_TIMEOUT_MS, Workspace, provider_runtime};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// No page and no lead: `next` is absent and `hints` holds at most prose.
fn no_continuation(value: &serde_json::Value) -> bool {
    value.get("next").is_none()
        && value
            .get("hints")
            .and_then(serde_json::Value::as_object)
            .is_none_or(|hints| hints.keys().all(|key| key == "text"))
}

/// The read lead of a clasify resource (`hints.read` or `next.read`), on the
/// resource or its first page.
fn resource_read(resource: &serde_json::Value) -> Option<serde_json::Value> {
    ["", "/pages/0"]
        .iter()
        .flat_map(|at| [format!("{at}/hints/read"), format!("{at}/next/read")])
        .find_map(|pointer| resource.pointer(&pointer).cloned())
}

/// The single row of a lead query (`{queries:[row]}` or a bare row).
fn lead_row(query: &serde_json::Value) -> serde_json::Value {
    query.pointer("/queries/0").unwrap_or(query).clone()
}

#[derive(Clone)]
struct DelayedJevResponse {
    arrivals: std::sync::Arc<std::sync::Mutex<Vec<Instant>>>,
}

impl Respond for DelayedJevResponse {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Instant::now());
        ResponseTemplate::new(200)
            .set_delay(Duration::from_millis(200))
            .set_body_json(json!({
                "model":"resolved",
                "answers":{"answer":{"type":"noul","noul":0.8}},
                "usage":{"input_tokens":2,"output_tokens":1}
            }))
    }
}

/// Aperiodic ASCII letters: byte pages of it differ, so identical-page
/// deduplication does not merge the pages a paging test counts.
fn filler(len: usize) -> String {
    let mut seed = 0x2545_f491_u32;
    (0..len)
        .map(|_| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            char::from(b'a' + ((seed >> 16) % 26) as u8)
        })
        .collect()
}

/// First line of a read query's single `ranges` window ("a-b").
fn range_start(query: &serde_json::Value) -> u64 {
    let window = query["ranges"][0]
        .as_str()
        .unwrap_or_else(|| panic!("ranges: {query}"));
    window.split_once('-').unwrap().0.parse().unwrap()
}

/// The call input for one matrix (`{queries:[matrix]}`); an input that is
/// already a call passes through.
fn call(matrix: serde_json::Value) -> serde_json::Value {
    if matrix.get("queries").is_some() {
        matrix
    } else {
        json!({"queries":[matrix]})
    }
}

/// The full per-page receipt (`debug:true`): these tests inspect page
/// internals (scopes, sources, runner-up matches) the default output compacts.
fn verbose(input: serde_json::Value) -> serde_json::Value {
    let mut input = call(input);
    if let Some(queries) = input
        .get_mut("queries")
        .and_then(serde_json::Value::as_array_mut)
    {
        queries
            .iter_mut()
            .for_each(|query| query["debug"] = json!(true));
    }
    input
}

/// A default-output answer: on the resource when it has one plain page,
/// else on its first page.
fn compact_answer(resource: &serde_json::Value, id: &str) -> serde_json::Value {
    resource["answers"]
        .get(id)
        .or_else(|| resource["pages"][0]["answers"].get(id))
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

fn query() -> serde_json::Value {
    json!({
        "id":"decision",
        "reasoning":"Choose the next inspection.",
        "mainGoal":"Decide the next read.",
        "resources":[{"id":"observed","value":{"fact":"present"}}],
        "questions":[
            {"id":"relevant","type":"yesno","ask":"Is it relevant?"},
            {"id":"risk","type":"score","ask":{"prompt":"Rate risk"},"labels":["low",{"label":"high"}]}
        ]
    })
}

fn noul_response(noul: f64) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "model":"resolved",
        "answers":{"answer":{"type":"noul","noul":noul}},
        "usage":{"input_tokens":2,"output_tokens":1}
    }))
}

/// Provider request states, in arrival order.
async fn sent_states(server: &MockServer) -> Vec<serde_json::Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["state"].clone()
        })
        .collect()
}

/// Three files of `matches` hit lines each, every line `width` characters and
/// tagged with its file's marker.
fn write_hit_files(workspace: &Workspace, matches: usize, width: usize) -> String {
    let mut root = None;
    for (index, marker) in ["ALPHA", "BRAVO", "CHARLIE"].iter().enumerate() {
        let body = (0..matches)
            .map(|line| {
                let text = format!("needle {marker} {line} ");
                format!("{text}{}\n", filler(width - text.len()))
            })
            .collect::<String>();
        let path = workspace.write(&format!("hits/file{index}.txt"), body);
        root = path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned());
    }
    root.unwrap()
}

/// Lines 1–8 are short, 9–16 long enough that their 8-line page exceeds the
/// whole budget while half of it fits, then short lines resume.
fn uneven_lines() -> String {
    (1..=24)
        .map(|line| {
            if (9..=16).contains(&line) {
                format!("L{line:02} {}\n", filler(86))
            } else {
                format!("L{line:02} short\n")
            }
        })
        .collect()
}

/// Every continuation in a clasify output: `(where, tool, query)` for each
/// `next.*` page and `hints.*` lead. Prose `hints.text` is not a call.
fn clasify_continuations(
    value: &serde_json::Value,
    at: &str,
    out: &mut Vec<(String, String, serde_json::Value)>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let here = format!("{at}.{key}");
                if key != "next" && key != "hints" {
                    clasify_continuations(child, &here, out);
                    continue;
                }
                for (name, entry) in child.as_object().into_iter().flatten() {
                    let path = format!("{here}.{name}");
                    if name == "text" {
                        continue;
                    }
                    if let (Some(tool), Some(query)) = (entry["tool"].as_str(), entry.get("query"))
                    {
                        out.push((path, tool.to_owned(), query.clone()));
                    } else if name == "clasify" {
                        out.push((path, "clasify".to_owned(), entry.clone()));
                    } else {
                        panic!("{path} is neither a call nor a clasify walk: {entry}");
                    }
                }
            }
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                clasify_continuations(item, &format!("{at}[{index}]"), out);
            }
        }
        _ => {}
    }
}

/// Runs every continuation of `output` exactly as emitted and returns the
/// next walk step, if any. A caller that sent no brief gets none back.
async fn replay_every_continuation(
    runtime: &octocode_native::runtime::ToolRuntime,
    output: &serde_json::Value,
    briefed: bool,
    kinds: &mut std::collections::BTreeSet<String>,
) -> Option<serde_json::Value> {
    let mut found = Vec::new();
    clasify_continuations(output, "", &mut found);
    let mut walk = None;
    for (at, tool, query) in found {
        let text = query.to_string();
        if !briefed {
            assert!(
                !text.contains("\"mainGoal\"") && !text.contains("\"reasoning\""),
                "{at} carries a brief the caller did not send: {text}"
            );
        }
        let kind = at
            .rsplit_once('.')
            .map_or(at.as_str(), |(scope, name)| {
                if scope.contains("best") || scope.contains("pages") {
                    "row-or-page"
                } else {
                    name
                }
            })
            .to_owned();
        kinds.insert(format!("{kind}:{tool}"));
        let input = if tool == "clasify" {
            call(query.clone())
        } else {
            json!({"queries":[query.clone()]})
        };
        let outcome = runtime
            .execute(format!("replay{at}"), tool.clone(), input)
            .await
            .unwrap_or_else(|error| panic!("{at} does not run verbatim: {error:?}: {text}"));
        let content = outcome.structured_content.to_string();
        assert!(
            outcome.failure.is_none() && !outcome.all_failed && !content.contains("\"errorCode\""),
            "{at} failed on replay: {content}"
        );
        if at.ends_with("next.clasify") {
            walk = Some(query);
        }
    }
    walk
}

#[tokio::test]
async fn clasify_sends_optional_briefs_on_the_provider_state() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.8}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        // The first call, then one state without reasoning and one without
        // mainGoal; repeats of a state replay the judgment cache.
        .expect(3)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("trace.txt", "Evidence is present.\n");
    let runtime = provider_runtime(&workspace, &server);
    let mut input = query();
    input["resources"] = json!([{"id":"source","tool":"localFetch","query":{"path":file}}]);
    input["questions"] = json!([{"id":"relevant","type":"yesno","ask":"Is evidence present?"}]);
    input["reasoning"] = json!("The next read depends on whether the file states the fact.");
    input["mainGoal"] = json!("Files that state whether the evidence is present.");
    let outcome = runtime
        .execute(
            "semantic-reasoning".into(),
            "clasify".into(),
            verbose(input.clone()),
        )
        .await
        .expect("required briefs reach the provider");
    assert_eq!(
        outcome.structured_content["queries"][0]["resources"][0]["pages"][0]["answers"]["relevant"]
            ["yesno"],
        0.8,
        "{}",
        outcome.structured_content
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body = String::from_utf8(requests[0].body.clone()).unwrap();
    assert!(
        body.contains("The next read depends on whether the file states the fact."),
        "reasoning must reach the provider state: {body}"
    );
    assert!(
        body.contains("Files that state whether the evidence is present."),
        "goal must reach the provider state: {body}"
    );
    // A blank or missing brief is dropped, not rejected; a non-string one
    // is still invalid.
    for (field, value) in [
        ("reasoning", json!("")),
        ("reasoning", json!("   ")),
        ("mainGoal", json!("")),
        ("mainGoal", json!("   ")),
    ] {
        let mut blank = input.clone();
        blank[field] = value;
        runtime
            .execute("semantic-reasoning".into(), "clasify".into(), call(blank))
            .await
            .expect("a blank brief is dropped");
    }
    for field in ["reasoning", "mainGoal"] {
        let mut missing = input.clone();
        missing.as_object_mut().unwrap().remove(field);
        runtime
            .execute("semantic-reasoning".into(), "clasify".into(), call(missing))
            .await
            .expect("a missing brief is accepted");
        let mut invalid = input.clone();
        invalid[field] = serde_json::Value::Null;
        let error = runtime
            .execute("semantic-reasoning".into(), "clasify".into(), call(invalid))
            .await
            .expect_err("a null brief is rejected");
        assert_eq!(error.code, "invalidInput");
    }
    runtime.close().await;
}

#[tokio::test]
async fn provider_byte_limit_forwards_reasoning_and_isolates_oversized_questions() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.8}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(26)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = provider_runtime(&workspace, &server);
    let mut ordinary = query();
    ordinary["questions"] = json!([{"id":"relevant","type":"yesno","ask":"Is evidence present?"}]);
    let mut trace = ordinary.clone();
    trace["id"] = json!("trace");
    trace["reasoning"] = json!("Bound each captured resource before the next read.");
    // The matrix exceeds 4 MiB, but each independently captured resource and
    // provider request is within its own budget.
    trace["resources"] = json!((0..25)
        .map(|index| json!({"id":format!("source-{index}"),"value":format!("{index}{}", "界".repeat(60_000))}))
        .collect::<Vec<_>>());
    let mut oversized = ordinary.clone();
    oversized["id"] = json!("oversized");
    oversized["questions"][0]["ask"] = json!(
        (0..500)
            .map(|index| (format!("section-{index}"), json!("evidence ".repeat(1000))))
            .collect::<serde_json::Map<_, _>>()
    );
    let outcome = runtime
        .execute(
            "provider-byte-limit".into(),
            "clasify".into(),
            verbose(json!({"queries":[trace, oversized, ordinary]})),
        )
        .await
        .expect("one oversized provider request must not fail the matrix batch");
    let queries = &outcome.structured_content["queries"];
    for index in [0, 2] {
        assert_eq!(
            queries[index]["resources"][0]["pages"][0]["answers"]["relevant"]["yesno"], 0.8,
            "{}",
            outcome.structured_content
        );
    }
    let oversized_resource = &queries[1]["resources"][0];
    assert_eq!(oversized_resource["coverage"], "error");
    let error = &oversized_resource["pages"][0]["answers"]["relevant"]["error"];
    assert_eq!(error["errorCode"], "invalidClassificationRequest");
    assert!(error["error"].as_str().unwrap().contains("4 MiB"));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 26);
    for request in requests {
        let body = String::from_utf8(request.body).unwrap();
        assert!(
            body.contains("Bound each captured resource before the next read.")
                || body.contains("Choose the next inspection."),
            "reasoning must reach the provider state"
        );
        assert!(body.len() < 200_000);
    }
    runtime.close().await;
}

#[tokio::test]
async fn retired_jev_vendor_key_does_not_enable_clasify() {
    // OCTOCODE_CLASSIFICATION_API is the only classification credential.
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_JEV_KEY", "jev-native-secret".into())]);
    assert!(!runtime.is_available("clasify"));
    runtime.close().await;
}

#[tokio::test]
async fn public_identity_is_a_hard_cutover_and_missing_key_is_actionable() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    assert!(!runtime.is_available("clasify"));
    assert!(!runtime.is_available("jev"));
    let catalog = runtime.catalog().unwrap();
    assert!(
        catalog["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| { tool["name"] == "clasify" && tool["available"] == false })
    );
    assert!(
        !catalog["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "jev")
    );

    let error = runtime
        .execute("missing-key".into(), "clasify".into(), call(query()))
        .await
        .unwrap_err();
    assert_eq!(error.code, "missingConfiguration");
    assert!(error.message.contains("OCTOCODE_CLASSIFICATION_API"));
    assert!(
        error
            .message
            .contains("https://docs.typesafe.ai/introduction")
    );
    runtime.close().await;
}

fn clasify_catalog_entry(runtime: &octocode_native::runtime::ToolRuntime) -> serde_json::Value {
    runtime.catalog().unwrap()["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "clasify")
        .cloned()
        .unwrap()
}

#[tokio::test]
async fn startup_probe_keeps_clasify_when_the_provider_answers() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(noul_response(0.9))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = provider_runtime(&workspace, &server);
    let probe = runtime.probe_classification().await;
    assert_eq!(probe, json!({"probed":true,"available":true}));
    assert!(runtime.is_available("clasify"));
    assert_eq!(clasify_catalog_entry(&runtime)["available"], true);
    runtime.close().await;
}

#[tokio::test]
async fn startup_probe_disables_clasify_when_the_provider_rejects_it() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = provider_runtime(&workspace, &server);
    let probe = runtime.probe_classification().await;
    assert_eq!(probe["probed"], true);
    assert_eq!(probe["available"], false);
    assert_eq!(probe["code"], "classificationProviderError");
    assert!(!runtime.is_available("clasify"));
    let entry = clasify_catalog_entry(&runtime);
    assert_eq!(entry["available"], false);
    assert_eq!(entry["unavailableReason"], "providerUnreachable");
    let error = runtime
        .execute("after-probe".into(), "clasify".into(), call(query()))
        .await
        .unwrap_err();
    assert_eq!(error.code, "toolUnavailable");
    runtime.close().await;
}

#[tokio::test]
async fn startup_probe_disables_clasify_when_the_provider_is_unreachable() {
    let workspace = Workspace::new();
    // Bind then drop a listener: the port refuses connections.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        (
            "OCTOCODE_CLASSIFICATION_API_HOST",
            format!("http://127.0.0.1:{port}"),
        ),
    ]);
    let started = Instant::now();
    let probe = runtime.probe_classification().await;
    assert!(started.elapsed() < Duration::from_secs(6), "{probe}");
    assert_eq!(probe["available"], false, "{probe}");
    assert!(!runtime.is_available("clasify"));
    runtime.close().await;
}

#[tokio::test]
async fn startup_probe_keeps_clasify_when_only_rate_limited() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = provider_runtime(&workspace, &server);
    let probe = runtime.probe_classification().await;
    assert_eq!(probe["available"], true, "{probe}");
    assert_eq!(probe["code"], "classificationRateLimited");
    assert!(runtime.is_available("clasify"));
    runtime.close().await;
}

#[tokio::test]
async fn startup_probe_is_skipped_without_a_key() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    assert_eq!(
        runtime.probe_classification().await,
        json!({"probed":false,"available":false})
    );
    runtime.close().await;
}

#[tokio::test]
async fn matrix_is_resource_major_without_agent_telemetry_or_duplicate_text() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"provider-resolved",
            "answers":{
                "answer_0":{"type":"noul","noul":0.9},
                "answer_1":{"type":"score","score":0.75,"confidence":0.8,"probabilities":{"0":0.25,"1":0.75},"legend":{"0":"low","1":{"label":"high"}}}
            },
            "usage":{"input_tokens":12,"output_tokens":3}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = provider_runtime(&workspace, &server);
    let outcome = runtime
        .execute("matrix".into(), "clasify".into(), call(query()))
        .await
        .unwrap();
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("nested output contract");
    assert!(
        outcome.content.is_empty(),
        "clasify must not duplicate structured hints as text"
    );
    let queries = outcome.structured_content["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0]["id"], "decision");
    assert!(queries[0].get("model").is_none());
    assert!(queries[0].get("usage").is_none());
    // Default output: a supplied value is one plain page, so its answers sit
    // on the resource; complete coverage is implied.
    assert_eq!(
        queries[0]["resources"],
        json!([{"id":"observed","answers":{
            "relevant":0.9,
            "risk":{"score":0.75,"confidence":0.8,"probabilities":{"0":0.25,"1":0.75}}
        }}]),
        "{}",
        outcome.structured_content
    );
    let text = outcome.structured_content.to_string();
    for redundant in [
        "requestedModel",
        "resolvedModel",
        "resultHash",
        "legend",
        "\"type\"",
    ] {
        assert!(!text.contains(redundant), "{redundant} in {text}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn independent_resource_assessments_are_dispatched_concurrently() {
    let server = MockServer::builder().start().await;
    let arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(DelayedJevResponse {
            arrivals: arrivals.clone(),
        })
        .expect(4)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"concurrent-resources",
        "reasoning":"Assess independent resources without serial provider latency.","mainGoal":"Decide the next read.",
        "resources":(0..4).map(|index| json!({
            "id":format!("resource-{index}"),
            "value":{"index":index}
        })).collect::<Vec<_>>(),
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });

    let outcome = runtime
        .execute("concurrent-resources".into(), "clasify".into(), call(input))
        .await
        .unwrap();
    {
        let arrivals = arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(arrivals.len(), 4);
        assert!(
            arrivals.last().unwrap().duration_since(arrivals[0]) < Duration::from_millis(150),
            "provider requests were dispatched serially: {arrivals:?}"
        );
    }
    let results = outcome.structured_content["queries"][0]["resources"]
        .as_array()
        .unwrap();
    assert_eq!(results.len(), 4);
    // Default output: complete coverage is implied and a single page's
    // answers sit on the resource.
    for (index, result) in results.iter().enumerate() {
        assert_eq!(result["id"], format!("resource-{index}"));
        assert!(result.get("coverage").is_none(), "{result}");
        assert!(result["answers"]["relevant"].is_number(), "{result}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn classification_max_concurrency_bounds_provider_requests_in_flight() {
    let server = MockServer::builder().start().await;
    let arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(DelayedJevResponse {
            arrivals: arrivals.clone(),
        })
        .expect(8)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // Limit 4 → one call may hold at most 3 permits (fairness cap).
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("OCTOCODE_CLASSIFICATION_CONCURRENCY", "4".into()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"bounded-resources",
        "reasoning":"Assess many resources without exceeding provider concurrency.","mainGoal":"Decide the next read.",
        "resources":(0..8).map(|index| json!({
            "id":format!("resource-{index}"),
            "value":{"index":index}
        })).collect::<Vec<_>>(),
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });
    let outcome = runtime
        .execute("bounded-resources".into(), "clasify".into(), call(input))
        .await
        .unwrap();
    let arrivals = arrivals
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    assert_eq!(arrivals.len(), 8);
    // Each response takes 200ms and a permit is reused only after its
    // response, so arrivals closer than 200ms were in flight together.
    let overlapping = arrivals
        .iter()
        .map(|start| {
            arrivals
                .iter()
                .filter(|other| {
                    *other >= start && other.duration_since(*start) < Duration::from_millis(180)
                })
                .count()
        })
        .max()
        .unwrap();
    assert!(
        (2..=3).contains(&overlapping),
        "expected 2..=3 concurrent provider requests, saw {overlapping}: {arrivals:?}"
    );
    let results = outcome.structured_content["queries"][0]["resources"]
        .as_array()
        .unwrap();
    assert!(
        results
            .iter()
            .all(|result| result.get("coverage").is_none()
                && result["answers"]["relevant"].is_number()),
        "{results:?}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn independent_query_matrices_are_dispatched_concurrently() {
    let server = MockServer::builder().start().await;
    let arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(DelayedJevResponse {
            arrivals: arrivals.clone(),
        })
        .expect(4)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = provider_runtime(&workspace, &server);
    let queries = (0..4)
        .map(|index| {
            json!({
                "id":format!("query-{index}"),
                "reasoning":"Assess an independent matrix without serial provider latency.","mainGoal":"Decide the next read.",
                "resources":[{"id":"resource","value":{"index":index}}],
                "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
            })
        })
        .collect::<Vec<_>>();

    let outcome = runtime
        .execute(
            "concurrent-matrices".into(),
            "clasify".into(),
            json!({"queries":queries}),
        )
        .await
        .unwrap();
    {
        let arrivals = arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(arrivals.len(), 4);
        assert!(
            arrivals.last().unwrap().duration_since(arrivals[0]) < Duration::from_millis(150),
            "independent matrices were dispatched serially: {arrivals:?}"
        );
    }
    let queries = outcome.structured_content["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 4);
    for (index, query) in queries.iter().enumerate() {
        assert_eq!(query["id"], format!("query-{index}"));
        let resource = &query["resources"][0];
        assert!(resource.get("coverage").is_none(), "{resource}");
        assert!(resource["answers"]["relevant"].is_number(), "{resource}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn oversized_first_page_is_not_classified_or_given_a_looping_continuation() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"bounded",
        "reasoning":"Bound the supplied resource.","mainGoal":"Decide the next read.",
        "resources":[{"id":"large","value":{"text":"far too large ".repeat(6_000)}}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });
    let outcome = runtime
        .execute("bounded".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    assert_eq!(query["resources"][0]["coverage"], "error");
    assert_eq!(
        query["resources"][0]["pages"][0]["error"]["errorCode"],
        "classificationContextTooLarge"
    );
    assert!(query["resources"][0]["pages"][0].get("answers").is_none());
    assert!(no_continuation(query));
    runtime.close().await;
}

#[tokio::test]
async fn max_chars_budgets_sanitized_resource_payload_not_serialized_envelope() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.8}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(5)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let marker = "FOURTH_PAGE_MARKER";
    let file = workspace.write(
        "large.txt",
        format!("{}{}", filler(78_377 - marker.len()), marker),
    );
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"recover-full-content",
        "reasoning":"Recover the exact pages of an oversized whole-file request.","mainGoal":"Decide the next read.",
        "resources":[{"id":"file","maxChars":80_000,"tool":"localFetch","query":{
            "path":file,"mainGoal": "test", "reasoning":"Read the complete file.","fullContent":true
        }}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });
    let outcome = runtime
        .execute(
            "recover-full-content".into(),
            "clasify".into(),
            verbose(input),
        )
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    assert!(no_continuation(query), "{query}");
    let cell = &query["resources"][0];
    assert_eq!(cell["coverage"], "complete", "{cell}");
    let pages = cell["pages"].as_array().unwrap();
    // 16 KiB pages already exceed half the 24 KiB coalescing budget, so each
    // stays its own scoped judgment; small line pages merge instead.
    assert_eq!(pages.len(), 5, "{cell}");
    for page in pages {
        assert!(page.get("answers").is_some(), "{page}");
        assert!(page.get("limitations").is_none(), "{page}");
    }
    let last = &pages[pages.len() - 1]["scope"];
    assert_eq!(last["endLine"], last["totalLines"], "{cell}");
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .any(|request| String::from_utf8_lossy(&request.body).contains(marker)),
        "the terminal source marker must reach semantic assessment"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("recovered semantic output contract");
    runtime.close().await;
}

#[tokio::test]
async fn scout_expands_explicit_question_type_and_preserves_source_identity() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved", "answers":{"answer":{"type":"noul","noul":0.82}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("hooks.md", "preClose runs before active requests finish.\n");
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"novelty", "reasoning":"Decide whether this unread section adds evidence.","mainGoal":"Decide the next read.",
        "resources":[{"id":"hooks","tool":"localFetch","query":{"path":file,"mainGoal": "test", "reasoning":"Screen the complete section."}}],
        "questions":[{"id":"new","type":"adds","ask":"Shutdown timing", "known":["onClose runs after requests finish"]}]
    });
    let result = runtime
        .execute("preset".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let output = &result.structured_content["queries"][0];
    assert!(output.get("templateVersion").is_none());
    let page = &output["resources"][0]["pages"][0];
    assert_eq!(page["answers"]["new"]["yesno"], 0.82);
    assert_eq!(
        page["source"]["path"],
        file.file_name().unwrap().to_str().unwrap()
    );
    assert_eq!(page["scope"]["startLine"], 1);
    assert!(
        !result
            .structured_content
            .to_string()
            .contains("preClose runs")
    );
    let requests = server.received_requests().await.unwrap();
    let sent: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(sent["state"].to_string().contains("preClose runs"));
    assert_eq!(sent["questions"]["answer"]["type"], "noul");
    assert_eq!(
        sent["questions"]["answer"]["instructions"]["knownEvidence"],
        json!(["onClose runs after requests finish"])
    );
    assert_eq!(sent["questions"].as_object().unwrap().len(), 1);
    runtime.close().await;
}

#[tokio::test]
async fn search_rejects_removed_semantic_addon_without_calling_provider() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_CLASSIFICATION_API", "secret".into())]);
    let error = runtime.execute("search".into(), "localSearch".into(), json!({"queries":[{
        "path":workspace.workspace, "matchString":"hooks", "mainGoal": "test", "reasoning":"Discover candidates.",
        "semanticRerank":{"questions":[{"id":"q","question":"Relevant?"}]}
    }]})).await.expect_err("semantic checks require clasify");
    assert_eq!(error.code, "invalidInput");
    runtime.close().await;
}

#[tokio::test]
async fn page_budget_continuation_round_trips_through_the_public_contract() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.6}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write(
        "many-lines.txt",
        (0..200)
            .map(|index| format!("line {index}\n"))
            .collect::<String>(),
    );
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"paged",
        "reasoning":"Assess bounded pages.","mainGoal":"Decide the next read.",
        "resources":[{"id":"file","maxChars":5000,"tool":"localFetch","query":{
            "path":file,"mainGoal": "test", "reasoning":"Read the next exact line.","length":1,"fullContent":false
        }}],
        "questions":[{"id":"relevant","type":"relevant","ask":"line content"}]
    });
    let outcome = runtime
        .execute("paged".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let assess = outcome.structured_content["queries"][0]["next"]["clasify"]["queries"][0].clone();
    assert!(assess.is_object(), "{}", outcome.structured_content);
    assert_eq!(assess["debug"], true, "a debug walk continues as one");
    assert_eq!(
        assess["questions"][0]["id"], "relevant",
        "Continuation must preserve question identity"
    );
    let resource = assess["resources"][0].as_object().unwrap();
    assert_eq!(
        resource
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["id", "maxChars", "query", "tool"]),
        "{assess}"
    );
    octocode_native::contracts::prepare_many_and_validate("clasify", call(assess.clone()))
        .expect("next.clasify must execute unchanged");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("nested query-cell-page output");
    assert_eq!(assess["questions"][0]["type"], "relevant");
    let mut next = Some(assess);
    let mut last_end = outcome.structured_content["queries"][0]["resources"][0]["pages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["scope"]["endLine"]
        .as_u64()
        .unwrap();
    for _ in 0..4 {
        let Some(query) = next.take() else {
            break;
        };
        let page = runtime
            .execute("continue-preset".into(), "clasify".into(), call(query))
            .await
            .unwrap();
        let row = &page.structured_content["queries"][0];
        assert!(row.get("templateVersion").is_none());
        for page in row["resources"][0]["pages"].as_array().unwrap() {
            assert_eq!(page["scope"]["startLine"].as_u64().unwrap(), last_end + 1);
            last_end = page["scope"]["endLine"].as_u64().unwrap();
        }
        next = row.pointer("/next/clasify").cloned();
    }
    assert!(next.is_none(), "continuation must terminate");
    assert_eq!(last_end, 200);
    runtime.close().await;
}

#[tokio::test]
async fn payload_over_max_chars_returns_an_executable_clasify_continuation() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.6}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(5)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("over-budget.txt", filler(80_001));
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"over-budget",
        "reasoning":"Assess no more than the resource payload budget.","mainGoal":"Decide the next read.",
        "resources":[{"id":"file","maxChars":80_000,"tool":"localFetch","query":{
            "path":file,"mainGoal": "test", "reasoning":"Read the complete file.","fullContent":true
        }}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });
    let first = runtime
        .execute("over-budget-first".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let assess = first.structured_content["queries"][0]["next"]["clasify"]["queries"][0].clone();
    octocode_native::contracts::prepare_many_and_validate("clasify", call(assess.clone()))
        .expect("next.clasify must satisfy the public input contract");

    let resumed = runtime
        .execute("over-budget-resume".into(), "clasify".into(), call(assess))
        .await
        .expect("next.clasify must execute unchanged");
    let resumed_query = &resumed.structured_content["queries"][0];
    assert!(no_continuation(resumed_query), "{resumed_query}");
    assert_eq!(resumed_query["resources"][0]["coverage"], "complete");
    runtime.close().await;
}

#[tokio::test]
async fn invalid_inner_query_is_rejected_with_the_exact_contract_field() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_CLASSIFICATION_API", "secret".into())]);
    let input = json!({
        "id": "bad-inner-query",
        "reasoning": "Test that an invalid inner path surfaces its field name.","mainGoal":"Decide the next read.",
        "resources": [{
            "id": "r1",
            "tool": "localFetch",
            "query": { "path": 42 }
        }],
        "questions": [{
            "id": "q1",
            "type": "yesno", "ask": "Relevant?"
        }]
    });
    let error = runtime
        .execute("bad-inner-query".into(), "clasify".into(), call(input))
        .await
        .expect_err("invalid delegated path must fail contract validation");
    assert_eq!(error.code, "invalidInput");
    let payload = error.payload.expect("structured validation payload");
    let details = payload["details"].as_array().expect("validation details");
    assert!(
        details.iter().any(|detail| detail
            .as_str()
            .is_some_and(|detail| detail.contains("queries.0.resources.0.query.path"))),
        "validation detail must name the nested path field: {details:?}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn search_resource_fans_out_candidates_from_only_the_requested_page() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.4}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(2)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for index in 0..6 {
        workspace.write(&format!("src/file{index}.txt"), "needle marker\n");
    }
    let root = workspace.write("src/file6.txt", "needle marker\n");
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"search-page",
        "reasoning":"Judge one search page.","mainGoal":"Decide the next read.",
        "resources":[{"id":"hits","candidateEvidence":"search","tool":"localSearch","query":{
            "path":root,"matchString":"needle","mainGoal": "test", "reasoning":"Find hits.",
            "resultView":"paginated","pageSize":2
        }}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });
    let outcome = runtime
        .execute("search-page".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    let cell = &query["resources"][0];
    let pages = cell["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2, "{cell}");
    assert_eq!(cell["coverage"], "partial");
    let source_paths = pages
        .iter()
        .map(|page| page["source"]["path"].as_str().unwrap().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        source_paths.len(),
        2,
        "each candidate needs its own source: {cell}"
    );
    // Workspace-relative (TOOL_DATA_CONTRACT "Paths"); localFetch resolves
    // them against the workspace root, so they stay executable.
    assert!(
        source_paths.iter().all(|path| {
            !std::path::Path::new(path).is_absolute() && workspace.workspace.join(path).is_file()
        }),
        "local candidate paths must be workspace-relative files: {source_paths:?}"
    );
    let resume = &query["next"]["clasify"]["queries"][0]["resources"][0];
    assert_eq!(resume["tool"], "localSearch", "{query}");
    assert_eq!(resume["query"]["page"], 2, "{resume}");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let judged_paths = requests
        .iter()
        .map(|request| {
            let sent: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            assert!(sent["state"].to_string().contains("needle"), "{sent}");
            assert!(
                sent["state"]["data"].get("next").is_none(),
                "continuations stay out of provider state: {sent}"
            );
            let files = sent["state"]["data"]["files"].as_array().unwrap();
            assert_eq!(
                files.len(),
                1,
                "one provider state per file candidate: {sent}"
            );
            files[0]["path"].as_str().unwrap().to_owned()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(judged_paths.len(), 2, "each file must be judged once");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("single-page search output contract");
    runtime.close().await;
}

/// clasify's own `next.clasify` is a generated `ClasifyInput` that passes the
/// same preparation and validation as caller input, and replays as-is.
#[tokio::test]
async fn emitted_next_clasify_is_schema_valid_input_and_replays() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.4}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for index in 0..5 {
        workspace.write(&format!("src/file{index}.txt"), "needle marker\n");
    }
    let root = workspace.write("src/file5.txt", "needle marker\n");
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"walk",
        "reasoning":"Judge the hits page by page.","mainGoal":"Decide the next read.",
        "resources":[{"id":"hits","tool":"localSearch","query":{
            "path":root,"matchString":"needle","resultView":"paginated","pageSize":2
        }}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });
    let first = runtime
        .execute("walk-1".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    octocode_native::contracts::validate_output("clasify", &first.structured_content)
        .expect("first page output contract");
    let next = first.structured_content["queries"][0]["next"]["clasify"]["queries"][0].clone();
    assert_eq!(next["mainGoal"], "Decide the next read.", "{next}");
    assert_eq!(next["questions"][0]["id"], "relevant", "{next}");
    assert_eq!(next["resources"][0]["query"]["page"], 2, "{next}");
    serde_json::from_value::<octocode_native::contracts::tool_types::ClasifyInput>(call(
        next.clone(),
    ))
    .unwrap_or_else(|error| panic!("generated ClasifyInput: {error}: {next}"));
    let prepared =
        octocode_native::contracts::prepare_many_and_validate("clasify", call(next.clone()))
            .unwrap_or_else(|error| {
                panic!(
                    "clasify input contract: {:?}: {next}",
                    error
                        .issues
                        .first()
                        .map(|issue| (&issue.path, &issue.message))
                )
            });
    assert_eq!(prepared.len(), 1);
    let first_paths = first.structured_content["queries"][0]["resources"][0]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|page| page["source"]["path"].clone())
        .collect::<Vec<_>>();
    let replay = runtime
        .execute("walk-2".into(), "clasify".into(), call(next))
        .await
        .expect("replayed next.clasify");
    octocode_native::contracts::validate_output("clasify", &replay.structured_content)
        .expect("replayed page output contract");
    let cell = &replay.structured_content["queries"][0]["resources"][0];
    assert_ne!(cell["coverage"], "error", "{cell}");
    let pages = cell["pages"].as_array().unwrap();
    assert!(!pages.is_empty(), "{cell}");
    assert!(
        pages
            .iter()
            .all(|page| !first_paths.contains(&page["source"]["path"])),
        "the replay reads the next page: {cell}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn file_chunk_scout_judges_every_hit_cluster_of_a_clipped_file() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":3,"output_tokens":1}
        })))
        .expect(2)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // Eleven hits near the top clip the file at ten rows; the deciding hit
    // sits far below, listed only in the file's moreLines.
    let mut body = String::from("header\n");
    for _ in 0..11 {
        body.push_str("needle marker\n");
    }
    // Long filler lines keep the file over one page, so it is judged by
    // hit windows rather than whole.
    for _ in 0..390 {
        body.push_str(&format!("filler line {}\n", "x".repeat(80)));
    }
    body.push_str("needle decides the answer\n");
    workspace.write("src/only.txt", body);
    let root = workspace
        .workspace
        .join("src")
        .to_string_lossy()
        .into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"clusters",
        "reasoning":"Judge every hit cluster.","mainGoal":"Find the deciding line.",
        "resources":[{"id":"hits",
            "tool":"localSearch","candidateEvidence":"fileChunks","query":{
                "path":root,"matchString":"needle"
            }
        }],
        "questions":[{"id":"decides","type":"yesno","ask":"Does this source decide the answer?"}]
    });
    let outcome = runtime
        .execute("clusters".into(), "clasify".into(), verbose(input))
        .await
        .expect("cluster scout");
    let pages = outcome.structured_content["queries"][0]["resources"][0]["pages"]
        .as_array()
        .unwrap()
        .clone();
    let starts = pages
        .iter()
        .map(|page| page["scope"]["startLine"].as_u64())
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 2, "{}", outcome.structured_content);
    assert!(
        pages.iter().all(|page| page.get("error").is_none()),
        "{pages:?}"
    );
    let sent = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["state"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert!(
        sent.iter()
            .any(|content| content.contains("needle decides the answer")),
        "{sent:?}"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("cluster output contract");
    runtime.close().await;
}

/// Two hit clusters of one file within a window radius are judged as one
/// contiguous page: one provider call instead of two (a small file is one
/// whole page). A span too large for one bounded page falls back to one page
/// per cluster.
#[tokio::test]
async fn file_chunk_scout_judges_near_clusters_of_one_file_in_one_call() {
    for (filler, calls) in [("filler line", 1u64), (&*"long filler ".repeat(18), 2)] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model":"resolved",
                "answers":{"answer":{"type":"noul","noul":0.7}},
                "usage":{"input_tokens":3,"output_tokens":1}
            })))
            .expect(calls)
            .mount(&server)
            .await;
        let workspace = Workspace::new();
        let mut body = String::from("header\n");
        for _ in 0..11 {
            body.push_str("needle marker\n");
        }
        for _ in 0..150 {
            body.push_str(filler);
            body.push('\n');
        }
        body.push_str("needle decides the answer\n");
        workspace.write("src/only.txt", body);
        let root = workspace
            .workspace
            .join("src")
            .to_string_lossy()
            .into_owned();
        let runtime = provider_runtime(&workspace, &server);
        let input = json!({
            "id":"near",
            "reasoning":"Judge near hit clusters.","mainGoal":"Find the deciding line.",
            "resources":[{"id":"hits",
                "tool":"localSearch","candidateEvidence":"fileChunks","query":{
                    "path":root,"matchString":"needle"
                }
            }],
            "questions":[{"id":"decides","type":"yesno","ask":"Does this source decide the answer?"}]
        });
        let outcome = runtime
            .execute("near".into(), "clasify".into(), verbose(input))
            .await
            .expect("near scout");
        let pages = outcome.structured_content["queries"][0]["resources"][0]["pages"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(pages.len() as u64, calls, "{}", outcome.structured_content);
        assert!(
            pages.iter().all(|page| page.get("error").is_none()),
            "{pages:?}"
        );
        let sent = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| {
                serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["state"]["content"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert!(
            sent.iter()
                .any(|content| content.contains("needle decides the answer")),
            "{sent:?}"
        );
        assert!(
            sent.iter().any(|content| content.contains("needle marker")),
            "{sent:?}"
        );
        octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
            .expect("near cluster output contract");
        runtime.close().await;
    }
}

#[tokio::test]
async fn file_chunk_scout_hydrates_the_candidate_cap_and_returns_exact_reads() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":3,"output_tokens":1}
        })))
        .expect(40)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for index in 0..45 {
        workspace.write(
            &format!("src/candidate{index}.txt"),
            format!("header\nneedle marker\nbody-only fact {index}\nfooter\n"),
        );
    }
    let root = workspace
        .workspace
        .join("src")
        .to_string_lossy()
        .into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"hydrated-search",
        "reasoning":"Judge source around each search hit.","mainGoal":"Decide the next read.",
        "resources":[{"id":"hits",
            "tool":"localSearch","candidateEvidence":"fileChunks","query":{
                "path":root,"matchString":"needle","mainGoal": "test", "reasoning":"Find candidates.",
                "resultView":"paginated","pageSize":50
            }
        ,"maxChars":20_000}],
        "questions":[{"id":"relevant",
            "type":"yesno","ask":"Does this source contain a body-only fact?"
        }]
    });
    let outcome = runtime
        .execute("hydrated-search".into(), "clasify".into(), verbose(input))
        .await
        .expect("hydrated scout");
    let query = &outcome.structured_content["queries"][0];
    let pages = query["resources"][0]["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 40, "{query}");
    for page in pages {
        assert_eq!(page["hints"]["read"]["tool"], "localFetch", "{page}");
        // An exact replay states no confidence.
        assert!(page["hints"]["read"].get("confidence").is_none(), "{page}");
        // Workspace-relative, like every local tool's rows.
        assert!(
            page["source"]["path"]
                .as_str()
                .is_some_and(|p| p.starts_with("src/candidate")),
            "{page}"
        );
        assert!(page.get("limitations").is_none(), "stated once: {page}");
    }
    let read = &pages[0]["hints"]["read"];
    let fetched = runtime
        .execute(
            "hydrated-read".into(),
            "localFetch".into(),
            json!({"queries":[read["query"]]}),
        )
        .await
        .expect("read replays");
    assert_eq!(
        fetched.structured_content["results"][0]["data"]["path"], pages[0]["source"]["path"],
        "{}",
        fetched.structured_content
    );
    // Each small file is judged whole, so no page claims a bounded chunk.
    assert!(
        !query.to_string().contains("bounded candidate chunk"),
        "{query}"
    );
    let resume = &query["next"]["clasify"]["queries"][0]["resources"][0];
    assert_eq!(resume["candidateEvidence"], "fileChunks");
    assert_eq!(resume["query"]["page"], 2);
    assert_eq!(resume["query"]["pageSize"], 40);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 40);
    for request in requests {
        let sent: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let state = &sent["state"];
        assert!(
            state["content"]
                .as_str()
                .is_some_and(|body| body.contains("body-only fact")),
            "{sent}"
        );
        assert!(state["content"].as_str().unwrap().chars().count() <= 4_000);
        assert!(
            state.get("matches").is_none(),
            "search snippets must not reach Jev: {sent}"
        );
    }
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("hydrated scout output contract");
    runtime.close().await;
}

/// Every line span one rendered page covers: its judged `scope`, and each
/// range its read names (one row or a `queries` batch).
fn page_spans(page: &serde_json::Value) -> Vec<(u64, u64)> {
    let mut spans = Vec::new();
    if let (Some(start), Some(end)) = (
        page.pointer("/scope/startLine")
            .and_then(serde_json::Value::as_u64),
        page.pointer("/scope/endLine")
            .and_then(serde_json::Value::as_u64),
    ) {
        spans.push((start, end));
    }
    let rows = page
        .pointer("/hints/read/query/queries")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for row in rows {
        for range in row["ranges"].as_array().into_iter().flatten() {
            if let Some((start, end)) = range.as_str().and_then(|range| range.split_once('-')) {
                spans.push((start.parse().unwrap(), end.parse().unwrap()));
            }
        }
    }
    spans
}

/// D1 (CL5: a fileChunks walk over tools/clasify covered 242 of 296 hit
/// lines): a file whose search page lists 10 rows and 24 more lines but only
/// counts the other 26 (`moreLinesUnlisted`) still has every hit line reach
/// a window, judged or unjudged with its read.
#[tokio::test]
async fn file_chunks_reach_hit_lines_the_search_page_only_counted() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":3,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // Two hundred long hit lines, one every three lines: too large to show
    // whole, so the page shows 10 rows, lists 24 more lines, and counts the
    // rest.
    let hits = (1..=200).map(|step| step * 3).collect::<Vec<u64>>();
    let mut body = String::new();
    for line in 1..=610 {
        if hits.contains(&line) {
            body.push_str(&format!("needle {line} {}\n", filler(900)));
        } else {
            body.push_str("filler\n");
        }
    }
    workspace.write("src/many.txt", body);
    let root = workspace
        .workspace
        .join("src")
        .to_string_lossy()
        .into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let search = json!({"path":root,"matchString":"needle"});
    let listed = runtime
        .execute(
            "unlisted-search".into(),
            "localSearch".into(),
            json!({"queries":[{"path":root,"matchString":"needle","pageSize":5}]}),
        )
        .await
        .expect("search");
    let file = &listed.structured_content["results"][0]["data"]["files"][0];
    assert!(
        file.pointer("/pagination/moreLinesUnlisted")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|unlisted| unlisted > 0),
        "the fixture leaves hit lines unlisted: {}",
        file["pagination"]
    );
    let input = json!({
        "id":"unlisted","reasoning":"r","mainGoal":"Find the deciding line.",
        "resources":[{"id":"hits","tool":"localSearch","candidateEvidence":"fileChunks","query":search}],
        "questions":[{"id":"decides","type":"yesno","ask":"Does this source decide the answer?"}]
    });
    let outcome = runtime
        .execute("unlisted".into(), "clasify".into(), verbose(input))
        .await
        .expect("clasify");
    let query = &outcome.structured_content["queries"][0];
    let spans = query["resources"][0]["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(page_spans)
        .collect::<Vec<_>>();
    let missing = hits
        .iter()
        .filter(|hit| {
            !spans
                .iter()
                .any(|(start, end)| (*start..=*end).contains(*hit))
        })
        .collect::<Vec<_>>();
    assert!(missing.is_empty(), "uncovered {missing:?}: {query}");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("unlisted hits output contract");
    runtime.close().await;
}

/// D3 (CL5 fc1.json: 64 identical error objects, 12.2 KB of 38 KB, each with
/// a one-window read): hit windows a spent budget left unjudged state their
/// error once per resource, and each file's unjudged windows share one read
/// that names each window once; judged and unjudged together cover every hit.
#[tokio::test]
async fn unjudged_hit_windows_state_their_error_once_and_batch_per_file() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":3,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // Ten files over one page each, with seven far hit clusters each: more
    // windows than the page budget judges.
    let hits = (0..7).map(|step| 10 + step * 400).collect::<Vec<u64>>();
    for file in 0..10 {
        let mut body = String::new();
        for line in 1..=2500 {
            body.push_str(if hits.contains(&line) {
                "needle marker\n"
            } else {
                "filler line padded so the file stays over one page\n"
            });
        }
        workspace.write(&format!("src/f{file}.txt"), body);
    }
    let root = workspace
        .workspace
        .join("src")
        .to_string_lossy()
        .into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"spent","reasoning":"r","mainGoal":"Find the deciding line.",
        "resources":[{"id":"hits","tool":"localSearch","candidateEvidence":"fileChunks",
            "query":{"path":root,"matchString":"needle"}}],
        "questions":[{"id":"a","type":"yesno","ask":"Does this source decide the answer?"}]
    });
    let outcome = runtime
        .execute("spent".into(), "clasify".into(), call(input))
        .await
        .expect("clasify");
    let query = &outcome.structured_content["queries"][0];
    let resource = &query["resources"][0];
    let pages = resource["pages"].as_array().cloned().unwrap_or_default();
    let unjudged = pages
        .iter()
        .filter(|page| page.get("answers").is_none())
        .collect::<Vec<_>>();
    assert!(
        !unjudged.is_empty(),
        "the fixture spends the page budget: {query}"
    );
    assert_eq!(
        resource["error"]["errorCode"], "classificationBudgetSpent",
        "{query}"
    );
    assert_eq!(
        outcome
            .structured_content
            .to_string()
            .matches("classificationBudgetSpent")
            .count(),
        1,
        "stated once: {query}"
    );
    let mut files = std::collections::HashSet::new();
    for page in &unjudged {
        assert!(page.get("error").is_none(), "{page}");
        let rows = page
            .pointer("/hints/read/query/queries")
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| panic!("a batched read: {page}"));
        let path = rows[0]["path"].as_str().unwrap().to_owned();
        assert!(files.insert(path), "one unjudged page per file: {query}");
    }
    // Judged pages name their window (`line`..`endLine`); together with the
    // batched reads they cover every hit of every file.
    for file in 0..10 {
        let path = format!("src/f{file}.txt");
        let mut spans = Vec::new();
        for page in &pages {
            let named = page["path"].as_str() == Some(path.as_str())
                || page
                    .pointer("/hints/read/query/queries/0/path")
                    .and_then(serde_json::Value::as_str)
                    == Some(path.as_str());
            if !named {
                continue;
            }
            if let (Some(start), Some(end)) = (page["line"].as_u64(), page["endLine"].as_u64()) {
                spans.push((start, end));
            }
            spans.extend(page_spans(page));
        }
        for hit in &hits {
            assert!(
                spans
                    .iter()
                    .any(|(start, end)| (*start..=*end).contains(hit)),
                "{path}:{hit} unreached: {query}"
            );
        }
    }
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("unjudged batch output contract");
    runtime.close().await;
}

/// Wide search pages share the matrix page budget: each resource judges its
/// share of candidates and `next.clasify` resumes the rest; nothing fails.
#[tokio::test]
async fn wide_search_pages_share_the_page_budget_and_resume() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer_0":{"type":"noul","noul":0.7},"answer_1":{"type":"noul","noul":0.6}},
            "usage":{"input_tokens":3,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for root in ["a", "b"] {
        for index in 0..31 {
            workspace.write(
                &format!("{root}/candidate{index}.txt"),
                "needle\nbody evidence\n",
            );
        }
    }
    let resource = |id: &str, root: &str| {
        json!({
            "id":id,"tool":"localSearch","candidateEvidence":"search","query":{
                "path":workspace.workspace.join(root),"matchString":"needle","mainGoal": "test", "reasoning":"Find candidates.",
                "pageSize":30
            }
        })
    };
    let questions = (0..2)
        .map(|index| {
            json!({
                "id":format!("q{index}"),"type":"yesno","ask":format!("Check {index}?")
            })
        })
        .collect::<Vec<_>>();
    let runtime = provider_runtime(&workspace, &server);
    let outcome = runtime
        .execute(
            "wide-pages".into(),
            "clasify".into(),
            verbose(json!({
                "id":"wide-pages","reasoning":"Exercise the page budget.","mainGoal":"Decide the next read.",
                "resources":[resource("a","a"),resource("b","b")],"questions":questions
            })),
        )
        .await
        .expect("bounded wide pages");
    let query = &outcome.structured_content["queries"][0];
    let resources = query["resources"].as_array().unwrap();
    let judged = resources
        .iter()
        .flat_map(|r| r["pages"].as_array().unwrap())
        .filter(|page| page.get("answers").is_some())
        .count();
    assert!(judged > 0 && judged <= 48, "{query}");
    assert!(
        resources
            .iter()
            .flat_map(|r| r["pages"].as_array().unwrap())
            .all(|page| page["error"]["errorCode"] != "classificationExpandedCellsExceeded"),
        "{query}"
    );
    assert!(
        query.pointer("/next/clasify").is_some(),
        "the rest resumes: {query}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn empty_file_is_reported_without_a_provider_call() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("empty.txt", "");
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"empty","reasoning":"Screen an empty artifact.","mainGoal":"Decide the next read.",
        "resources":[{"id":"e","tool":"localFetch","query":{"path":file,"mainGoal": "test", "reasoning":"Read it."}}],
        "questions":[{"id":"q","type":"yesno","ask":"Relevant?"}]
    });
    let outcome = runtime
        .execute("empty".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let resource = &outcome.structured_content["queries"][0]["resources"][0];
    assert_eq!(resource["coverage"], "error", "{resource}");
    assert!(
        outcome.all_failed,
        "MCP must expose failed evidence as a tool error"
    );
    assert_eq!(
        resource["pages"][0]["error"]["errorCode"],
        "classificationContextEmpty"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("empty-file output contract");
    runtime.close().await;
}

#[tokio::test]
async fn empty_search_page_is_not_sent_to_the_provider() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("present.txt", "hello world\n");
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
    ]);
    let input = json!({
        "id":"empty-search","reasoning":"Screen a search page.","mainGoal":"Decide the next read.",
        "resources":[{"id":"e","tool":"localSearch","query":{
            "path":file,"matchString":"UNLIKELY_OCTOCODE_SENTINEL_673829",
            "mainGoal": "test", "reasoning":"Find matching source."
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Does this page show a match?"}]
    });
    let outcome = runtime
        .execute("empty-search".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let resource = &outcome.structured_content["queries"][0]["resources"][0];
    assert_eq!(resource["coverage"], "error", "{resource}");
    assert!(
        outcome.all_failed,
        "MCP must expose failed evidence as a tool error"
    );
    assert_eq!(
        resource["pages"][0]["error"]["errorCode"],
        "classificationContextEmpty"
    );
    runtime.close().await;
}

#[tokio::test]
async fn disjoint_file_match_windows_return_real_ranges_without_a_focus() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.95}},
            "usage":{"input_tokens":5,"output_tokens":1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let body = (1..=220)
        .map(|line| {
            if line == 10 || line == 160 {
                format!("MARKER {line}\n")
            } else {
                format!("line {line}\n")
            }
        })
        .collect::<String>();
    let file = workspace.write("disjoint.rs", body);
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"disjoint","reasoning":"Find relevant match windows.","mainGoal":"Decide the next read.",
        "resources":[{"id":"f","tool":"localFetch","query":{
            "path":file,"mainGoal": "test", "reasoning":"Read matching windows.",
            "matchString":"MARKER","contextLines":45,"length":50000
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Does this content show MARKER?"}]
    });
    let outcome = runtime
        .execute("disjoint".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let page = &outcome.structured_content["queries"][0]["resources"][0]["pages"][0];
    assert_eq!(
        page["scope"]["lineRanges"].as_array().map(Vec::len),
        Some(2),
        "{page}"
    );
    assert!(page.get("focus").is_none(), "{page}");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("disjoint scope output contract");
    runtime.close().await;
}

#[tokio::test]
async fn long_positive_scout_sends_only_authored_questions_and_preserves_probabilities() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{
                "answer_0":{"type":"noul","noul":0.93456789},
                "answer_1":{"type":"choice","choice":"yes","confidence":0.87654321,
                    "probabilities":{"yes":0.9995,"no":0.0005}}
            },
            "usage":{"input_tokens":5,"output_tokens":2}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let body = (1..=200).map(|n| format!("line {n}\n")).collect::<String>();
    let file = workspace.write("long.rs", body);
    let expected_source_path = file.clone();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"pure-scout","reasoning":"Screen the document.","mainGoal":"Decide the next read.",
        "resources":[{"id":"f","tool":"localFetch","query":{
            "path":file,"mainGoal": "test", "reasoning":"Read the source."
        }}],
        "questions":[
            {"id":"shutdown","type":"yesno","ask":"Could this document contain shutdown guidance?"},
            {"id":"role","type":"choice","ask":"Classify the document's role.",
                "labels":{"yes":"Has guidance","no":"No guidance"}}
        ]
    });
    let outcome = runtime
        .execute("pure-scout".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let page = &outcome.structured_content["queries"][0]["resources"][0]["pages"][0];
    assert_eq!(page["answers"]["shutdown"]["yesno"], 0.93456789);
    assert_eq!(
        page["answers"]["role"],
        json!({
            "choice":"yes","confidence":0.87654321,
            "probabilities":{"yes":0.9995,"no":0.0005}
        })
    );
    assert!(page.get("focus").is_none(), "{page}");
    assert!(
        outcome.structured_content["queries"][0]
            .get("lowSignal")
            .is_none()
    );
    // Local sources display workspace-relative (TOOL_DATA_CONTRACT "Paths").
    assert_eq!(
        page["source"]["path"],
        expected_source_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    assert!(page["source"].get("evidenceHash").is_none());
    let requests = server.received_requests().await.expect("mock requests");
    assert_eq!(requests.len(), 1, "Clasify must not add a focus call");
    let sent: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        sent["questions"]
            .as_object()
            .map(|questions| questions.len()),
        Some(2)
    );
    assert_eq!(
        sent["questions"]["answer_0"],
        json!({
            "instructions":{
                "question":"Could this document contain shutdown guidance?",
                "goal":"Decide the next read."
            },
            "type":"noul"
        })
    );
    assert_eq!(
        sent["questions"]["answer_1"],
        json!({
            "instructions":{
                "question":"Classify the document's role.",
                "goal":"Decide the next read."
            },
            "type":"choice",
            "criteria":{"yes":"Has guidance","no":"No guidance"}
        })
    );
    assert!(sent["state"].to_string().contains("line 200"));
    assert!(sent.get("windows").is_none());
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("pure scout output contract");
    runtime.close().await;
}

#[tokio::test]
async fn snippet_continuations_visit_every_file_and_match_page_before_completing() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.1}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write(
        "fixture/a.txt",
        "marker: alpha\nmarker: secret needle\nmarker: omega\n",
    );
    let root = workspace.write(
        "fixture/b.txt",
        "marker: beta\nmarker: gamma\nmarker: delta\n",
    );
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let mut input = json!({
        "id":"matrix-1","reasoning":"Cover every file and match page.","mainGoal":"Decide the next read.",
        "resources":[{"id":"files","candidateEvidence":"search","tool":"localSearch","query":{
            "mainGoal": "test", "reasoning":"Page snippets.","path":root,"matchString":"marker",
            "pageSize":1,"matchPageSize":1,"sort":"path"
        }}],
        "questions":[{"id":"needle","type":"yesno","ask":"Does the evidence contain secret needle?"}]
    });
    let mut coverages = Vec::new();
    for call in 0..10 {
        let outcome = runtime
            .execute(
                format!("page-{call}"),
                "clasify".into(),
                verbose(input.clone()),
            )
            .await
            .unwrap();
        let query = &outcome.structured_content["queries"][0];
        coverages.push(query["resources"][0]["coverage"].clone());
        match query["next"].get("clasify") {
            Some(next) => input = next.clone(),
            None => break,
        }
    }
    let sent = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            body["state"].to_string()
        })
        .collect::<Vec<_>>();
    let visited = |needle: &str| sent.iter().filter(|state| state.contains(needle)).count();
    for line in ["alpha", "secret needle", "omega", "beta", "gamma", "delta"] {
        assert_eq!(
            visited(line),
            1,
            "{line} must be judged exactly once: {sent:#?}"
        );
    }
    assert_eq!(coverages.len(), 6, "{coverages:?}");
    assert!(
        coverages[..5].iter().all(|coverage| coverage == "partial"),
        "every page with an outstanding branch is partial: {coverages:?}"
    );
    assert_eq!(coverages[5], "complete", "{coverages:?}");
    runtime.close().await;
}

#[tokio::test]
async fn clasify_locate_preflight_returns_a_typed_error_without_capture_or_provider_calls() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
    ]);
    let input = json!({
        "reasoning":"Reject locate on a search resource.",
        "mainGoal":"Files that state the validation rules.",
        "resources":[
            {"id":"unread","tool":"localFetch","query":{"path":"/does-not-exist"}},
            {"id":"search","tool":"ghSearchCode","query":{"owner":"nonexistent"},"candidateEvidence":"search"}
        ],
        "questions":[
            {"type":"locate","ask":"Validation rules"},
            {"type":"yesno","ask":"Does this validate requests?"}
        ]
    });
    let outcome = runtime
        .execute("preflight".into(), "clasify".into(), verbose(input))
        .await
        .expect("typed rejection rather than WorkerFailed");
    let resources = outcome.structured_content["queries"][0]["resources"]
        .as_array()
        .unwrap();
    assert_eq!(resources.len(), 2);
    for resource in resources {
        assert_eq!(resource["coverage"], "error");
        let page = &resource["pages"][0];
        assert_eq!(
            page["error"]["errorCode"],
            "classificationLocateUnsupported"
        );
        assert!(page["error"]["error"].as_str().unwrap().contains("search"));
        assert!(page.get("answers").is_none());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_repeated_matrix_replays_its_judgment_without_a_second_provider_request() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("replay.txt", "The retry floor is 100 ms.\n");
    let runtime = provider_runtime(&workspace, &server);
    let mut input = query();
    input["resources"] = json!([{"id":"source","tool":"localFetch","query":{"path":file}}]);
    input["questions"] = json!([{"id":"floor","type":"yesno","ask":"Is the retry floor stated?"}]);
    let first = runtime
        .execute("replay-1".into(), "clasify".into(), call(input.clone()))
        .await
        .expect("first judgment");
    let second = runtime
        .execute("replay-2".into(), "clasify".into(), call(input))
        .await
        .expect("replayed judgment");
    let answer = |outcome: &octocode_native::runtime::ToolOutcome| {
        compact_answer(
            &outcome.structured_content["queries"][0]["resources"][0],
            "floor",
        )
    };
    assert_eq!(answer(&first), json!(0.7), "{}", first.structured_content);
    assert_eq!(answer(&second), answer(&first));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn identical_pages_in_one_call_share_a_single_provider_request() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(noul_response(0.6).set_delay(Duration::from_millis(200)))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = provider_runtime(&workspace, &server);
    let evidence = "The dedupe sentinel 4411 is stated here.";
    let matrix = |id: &str| {
        json!({
            "id":id,"reasoning":"Screen duplicated evidence.","mainGoal":"Decide the next read.",
            "resources":[{"id":"a","value":evidence},{"id":"b","value":evidence}],
            "questions":[{"id":"q","type":"yesno","ask":"Is sentinel 4411 stated?"}]
        })
    };
    let outcome = runtime
        .execute(
            "dedupe".into(),
            "clasify".into(),
            json!({"queries":[matrix("m1"), matrix("m2")]}),
        )
        .await
        .unwrap();
    for query in outcome.structured_content["queries"].as_array().unwrap() {
        for resource in query["resources"].as_array().unwrap() {
            assert_eq!(
                compact_answer(resource, "q"),
                json!(0.6),
                "{}",
                outcome.structured_content
            );
        }
    }
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "four identical cells must not race four provider requests"
    );
    runtime.close().await;
}

#[tokio::test]
async fn judgment_cache_ignores_correlation_ids_but_not_question_text() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(noul_response(0.4))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = provider_runtime(&workspace, &server);
    let input = |question_id: &str, ask: &str| {
        json!({
            "id":"ids","reasoning":"Cache key probe.","mainGoal":"Decide the next read.",
            "resources":[{"id":"v","value":"Cache id sentinel 7719."}],
            "questions":[{"id":question_id,"type":"yesno","ask":ask}]
        })
    };
    for (label, question_id, ask) in [
        ("first", "q", "Is sentinel 7719 stated?"),
        ("renamed", "renamed", "Is sentinel 7719 stated?"),
        ("changed", "q", "Is sentinel 7720 stated?"),
    ] {
        let outcome = runtime
            .execute(
                label.into(),
                "clasify".into(),
                call(input(question_id, ask)),
            )
            .await
            .unwrap();
        assert_eq!(
            compact_answer(
                &outcome.structured_content["queries"][0]["resources"][0],
                question_id
            ),
            json!(0.4),
            "{label}: {}",
            outcome.structured_content
        );
    }
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "a renamed id replays; changed question text asks again"
    );
    runtime.close().await;
}

#[tokio::test]
async fn prefilter_window_is_centered_on_the_hit_not_aligned_to_a_bucket() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(noul_response(0.9))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // The hit sits on line 601, the first line of the second 600-line bucket;
    // its enclosing function header is on line 596.
    let source = (1..=1300)
        .map(|line| match line {
            596 => "function retryDelay(attempt) { // HEADER_SENTINEL\n".to_owned(),
            601 => "  const RETRY_FLOOR_MS = 100;\n".to_owned(),
            _ => format!("const filler_{line} = {line};\n"),
        })
        .collect::<String>();
    let file = workspace.write("huge.js", source);
    let runtime = provider_runtime(&workspace, &server);
    let outcome = runtime
        .execute(
            "prefilter".into(),
            "clasify".into(),
            verbose(json!({
                "id":"pf","reasoning":"Judge only the hit window.","mainGoal":"Find the retry floor.",
                "resources":[{"id":"f","prefilter":["RETRY_FLOOR_MS"],
                    "tool":"localFetch","query":{"path":file}}],
                "questions":[{"id":"q","type":"yesno","ask":"Is the retry floor defined?"}]
            })),
        )
        .await
        .unwrap();
    let pages = outcome.structured_content["queries"][0]["resources"][0]["pages"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(pages.len(), 1, "{}", outcome.structured_content);
    let scope = &pages[0]["scope"];
    let (start, end) = (
        scope["startLine"].as_u64().unwrap(),
        scope["endLine"].as_u64().unwrap(),
    );
    assert!(
        start < 596 && end > 601 && end - start < 600,
        "window {start}-{end} must surround the hit"
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(String::from_utf8_lossy(&requests[0].body).contains("HEADER_SENTINEL"));
    runtime.close().await;
}

#[tokio::test]
async fn prefilter_hits_beyond_three_windows_resume_through_next_clasify() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(noul_response(0.9))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // Four hits, each needing its own 600-line window.
    let hits = [100_u64, 1100, 2100, 3100];
    let source = (1..=4000_u64)
        .map(|line| {
            if hits.contains(&line) {
                format!("const NEEDLE_{line} = {line}; // NEEDLE_MARK\n")
            } else {
                format!("const filler_{line} = {line};\n")
            }
        })
        .collect::<String>();
    let file = workspace.write("walk.js", source);
    let runtime = provider_runtime(&workspace, &server);
    let judged = |content: &serde_json::Value| {
        content["queries"][0]["resources"][0]["pages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|page| {
                (
                    page["scope"]["startLine"].as_u64().unwrap(),
                    page["scope"]["endLine"].as_u64().unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    let covers = |pages: &[(u64, u64)], line: u64| {
        pages
            .iter()
            .any(|(start, end)| (*start..=*end).contains(&line))
    };
    let input = |max_chars: Option<u64>| {
        let mut resource = json!({"id":"f","prefilter":["NEEDLE_MARK"],
            "tool":"localFetch","query":{"path":file}});
        if let Some(max_chars) = max_chars {
            resource["maxChars"] = json!(max_chars);
        }
        json!({
            "id":"walk","reasoning":"Judge every hit window.","mainGoal":"Find the needle.",
            "resources":[resource],
            "questions":[{"id":"q","type":"yesno","ask":"Is a needle defined?"}]
        })
    };

    let first = runtime
        .execute("walk-1".into(), "clasify".into(), verbose(input(None)))
        .await
        .unwrap();
    let pages = judged(&first.structured_content);
    assert_eq!(pages.len(), 3, "{}", first.structured_content);
    for line in &hits[..3] {
        assert!(covers(&pages, *line), "{line} in {pages:?}");
    }
    assert!(!covers(&pages, 3100));
    let query = &first.structured_content["queries"][0];
    assert!(
        query.to_string().contains("Prefilter"),
        "dropped hits must be reported: {query}"
    );
    let resume = query["next"]["clasify"]["queries"][0].clone();
    assert!(
        resume.is_object(),
        "dropped hits need a continuation: {query}"
    );
    let resumed = runtime
        .execute("walk-2".into(), "clasify".into(), call(resume))
        .await
        .expect("next.clasify must execute unchanged");
    let pages = judged(&resumed.structured_content);
    assert!(covers(&pages, 3100), "{}", resumed.structured_content);
    assert!(pages.iter().all(|(start, _)| *start > 2100), "{pages:?}");
    assert!(
        resumed.structured_content["queries"][0]
            .get("next")
            .is_none()
    );

    // One maxChars budget spans every window: the second window waits for
    // the next call instead of each window getting its own full budget.
    let tight = runtime
        .execute(
            "walk-tight".into(),
            "clasify".into(),
            verbose(input(Some(20_000))),
        )
        .await
        .unwrap();
    let pages = judged(&tight.structured_content);
    assert_eq!(pages.len(), 1, "{}", tight.structured_content);
    assert!(covers(&pages, 100));
    let resume =
        &tight.structured_content["queries"][0]["next"]["clasify"]["queries"][0]["resources"][0];
    assert_eq!(resume["prefilter"], json!(["NEEDLE_MARK"]), "{resume}");
    let from = range_start(&resume["query"]);
    assert!(from > 100 && from <= 1100, "resume at {from}");

    // Local absolute paths never reach the external provider.
    for request in server.received_requests().await.unwrap() {
        let body = String::from_utf8_lossy(&request.body);
        for root in [
            workspace.workspace.clone(),
            std::fs::canonicalize(&workspace.workspace).unwrap(),
        ] {
            let root = root.to_string_lossy().replace('\\', "/");
            assert!(
                !body.contains(&root),
                "absolute path sent to provider: {body}"
            );
        }
    }
    runtime.close().await;
}

#[tokio::test]
async fn partial_provider_answers_are_not_cached_and_a_failed_read_is_isolated() {
    let server = MockServer::builder().start().await;
    // First response drops answer_1 (partial); later responses are complete.
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer_0":{"type":"noul","noul":0.3}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer_0":{"type":"noul","noul":0.3},"answer_1":{"type":"noul","noul":0.9}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let missing = workspace.workspace.join("missing-8812.txt");
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"partial","reasoning":"Partial answers stay unresolved.","mainGoal":"Decide the next read.",
        "resources":[
            {"id":"gone","tool":"localFetch","query":{"path":missing}},
            {"id":"v","value":"Partial cache sentinel 5521."}
        ],
        "questions":[
            {"id":"a","type":"yesno","ask":"Is sentinel 5521 stated?"},
            {"id":"b","type":"yesno","ask":"Is sentinel 5522 stated?"}
        ]
    });
    let first = runtime
        .execute("partial-1".into(), "clasify".into(), verbose(input.clone()))
        .await
        .unwrap();
    let resources = &first.structured_content["queries"][0]["resources"];
    assert_eq!(resources[0]["coverage"], "error", "{resources}");
    assert!(resources[0]["pages"][0]["error"]["errorCode"].is_string());
    assert_eq!(resources[1]["coverage"], "partial", "{resources}");
    assert_eq!(
        resources[1]["pages"][0]["answers"]["a"],
        json!({"yesno":0.3})
    );
    assert!(resources[1]["pages"][0]["answers"]["b"]["error"].is_object());
    let second = runtime
        .execute("partial-2".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let page = &second.structured_content["queries"][0]["resources"][1]["pages"][0];
    assert_eq!(page["answers"]["b"], json!({"yesno":0.9}), "{page}");
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "a partial answer set must be asked again, not replayed"
    );
    runtime.close().await;
}

/// orangu 09-28: clasify with a ghSearchCode resource failed with
/// `classificationContextContractViolation` on every resource. A code-search
/// page must be judged like any other search resource.
#[tokio::test]
async fn gh_search_code_resource_is_judged_without_a_context_contract_violation() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.6}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let items = (0..3)
        .map(|n| {
            json!({
                "name": format!("f{n}.rs"), "path": format!("src/f{n}.rs"),
                "sha": format!("{n:040}"), "html_url": "https://github.com/o/r",
                "repository": {"full_name":"o/r","html_url":"https://github.com/o/r","url":"https://api.github.com/repos/o/r"},
                "text_matches": [{"fragment": format!("fn semaphore_acquire_{n}() {{}}"), "matches": [{"indices":[3,12]}]}]
            })
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 3, "incomplete_results": false, "items": items
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        // Own GitHub limiter key: the process-wide budget is keyed by host and
        // token, so concurrent GitHub fixtures must not share throttling state.
        ("GITHUB_TOKEN", "clasify-gh-search-code-fixture".into()),
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "reasoning":"Judge the code-search hits.","mainGoal":"Where the semaphore is acquired.",
        "resources":[{"id":"hits","candidateEvidence":"search","tool":"ghSearchCode","query":{
            "owner":"o","repo":"r","keywords":["semaphore"]
        }}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Does this acquire the semaphore?"}]
    });
    let outcome = runtime
        .execute("gh-code".into(), "clasify".into(), verbose(input))
        .await
        .expect("clasify");
    let rendered = outcome.structured_content.to_string();
    assert!(!rendered.contains("ContextContractViolation"), "{rendered}");
    let cell = &outcome.structured_content["queries"][0]["resources"][0];
    assert_ne!(cell["coverage"], "error", "{cell}");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("clasify output contract");
    runtime.close().await;
}

/// A lone strong locate window on a GitHub file is public in `best` with an
/// exact, schema-valid read that names owner/repo/path/branch.
#[tokio::test]
async fn a_lone_strong_locate_window_in_best_carries_an_exact_github_read() {
    use base64::Engine as _;
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(LocateTopPassage {
            top: 0.9,
            exists: 0.9,
        })
        .mount(&server)
        .await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let source = (1..=40)
        .map(|n| format!("fn step_{n}() -> u32 {{\n    {n}\n}}\n"))
        .collect::<String>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/contents/src%2Fsteps.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64",
            "content": base64::engine::general_purpose::STANDARD.encode(&source),
            "size": source.len(), "sha": "f".repeat(40), "path":"src/steps.rs"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        // Own GitHub limiter key: the process-wide budget is keyed by host and
        // token, so concurrent GitHub fixtures must not share throttling state.
        ("GITHUB_TOKEN", "clasify-locate-read-fixture".into()),
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "reasoning":"Locate the first step.","mainGoal":"Where step one is defined.",
        "resources":[{"id":"gh","tool":"ghGetFileContent","query":{
            "owner":"o","repo":"r","path":"src/steps.rs","ref":"main","fullContent":true
        }}],
        "questions":[{"id":"t","type":"locate","ask":"The function that returns one."}]
    });
    let outcome = runtime
        .execute("locate-gh".into(), "clasify".into(), verbose(input))
        .await
        .expect("clasify");
    let output = &outcome.structured_content;
    octocode_native::contracts::validate_output("clasify", output)
        .expect("best row read output contract");
    let rows = output["queries"][0]["best"]["t"]
        .as_array()
        .unwrap_or_else(|| panic!("best rows: {output}"));
    assert!(!rows.is_empty(), "{output}");
    for row in rows {
        let read = &row["hints"]["read"];
        assert_eq!(read["tool"], "ghGetFileContent", "{row}");
        let query = &read["query"]["queries"][0];
        assert_eq!(query["owner"], "o", "{row}");
        assert_eq!(query["repo"], "r", "{row}");
        assert_eq!(query["path"], "src/steps.rs", "{row}");
        assert_eq!(query["ref"], "main", "{row}");
        // ghGetFileContent's published line-span spelling.
        assert_eq!(
            query["ranges"],
            json!([format!("{}-{}", row["line"], row["endLine"])]),
            "{row}"
        );
        assert!(query.get("fullContent").is_none(), "{row}");
    }
    assert_eq!(rows[0]["line"], 1, "{output}");
    // A finished walk has no carry; nothing private leaks.
    assert!(no_continuation(&output["queries"][0]), "{output}");
    assert!(!output.to_string().contains("fileRead"), "{output}");
    runtime.close().await;
}

/// A matrix that also asks a described target runs locate; its literal
/// target (one identifier in a lookup sentence) over a local file keeps its
/// string hint and gains `hints.textSearch`, a schema-valid literal search
/// that runs. The described target adds no hint. Two literals share one
/// search that reaches both.
#[tokio::test]
async fn identifier_locate_target_emits_an_executable_local_search() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(LocateTopPassage {
            top: 0.9,
            exists: 0.9,
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let source = (1..=40)
        .map(|n| format!("fn step_{n}() -> u32 {{\n    {n}\n}}\n"))
        .collect::<String>();
    let file = workspace.write("src/steps.rs", source);
    let file = file.to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "reasoning":"Locate a step.","mainGoal":"Where step_17 is defined.",
        "resources":[{"id":"steps","tool":"localFetch","query":{
            "path":file,"fullContent":true
        }}],
        "questions":[
            {"id":"t","type":"locate","ask":"Where is step_17 defined?"},
            {"id":"u","type":"locate","ask":"find step_23"},
            {"id":"d","type":"locate","ask":"The step function that returns one."}
        ]
    });
    let outcome = runtime
        .execute("locate-literal".into(), "clasify".into(), verbose(input))
        .await
        .expect("clasify");
    let output = &outcome.structured_content;
    octocode_native::contracts::validate_output("clasify", output)
        .expect("literal search continuation output contract");
    let query = &output["queries"][0];
    assert!(query["best"]["d"].is_array(), "locate ran: {query}");
    assert_eq!(
        query["hints"]["text"].as_array().map(Vec::len),
        Some(2),
        "one tip per literal target: {query}"
    );
    assert!(
        query["hints"]["text"][0]
            .as_str()
            .unwrap_or_default()
            .contains("step_17"),
        "{query}"
    );
    let search = &query["hints"]["textSearch"];
    assert_eq!(search["tool"], "localSearch", "{query}");
    assert_eq!(
        search["query"]["queries"][0]["path"],
        file.as_str(),
        "{search}"
    );
    // One search reaches every literal: an alternation of both.
    assert_eq!(
        search["query"]["queries"][0]["matchString"], "step_17|step_23",
        "{search}"
    );
    assert_eq!(search["query"]["queries"][0]["regex"], "rust", "{search}");
    let found = runtime
        .execute(
            "literal-search".into(),
            "localSearch".into(),
            search["query"].clone(),
        )
        .await
        .expect("next.localSearch executes");
    let rendered = found.structured_content.to_string();
    assert!(rendered.contains("fn step_17()"), "{rendered}");
    assert!(rendered.contains("fn step_23()"), "{rendered}");
    assert!(
        !rendered.contains("fn step_1()"),
        "literal match only: {rendered}"
    );
    runtime.close().await;
}

/// A directory outline pages by declaration rows, so one file's declarations
/// can straddle a symbols page. Scout judges each file once, whole: the page
/// holding a file's first row judges all of its rows, and the next page skips
/// the rows it already judged.
#[tokio::test]
async fn symbols_scout_judges_each_file_once_across_outline_pages() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.4}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write(
        "src/a.rs",
        "fn a1() {}\nfn a2() {}\nfn a3() {}\nfn a4() {}\nfn a5() {}\n",
    );
    workspace.write("src/b.rs", "fn b1() {}\nfn b2() {}\n");
    let root = workspace.write("src/c.rs", "fn c1() {}\n");
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let mut input = json!({
        "id":"outline",
        "reasoning":"Pick a file.","mainGoal":"Which file declares the handler.",
        "resources":[{"id":"ast","tool":"astSearch","query":{
            "path":root,"operation":"symbols","pageSize":2
        }}],
        "questions":[{"id":"rel","type":"yesno","ask":"Relevant?"}]
    });
    let mut judged: Vec<(String, usize)> = Vec::new();
    for call in 0..6 {
        let outcome = runtime
            .execute(
                format!("outline-{call}"),
                "clasify".into(),
                verbose(input.clone()),
            )
            .await
            .expect("clasify");
        let query = &outcome.structured_content["queries"][0];
        for page in query["resources"][0]["pages"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let path = page["source"]["path"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let lines = page["hints"]["read"]["query"]["queries"][0]["ranges"][0]
                .as_str()
                .and_then(|window| window.split_once('-'))
                .and_then(|(_, end)| end.parse::<usize>().ok())
                .unwrap_or(0);
            judged.push((path, lines));
        }
        match query["next"].get("clasify") {
            Some(next) => input = next.clone(),
            None => break,
        }
    }
    let files = judged
        .iter()
        .map(|(path, _)| path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(files, ["src/a.rs", "src/b.rs", "src/c.rs"], "{judged:?}");
    runtime.close().await;
}

/// The default output is compact; `debug:true` returns the full receipt
/// plus provider usage.
#[tokio::test]
async fn a_matrix_compacts_by_default_and_debug_returns_the_full_receipt() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{
                "answer_0":{"type":"noul","noul":0.8},
                "answer_1":{"type":"choice","choice":"runtime","confidence":1.0,
                    "probabilities":{"runtime":1.0,"test":0.0}}
            },
            "usage":{"input_tokens":7,"output_tokens":2}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("trace.txt", "Evidence is present.\n");
    let runtime = provider_runtime(&workspace, &server);
    let mut unified = json!({
        "id":"m","mainGoal":"Decide whether the trace states the fact.",
        "reasoning":"The next read depends on it.",
        "resources":[{"id":"src","tool":"localFetch","query":{"path":file}}],
        "questions":[
            {"id":"present","type":"relevant","ask":"the fact"},
            {"id":"kind","type":"choice","ask":"Kind?","labels":{"runtime":null,"test":null}}
        ]
    });
    let compact = runtime
        .execute("unified".into(), "clasify".into(), call(unified.clone()))
        .await
        .expect("the matrix is valid");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    octocode_native::contracts::validate_output("clasify", &compact.structured_content)
        .expect("compact output contract");
    assert_eq!(
        compact.structured_content["queries"][0]["resources"][0],
        // The host never saw the delegated read: a positive verdict
        // carries the read of exactly the judged lines.
        json!({"id":"src","path":"trace.txt","totalLines":1,
            "pages":[{"line":1,"endLine":1,"answers":{"present":0.8,"kind":"runtime"},
                // A cross-tool lead starts its own step: no matrix brief.
                "hints":{"read":{"tool":"localFetch","query":{"queries":[{
                    "path":"trace.txt","ranges":["1-1"]}]}}}}]}),
        "{}",
        compact.structured_content
    );
    unified["debug"] = json!(true);
    unified["questions"][0]["ask"] = json!("the fact, judged again");
    let debug = runtime
        .execute("debug".into(), "clasify".into(), call(unified))
        .await
        .expect("debug is accepted");
    let query = &debug.structured_content["queries"][0];
    assert_eq!(query["usage"]["calls"], 1, "{query}");
    assert_eq!(query["usage"]["inputTokens"], 7);
    assert_eq!(query["usage"]["outputTokens"], 2);
    assert!(query["usage"]["ms"].is_u64());
    let page = &query["resources"][0]["pages"][0];
    assert_eq!(
        page["usage"],
        json!({"calls":1,"inputTokens":7,"outputTokens":2})
    );
    assert_eq!(page["answers"]["present"], json!({"yesno":0.8}));
    assert_eq!(page["answers"]["kind"]["probabilities"]["runtime"], 1.0);
    runtime.close().await;
}

#[tokio::test]
async fn search_candidates_above_max_chars_never_reach_the_provider() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.7))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let root = write_hit_files(&workspace, 1, 120);
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"capped","reasoning":"Bound candidate evidence.","mainGoal":"Decide the next read.",
        "resources":[{"id":"hits","maxChars":1,"candidateEvidence":"search","tool":"localSearch","query":{
            "path":root,"matchString":"needle","sort":"path","pageSize":3,"reasoning":"Find hits."
        }}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });
    let outcome = runtime
        .execute("capped".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    let cell = &query["resources"][0];
    assert_eq!(cell["coverage"], "error", "{cell}");
    let pages = cell["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 3, "every candidate stays visible: {cell}");
    for page in pages {
        assert_eq!(
            page["error"]["errorCode"], "classificationContextTooLarge",
            "{page}"
        );
        assert_eq!(
            page["hints"]["read"]["tool"], "localFetch",
            "an unjudged candidate keeps its read: {page}"
        );
    }
    assert!(no_continuation(query), "{query}");
    assert_eq!(query["usage"]["calls"], 0, "{query}");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("capped search output contract");
    runtime.close().await;
}

#[tokio::test]
async fn search_candidates_past_the_remaining_budget_resume_without_skips() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.4))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // One candidate carries 3 × 190 characters of snippets plus its row
    // fields; two never fit 1,100.
    let root = write_hit_files(&workspace, 3, 190);
    let runtime = provider_runtime(&workspace, &server);
    let mut input = json!({
        "id":"budget","reasoning":"Bound candidate evidence.","mainGoal":"Decide the next read.",
        "resources":[{"id":"hits","maxChars":1100,"candidateEvidence":"search","tool":"localSearch","query":{
            "path":root,"matchString":"needle","sort":"path","pageSize":3,"reasoning":"Find hits."
        }}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 5, "the walk must terminate");
        let outcome = runtime
            .execute(
                format!("budget-{calls}"),
                "clasify".into(),
                verbose(input.clone()),
            )
            .await
            .unwrap();
        octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
            .expect("budgeted search output contract");
        let query = &outcome.structured_content["queries"][0];
        for page in query["resources"][0]["pages"].as_array().unwrap() {
            assert!(page.get("error").is_none(), "{page}");
        }
        match query["next"].get("clasify") {
            Some(next) => {
                octocode_native::contracts::prepare_many_and_validate(
                    "clasify",
                    call(next.clone()),
                )
                .expect("next.clasify replays unchanged");
                input = next.clone();
            }
            None => break,
        }
    }
    let states = sent_states(&server).await;
    for marker in ["ALPHA", "BRAVO", "CHARLIE"] {
        assert_eq!(
            states
                .iter()
                .filter(|state| state.to_string().contains(marker))
                .count(),
            1,
            "{marker} must be judged exactly once: {states:#?}"
        );
    }
    assert_eq!(states.len(), 3);
    assert!(calls >= 2, "1,100 characters cannot hold two candidates");
    runtime.close().await;
}

#[tokio::test]
async fn list_items_above_max_chars_never_reach_the_provider() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.7))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write("src/a.rs", "fn alpha_handler() {}\n");
    let root = workspace.write("src/b.rs", "fn bravo_handler() {}\n");
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"outline","reasoning":"Pick a file.","mainGoal":"Which file declares the handler.",
        "resources":[{"id":"ast","maxChars":1,"tool":"astSearch","query":{
            "path":root,"operation":"symbols"
        }}],
        "questions":[{"id":"rel","type":"yesno","ask":"Relevant?"}]
    });
    let outcome = runtime
        .execute("outline".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let cell = &outcome.structured_content["queries"][0]["resources"][0];
    let pages = cell["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2, "{cell}");
    for page in pages {
        assert_eq!(
            page["error"]["errorCode"], "classificationContextTooLarge",
            "{page}"
        );
        assert!(page["hints"]["read"].is_object(), "{page}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn hydrated_candidates_share_one_max_chars_budget() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.4))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // Multi-byte text in three distant hit clusters per file: every window is
    // its own read, so a per-candidate split alone would overrun the budget.
    let body = |marker: &str| {
        (1..=400)
            .map(|line| {
                if [1, 200, 400].contains(&line) {
                    format!("needle {marker} ünïcödé ✓ {}\n", filler(40))
                } else {
                    format!("filler ëëëë {}\n", filler(30))
                }
            })
            .collect::<String>()
    };
    workspace.write("hyd/a.txt", body("ALPHA"));
    let root = workspace.write("hyd/b.txt", body("BRAVO"));
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let max_chars = 900;
    let input = json!({
        "id":"hydrated","reasoning":"Bound hydrated evidence.","mainGoal":"Decide the next read.",
        "resources":[{"id":"hits","maxChars":max_chars,"tool":"localSearch","query":{
            "path":root,"matchString":"needle","sort":"path","reasoning":"Find hits."
        },"candidateEvidence":"fileChunks"}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });
    runtime
        .execute("hydrated".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    fn content_chars(value: &serde_json::Value) -> usize {
        match value {
            serde_json::Value::Array(items) => items.iter().map(content_chars).sum(),
            serde_json::Value::Object(fields) => fields
                .get("content")
                .and_then(serde_json::Value::as_str)
                .map_or_else(
                    || fields.values().map(content_chars).sum(),
                    |content| content.chars().count(),
                ),
            _ => 0,
        }
    }
    let states = sent_states(&server).await;
    assert!(!states.is_empty());
    let total = states.iter().map(content_chars).sum::<usize>();
    assert!(
        total <= max_chars,
        "hydrated evidence {total} exceeds maxChars {max_chars}: {states:#?}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_hydrated_window_cut_by_its_budget_still_contains_its_hit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.9))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // One hit in the middle of long lines: the hit-centered window is several
    // times the read budget.
    let body = (1..=300)
        .map(|line| {
            if line == 150 {
                format!("needle DECIDING verdict-line {}\n", filler(70))
            } else {
                format!("prose {line} {}\n", filler(90))
            }
        })
        .collect::<String>();
    let root = workspace.write("cut/a.txt", body);
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let max_chars = 3000;
    let input = json!({
        "id":"cut","reasoning":"Judge the hit region.","mainGoal":"Decide the next read.",
        "resources":[{"id":"hits","maxChars":max_chars,"tool":"localSearch","query":{
            "path":root,"matchString":"needle DECIDING","reasoning":"Find hits."
        },"candidateEvidence":"fileChunks"}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Relevant?"}]
    });
    let outcome = runtime
        .execute("cut".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let states = sent_states(&server).await;
    assert_eq!(states.len(), 1, "{states:#?}");
    let state = states[0].to_string();
    assert!(
        state.contains("verdict-line"),
        "the judged window must contain its hit: {state}"
    );
    assert!(
        state.len() < 2 * max_chars + 2_000,
        "the window stays within its budget: {} chars",
        state.len()
    );
    let page = &outcome.structured_content["queries"][0]["resources"][0]["pages"][0];
    let scope = &page["scope"];
    let (start, end) = (
        scope["startLine"].as_u64().unwrap_or(0),
        scope["endLine"].as_u64().unwrap_or(0),
    );
    assert!(
        start <= 150 && 150 <= end,
        "the scope names the judged lines around the hit: {page:#?}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn sufficient_unread_file_evidence_returns_a_bounded_verification_read() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.95))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write(
        "policy.rs",
        "// header\nconst WIDE_RESULT_FILES: usize = 8;\nconst OTHER: usize = 3;\n// tail\n",
    );
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"threshold","reasoning":"Answer from the deciding source.","mainGoal":"Find the handoff threshold.",
        "resources":[{"id":"threshold","tool":"localFetch","query":{"path":file,"ranges":["2-3"]}}],
        "questions":[{"id":"sufficient","type":"sufficient","ask":"How many files trigger the handoff?"}]
    });
    let outcome = runtime
        .execute("threshold".into(), "clasify".into(), call(input))
        .await
        .unwrap();
    let resource = &outcome.structured_content["queries"][0]["resources"][0];
    let page = &resource["pages"][0];
    assert_eq!(page["answers"]["sufficient"], 0.95, "{resource}");
    let read = &page["hints"]["read"];
    assert_eq!(read["tool"], "localFetch", "{resource}");
    assert_eq!(read["query"]["queries"][0]["path"], "policy.rs", "{read}");
    assert_eq!(
        read["query"]["queries"][0]["ranges"],
        json!(["2-3"]),
        "{read}"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("verification read output contract");

    // Supplied evidence is already held: no read is invented for it.
    let held = json!({
        "id":"held","reasoning":"Judge held evidence.","mainGoal":"Find the handoff threshold.",
        "resources":[{"id":"held","value":"const WIDE_RESULT_FILES: usize = 8;"}],
        "questions":[{"id":"sufficient","type":"sufficient","ask":"How many files trigger the handoff?"}]
    });
    let outcome = runtime
        .execute("held".into(), "clasify".into(), call(held))
        .await
        .unwrap();
    assert!(
        !outcome.structured_content.to_string().contains("\"read\""),
        "{}",
        outcome.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn confident_negative_file_pages_add_no_read() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.05))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("other.rs", "const OTHER: usize = 3;\n");
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"negative","reasoning":"Answer from the deciding source.","mainGoal":"Find the handoff threshold.",
        "resources":[{"id":"other","tool":"localFetch","query":{"path":file}}],
        "questions":[{"id":"sufficient","type":"sufficient","ask":"How many files trigger the handoff?"}]
    });
    let outcome = runtime
        .execute("negative".into(), "clasify".into(), call(input))
        .await
        .unwrap();
    assert!(
        !outcome.structured_content.to_string().contains("\"read\""),
        "{}",
        outcome.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn an_oversized_next_page_shrinks_and_the_replay_advances() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.2))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("uneven.txt", uneven_lines());
    let runtime = provider_runtime(&workspace, &server);
    let mut input = json!({
        "id":"walk","reasoning":"Walk the file in bounded pages.","mainGoal":"Decide the next read.",
        "debug":true,
        "resources":[{"id":"doc","maxChars":400,"tool":"localFetch","query":{
            "path":file,"unit":"lines","length":8
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let mut covered = 0u64;
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 10, "the walk must terminate");
        let outcome = runtime
            .execute(
                format!("walk-{calls}"),
                "clasify".into(),
                call(input.clone()),
            )
            .await
            .unwrap();
        let query = &outcome.structured_content["queries"][0];
        for page in query["resources"][0]["pages"].as_array().unwrap() {
            assert!(page.get("error").is_none(), "replay must advance: {query}");
            let scope = &page["scope"];
            assert_eq!(
                scope["startLine"].as_u64().unwrap(),
                covered + 1,
                "no interval is skipped or repeated: {query}"
            );
            covered = scope["endLine"].as_u64().unwrap();
        }
        match query["next"].get("clasify") {
            Some(next) => {
                assert_eq!(next["queries"][0]["debug"], true, "{next}");
                input = next.clone();
            }
            None => break,
        }
    }
    assert_eq!(covered, 24);
    for state in sent_states(&server).await {
        assert!(
            state.to_string().chars().count() < 2_000,
            "every judged page stayed bounded: {state}"
        );
    }
    runtime.close().await;
}

/// Leftover of QA P5: a `ranges` read larger than `maxChars` is judged in
/// line windows that each fit, with `next.clasify` reading the rest of the
/// range, instead of failing `classificationContextTooLarge` with no read.
/// Every requested line is judged exactly once, and none outside the range.
#[tokio::test]
async fn an_oversized_ranges_read_is_judged_in_fitting_windows() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.2))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("uneven.txt", uneven_lines());
    let runtime = provider_runtime(&workspace, &server);
    let mut input = json!({
        "id":"ranges","reasoning":"Judge one range in bounded pages.","mainGoal":"Decide the next read.",
        "debug":true,
        "resources":[{"id":"doc","maxChars":400,"tool":"localFetch","query":{
            "path":file,"ranges":["3-22"]
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let mut covered = 2u64;
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 12, "the walk must terminate");
        let outcome = runtime
            .execute(
                format!("ranges-{calls}"),
                "clasify".into(),
                call(input.clone()),
            )
            .await
            .unwrap();
        let query = &outcome.structured_content["queries"][0];
        let resource = &query["resources"][0];
        assert!(
            resource.get("error").is_none(),
            "an oversized range is split, not failed: {query}"
        );
        for page in resource["pages"].as_array().unwrap() {
            assert!(page.get("error").is_none(), "{query}");
            let scope = &page["scope"];
            assert_eq!(
                scope["startLine"].as_u64().unwrap(),
                covered + 1,
                "no line is skipped or repeated: {query}"
            );
            covered = scope["endLine"].as_u64().unwrap();
        }
        match query["next"].get("clasify") {
            Some(next) => input = next.clone(),
            None => break,
        }
    }
    assert_eq!(covered, 22);
    assert!(calls > 1, "the range did not fit one call");
    runtime.close().await;
}

#[tokio::test]
async fn an_oversized_whole_file_read_shrinks_into_line_chunks() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.2))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("uneven.txt", uneven_lines());
    let runtime = provider_runtime(&workspace, &server);
    let mut input = json!({
        "id":"whole","reasoning":"Judge the whole file in bounded pages.","mainGoal":"Decide the next read.",
        "debug":true,
        "resources":[{"id":"doc","maxChars":400,"tool":"localFetch","query":{"path":file}}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let mut covered = 0u64;
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 10, "the walk must terminate");
        let outcome = runtime
            .execute(
                format!("whole-{calls}"),
                "clasify".into(),
                call(input.clone()),
            )
            .await
            .unwrap();
        let query = &outcome.structured_content["queries"][0];
        for page in query["resources"][0]["pages"].as_array().unwrap() {
            assert!(
                page.get("error").is_none(),
                "the whole file shrinks: {query}"
            );
            let scope = &page["scope"];
            assert_eq!(
                scope["startLine"].as_u64().unwrap(),
                covered + 1,
                "no interval is skipped or repeated: {query}"
            );
            covered = scope["endLine"].as_u64().unwrap();
        }
        match query["next"].get("clasify") {
            Some(next) => input = next.clone(),
            None => break,
        }
    }
    assert_eq!(covered, 24);
    for state in sent_states(&server).await {
        assert!(
            state.to_string().chars().count() < 2_000,
            "every judged page stayed bounded: {state}"
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_line_larger_than_the_whole_budget_is_terminal_not_a_repeating_continuation() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.2))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write(
        "wide.txt",
        format!("short one\n{}\nshort three\n", filler(500)),
    );
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"wide","reasoning":"Walk the file in bounded pages.","mainGoal":"Decide the next read.",
        "resources":[{"id":"doc","maxChars":100,"tool":"localFetch","query":{
            "path":file,"unit":"lines","length":1
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let outcome = runtime
        .execute("wide".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    let pages = query["resources"][0]["pages"].as_array().unwrap();
    assert!(pages[0].get("answers").is_some(), "{query}");
    let failed = pages.last().unwrap();
    assert_eq!(
        failed["error"]["errorCode"], "classificationContextTooLarge",
        "{query}"
    );
    assert!(
        no_continuation(query),
        "no continuation may replay the same oversized page: {query}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_changed_source_is_rejected_on_replay_instead_of_mixing_versions() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.2))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("doc.txt", uneven_lines());
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"versions","reasoning":"Walk the file in bounded pages.","mainGoal":"Decide the next read.",
        "resources":[{"id":"doc","maxChars":200,"tool":"localFetch","query":{
            "path":file,"unit":"lines","length":8
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let first = runtime
        .execute("versions-1".into(), "clasify".into(), call(input))
        .await
        .unwrap();
    let next = first.structured_content["queries"][0]["next"]["clasify"]["queries"][0].clone();
    assert!(next.is_object(), "{}", first.structured_content);
    let before = server.received_requests().await.unwrap().len();
    workspace.write("doc.txt", format!("changed\n{}", uneven_lines()));
    let replay = runtime
        .execute("versions-2".into(), "clasify".into(), verbose(next))
        .await
        .unwrap();
    let query = &replay.structured_content["queries"][0];
    let page = &query["resources"][0]["pages"][0];
    assert_eq!(page["error"]["errorCode"], "staleSnapshot", "{query}");
    assert!(page.get("answers").is_none(), "{query}");
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        before,
        "no page of the new version is judged"
    );
    runtime.close().await;
}

/// A located window on a walk that is still open is offered as `hints.read`
/// ahead of the walk, which stays in `next.clasify`; no prose repeats the
/// read. Every clasify continuation kind (the walk, the top read, row and
/// page reads, the literal search, chunk-page verification reads) runs
/// exactly as emitted, with and without the caller's brief.
#[tokio::test]
async fn every_clasify_continuation_kind_replays_verbatim() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(LocateTopPassage {
            top: 0.99,
            exists: 0.95,
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let source = (1..=4000_u64)
        .map(|line| format!("const walk_filler_{line} = {line}; // ordinary line\n"))
        .collect::<String>();
    let file = workspace.write("walk.js", source);
    let file = file.to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let mut kinds = std::collections::BTreeSet::new();
    for (debug, briefed, ask) in [
        (false, false, "Where the walk sets its filler value"),
        (true, false, "Where the walk sets its filler value"),
        (false, true, "Where the walk sets its filler value"),
    ] {
        let mut input = json!({
            "resources":[{"id":"f","maxChars":20000,"tool":"localFetch","query":{"path":file}}],
            "questions":[{"id":"t","type":"locate","ask":ask}]
        });
        if debug {
            input["debug"] = json!(true);
        }
        if briefed {
            input["mainGoal"] = json!("Find where the walk sets its value.");
            input["reasoning"] = json!("Read the deciding line.");
        }
        let mut calls = 0;
        loop {
            calls += 1;
            assert!(calls <= 12, "the walk must terminate");
            let outcome = runtime
                .execute(
                    format!("walk-{calls}"),
                    "clasify".into(),
                    call(input.clone()),
                )
                .await
                .unwrap();
            let output = &outcome.structured_content;
            octocode_native::contracts::validate_output("clasify", output)
                .expect("clasify output contract");
            let query = &output["queries"][0];
            if calls == 1 {
                assert!(
                    query["next"]["clasify"]["queries"][0].is_object(),
                    "the walk is open: {query}"
                );
                let tips = query["hints"]["text"].to_string();
                assert!(!tips.contains("hints.read"), "no read-first prose: {query}");
                if !debug {
                    assert_eq!(query["hints"]["read"]["tool"], "localFetch", "{query}");
                }
                let keys: Vec<&String> = query.as_object().unwrap().keys().collect();
                let position = |key: &str| keys.iter().position(|name| *name == key);
                assert!(
                    position("hints") < position("next"),
                    "the read comes before the walk: {keys:?}"
                );
            }
            match replay_every_continuation(&runtime, output, briefed, &mut kinds).await {
                Some(next) => input = next,
                None => break,
            }
        }
        assert!(calls >= 2, "{ask}: the file spans several calls");
    }
    // A literal target routes straight to its literal search (no walk).
    for (debug, briefed) in [(true, false), (false, true)] {
        let mut input = json!({
            "resources":[{"id":"f","maxChars":20000,"tool":"localFetch","query":{"path":file}}],
            "questions":[{"id":"t","type":"locate","ask":"Where walk_filler_7 is set"}]
        });
        if debug {
            input["debug"] = json!(true);
        }
        if briefed {
            input["mainGoal"] = json!("Find where the walk sets its value.");
            input["reasoning"] = json!("Read the deciding line.");
        }
        let outcome = runtime
            .execute("literal".into(), "clasify".into(), call(input))
            .await
            .unwrap();
        let output = &outcome.structured_content;
        octocode_native::contracts::validate_output("clasify", output)
            .expect("clasify output contract");
        assert!(
            replay_every_continuation(&runtime, output, briefed, &mut kinds)
                .await
                .is_none(),
            "no walk: {output}"
        );
    }

    // Chunk pages of a yes/no screen offer verification reads of exactly
    // their judged lines.
    let chunk_server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.6))
        .mount(&chunk_server)
        .await;
    let chunked = workspace.write("uneven.txt", uneven_lines());
    let chunk_runtime = provider_runtime(&workspace, &chunk_server);
    let mut input = json!({
        "resources":[{"id":"doc","maxChars":400,"tool":"localFetch","query":{
            "path":chunked,"unit":"lines","length":8
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let mut chunk_kinds = std::collections::BTreeSet::new();
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 10, "the walk must terminate");
        let outcome = chunk_runtime
            .execute(
                format!("chunk-{calls}"),
                "clasify".into(),
                call(input.clone()),
            )
            .await
            .unwrap();
        match replay_every_continuation(
            &chunk_runtime,
            &outcome.structured_content,
            false,
            &mut chunk_kinds,
        )
        .await
        {
            Some(next) => input = next,
            None => break,
        }
    }
    for kind in ["clasify:clasify", "row-or-page:localFetch"] {
        assert!(
            chunk_kinds.contains(kind),
            "chunk walk: {kind} not exercised: {chunk_kinds:?}"
        );
    }
    for kind in [
        "clasify:clasify",
        "read:localFetch",
        "row-or-page:localFetch",
        "textSearch:localSearch",
    ] {
        assert!(kinds.contains(kind), "{kind} not exercised: {kinds:?}");
    }
    runtime.close().await;
    chunk_runtime.close().await;
}

#[tokio::test]
async fn exhausted_provider_quota_fails_once_per_resource_and_is_remembered() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(402).set_body_json(json!({"detail":"no credit"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        // Its own account: the remembered 402 is per endpoint and key, and
        // pooled mock servers reuse ports across tests.
        ("OCTOCODE_CLASSIFICATION_API", "quota-secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("OCTOCODE_CLASSIFICATION_CONCURRENCY", "1".into()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"bill","reasoning":"r","mainGoal":"g",
        "resources":[
            {"id":"v1","value":"alpha one"},
            {"id":"v2","value":"beta two"}
        ],
        "questions":[
            {"id":"a","type":"yesno","ask":"Is alpha stated?"},
            {"id":"b","type":"yesno","ask":"Is beta stated?"},
            {"id":"c","type":"yesno","ask":"Is gamma stated?"}
        ]
    });
    let out = runtime
        .execute("quota".into(), "clasify".into(), call(input))
        .await
        .unwrap();
    let resources = out.structured_content["queries"][0]["resources"]
        .as_array()
        .unwrap();
    assert_eq!(resources.len(), 2);
    for resource in resources {
        assert_eq!(resource["coverage"], "error", "{resource}");
        assert_eq!(
            resource["error"]["errorCode"], "classificationQuotaExhausted",
            "{resource}"
        );
        assert!(resource.get("answers").is_none(), "{resource}");
    }
    // The first 402 stops the call: one provider request, not one per page.
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    // A walk that judged nothing offers no next.clasify: it would skip the
    // failed pages; the rerun is the original query.
    let file = workspace.write(
        "quota-walk.txt",
        (0..200)
            .map(|index| format!("line {index}\n"))
            .collect::<String>(),
    );
    let walk = json!({
        "id":"walk","reasoning":"r","mainGoal":"g",
        "resources":[{"id":"file","maxChars":5000,"tool":"localFetch","query":{
            "path":file,"length":1,"fullContent":false
        }}],
        "questions":[{"id":"relevant","type":"relevant","ask":"line content"}]
    });
    let out = runtime
        .execute("quota-walk".into(), "clasify".into(), call(walk))
        .await
        .unwrap();
    let row = &out.structured_content["queries"][0];
    assert_eq!(row["resources"][0]["coverage"], "error", "{row}");
    assert!(row.pointer("/next/clasify").is_none(), "{row}");
    // The 402 is remembered across calls: the walk sent no request and
    // answers with the same error (N12: its bounded reads still run, so
    // per-page input checks come first) and the read the host runs instead
    // (CL1): the resource's own query, valid input for its tool.
    assert_eq!(
        row["resources"][0]["error"]["errorCode"], "classificationQuotaExhausted",
        "{row}"
    );
    let read = resource_read(&row["resources"][0]).unwrap_or_else(|| panic!("read: {row}"));
    assert_eq!(read["tool"], "localFetch", "{row}");
    octocode_native::contracts::validate_query("localFetch", lead_row(&read["query"]))
        .expect("the read lead is valid localFetch input");
    assert_eq!(
        out.exit_class(),
        octocode_native::runtime::ExitClass::Failed(
            octocode_native::runtime::FailureKind::Execution
        ),
        "{row}"
    );
    // N12: inside the remembered 402, a per-page input error (locate over a
    // minified view) is still the caller's error, not the quota.
    let minified = json!({
        "id":"min","reasoning":"r","mainGoal":"g",
        "resources":[{"id":"file","tool":"localFetch","query":{"path":file,"ranges":["1-40"],"minify":"standard"}}],
        "questions":[{"id":"where","type":"locate","ask":"the line that names line 7"}]
    });
    let out = runtime
        .execute("quota-minified".into(), "clasify".into(), call(minified))
        .await
        .unwrap();
    let row = &out.structured_content["queries"][0];
    assert!(
        row.to_string().contains("classificationLocateUnsupported"),
        "{row}"
    );
    assert_eq!(
        out.exit_class(),
        octocode_native::runtime::ExitClass::InvalidInput,
        "{row}"
    );
    // The remembered 402 still routes a literal target: the tip and its
    // exact search are local facts, not provider answers.
    let named = json!({
        "id":"named","reasoning":"r","mainGoal":"g",
        "resources":[{"id":"file","tool":"localFetch","query":{"path":file,"ranges":["1-40"]}}],
        "questions":[{"id":"h","type":"locate","ask":"Where is serverCron defined?"}]
    });
    let out = runtime
        .execute("quota-named".into(), "clasify".into(), call(named.clone()))
        .await
        .unwrap();
    let row = &out.structured_content["queries"][0];
    assert!(
        row["hints"]["text"]
            .as_array()
            .is_some_and(|tips| tips.iter().any(|tip| tip
                .as_str()
                .is_some_and(|tip| tip.contains("serverCron") && tip.contains("localSearch")))),
        "{row}"
    );
    assert_eq!(row["hints"]["textSearch"]["tool"], "localSearch", "{row}");
    // MCP sees the same routes.
    let out = runtime
        .execute_mcp("quota-named-mcp".into(), "clasify".into(), call(named))
        .await
        .unwrap();
    let row = &out["structuredContent"]["queries"][0];
    assert_eq!(row["hints"]["textSearch"]["tool"], "localSearch", "{row}");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    runtime.close().await;
}

/// A provider refusal over a fanned-out search (E7): the refusal is stated
/// once on the resource, not on every page; pages a spent page budget left
/// unjudged keep their own error; every captured window keeps its read as
/// the host's fallback; and the candidates past the captured page are
/// counted, since a walk that judged nothing offers no next.clasify.
#[tokio::test]
async fn provider_refusal_is_stated_once_and_keeps_every_captured_read() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(402).set_body_json(json!({"detail":"no credit"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // 43 files, each with three hit clusters far apart: 40 hydrate (the
    // fileChunks cap), and their windows exceed the page budget.
    for file in 0..43 {
        let mut body = String::new();
        for line in 1..=900 {
            // Long filler keeps each file over one page (hit windows, not whole).
            body.push_str(if line % 400 == 10 {
                "needle marker\n"
            } else {
                "filler line with enough padding to keep this file over one page\n"
            });
        }
        workspace.write(&format!("src/f{file}.txt"), body);
    }
    let root = workspace
        .workspace
        .join("src")
        .to_string_lossy()
        .into_owned();
    let runtime = workspace.runtime(&[
        // Its own account: the remembered 402 is per endpoint and key.
        ("OCTOCODE_CLASSIFICATION_API", "quota-fanout-secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("OCTOCODE_CLASSIFICATION_CONCURRENCY", "1".into()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"refused","reasoning":"r","mainGoal":"Find the deciding line.",
        "resources":[{"id":"hits","tool":"localSearch","candidateEvidence":"fileChunks",
            "query":{"path":root,"matchString":"needle"}}],
        "questions":[
            {"id":"decides","type":"yesno","ask":"Does this source decide the answer?"},
            {"id":"names","type":"yesno","ask":"Does this source name the marker?"}
        ]
    });
    let out = runtime
        .execute("refused".into(), "clasify".into(), call(input))
        .await
        .unwrap();
    let row = &out.structured_content["queries"][0];
    let resource = &row["resources"][0];
    assert_eq!(resource["coverage"], "error", "{row}");
    assert_eq!(
        resource["error"]["errorCode"], "classificationQuotaExhausted",
        "{row}"
    );
    let pages = resource["pages"].as_array().cloned().unwrap_or_default();
    let refusals = out
        .structured_content
        .to_string()
        .matches("classificationQuotaExhausted")
        .count();
    assert_eq!(refusals, 1, "the refusal is stated once: {row}");
    assert!(
        pages
            .iter()
            .filter(|page| page.get("error").is_some())
            .all(|page| page["error"]["errorCode"] == "classificationBudgetSpent"),
        "{row}"
    );
    assert!(
        pages.len() > 5
            && pages
                .iter()
                .all(|page| page.pointer("/hints/read").is_some()),
        "every captured window keeps its read: {row}"
    );
    assert!(row.pointer("/next/clasify").is_none(), "{row}");
    assert!(
        resource["limitations"]
            .as_array()
            .is_some_and(|limits| limits.iter().any(|limit| limit
                .as_str()
                .is_some_and(|limit| limit.starts_with("3 more search candidates")))),
        "the uncaptured candidates are counted: {row}"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    octocode_native::contracts::validate_output("clasify", &out.structured_content)
        .expect("refusal output contract");
    runtime.close().await;
}

/// GitHub repository search pages sliced by `page`/`per_page`, as GitHub
/// serves them.
#[derive(Clone)]
struct RepoSearchPages {
    repos: usize,
}

impl Respond for RepoSearchPages {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let param = |name: &str, default: usize| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<usize>().ok())
                .unwrap_or(default)
        };
        let (page, per) = (param("page", 1).max(1), param("per_page", 30).max(1));
        let items = ((page - 1) * per..(page * per).min(self.repos))
            .map(|n| {
                json!({"full_name": format!("o/repo{n}"), "name": format!("repo{n}"),
                       "owner": {"login": "o"}, "html_url": "https://x", "default_branch": "main",
                       "description": format!("Repository {n} {}", "detail ".repeat(20)),
                       "archived": false})
            })
            .collect::<Vec<_>>();
        ResponseTemplate::new(200).set_body_json(json!({
            "total_count": self.repos, "incomplete_results": false, "items": items
        }))
    }
}

/// P3: a list resource whose `maxChars` fits only some candidates of a page
/// defers the rest to `next.clasify`, which resumes at the first deferred
/// candidate (a page that starts there), so the walk judges every
/// candidate exactly once instead of skipping to the next page.
#[tokio::test]
async fn a_list_walk_rejudges_budget_deferred_candidates() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.6}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(RepoSearchPages { repos: 12 })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("GITHUB_TOKEN", "clasify-list-budget-fixture".into()),
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let mut next = Some(json!({
        "reasoning":"Judge the repositories.","mainGoal":"Which repository holds the parser.",
        "resources":[{"id":"repos","tool":"ghSearchRepo","maxChars":500,
            "query":{"keywords":["parser"],"pageSize":5}}],
        "questions":[{"id":"a","type":"yesno","ask":"Does this repository hold the parser?"}]
    }));
    let mut judged = Vec::<String>::new();
    let mut pages = Vec::<(u64, u64)>::new();
    let mut calls = 0;
    while let Some(input) = next.take() {
        calls += 1;
        let query = &input
            .pointer("/queries/0/resources/0/query")
            .unwrap_or(&input["resources"][0]["query"]);
        pages.push((
            query["page"].as_u64().unwrap_or(1),
            query["pageSize"].as_u64().unwrap_or(0),
        ));
        assert!(calls <= 20, "walk did not finish");
        let outcome = runtime
            .execute(format!("list-{calls}"), "clasify".into(), verbose(input))
            .await
            .expect("clasify");
        let row = &outcome.structured_content["queries"][0];
        assert!(
            !row.to_string().contains("classificationBudgetSpent"),
            "call {calls}: a deferred candidate was left unjudged: {row}"
        );
        for page in row["resources"][0]["pages"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if page.get("answers").is_some() {
                let item = page
                    .pointer("/source/item")
                    .or_else(|| page.get("item"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("call {calls}: page names its item: {page}"));
                judged.push(item.to_owned());
            }
        }
        next = row.pointer("/next/clasify").cloned();
    }
    let mut sorted = judged.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), judged.len(), "judged twice: {judged:?}");
    let mut expected = (0..12).map(|n| format!("o/repo{n}")).collect::<Vec<_>>();
    expected.sort();
    assert_eq!(sorted, expected, "every candidate judged: {judged:?}");
    // After a resume re-pages mid-page, the walk steps back to the
    // original 5-row pages at the next aligned boundary.
    assert_eq!(
        pages,
        vec![(1, 5), (2, 2), (5, 1), (2, 5), (8, 1), (5, 2), (3, 5)],
        "{pages:?}"
    );
    runtime.close().await;
}

/// QA P5 was reproduced on a GitHub file: a `ghGetFileContent` `ranges` read
/// larger than `maxChars` is judged in fitting line windows, with
/// `next.clasify` reading the rest, every requested line exactly once.
#[tokio::test]
async fn an_oversized_github_ranges_read_is_judged_in_fitting_windows() {
    use base64::Engine as _;
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(noul_response(0.2))
        .mount(&server)
        .await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let source = uneven_lines();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/contents/uneven.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64",
            "content": base64::engine::general_purpose::STANDARD.encode(&source),
            "size": source.len(), "sha": "f".repeat(40), "path":"uneven.txt"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("GITHUB_TOKEN", "clasify-gh-ranges-fixture".into()),
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let mut input = json!({
        "id":"ghranges","reasoning":"Judge one range in bounded pages.","mainGoal":"Decide the next read.",
        "debug":true,
        "resources":[{"id":"doc","maxChars":400,"tool":"ghGetFileContent","query":{
            "owner":"o","repo":"r","path":"uneven.txt","ref":"main","ranges":["3-22"]
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let mut covered = 2u64;
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 12, "the walk must terminate");
        let outcome = runtime
            .execute(
                format!("ghranges-{calls}"),
                "clasify".into(),
                call(input.clone()),
            )
            .await
            .unwrap();
        let query = &outcome.structured_content["queries"][0];
        let resource = &query["resources"][0];
        assert!(resource.get("error").is_none(), "{query}");
        for page in resource["pages"].as_array().unwrap() {
            assert!(page.get("error").is_none(), "{query}");
            let scope = &page["scope"];
            assert_eq!(
                scope["startLine"].as_u64().unwrap(),
                covered + 1,
                "no line is skipped or repeated: {query}"
            );
            covered = scope["endLine"].as_u64().unwrap();
        }
        match query["next"].get("clasify") {
            Some(next) => input = next.clone(),
            None => break,
        }
    }
    assert_eq!(covered, 22);
    assert!(calls > 1, "the range did not fit one call");
    runtime.close().await;
}

/// A single line inside a `ranges` read that alone exceeds `maxChars` cannot
/// be judged, but stays reachable: its page carries an executable read, and
/// the lines after it continue through `next.clasify`.
#[tokio::test]
async fn an_unsplittable_line_in_a_ranges_read_keeps_its_read_and_the_rest() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.2))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write(
        "wide.txt",
        format!("short one\n{}\nshort three\nshort four\n", filler(500)),
    );
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"wide","reasoning":"Judge one range in bounded pages.","mainGoal":"Decide the next read.",
        "resources":[{"id":"doc","maxChars":100,"tool":"localFetch","query":{
            "path":file,"ranges":["2-4"]
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let outcome = runtime
        .execute("wide-range".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    let resource = &query["resources"][0];
    let failed = resource["pages"]
        .as_array()
        .and_then(|pages| pages.iter().find(|page| page.get("error").is_some()))
        .unwrap_or(resource);
    assert_eq!(
        failed["error"]["errorCode"], "classificationContextTooLarge",
        "{query}"
    );
    let read = resource_read(resource)
        .or_else(|| failed["hints"]["read"].as_object().cloned().map(Into::into))
        .unwrap_or_else(|| panic!("the oversized line keeps an executable read: {query}"));
    assert_eq!(read["tool"], "localFetch", "{query}");
    assert_eq!(
        lead_row(&read["query"])["ranges"],
        json!(["2-2"]),
        "{query}"
    );
    let next = &query["next"]["clasify"];
    assert_eq!(
        next["queries"][0]["resources"][0]["query"]["ranges"],
        json!(["3-4"]),
        "the lines after the oversized one continue: {query}"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("output contract");
    runtime.close().await;
}

/// A path list (structureSearch, ghStructure, astTopology) stays one page, so
/// `relevant`/`sufficient` over it is one whole-list verdict. The verdict is
/// kept and one tip routes the agent to `choice` over the paths; a `choice`
/// matrix gets no tip.
#[tokio::test]
async fn whole_list_questions_over_a_path_list_hint_toward_choice() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.6))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write("src/alpha.rs", "fn alpha() {}\n");
    let file = workspace.write("src/bravo.rs", "fn bravo() {}\n");
    let dir = file.parent().unwrap().to_path_buf();
    let runtime = provider_runtime(&workspace, &server);
    let matrix = |id: &str, question: serde_json::Value| {
        json!({
            "id":id,"reasoning":"Pick the file to read.","mainGoal":"Find the bravo handler.",
            "resources":[{"id":"tree","tool":"structureSearch","query":{"operation":"files","path":dir}}],
            "questions":[question]
        })
    };
    let tip = |query: &serde_json::Value| {
        query["hints"]["text"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .find(|hint| hint.contains("choice"))
            .map(str::to_owned)
    };
    for kind in ["relevant", "sufficient"] {
        let outcome = runtime
            .execute(
                format!("list-{kind}"),
                "clasify".into(),
                call(matrix(
                    kind,
                    json!({"id":"q","type":kind,"ask":"Which file defines bravo?"}),
                )),
            )
            .await
            .unwrap();
        let query = &outcome.structured_content["queries"][0];
        assert!(
            compact_answer(&query["resources"][0], "q").is_number(),
            "{query}"
        );
        let tip = tip(query).unwrap_or_else(|| panic!("no choice tip: {query}"));
        assert!(tip.chars().count() <= 120, "{tip}");
        octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
            .expect("output contract");
    }
    let outcome = runtime
        .execute(
            "list-choice".into(),
            "clasify".into(),
            call(matrix(
                "choice",
                json!({"id":"q","type":"choice","ask":"Does this list name the bravo file?",
                    "labels":{"yes":"Names it","no":"Does not"}}),
            )),
        )
        .await
        .unwrap();
    assert!(
        tip(&outcome.structured_content["queries"][0]).is_none(),
        "{}",
        outcome.structured_content
    );
    runtime.close().await;
}

/// Every page of a multi-page file read is pinned to the captured version:
/// page 1's verification read carries the same `snapshot` as page 2's, so a
/// changed file fails the read instead of returning other lines.
#[tokio::test]
async fn every_page_read_of_a_paged_file_pins_the_captured_snapshot() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.95))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let body = (1..=24).map(|n| format!("line {n}\n")).collect::<String>();
    let file = workspace.write("doc.txt", body);
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"pinned","reasoning":"Walk the file in pages.","mainGoal":"Find the marker.",
        "resources":[{"id":"doc","tool":"localFetch","query":{"path":file,"unit":"lines","length":8}}],
        "questions":[{"id":"s","type":"sufficient","ask":"Is the marker stated?"}]
    });
    let outcome = runtime
        .execute("pinned".into(), "clasify".into(), call(input))
        .await
        .unwrap();
    let resource = &outcome.structured_content["queries"][0]["resources"][0];
    let pages = resource["pages"]
        .as_array()
        .unwrap_or_else(|| panic!("{resource}"));
    assert!(pages.len() >= 2, "{resource}");
    let snapshots = pages
        .iter()
        .map(|page| lead_row(&page["hints"]["read"]["query"])["snapshot"].clone())
        .collect::<Vec<_>>();
    assert!(
        snapshots[0].is_string(),
        "page 1 read is pinned: {resource}"
    );
    assert!(
        snapshots.iter().all(|snapshot| *snapshot == snapshots[0]),
        "{resource}"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("output contract");
    runtime.close().await;
}
