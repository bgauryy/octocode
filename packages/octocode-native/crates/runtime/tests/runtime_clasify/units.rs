//! Many small things judged one by one in a single row: the rows of a tool's
//! page named as the `source`, by reference, or the items of a `content`
//! array with `perItem`. The row holds answers keyed by each unit's identity
//! and the counts; the units' text never comes back.

use super::{clasify, noul, sent};
use crate::support::{self, Workspace, clasify_runtime};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// What a unit that passes holds.
const KEEP: &str = "KEEP";

/// Says yes (0.94) to a unit whose state holds [`KEEP`], else no (0.03).
async fn keeper() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let hit = body["state"].to_string().contains(KEEP);
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| {
                    (
                        id.clone(),
                        json!({"type":"noul","noul":if hit { 0.94 } else { 0.03 }}),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-test","answers":answers,
                "usage":{"input_tokens":50,"output_tokens":4},
            }))
        })
        .mount(&server)
        .await;
    server
}

/// Six `needle` lines, two of which hold [`KEEP`] (lines 2 and 5).
fn dense() -> String {
    (1..=6)
        .map(|line| {
            let mark = if line == 2 || line == 5 { KEEP } else { "skip" };
            format!("needle {mark} entry {line}\n")
        })
        .collect()
}

fn unit_keys(data: &Value) -> Vec<String> {
    data["units"]
        .as_object()
        .map(|units| units.keys().cloned().collect())
        .unwrap_or_default()
}

fn rows_row(tool: &str, query: Value, view: &str) -> Value {
    json!({"operation":"evaluate","view":view,
        "source":{"tool":tool,"query":query},
        "questions":{"keep":noul("The entry is marked to keep.")}})
}

