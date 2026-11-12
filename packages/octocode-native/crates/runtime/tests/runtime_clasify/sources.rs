//! Captured sources: local ranges, multi-page captures, the read lead, oversized sources and GitHub refs.

use super::{Typed, clasify, content_row, noul, provider, sent};
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
    // The mock's companion points at the first part: the read is its line.
    assert!(content.contains("fn a() {}"), "{content}");
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

/// A text judged whole names the lines its companion choice pointed to, in
/// the same one request: the marked text is sent, `next.read` reads only
/// those lines, `readChars` counts them, and no sections call is offered to
/// locate the yes. Literal text names its lines too.
#[tokio::test]
async fn a_whole_window_answer_names_its_deciding_lines_in_the_same_request() {
    let server = provider(2).await;
    let workspace = Workspace::new();
    let line = |n: usize| format!("test case_{n:04} ... ok\n");
    let lines = 3_000;
    let body: String = (1..=lines).map(line).collect();
    workspace.write("out.log", &body);
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![
            json!({"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"out.log"}},
                "questions":{"fail":noul("Does a test fail?")}}),
            json!({"operation":"evaluate","content":body,
                "questions":{"fail":noul("Does a test fail in this output?")}}),
        ],
    )
    .await;
    // A part is about one automatic read page of shown characters (each line
    // and its gutter); the mock's companion picks the first part.
    let page = octocode_native::execution::RuntimeLimits::DEFAULT.auto_page_chars;
    let shown: usize = (1..=lines)
        .map(|n| line(n).chars().count() + n.to_string().len() + 1)
        .sum();
    let parts = shown.div_ceil(page).max(2);
    let data = &out["results"][0]["data"];
    assert_eq!(data["answers"]["fail"]["noul"], 0.9, "{out}");
    let decided = &data["answers"]["fail"]["lines"];
    assert_eq!(decided["startLine"], 1, "{out}");
    let end = usize::try_from(decided["endLine"].as_u64().expect("end")).unwrap();
    assert!(end < lines, "{out}");
    let first: usize = (1..=end)
        .map(|n| line(n).chars().count() + n.to_string().len() + 1)
        .sum();
    assert!(first <= shown.div_ceil(parts) + 16, "one page: {first}");
    assert_eq!(
        data["readChars"],
        json!((1..=end).map(|n| line(n).chars().count()).sum::<usize>())
    );
    assert!(data.get("related").is_none(), "{data}");
    assert!(
        data["usage"]["calls"]
            .as_u64()
            .is_none_or(|calls| calls == 1),
        "one request: {data}"
    );
    let literal = &out["results"][1]["data"];
    assert_eq!(literal["answers"]["fail"]["lines"], *decided);
    assert!(literal["next"].get("read").is_none(), "{literal}");
    for request in sent(&server).await {
        let questions = request["questions"].as_object().expect("questions");
        assert_eq!(questions.len(), 2, "the question and its companion");
        assert_eq!(questions["answer_1"]["type"], "choice");
        assert_eq!(
            questions["answer_1"]["criteria"].as_object().unwrap().len(),
            parts + 1,
            "{parts} parts and unknown"
        );
        let text = request["state"]["text"]
            .as_str()
            .or(request["state"].as_str())
            .unwrap();
        assert!(
            text.starts_with(&format!("[p1]\n{}", line(1))),
            "{text:.80}"
        );
    }
    let read = data["next"]["read"].clone();
    let replay = runtime
        .run(
            "deciding-lines".into(),
            read["tool"].as_str().unwrap().into(),
            read["input"].clone(),
        )
        .await
        .expect("lead runs");
    let content = replay.structured_content["results"][0]["data"]["content"]
        .as_str()
        .unwrap();
    assert!(
        content.contains(&format!("case_{end:04} ")),
        "{content:.200}"
    );
    assert!(
        !content.contains(&format!("case_{:04} ", end + 1)),
        "{content:.200}"
    );
    runtime.close().await;
}

/// B210: an identical row recaptures its local source on every call, so an
/// edit that keeps the size and restores the modification time is judged on
/// its new bytes; no call replays a saved judgment.
#[tokio::test]
async fn an_identical_row_rejudges_a_local_source_after_an_edit() {
    let server = provider(3).await;
    let workspace = Workspace::new();
    let original = "Restart the service after rotation.\n";
    let edited = "Never restart; keys reload live now\n";
    assert_eq!(original.len(), edited.len());
    workspace.write("ops/runbook.md", original);
    let file = workspace.workspace.join("ops/runbook.md");
    let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = || {
        json!({"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"ops/runbook.md"}},
            "questions":{"restart":noul("Does rotation require a restart?")}
        })
    };
    let first = clasify(&runtime, vec![row()]).await;
    let unchanged = clasify(&runtime, vec![row()]).await;
    assert!(
        first["results"][0]["data"].get("replayed").is_none(),
        "{first}"
    );
    assert!(
        unchanged["results"][0]["data"].get("replayed").is_none(),
        "{unchanged}"
    );
    workspace.write("ops/runbook.md", edited);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let after_edit = clasify(&runtime, vec![row()]).await;
    assert!(
        after_edit["results"][0]["data"].get("replayed").is_none(),
        "{after_edit}"
    );
    let states = sent(&server).await;
    assert_eq!(states.len(), 3);
    assert_eq!(states[0]["state"]["text"], original);
    assert_eq!(states[1]["state"]["text"], original);
    assert_eq!(states[2]["state"]["text"], edited);
    runtime.close().await;
}

