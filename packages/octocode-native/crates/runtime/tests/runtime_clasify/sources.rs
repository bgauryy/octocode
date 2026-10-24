//! Captured sources: local ranges, multi-page captures, the read lead, oversized sources and GitHub refs.

use super::{clasify, content_row, noul, provider, sent};
use crate::support::{Run as _, Workspace, clasify_runtime, github_api};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
        vec![json!({"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"docs/guide.md","ranges":[{"startLine":2,"endLine":4},{"startLine":40,"endLine":41}]}},
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
    let result = &out["results"][0]["data"];
    assert_eq!(result["answers"]["inside"]["noul"], 0.9);
    let read = &result["next"]["read"];
    assert_eq!(read["tool"], "localFetch");
    crate::support::assert_cursor_only(&read["input"]);
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
        vec![json!({"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"data/rows.txt","ranges":[{"startLine":1,"endLine":400},{"startLine":600,"endLine":1000}]}},
            "questions":{"q":noul("Does every selected row say in?")}
        })],
    )
    .await;
    assert!(out["results"][0]["data"].get("answers").is_some(), "{out}");
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
        vec![json!({"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"src/a.rs"}},
            "questions":{"b":noul("Does the file define b?")}
        })],
    )
    .await;
    let read = out["results"][0]["data"]["next"]["read"].clone();
    let replay = runtime
        .run(
            "read-lead".into(),
            read["tool"].as_str().unwrap().into(),
            read["input"].clone(),
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
            read["input"].clone(),
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
            json!({"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"big.txt"}},
                "questions":{"q":noul("Is it big?")}}),
            json!({"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"missing.txt","ranges":[{"startLine":1,"endLine":2}]}},
                "questions":{"q":noul("Is it there?")}}),
            content_row(),
        ],
    )
    .await;
    let rows = out["results"].as_array().unwrap();
    assert_eq!(
        rows[0]["data"]["errorCode"], "classificationContextTooLarge",
        "{out}"
    );
    assert!(rows[0]["data"].get("answers").is_none());
    assert_eq!(rows[1]["data"]["errorCode"], "pathNotFound", "{out}");
    assert_eq!(
        rows[2]["data"]["answers"]["restart"]["noul"], 0.9,
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
        json!({"operation":"evaluate","source":{"tool":"ghGetFileContent","query":{
            "owner":"a","repo":"b","path":"src/lib.rs","ref":reference}},
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
    for (index, (_, _, text)) in shas.iter().enumerate() {
        let lead = &out["results"][index]["data"]["next"]["read"];
        assert_eq!(lead["tool"], "ghGetFileContent");
        crate::support::assert_cursor_only(&lead["input"]);
        let read = runtime
            .run(
                format!("inspect-{index}"),
                "ghGetFileContent".into(),
                lead["input"].clone(),
            )
            .await
            .expect("pinned source read");
        assert!(
            read.structured_content["results"][0]["data"]["content"]
                .as_str()
                .unwrap()
                .contains(text)
        );
    }
    runtime.close().await;
}
