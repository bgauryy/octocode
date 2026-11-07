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
    judging(move |content| {
        if exclude_first && content.starts_with("line-1 ") {
            json!({"0":1,"1":0,"2":0})
        } else {
            json!({"0":0,"1":0,"2":1})
        }
    })
    .await
}

/// Selects the chunks of lines 1-3 and answers every other chunk unsure (no
/// outcome above the threshold).
async fn unsure_after_third() -> MockServer {
    judging(|content| {
        if ["line-1 ", "line-2 ", "line-3 "].iter().any(|head| content.starts_with(head)) {
            json!({"0":0,"1":0,"2":1})
        } else {
            json!({"0":0.3,"1":0.4,"2":0.3})
        }
    })
    .await
}

/// A provider answering each question with `distribution(content)` over
/// its three levels, its score the expected level.
async fn judging(distribution: impl Fn(&str) -> Value + Send + Sync + 'static) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(move |request: &Request| {
            let body: Value = request.body_json().expect("provider request");
            let probabilities = distribution(body["state"]["content"].as_str().unwrap_or_default());
            let score = (0..3)
                .map(|level| f64::from(level) * probabilities[level.to_string()].as_f64().unwrap_or(0.0))
                .sum::<f64>();
            let answers = body["questions"].as_object().expect("questions").iter()
                .map(|(id, question)| {
                    let legend = question["criteria"].as_array().expect("criteria").iter()
                        .enumerate().map(|(i, criterion)| (i.to_string(), criterion.clone()))
                        .collect::<serde_json::Map<_, _>>();
                    (id.clone(), json!({"type":"score","score":score,"confidence":1,
                        "legend":legend,"probabilities":probabilities}))
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
            for name in ["nextPage", "nextCandidatePage"] {
                if let Some(call) = data["next"].get(name) {
                    assert_eq!(call["tool"], tool);
                    if name == "nextPage" {
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
    limits.selection.max_chars = 1;
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

async fn local_fixture(server: MockServer) -> (Workspace, MockServer, ToolRuntime, Value) {
    let workspace = Workspace::new();
    let path = workspace.write("pages.txt", source());
    let mut limits = support::PROVIDER_LIMITS;
    limits.selection.max_chars = 1;
    let runtime = workspace.runtime_with(&support::provider_env(&server, &[]), limits);
    let input = json!({"queries":[{"path":path,"classification":{"predicates":["Select evidence"]}}],
        "responseChars":4000});
    (workspace, server, runtime, input)
}

#[tokio::test]
async fn display_paging_preserves_raw_recovery_replay_and_source_snapshot() {
    let (workspace, server, runtime, input) = local_fixture(provider(false).await).await;
    let first = support::call(&runtime, "localFetch", input)
        .await
        .expect("first page");
    let next = &support::row_data(&first)["next"];
    assert!(
        next.get("nextCandidatePage").is_none(),
        "display page owns progress until exhausted"
    );
    // The judge decided every chunk it judged; the rest wait on the
    // candidate budget, counted in `unknownItems` and judged by later
    // pages, so no undecided view is offered.
    let data = support::row_data(&first);
    assert!(
        data["selection"]["unknownItems"].as_u64().is_some_and(|n| n > 0),
        "{data}"
    );
    assert!(next.get("unfilteredUnknown").is_none(), "{data}");
    let mut saved = Vec::new();
    let input = next["unfiltered"]["input"].clone();
    support::assert_cursor_only(&input);
    let actual = walk(&runtime, "localFetch", input.clone(), false).await;
    let expected = source()
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{}\t{line}", i + 1))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "unfiltered preserves exact original evidence");
    saved.push(input);
    let display = next["nextPage"]["input"].clone();
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
    // The display pages hand the pending chunks to `nextCandidatePage`,
    // which judges and returns them: every line once.
    let mut shown = support::row_data(&first)["content"]
        .as_str()
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .chain(walk(&runtime, "localFetch", display.clone(), true).await)
        .collect::<Vec<_>>();
    shown.sort();
    let mut every = expected.clone();
    every.sort();
    assert_eq!(shown, every, "the candidate pages reach the pending chunks");
    saved.push(display);
    workspace.write("pages.txt", "changed source\n");
    for input in saved {
        let stale = support::call(&runtime, "localFetch", input)
            .await
            .expect("stale row");
        let data = support::row_data(&stale);
        assert_eq!(data["errorCode"], "staleSnapshot");
        assert!(data.get("content").is_none());
        // The restart is required work, never an optional related call.
        assert!(data.get("related").is_none(), "{data}");
        let restart = data["next"]["restart"]["input"].clone();
        support::assert_cursor_only(&restart);
        let fresh = support::call(&runtime, "localFetch", restart)
            .await
            .expect("restart");
        // It reads the changed source afresh (here the old range is past
        // its new end), never the stale snapshot again.
        assert_ne!(
            support::row_data(&fresh)["errorCode"],
            "staleSnapshot",
            "{fresh:?}"
        );
    }
    runtime.close().await;
}

/// With chunks judged unsure, the display page offers their undecided view:
/// it replays exactly those lines from the captured source, the display
/// page replays without judging again, and both go stale with the source.
#[tokio::test]
async fn display_paging_preserves_the_unknown_recovery_and_source_snapshot() {
    let (workspace, server, runtime, input) = local_fixture(unsure_after_third().await).await;
    // The candidate budget judges one chunk per page: follow the emitted
    // leads, display pages first, to the page that judged one unsure.
    let mut input = input;
    let (data, display) = loop {
        let page = support::call(&runtime, "localFetch", input.clone())
            .await
            .expect("page");
        let data = support::row_data(&page).clone();
        if data["next"].get("unfilteredUnknown").is_some() {
            break (data, input);
        }
        input = ["nextPage", "nextCandidatePage"]
            .iter()
            .find_map(|lead| data["next"].get(*lead))
            .unwrap_or_else(|| panic!("a lead reaches the unsure chunks: {data}"))["input"]
            .clone();
    };
    let next = &data["next"];
    assert!(
        next.get("nextCandidatePage").is_some(),
        "the last chunk is still pending: {data}"
    );
    let unknown = next["unfilteredUnknown"]["input"].clone();
    support::assert_cursor_only(&unknown);
    let undecided = walk(&runtime, "localFetch", unknown.clone(), false).await;
    let lines = source().lines().map(str::to_owned).collect::<Vec<_>>();
    assert!(!undecided.is_empty(), "{data}");
    for line in &undecided {
        let (number, text) = line.split_once('\t').expect("numbered line");
        let number = number.parse::<usize>().expect("line number");
        assert!(number > 3, "only unsure chunks: {line:.12}");
        assert_eq!(text, lines[number - 1], "exact original evidence");
    }
    let mut once = undecided.clone();
    once.dedup();
    assert_eq!(once, undecided, "each undecided line once");
    // The candidate page that offered the lead replays the same view.
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
    workspace.write("pages.txt", "changed source\n");
    for input in [unknown, display] {
        let stale = support::call(&runtime, "localFetch", input)
            .await
            .expect("stale row");
        let data = support::row_data(&stale);
        assert_eq!(data["errorCode"], "staleSnapshot", "{data}");
        assert!(data.get("content").is_none(), "{data}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn empty_selected_display_still_advances_pending_candidates() {
    let (_workspace, _server, runtime, input) = local_fixture(provider(true).await).await;
    let first = support::call(&runtime, "localFetch", input)
        .await
        .expect("empty selection");
    let data = support::row_data(&first);
    assert_eq!(data["content"], "");
    assert_eq!(data["selection"]["excludedItems"], 1);
    assert!(data["next"].get("nextPage").is_none());
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
