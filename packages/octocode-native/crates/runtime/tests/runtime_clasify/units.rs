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

/// Every unit a row lists, in order: its files' hits as `path:line`, then
/// its other units.
fn unit_keys(data: &Value) -> Vec<String> {
    let files = data["files"]
        .as_object()
        .into_iter()
        .flatten()
        .flat_map(|(path, lines)| {
            lines
                .as_object()
                .into_iter()
                .flatten()
                .map(move |(line, _)| format!("{path}:{line}"))
        });
    let units = data["units"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, _)| key.clone());
    files.chain(units).collect()
}

/// A unit's cells by its key: a hit under its file by line, else under
/// `units`.
fn cells<'a>(data: &'a Value, key: &str) -> &'a Value {
    match key.rsplit_once(':') {
        Some((path, line)) if data["files"][path].get(line).is_some() => &data["files"][path][line],
        _ => &data["units"][key],
    }
}

fn rows_row(tool: &str, query: Value, view: &str) -> Value {
    json!({"operation":"evaluate","view":view,
        "source":{"tool":tool,"query":query},
        "questions":{"keep":noul("The entry is marked to keep.")}})
}

/// Says yes (0.94) to each question whose hit's own line holds [`KEEP`],
/// else no (0.03), whatever the windows around it show: a judge that reads
/// only the hit a question names. A packed request names it in the
/// question's `target` label (`line 4: <its text> (in fn walk)`); a hit sent
/// alone names it in the state's `target`.
async fn target_keeper() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let state = body["state"].get("content").unwrap_or(&body["state"]);
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .iter()
                .map(|(id, question)| {
                    // A label's first line names the hit; any lines after it
                    // are its window, which the judge does not answer from.
                    let hit = match question["instructions"]["target"].as_str() {
                        Some(label) => label
                            .lines()
                            .next()
                            .and_then(|first| first.split_once(": "))
                            .map_or("", |(_, text)| text),
                        None => state["target"]["text"].as_str().unwrap_or_default(),
                    };
                    let noul = if hit.contains(KEEP) { 0.94 } else { 0.03 };
                    (id.clone(), json!({"type":"noul","noul":noul}))
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

/// The hit lines a request's questions name, from their `target` labels.
fn targeted_lines(body: &Value) -> Vec<u64> {
    let mut lines = body["questions"]
        .as_object()
        .expect("questions")
        .values()
        .map(|question| {
            let label = question["instructions"]["target"]
                .as_str()
                .expect("a hit question names its target");
            label
                .strip_prefix("line ")
                .and_then(|rest| rest.split(':').next())
                .and_then(|line| line.parse().ok())
                .expect("the label opens with its line")
        })
        .collect::<Vec<u64>>();
    lines.sort_unstable();
    lines
}

/// Local search: every hit is one unit, and the hits of one file share one
/// provider request with one question per hit naming its line; the summary
/// returns the hits that pass under their file, by line, counts the rest,
/// states the ceiling, and offers one read of the passing lines.
#[tokio::test]
async fn the_hits_of_a_file_share_one_request_and_only_the_passing_ones_return_under_their_file() {
    let server = target_keeper().await;
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
        data["files"]["dense.txt"]["2"],
        json_fixture(r#"{"keep": {"noul": 0.94}}"#),
        "{out}"
    );
    assert!(data.get("units").is_none(), "{out}");
    assert_eq!(
        data["coverage"],
        json_fixture(r#"{"totalUnits": 6, "answered": 6, "quiet": 4}"#),
        "{out}"
    );
    // One request for the file, one question per hit, each naming its line;
    // the summary leaves out a request count of one and the plan that came
    // true, and states the ceiling: the request and one re-ask.
    let bodies = sent(&server).await;
    assert_eq!(bodies.len(), 1, "{out}");
    assert_eq!(targeted_lines(&bodies[0]), [1, 2, 3, 4, 5, 6]);
    assert_eq!(bodies[0]["state"]["path"], "dense.txt");
    assert_eq!(data["usage"], json!({"requestsCeiling": 2}), "{out}");
    // One read of the passing lines, two either side, merged.
    let read = &data["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{out}");
    // Answers and identities, never the rows' text.
    assert!(!out.to_string().contains("entry"), "{out}");

    // details lists every unit, quiet ones too, each answered cell its
    // answer, and states the one request the row billed and its plan.
    let out = clasify(&runtime, vec![rows_row("localSearch", query, "details")]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(unit_keys(data).len(), 6, "{out}");
    assert_eq!(
        cells(data, "dense.txt:1")["keep"],
        json_fixture(r#"{"noul": 0.03}"#),
        "{out}"
    );
    assert_eq!(cells(data, "dense.txt:5")["keep"]["noul"], 0.94, "{out}");
    assert_eq!(data["usage"]["calls"], 1, "{out}");
    assert_eq!(data["usage"]["requestsPlanned"], 1, "{out}");
    assert_eq!(data["usage"]["requestsCeiling"], 2, "{out}");
    assert_eq!(sent(&server).await.len(), 2);
    assert_eq!(data["model"], "jev-test", "{out}");
    runtime.close().await;
}

/// Context windows that overlap merge into one search row, yet each hit is
/// still its own unit named `path:line`. The file's hits share one request,
/// and each hit is one question naming its line, its text, the declaration
/// around it and its own window: the shared state lists no line, so no hit
/// reads another's window.
#[tokio::test]
async fn merged_search_windows_share_one_request_each_hit_with_its_own_window() {
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
    assert_eq!(cells(data, "walk.rs:4")["keep"]["noul"], 0.94, "{out}");
    assert_eq!(cells(data, "walk.rs:3")["keep"]["noul"], 0.03, "{out}");
    let bodies = sent(&server).await;
    assert_eq!(bodies.len(), 1, "one request for the file");
    assert_eq!(data["usage"]["calls"], 1, "{out}");
    let body = &bodies[0];
    let state = &body["state"];
    assert_eq!(state["path"], "walk.rs", "{body}");
    assert!(
        state["question"]
            .as_str()
            .is_some_and(|scope| scope.contains("target")),
        "{body}"
    );
    // No shared window: each question carries its own, the hit's line and
    // the `before` lines above it.
    assert!(state.get("lines").is_none(), "{body}");
    for question in body["questions"].as_object().expect("questions").values() {
        let label = question["instructions"]["target"].as_str().expect("target");
        let line = label
            .strip_prefix("line ")
            .and_then(|rest| rest.split(':').next())
            .and_then(|line| line.parse::<u64>().ok())
            .expect("the label opens with its line");
        let shown = label
            .lines()
            .skip(1)
            .filter_map(|row| row.split('\t').next()?.parse::<u64>().ok())
            .collect::<Vec<_>>();
        assert_eq!(
            shown,
            (line.saturating_sub(before)..=line).collect::<Vec<_>>(),
            "{label}"
        );
    }
    // One question per hit, naming its line and the declaration around it.
    assert_eq!(
        targeted_lines(body),
        hits.iter().filter_map(Value::as_u64).collect::<Vec<_>>()
    );
    for question in body["questions"].as_object().expect("questions").values() {
        let label = question["instructions"]["target"].as_str().expect("target");
        assert!(
            label.contains("needle(depth)") && label.contains(" walk)"),
            "{label}"
        );
    }
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
    // The page names its repository once.
    assert_eq!(
        (&data["owner"], &data["repo"]),
        (&json!("octo"), &json!("demo")),
        "{out}"
    );
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
async fn identical_items_report_one_requests_spend_and_price_once() {
    let server = reporting().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(&runtime, vec![item_row(&["x", "x", "x"])]).await;
    assert_eq!(sent(&server).await.len(), 1, "{out}");
    assert_eq!(
        out["results"][0]["data"]["usage"],
        json_fixture(
            r#"{"calls": 1, "inputTokens": 10, "outputTokens": 2, "requestsPlanned": 1, "requestsCeiling": 1}"#
        ),
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

/// A units row whose ceiling is over its `maxRequests` sends nothing and
/// states its price (VD2); its `next.confirm`, run unchanged, judges it
/// within the ceiling; and its read, run unchanged, shows the passing lines
/// (VD3).
#[tokio::test]
async fn a_units_row_over_its_max_requests_sends_nothing_and_its_confirm_runs() {
    let server = target_keeper().await;
    let workspace = Workspace::new();
    workspace.write("dense.txt", dense());
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let query = json_fixture(r#"{"path": "dense.txt", "matchString": ["needle"]}"#);
    let mut row = rows_row("localSearch", query, "summary");
    row["maxRequests"] = json!(1);
    let out = clasify(&runtime, vec![row]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["errorCode"], "classificationOverBudget", "{out}");
    assert_eq!(data["usage"]["calls"], 0, "{out}");
    assert_eq!(data["usage"]["requestsPlanned"], 1, "{out}");
    assert_eq!(data["usage"]["requestsCeiling"], 2, "{out}");
    assert!(
        data["usage"]["inputTokensAtMost"]
            .as_u64()
            .is_some_and(|tokens| tokens > 0),
        "{out}"
    );
    assert!(sent(&server).await.is_empty(), "nothing was sent");
    let run = |name: &str, call: &Value| {
        let (name, tool, input) = (
            name.to_owned(),
            call["tool"].as_str().expect("tool").to_owned(),
            call["input"].clone(),
        );
        let runtime = &runtime;
        async move {
            crate::support::Run::run(runtime, name, tool, input)
                .await
                .expect("the call runs")
                .structured_content
                .into_value()
        }
    };
    let confirmed = run("confirm", &data["next"]["confirm"]).await;
    let again = &confirmed["results"][0]["data"];
    assert!(again.get("errorCode").is_none(), "{confirmed}");
    assert_eq!(
        unit_keys(again),
        ["dense.txt:2", "dense.txt:5"],
        "{confirmed}"
    );
    assert_eq!(
        sent(&server).await.len(),
        1,
        "the first round, within the ceiling"
    );
    let read = run("read", &again["next"]["read"]).await;
    let content = read["results"][0]["data"]["content"]
        .as_str()
        .unwrap_or_default();
    assert!(
        content.contains("needle KEEP entry 2") && content.contains("needle KEEP entry 5"),
        "{read}"
    );
    runtime.close().await;
}

/// A source the default window may refuse is priced with its long-window
/// retry: under a `maxRequests` below that nothing is sent; at it, the
/// source is judged.
#[tokio::test]
async fn a_whole_source_over_its_max_requests_sends_nothing() {
    let server = keeper().await;
    let workspace = Workspace::new();
    workspace.write("big.txt", "needle skip line\n".repeat(2_500));
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |max: u64| {
        json!({"operation":"evaluate","maxRequests":max,
            "source":{"tool":"localFetch","query":{"path":"big.txt"}},
            "questions":{"keep":noul("The entry is marked to keep.")}})
    };
    let out = clasify(&runtime, vec![row(1)]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["errorCode"], "classificationOverBudget", "{out}");
    assert_eq!(data["usage"]["requestsPlanned"], 1, "{out}");
    assert_eq!(data["usage"]["requestsCeiling"], 2, "{out}");
    assert!(sent(&server).await.is_empty(), "nothing was sent");
    let out = clasify(&runtime, vec![row(2)]).await;
    let data = &out["results"][0]["data"];
    assert!(data.get("errorCode").is_none(), "{out}");
    assert_eq!(data["usage"]["requestsCeiling"], 2, "{out}");
    assert_eq!(sent(&server).await.len(), 1);
    runtime.close().await;
}

/// A glob's resources are priced together before any is sent: a total
/// ceiling over `maxRequests` sends nothing; within it, each resource is
/// judged and states its own price.
#[tokio::test]
async fn a_resources_row_over_its_max_requests_sends_nothing() {
    let server = keeper().await;
    let workspace = Workspace::new();
    for file in ["d/a.md", "d/b.md", "d/c.md"] {
        workspace.write(file, "needle skip\n");
    }
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |max: u64| {
        json!({"operation":"evaluate","maxRequests":max,
            "resources":{"d":{"tool":"localFetch","query":{"path":"d/*.md"}}},
            "questions":{"keep":noul("The entry is marked to keep.")}})
    };
    let out = clasify(&runtime, vec![row(2)]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["errorCode"], "classificationOverBudget", "{out}");
    assert_eq!(data["usage"]["requestsPlanned"], 3, "{out}");
    assert_eq!(data["usage"]["requestsCeiling"], 3, "{out}");
    assert!(sent(&server).await.is_empty(), "nothing was sent");
    let out = clasify(&runtime, vec![row(3)]).await;
    let data = &out["results"][0]["data"];
    assert!(data.get("errorCode").is_none(), "{out}");
    let resources = data["resources"].as_object().expect("resources");
    assert_eq!(resources.len(), 3, "{out}");
    for resource in resources.values() {
        assert_eq!(resource["usage"]["requestsCeiling"], 1, "{out}");
    }
    assert_eq!(sent(&server).await.len(), 3);
    runtime.close().await;
}

/// A units row is built from the redacted read: a secret in a hit line or
/// in a path reaches neither the judge nor the served row (its `files`
/// keys, cells, read call and repository stated once), and a credential
/// file's hits stay withheld from the judge.
#[tokio::test]
async fn a_units_row_is_built_from_the_redacted_read_and_withholds_credential_files() {
    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    const SECRET: &str = "ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
    let server = keeper().await;
    let github = MockServer::builder().start().await;
    let item = |file: &str, fragment: &str, blob: &str| {
        json!({"name":file,"path":file,"sha":blob,
            "html_url":format!("https://github.com/octo/demo/blob/{SHA}/{file}"),
            "repository":{"full_name":"octo/demo","html_url":"https://x","url":"https://x"},
            "text_matches":[{"fragment":fragment,"matches":[{"text":"needle","indices":[0,6]}]}]})
    };
    let in_path = format!("src/{SECRET}/x.rs");
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"page\"")
                .set_body_json(json!({"total_count":3,"incomplete_results":false,"items":[
                    item("src/app.rs", &format!("needle KEEP {SECRET}"), "6"),
                    item(&in_path, "needle KEEP there", "7"),
                    item("config/credentials.json", "needle KEEP token", "8"),
                ]})),
        )
        .mount(&github)
        .await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &support::github_api(&github));
    let query = json_fixture(r#"{"owner": "octo", "repo": "demo", "keywords": ["needle"]}"#);
    for view in ["summary", "details"] {
        let out = clasify(
            &runtime,
            vec![rows_row("ghSearchCode", query.clone(), view)],
        )
        .await;
        let data = &out["results"][0]["data"];
        assert!(data.get("errorCode").is_none(), "{out}");
        assert!(
            !out.to_string().contains(SECRET),
            "the served row is redacted: {out}"
        );
        assert_eq!(
            data["coverage"]["withheld"], 1,
            "the credential file: {out}"
        );
        assert!(
            unit_keys(data)
                .iter()
                .all(|key| !key.starts_with("config/credentials.json")),
            "{out}"
        );
    }
    for body in sent(&server).await {
        let body = body.to_string();
        assert!(!body.contains(SECRET), "{body}");
        assert!(
            !body.contains("needle KEEP token"),
            "a credential file never reaches the judge: {body}"
        );
    }
    runtime.close().await;
}

/// Opted in with `perHit`, each hit is its own request, as before a file's
/// hits shared one: six hits are priced six before anything is sent (a cap
/// below that sends nothing), then six requests of one question each, with
/// the same passing hits.
#[tokio::test]
async fn per_hit_judges_each_hit_in_its_own_request_at_its_price() {
    let server = target_keeper().await;
    let workspace = Workspace::new();
    workspace.write("dense.txt", dense());
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let query = json_fixture(r#"{"path": "dense.txt", "matchString": ["needle"]}"#);
    let mut row = rows_row("localSearch", query, "details");
    row["perHit"] = json!(true);
    row["maxRequests"] = json!(5);
    let out = clasify(&runtime, vec![row.clone()]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["errorCode"], "classificationOverBudget", "{out}");
    assert_eq!(data["usage"]["requestsPlanned"], 6, "{out}");
    assert_eq!(data["usage"]["requestsCeiling"], 6, "{out}");
    assert!(sent(&server).await.is_empty(), "nothing was sent");
    row["maxRequests"] = json!(6);
    let out = clasify(&runtime, vec![row]).await;
    let data = &out["results"][0]["data"];
    assert!(data.get("errorCode").is_none(), "{out}");
    assert_eq!(data["usage"]["calls"], 6, "{out}");
    assert_eq!(data["usage"]["requestsPlanned"], 6, "{out}");
    let bodies = sent(&server).await;
    assert_eq!(bodies.len(), 6, "one request per hit");
    for body in &bodies {
        assert_eq!(
            body["questions"].as_object().map(serde_json::Map::len),
            Some(1),
            "{body}"
        );
    }
    assert_eq!(cells(data, "dense.txt:2")["keep"]["noul"], 0.94, "{out}");
    assert_eq!(cells(data, "dense.txt:5")["keep"]["noul"], 0.94, "{out}");
    assert_eq!(cells(data, "dense.txt:1")["keep"]["noul"], 0.03, "{out}");
    runtime.close().await;
}
