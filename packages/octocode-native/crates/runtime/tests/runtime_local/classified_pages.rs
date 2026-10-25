use crate::support::{self, Run as _, Workspace};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

async fn provider(exclude_first: bool) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(move |request: &Request| {
            let body: Value = request.body_json().expect("provider request");
            let excluded = exclude_first && body["state"]["content"].as_str()
                .is_some_and(|content| content.starts_with("line-1 "));
            let answers = body["questions"].as_object().expect("questions").iter()
                .map(|(id, question)| {
                    let legend = question["criteria"].as_array().expect("criteria").iter()
                        .enumerate().map(|(i, criterion)| (i.to_string(), criterion.clone()))
                        .collect::<serde_json::Map<_, _>>();
                    (id.clone(), json!({"type":"score","score":if excluded {0} else {2},"confidence":1,
                        "legend":legend,"probabilities":{"0":u8::from(excluded),"1":0,"2":u8::from(!excluded)}}))
                }).collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({"model":"fixture","answers":answers,
                "usage":{"input_tokens":10,"output_tokens":2}}))
        }).mount(&server).await;
    server
}

async fn walk(
    runtime: &ToolRuntime,
    tool: &str,
    input: Value,
    require_both_axes: bool,
) -> Vec<String> {
    let mut queue = VecDeque::from([input]);
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    let mut display_pages = 0;
    let mut candidate_pages = 0;
    while let Some(input) = queue.pop_front() {
        // Only identical public cursor inputs are replays. Do not deduplicate
        // internal query state or returned lines: that hides duplicate branches.
        if !seen.insert(input.to_string()) {
            continue;
        }
        assert!(seen.len() < 100, "public traversal must terminate");
        let output = runtime
            .run("classified-walk".into(), tool.into(), input)
            .await
            .expect("public call")
            .structured_content
            .into_value();
        support::checks::assert_output_contract(tool, &output);
        for result in output["results"].as_array().expect("results") {
            let data = &result["data"];
            assert!(data.get("errorCode").is_none(), "{data}");
            if let Some(content) = data["content"].as_str() {
                lines.extend(content.lines().map(str::to_owned));
            }
            for name in ["continueWindow", "nextCandidatePage"] {
                if let Some(call) = data["next"].get(name) {
                    assert_eq!(call["tool"], tool);
                    if name == "continueWindow" {
                        display_pages += 1;
                    } else {
                        candidate_pages += 1;
                    }
                    queue.push_back(call["input"].clone());
                }
            }
        }
        if let Some(call) = output["responsePagination"].get("next") {
            queue.push_front(call["input"].clone());
        }
    }
    if require_both_axes {
        assert!(
            display_pages > 0 && candidate_pages > 0,
            "fixture must exercise both axes"
        );
    }
    lines
}

fn source() -> String {
    (1..=12)
        .map(|line| format!("line-{line} {}\n", "界".repeat(400)))
        .collect()
}

async fn selected_pages(tool: &str) {
    let workspace = Workspace::new();
    let body = source();
    let local = workspace.write("pages.txt", &body);
    let server = provider(false).await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/pages.txt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"type":"file",
            "encoding":"base64","content":STANDARD.encode(&body)})),
        )
        .mount(&server)
        .await;
    let mut limits = support::PROVIDER_LIMITS;
    limits.selection.max_candidates = 1;
    let runtime = workspace.runtime_with(
        &support::provider_env(&server, &support::github_api(&server)),
        limits,
    );
    let mut query = if tool == "localFetch" {
        json!({"path":local})
    } else {
        json!({"owner":"a","repo":"b","path":"pages.txt","ref":sha})
    };
    query["classification"] = json!({"predicates":["Select all evidence"]});
    let mut actual = walk(
        &runtime,
        tool,
        json!({"queries":[query],"responseChars":4000}),
        true,
    )
    .await;
    runtime.close().await;
    let mut expected = body
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{}\t{line}", i + 1))
        .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    assert_eq!(
        actual.len(),
        expected.len(),
        "each source line must appear exactly once"
    );
    assert_eq!(
        actual, expected,
        "complete source evidence, with no deduplication"
    );
}