/// A provider that refuses any state over `limit` body bytes in its default
/// window, and any at all over `long_limit`, as `max_tokens_exceeded`. Its
/// windows are the stated token windows, so a taken request reports the
/// tokens its bytes cost at that rate.
pub(super) async fn window_provider(limit: usize, long_limit: usize) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(move |request: &wiremock::Request| {
            let long = request
                .headers
                .get("typesafe-danger-zone-long-context")
                .is_some();
            let (bytes, tokens) = if long {
                (long_limit, crate::support::LONG_WINDOW_TOKENS)
            } else {
                (limit, crate::support::WINDOW_TOKENS)
            };
            if request.body.len() > bytes {
                return ResponseTemplate::new(400)
                    .set_body_json(json!({"detail":{"error_type":"max_tokens_exceeded"}}));
            }
            let mut reply = Typed::reply(request);
            reply["usage"]["input_tokens"] = json!((request.body.len() * tokens).div_ceil(bytes));
            ResponseTemplate::new(200).set_body_json(reply)
        })
        .mount(&server)
        .await;
    server
}

/// A source past the provider's windows is judged in sections and answered
/// once per question; a missing sibling fails alone and another keeps its
/// answer.
#[tokio::test]
async fn an_oversized_source_is_answered_from_its_sections_beside_its_siblings() {
    let server = window_provider(100_000, 100_000).await;
    let workspace = Workspace::new();
    workspace.write(
        "big.txt",
        "x".repeat(80) + &"\nfiller line of text".repeat(14_000),
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
    let big = &rows[0]["data"];
    assert!(big.get("errorCode").is_none(), "{out}");
    assert_eq!(big["answers"]["q"]["noul"], 0.9, "{out}");
    assert!(big["answers"]["q"]["lines"]["startLine"].is_u64(), "{out}");
    let sections = big["coverage"]["totalSections"].as_u64().unwrap();
    assert!(sections > 1, "{out}");
    assert_eq!(big["coverage"]["answered"], json!(sections), "{out}");
    // The refused whole-source request was sent too: the row counts it. A
    // source too large to be shown to fit the long window is not sent there.
    assert_eq!(big["usage"]["calls"], json!(sections + 1), "{out}");
    let detail = &big["related"][0];
    assert_eq!(detail["tool"], "clasify", "{out}");
    let query = &detail["input"]["queries"][0];
    assert_eq!(query["operation"], "sections", "{out}");
    assert_eq!(
        query["resources"]["source"]["query"]["path"], "big.txt",
        "{out}"
    );
    assert_eq!(rows[1]["data"]["errorCode"], "notFound", "{out}");
    assert_eq!(
        rows[2]["data"]["answers"]["restart"]["noul"], 0.9,
        "a sibling keeps its answer"
    );
    assert_eq!(sent(&server).await.len() as u64, sections + 2);
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

/// The default window serves whatever it can: only the provider's refusal asks
/// for the slower, less accurate long one, and `false` never does.
#[tokio::test]
async fn an_omitted_long_context_asks_for_the_long_window_only_after_a_refusal() {
    let server = window_provider(30_000, 1_000_000).await;
    let workspace = Workspace::new();
    workspace.write(
        "mid.txt",
        "x".repeat(80) + &"\nfiller line of text".repeat(3000),
    );
    workspace.write("small.txt", "short text\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |file: &str, long_context: Option<bool>| {
        let mut row = json!({"operation":"evaluate","source":{"tool":"localFetch","query":{"path":file}},
            "questions":{"q":noul("Is it big?")}});
        if let Some(long_context) = long_context {
            row["longContext"] = json!(long_context);
        }
        row
    };
    let long = |request: &wiremock::Request| {
        request
            .headers
            .get("typesafe-danger-zone-long-context")
            .is_some()
    };
    let posts = async |server: &MockServer| {
        let requests = server.received_requests().await.unwrap();
        (
            requests.len(),
            requests.iter().filter(|request| long(request)).count(),
        )
    };
    let out = clasify(&runtime, vec![row("small.txt", None)]).await;
    assert!(
        out["results"][0]["data"]["answers"]["q"].is_object(),
        "{out}"
    );
    assert_eq!(posts(&server).await, (1, 0), "the default window served it");
    let out = clasify(&runtime, vec![row("mid.txt", None)]).await;
    assert!(
        out["results"][0]["data"]["answers"]["q"].is_object(),
        "{out}"
    );
    assert_eq!(
        posts(&server).await,
        (3, 1),
        "refused, then the long window"
    );
    // The usage says which of its requests the default window refused, as a
    // sectioned row's does.
    let usage = &out["results"][0]["data"]["usage"];
    assert_eq!(usage["calls"], 2, "{out}");
    assert_eq!(usage["refused"], 1, "{out}");
    // false never asks for the long window. The source is judged in
    // sections the default window takes: one it refuses is split smaller,
    // and every request sent, refused ones too, is counted.
    let out = clasify(&runtime, vec![row("mid.txt", Some(false))]).await;
    let data = &out["results"][0]["data"];
    assert!(data.get("errorCode").is_none(), "{out}");
    assert_eq!(data["answers"]["q"]["noul"], 0.9, "{out}");
    let (sent, long) = posts(&server).await;
    assert_eq!(long, 1, "false never asks for it");
    assert_eq!(data["usage"]["calls"], json!(sent - 3), "{out}");
    runtime.close().await;
}
