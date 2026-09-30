#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

mod support;

use serde_json::{Value, json};
use std::collections::BTreeSet;
use support::Workspace;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[derive(Clone)]
struct CandidateJudgment;

impl Respond for CandidateJudgment {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).expect("provider request");
        let relevant = body["state"]
            .to_string()
            .contains("retries stop after three failed attempts");
        let answers = body["questions"]
            .as_object()
            .expect("grouped questions")
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    json!({"type":"noul", "noul": if relevant { 0.98 } else { 0.05 }}),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        ResponseTemplate::new(200).set_body_json(json!({
            "model":"fixture", "answers":answers,
            "usage":{"input_tokens":2,"output_tokens":1}
        }))
    }
}

async fn assert_candidate_walk(total: usize, page: u64, page_size: u64, expected: u64) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(CandidateJudgment)
        .expect(expected)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for index in 0..total {
        let content = if index + 1 == total {
            "retry delay policy\nretries stop after three failed attempts\n"
        } else {
            "retry delay example\nunrelated logging example\n"
        };
        workspace.write(&format!("src/file{index:02}.txt"), content);
    }
    let root = workspace.write("src/root.txt", "no match\n");
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "fixture-key".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", "30000".into()),
    ]);
    let search = json!({
        "goal":"How does the retry policy stop repeated failed attempts?",
        "reasoning":"Select the deciding source among candidates and preserve exceptions.",
        "path":root.parent().unwrap(), "searchText":"retry delay",
        "sort":"path", "page":page, "pageSize":page_size, "contextLines":1
    });
    let outcome = runtime
        .execute(
            "search-handoff".into(),
            "localSearch".into(),
            search.clone(),
        )
        .await
        .expect("search");
    let mut matrix =
        outcome.structured_content["results"][0]["data"]["next"]["clasify"]["query"].clone();
    assert_eq!(matrix["reasoning"], search["reasoning"]);
    assert_eq!(matrix["resources"][0]["context"]["tool"], "localSearch");
    assert_eq!(
        matrix["resources"][0]["context"]["candidateEvidence"],
        "fileChunks"
    );
    assert_eq!(matrix["questions"][1]["questionType"], "sufficient");
    let mut visited = BTreeSet::new();
    let mut deciding_read = None;
    let mut complete = false;
    // Every call screens at least one candidate.
    for _ in 0..expected {
        let outcome = runtime
            .execute("classify-handoff".into(), "clasify".into(), matrix)
            .await
            .expect("emitted matrix replays");
        octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
            .expect("output contract");
        let query = &outcome.structured_content["queries"][0];
        for page in query["resources"][0]["pages"].as_array().unwrap() {
            let path = page["source"]["path"].as_str().expect("candidate path");
            assert!(
                visited.insert(path.to_owned()),
                "repeated candidate: {path}"
            );
            assert!(page["answers"]["relevant"]["noul"].is_number(), "{page}");
            assert!(page["answers"]["sufficient"]["noul"].is_number(), "{page}");
            if page["answers"]["relevant"]["noul"].as_f64().unwrap() > 0.9 {
                assert!(
                    path.ends_with(&format!("file{:02}.txt", total - 1)),
                    "{page}"
                );
                assert_eq!(page["answers"]["sufficient"]["noul"], 0.98);
                deciding_read = Some(page["next"]["read"].clone());
            }
        }
        let Some(next) = query.pointer("/next/clasify") else {
            assert_eq!(query["resources"][0]["coverage"], "complete");
            complete = true;
            break;
        };
        matrix = next.clone();
    }
    assert!(complete, "the candidate walk must terminate");
    assert_eq!(
        visited.len() as u64,
        expected,
        "every candidate remains reachable"
    );
    let read = deciding_read.expect("the last file must be screened");
    let verified = runtime
        .execute(
            "verify-source".into(),
            read["tool"].as_str().unwrap().into(),
            read["query"].clone(),
        )
        .await
        .expect("exact read replays");
    assert!(
        verified.structured_content["results"][0]["data"]["content"]
            .as_str()
            .unwrap()
            .contains("retries stop after three failed attempts")
    );
    runtime.close().await;
}

#[tokio::test]
async fn search_handoff_screens_late_candidates_and_replays_their_exact_read() {
    assert_candidate_walk(15, 1, 20, 15).await;
}

#[tokio::test]
async fn later_search_handoff_preserves_the_starting_candidate_when_bounded() {
    assert_candidate_walk(31, 2, 16, 15).await;
}
