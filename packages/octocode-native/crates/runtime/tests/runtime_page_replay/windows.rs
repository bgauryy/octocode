//! Response budgets preserve complete numbered source evidence on both transports.
use super::SHA;
use crate::support::{self, Run as _, Workspace};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn retired_window_controls_are_rejected() {
    let workspace = Workspace::new();
    workspace.write("one.txt", "source\n");
    let runtime = workspace.runtime(&[]);
    for field in [
        "responseOffset",
        "responseLength",
        "responseScope",
        "responseSnapshot",
    ] {
        let mut input = json!({"queries":[{"path":"one.txt"}]});
        input[field] = json!(0);
        let error = runtime
            .run("retired".into(), "localFetch".into(), input)
            .await
            .expect_err("unknown envelope field");
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
    runtime.close().await;
}

fn body() -> String {
    (0..400)
        .map(|line| format!("evidence_{line:04} 界😀\n"))
        .collect()
}

fn source_lines(output: &Value) -> Vec<String> {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| {
            row["data"]["content"]
                .as_str()
                .into_iter()
                .flat_map(str::lines)
                .filter_map(|line| line.find("evidence_").map(|start| line[start..].to_owned()))
        })
        .collect()
}

async fn assert_complete(runtime: &octocode_native::runtime::ToolRuntime, tool: &str, row: Value) {
    use octocode_native::runtime::Channel;
    for channel in [Channel::Cli, Channel::Mcp] {
        for budget in [2500, 10000] {
            let mut walker =
                support::checks::Walker::new(tool, json!({"queries":[row],"responseChars":budget}));
            while let Some(input) = walker.next_query() {
                let admission = runtime.admit("source-page".into()).expect("admit");
                let output = runtime
                    .execute(admission, tool.into(), input, channel)
                    .await
                    .expect("read")
                    .structured_content
                    .into_value();
                // Per-call provider metadata rides outside the page measure.
                let chars =
                    octocode_native::response::pager::budgeted_chars(&output).expect("page");
                assert!(
                    chars <= budget || output["oversized"] == true,
                    "oversize must be explicit: {chars}/{budget} {output}"
                );
                walker.accept(support::checks::Page {
                    rows: source_lines(&output),
                    output,
                });
            }
            let walk = walker.finish();
            if budget == 2500 {
                assert!(walk.queries.len() > 1);
            }
            assert_eq!(
                walk.rows,
                body().lines().map(str::to_owned).collect::<Vec<_>>()
            );
        }
    }
}

/// B105 (i): a fresh first page drops the fields that echo its query after
/// the cut. A page the cut flagged only for those fields fits once they go,
/// so `oversized` holds exactly when the delivered page is wider than its
/// budget. A long echoed `path` puts the flag's edge inside the scan: the
/// scan runs from below the whole file's delivered page, where pages are
/// wider than their budget, to past that page plus the echo the cut counts.
#[tokio::test]
async fn a_fresh_first_page_is_oversized_only_when_what_it_delivers_is() {
    let workspace = Workspace::new();
    let file = format!("{}/f.txt", "d".repeat(200));
    workspace.write(&file, "one\ntwo\nthree\n");
    let runtime = workspace.runtime(&[]);
    let read = |budget: Option<usize>| {
        let mut query = json!({"queries":[{"path":file}]});
        if let Some(budget) = budget {
            query["responseChars"] = json!(budget);
        }
        let runtime = &runtime;
        async move {
            runtime
                .run("echo-page".into(), "localFetch".into(), query)
                .await
                .expect("read")
                .structured_content
                .into_value()
        }
    };
    // The whole file delivered on one page, and the echoed `path` entry the
    // cut measures beside it before the echo goes.
    let default = read(None).await;
    let row = &default["results"][0]["data"];
    assert_eq!(
        row["content"].as_str().expect("content"),
        "1\tone\n2\ttwo\n3\tthree\n",
        "the default read holds the whole file: {default}"
    );
    assert!(
        row.get("pagination").is_none() && row.get("hasMore").is_none(),
        "{default}"
    );
    let whole = octocode_native::response::pager::budgeted_chars(&default).expect("page");
    let echo = serde_json::to_string(&json!({ "path": file }))
        .expect("echo")
        .len();
    let mut flagged = 0;
    for budget in (whole / 2..=whole + 2 * echo).step_by(5) {
        let output = read(Some(budget)).await;
        let chars = octocode_native::response::pager::budgeted_chars(&output).expect("page");
        flagged += usize::from(output.get("oversized").is_some());
        assert_eq!(
            output.get("oversized").is_some(),
            chars > budget,
            "{chars}/{budget}: {output}"
        );
    }
    assert!(
        flagged > 0,
        "the scan reaches pages wider than their budget"
    );
    runtime.close().await;
}