/// Local search: every hit is one unit and one provider request; the summary
/// returns the hits that pass, named `path:line`, and counts the rest.
#[tokio::test]
async fn search_hits_are_judged_one_by_one_and_only_the_passing_ones_return() {
    let server = keeper().await;
    let workspace = Workspace::new();
    workspace.write("dense.txt", dense());
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let query = json_fixture(r#"{"path": "dense.txt", "matchString": ["needle"]}"#);
    let out = clasify(
        &runtime,
        vec![rows_row("localSearch", query.clone(), "summary")],
    )
    .await;
    let data = &out["results"][0]["data"];
    assert_eq!(unit_keys(data), ["dense.txt:2", "dense.txt:5"], "{out}");
    assert_eq!(
        data["units"]["dense.txt:2"],
        json_fixture(r#"{"keep": {"noul": 0.94}}"#),
        "{out}"
    );
    assert_eq!(
        data["coverage"],
        json_fixture(r#"{"totalUnits": 6, "answered": 6, "quiet": 4}"#),
        "{out}"
    );
    assert_eq!(data["usage"]["calls"], 6, "{out}");
    assert_eq!(sent(&server).await.len(), 6);
    // Answers and identities, never the rows' text.
    assert!(!out.to_string().contains("entry"), "{out}");
    // Each request judged one hit alone.
    for body in sent(&server).await {
        assert_eq!(
            body["state"].to_string().matches("needle").count(),
            1,
            "{body}"
        );
    }

    // details lists every unit, quiet ones too, each cell in its status.
    let out = clasify(&runtime, vec![rows_row("localSearch", query, "details")]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(unit_keys(data).len(), 6, "{out}");
    assert_eq!(
        data["units"]["dense.txt:1"]["keep"],
        json_fixture(r#"{"status": "answered", "answer": {"noul": 0.03}}"#),
        "{out}"
    );
    assert_eq!(data["model"], "jev-test", "{out}");
    runtime.close().await;
}

/// Says yes (0.94) to a hit whose own target line holds [`KEEP`], else no
/// (0.03), whatever its window shows: a judge that reads only the target.
async fn target_keeper() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let state = body["state"].get("content").unwrap_or(&body["state"]);
            let hit = state["target"]["text"]
                .as_str()
                .is_some_and(|text| text.contains(KEEP));
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| {
                    (
                        id.clone(),
                        json!({"type":"noul","noul":if hit { 0.94 } else { 0.03 }}),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-test","answers":answers,
                "usage":{"input_tokens":50,"output_tokens":4},
            }))
        })
        .mount(&server)
        .await;
    server
}

/// Context windows that overlap merge into one search row, yet each hit is
/// still its own unit named `path:line`: judged with its target line marked,
/// its own window, and the declaration around it from the row.
#[tokio::test]
async fn merged_search_windows_are_judged_hit_by_hit_with_the_hit_marked() {
    let server = target_keeper().await;
    let workspace = Workspace::new();
    workspace.write(
        "walk.rs",
        format!(
            "fn walk(depth: u32) -> u32 {{\n    let base = 1;\n    let a = needle(depth);\n    let b = needle(depth); // {KEEP}\n    a + b + base\n}}\n"
        ),
    );
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let before = 2;
    let query = json!({"path":"walk.rs","matchString":["needle"],"contextLines":{"before":before,"after":0}});
    // The tool itself merges the two windows into one row.
    let search = crate::support::Run::run(
        &runtime,
        "merged-search".into(),
        "localSearch".into(),
        json!({"queries":[query.clone()]}),
    )
    .await
    .expect("search")
    .structured_content
    .into_value();
    let rows = &search["results"][0]["data"]["files"][0]["matches"];
    assert_eq!(rows.as_array().map(Vec::len), Some(1), "{search}");
    let hits = rows[0]["matchedLines"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(hits.len(), 2, "{search}");

    let out = clasify(&runtime, vec![rows_row("localSearch", query, "details")]).await;
    let data = &out["results"][0]["data"];
    let expected = hits
        .iter()
        .map(|line| format!("walk.rs:{line}"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        unit_keys(data)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        expected,
        "{out}"
    );
    assert_eq!(data["coverage"]["totalUnits"], hits.len(), "{out}");
    // Only the hit whose own line says KEEP passes, though both windows hold it.
    assert_eq!(
        data["units"]["walk.rs:4"]["keep"]["answer"]["noul"], 0.94,
        "{out}"
    );
    assert_eq!(
        data["units"]["walk.rs:3"]["keep"]["answer"]["noul"], 0.03,
        "{out}"
    );
    let bodies = sent(&server).await;
    assert_eq!(bodies.len(), hits.len());
    let mut targets = Vec::new();
    for body in bodies {
        let state = &body["state"];
        let target = &state["target"];
        let line = target["line"].as_u64().expect("target line");
        targets.push(line);
        assert_eq!(target["enclosing"]["symbolName"], "walk", "{body}");
        assert!(
            state["question"]
                .as_str()
                .is_some_and(|scope| scope.contains("target")),
            "{body}"
        );
        // Its own window: the lines up to `before` above it, its own included.
        for entry in state["lines"].as_array().into_iter().flatten() {
            let shown = entry[0].as_u64().expect("window line");
            assert!(shown <= line && shown + before >= line, "{body}");
        }
    }
    targets.sort_unstable();
    assert_eq!(
        targets,
        hits.iter().filter_map(Value::as_u64).collect::<Vec<_>>()
    );
    runtime.close().await;
}

/// A listing: each entry is one unit, judged from what the listing shows of
/// it, named by its path.
#[tokio::test]
async fn listed_paths_are_units_named_by_path() {
    let server = keeper().await;
    let workspace = Workspace::new();
    for file in ["src/retry_KEEP.rs", "src/plain.rs", "src/other.rs"] {
        workspace.write(file, "fn f() {}\n");
    }
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![rows_row(
            "structureSearch",
            json_fixture(r#"{"path": "src"}"#),
            "summary",
        )],
    )
    .await;
    let data = &out["results"][0]["data"];
    assert_eq!(unit_keys(data), ["src/retry_KEEP.rs"], "{out}");
    assert_eq!(data["coverage"]["totalUnits"], 3, "{out}");
    assert_eq!(data["coverage"]["quiet"], 2, "{out}");
    runtime.close().await;
}

/// The tool's own paging is the row's: one page of the tool is one page of
/// units, and `next.nextPage` judges the tool's next page with the same
/// questions.
#[tokio::test]
async fn a_tools_next_page_is_the_rows_next_page() {
    let server = keeper().await;
    let workspace = Workspace::new();
    for file in ["a.txt", "b.txt", "c.txt"] {
        workspace.write(file, dense());
    }
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let query = json_fixture(r#"{"path": ".", "matchString": ["needle"], "limit": 1}"#);
    let first = clasify(&runtime, vec![rows_row("localSearch", query, "details")]).await;
    let data = &first["results"][0]["data"];
    let next = &data["next"]["nextPage"];
    assert_eq!(next["tool"], "clasify", "{first}");
    let second = crate::support::Run::run(
        &runtime,
        "units-next".into(),
        "clasify".into(),
        next["input"].clone(),
    )
    .await
    .expect("next page")
    .structured_content
    .into_value();
    let more = &second["results"][0]["data"];
    // A details page continues in details: every unit of each page, no
    // unit of the first on the second.
    let (one, two) = (unit_keys(data), unit_keys(more));
    assert_eq!(
        one.len() as u64,
        data["coverage"]["totalUnits"].as_u64().unwrap(),
        "{first}"
    );
    assert_eq!(
        two.len() as u64,
        more["coverage"]["totalUnits"].as_u64().unwrap(),
        "{second}"
    );
    assert!(!one.is_empty() && !two.is_empty(), "{first}\n{second}");
    assert!(
        one.iter().all(|key| !two.contains(key)),
        "{first}\n{second}"
    );
    runtime.close().await;
}

/// GitHub code search: each file's matched lines are the units.
#[tokio::test]
async fn github_search_rows_are_units() {
    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    let server = keeper().await;
    let github = MockServer::builder().start().await;
    let item = |file: &str, fragment: &str, blob: &str| {
        json!({"name":file,"path":file,"sha":blob,
            "html_url":format!("https://github.com/octo/demo/blob/{SHA}/{file}"),
            "repository":{"full_name":"octo/demo","html_url":"https://x","url":"https://x"},
            "text_matches":[{"fragment":fragment,"matches":[{"text":"needle","indices":[0,6]}]}]})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"page\"")
                .set_body_json(json!({"total_count":2,"incomplete_results":false,"items":[
                    item("src/app.rs", "needle KEEP here", "6"),
                    item("src/lib.rs", "needle elsewhere", "7"),
                ]})),
        )
        .mount(&github)
        .await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &support::github_api(&github));
    let query = json_fixture(r#"{"owner": "octo", "repo": "demo", "keywords": ["needle"]}"#);
    let out = clasify(&runtime, vec![rows_row("ghSearchCode", query, "details")]).await;
    let data = &out["results"][0]["data"];
    assert!(data.get("errorCode").is_none(), "{out}");
    assert_eq!(data["coverage"]["totalUnits"], 2, "{out}");
    let keys = unit_keys(data);
    assert!(keys[0].starts_with("src/app.rs"), "{out}");
    assert!(keys[1].starts_with("src/lib.rs"), "{out}");
    assert_eq!(sent(&server).await.len(), 2);
    runtime.close().await;
}

/// A `content` array with `perItem`: each item is one unit keyed by its
/// index, with no item cap; without it the array is one source.
#[tokio::test]
async fn content_items_are_units_when_asked_and_one_source_otherwise() {
    let server = keeper().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let items: Vec<Value> = (0..150)
        .map(|at| {
            let mark = if at == 17 || at == 140 { KEEP } else { "skip" };
            json!(format!("comment {at}: {mark}"))
        })
        .collect();
    let row = |per_item: bool| {
        let mut row = json!({"operation":"evaluate","view":"summary","content":items,
            "questions":{"keep":noul("The comment is marked to keep.")}});
        if per_item {
            row["perItem"] = json!(true);
        }
        row
    };
    let out = clasify(&runtime, vec![row(true)]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(unit_keys(data), ["17", "140"], "{out}");
    assert_eq!(
        data["coverage"],
        json_fixture(r#"{"totalUnits": 150, "answered": 150, "quiet": 148}"#),
        "{out}"
    );
    assert_eq!(sent(&server).await.len(), 150);

    let out = clasify(&runtime, vec![row(false)]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(
        data["answers"]["keep"],
        json_fixture(r#"{"noul": 0.94}"#),
        "{out}"
    );
    assert_eq!(sent(&server).await.len(), 151);
    runtime.close().await;
}

/// A tool that writes, clasify itself, or an unknown tool is no source; and
/// the tool's own contract checks its query.
#[tokio::test]
async fn only_read_only_row_tools_are_sources_and_their_contract_checks_the_query() {
    let server = keeper().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    for tool in [
        "ghCloneRepo",
        "clasify",
        "graphBuild",
        "ghStructure",
        "nope",
    ] {
        let result = crate::support::Run::run(
            &runtime,
            format!("refused-{tool}"),
            "clasify".into(),
            json!({"queries":[rows_row(tool, json_fixture(r#"{"path": "."}"#), "summary")]}),
        )
        .await;
        let refused = match result {
            Err(_) => true,
            Ok(outcome) => {
                let out = outcome.structured_content.into_value();
                out.get("errorCode").is_some()
                    || out["results"][0]["data"].get("errorCode").is_some()
            }
        };
        assert!(refused, "{tool} must be refused as a source");
    }
    // The query is the tool's own row: admission refuses a field that tool
    // would refuse, naming it under source.query, before any read or request.
    let refused = crate::support::Run::run(
        &runtime,
        "refused-field".into(),
        "clasify".into(),
        json!({"queries":[rows_row("localSearch", json_fixture(r#"{"path": ".", "noSuchField": 1}"#), "summary")]}),
    )
    .await
    .expect_err("admission refuses the query");
    let shown = format!("{refused:?}");
    assert!(
        shown.contains("noSuchField") && shown.contains("the row a direct localSearch call takes"),
        "{shown}"
    );
    assert!(sent(&server).await.is_empty());
    runtime.close().await;
}

/// Answers every question yes and reports the usage of one small request.
async fn reporting() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| (id.clone(), json_fixture(r#"{"type": "noul", "noul": 0.9}"#)))
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-test","answers":answers,
                "usage":{"input_tokens":10,"output_tokens":2},
            }))
        })
        .mount(&server)
        .await;
    server
}

fn item_row(items: &[&str]) -> Value {
    json!({"operation":"evaluate","view":"details","perItem":true,
        "content":items,
        "questions":{"keep":noul("The item is marked to keep.")}})
}

fn spend(row: &Value) -> (u64, u64, u64) {
    let usage = &row["data"]["usage"];
    (
        usage["calls"].as_u64().unwrap_or(0),
        usage["inputTokens"].as_u64().unwrap_or(0),
        usage["outputTokens"].as_u64().unwrap_or(0),
    )
}

/// Identical items share one provider request, so the row reports that one
/// request's tokens once, not once per item.
#[tokio::test]
async fn identical_items_report_one_requests_spend_once() {
    let server = reporting().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(&runtime, vec![item_row(&["x", "x", "x"])]).await;
    assert_eq!(sent(&server).await.len(), 1, "{out}");
    assert_eq!(
        out["results"][0]["data"]["usage"],
        json_fixture(r#"{"calls": 1, "inputTokens": 10, "outputTokens": 2}"#),
        "{out}"
    );
    runtime.close().await;
}

/// Rows whose items overlap send each distinct request once; their usage
/// sums to what was billed, and the same row carries it on every run.
#[tokio::test]
async fn items_shared_across_rows_are_billed_once_to_the_first_row() {
    let server = reporting().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    for _ in 0..5 {
        let out = clasify(&runtime, vec![item_row(&["x", "y"]), item_row(&["x"])]).await;
        let (first, second) = (spend(&out["results"][0]), spend(&out["results"][1]));
        assert_eq!(first, (2, 20, 4), "{out}");
        assert_eq!(second, (0, 0, 0), "{out}");
    }
    assert_eq!(sent(&server).await.len(), 10);
    runtime.close().await;
}