#[tokio::test]
async fn local_selected_public_cursors_return_each_line_once() {
    selected_pages("localFetch").await;
}

#[tokio::test]
async fn github_selected_public_cursors_return_each_line_once() {
    selected_pages("ghGetFileContent").await;
}

async fn local_fixture(exclude_first: bool) -> (Workspace, MockServer, ToolRuntime, Value) {
    let workspace = Workspace::new();
    let path = workspace.write("pages.txt", source());
    let server = provider(exclude_first).await;
    let mut limits = support::PROVIDER_LIMITS;
    limits.selection.max_candidates = 1;
    let runtime = workspace.runtime_with(&support::provider_env(&server, &[]), limits);
    let input = json!({"queries":[{"path":path,"classification":{"predicates":["Select evidence"]}}],
        "responseChars":4000});
    (workspace, server, runtime, input)
}

#[tokio::test]
async fn display_paging_preserves_raw_recovery_replay_and_source_snapshot() {
    let (workspace, server, runtime, input) = local_fixture(false).await;
    let first = support::call(&runtime, "localFetch", input)
        .await
        .expect("first page");
    let next = &support::row_data(&first)["next"];
    assert!(
        next.get("nextCandidatePage").is_none(),
        "display page owns progress until exhausted"
    );
    let mut saved = Vec::new();
    for (name, start) in [("unfiltered", 0), ("unfilteredUnknown", 5)] {
        let input = next[name]["input"].clone();
        support::assert_cursor_only(&input);
        let actual = walk(&runtime, "localFetch", input.clone(), false).await;
        let expected = source()
            .lines()
            .enumerate()
            .skip(start)
            .map(|(i, line)| format!("{}\t{line}", i + 1))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{name} preserves exact original evidence");
        saved.push(input);
    }
    let display = next["continueWindow"]["input"].clone();
    support::assert_cursor_only(&display);
    let second = support::call(&runtime, "localFetch", display.clone())
        .await
        .expect("display");
    let replay = support::call(&runtime, "localFetch", display.clone())
        .await
        .expect("replay");
    assert_eq!(
        support::row_data(&second)["content"],
        support::row_data(&replay)["content"]
    );
    let provider_calls = server
        .received_requests()
        .await
        .expect("requests")
        .iter()
        .filter(|request| request.method == "POST")
        .count();
    assert_eq!(
        provider_calls, 1,
        "raw recovery and display replay do not judge again"
    );
    saved.push(display);
    workspace.write("pages.txt", "changed source\n");
    for input in saved {
        let stale = support::call(&runtime, "localFetch", input)
            .await
            .expect("stale row");
        assert_eq!(support::row_data(&stale)["errorCode"], "staleSnapshot");
        assert!(support::row_data(&stale).get("content").is_none());
    }
    runtime.close().await;
}

#[tokio::test]
async fn empty_selected_display_still_advances_pending_candidates() {
    let (_workspace, _server, runtime, input) = local_fixture(true).await;
    let first = support::call(&runtime, "localFetch", input)
        .await
        .expect("empty selection");
    let data = support::row_data(&first);
    assert_eq!(data["content"], "");
    assert_eq!(data["selection"]["excludedItems"], 1);
    assert!(data["next"].get("continueWindow").is_none());
    let next = data["next"]["nextCandidatePage"]["input"].clone();
    support::assert_cursor_only(&next);
    let actual = walk(&runtime, "localFetch", next, true).await;
    let expected = source()
        .lines()
        .enumerate()
        .skip(5)
        .map(|(i, line)| format!("{}\t{line}", i + 1))
        .collect::<Vec<_>>();
    assert_eq!(
        actual, expected,
        "later candidates survive an empty selected page"
    );
    runtime.close().await;
}