/// Walk a localFetch of `path` at `budget` on both transports: every row
/// part's `data.content` in delivery order, and every part's `data.path`.
async fn content_parts(
    runtime: &octocode_native::runtime::ToolRuntime,
    path: &str,
    budget: usize,
) -> Vec<(Vec<String>, Vec<Value>)> {
    use octocode_native::runtime::Channel;
    let mut walks = Vec::new();
    for channel in [Channel::Cli, Channel::Mcp] {
        let mut walker = support::checks::Walker::new(
            "localFetch",
            json!({"queries":[{"path":path}],"responseChars":budget}),
        );
        let (mut paths, mut parts) = (Vec::new(), Vec::new());
        while let Some(input) = walker.next_query() {
            let admission = runtime.admit("b113-walk".into()).expect("admit");
            let output = runtime
                .execute(admission, "localFetch".into(), input, channel)
                .await
                .expect("read")
                .structured_content
                .into_value();
            let chars = octocode_native::response::pager::budgeted_chars(&output).expect("page");
            assert!(
                chars <= budget || output["oversized"] == true,
                "oversize must be explicit: {chars}/{budget} {output}"
            );
            let rows = output["results"].as_array().cloned().unwrap_or_default();
            paths.extend(rows.iter().map(|row| row["data"]["path"].clone()));
            parts.extend(
                rows.iter()
                    .map(|row| row["data"]["content"].as_str().unwrap_or("").to_owned()),
            );
            // Parts repeat empty strings by design: the walker checks the
            // page contract and leads, these checks the join.
            walker.accept(support::checks::Page {
                rows: Vec::new(),
                output,
            });
        }
        walker.finish();
        walks.push((parts, paths));
    }
    walks
}

/// B113: a row whose long `path` (an identity field) spans parts delivers
/// each `content` line once: later parts repeat the path whole and carry
/// the required `content` empty, so the joined parts equal the single-page
/// row's content at every budget.
#[tokio::test]
async fn a_long_path_row_delivers_each_content_line_once_across_its_parts() {
    let workspace = Workspace::new();
    let file = format!("{}/f.txt", "d".repeat(200));
    let text = "one\ntwo\nthree\n";
    workspace.write(&file, text);
    let runtime = workspace.runtime(&[]);
    let (whole, _) = content_parts(&runtime, &file, 100_000).await.remove(0);
    let single = whole.concat();
    assert!(single.contains("three"), "{single}");
    for budget in (460..=520).step_by(10) {
        for (parts, paths) in content_parts(&runtime, &file, budget).await {
            assert_eq!(parts.concat(), single, "budget {budget}: joined content");
            for line in ["one", "two", "three"] {
                let count = parts
                    .iter()
                    .flat_map(|part| part.lines())
                    .filter(|l| l.ends_with(line))
                    .count();
                assert_eq!(count, 1, "budget {budget}: {line} in {parts:?}");
            }
            // A fresh first page may drop the path its query echoes; every
            // part that names it names it whole.
            assert!(
                paths
                    .iter()
                    .all(|path| path.is_null() || path == &json!(file)),
                "budget {budget}: identity repeats whole: {paths:?}"
            );
        }
    }
    runtime.close().await;
}

/// A single `content` line over three pages long still pages in windows
/// whose concatenation is the line.
#[tokio::test]
async fn a_single_huge_content_line_pages_in_windows() {
    let workspace = Workspace::new();
    let line = "界😀evidence ".repeat(300);
    workspace.write("huge.txt", format!("{line}\n"));
    let runtime = workspace.runtime(&[]);
    let budget = 800;
    assert!(line.encode_utf16().count() > 3 * budget);
    for (parts, _) in content_parts(&runtime, "huge.txt", budget).await {
        assert!(parts.len() > 3, "windows: {}", parts.len());
        assert!(parts.concat().contains(&line));
    }
    runtime.close().await;
}

#[tokio::test]
async fn local_source_pages_preserve_numbered_unicode_lines_on_both_transports() {
    let workspace = Workspace::new();
    let file = workspace.write("source.txt", body());
    let runtime = workspace.runtime(&[]);
    assert_complete(&runtime, "localFetch", json!({"path":file})).await;
    runtime.close().await;
}

#[tokio::test]
async fn github_source_pages_preserve_numbered_unicode_lines_on_both_transports() {
    let server = MockServer::builder().start().await;
    let body = body();
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/src%2Fsource.rs")).and(query_param("ref",SHA)).respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body),"size":body.len(),"sha":"f".repeat(40),"path":"src/source.rs"}))).mount(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    assert_complete(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","ref":SHA,"path":"src/source.rs"}),
    )
    .await;
    runtime.close().await;
}
