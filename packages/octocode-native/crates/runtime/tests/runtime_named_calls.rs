//! A served reply names no call it does not carry, one case per tool
//! family, through the response stage. Every `call` of this test crate
//! checks the rule ([`crate::support::assert_named_calls_carried`]); these
//! cases are the ones a producer below the stage cannot see: an answer
//! whose warning sends the reader to a `related` call the row needs, and an
//! empty or failed row whose recovery names its calls.
use crate::support::{Workspace, assert_named_calls_carried, call, row_data, row_status};
use octocode_native::runtime::ToolRuntime;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// More matches than any search provider window reaches.
const PAST_EVERY_WINDOW: u64 = 1_000_000;

/// Whether a sentence of `data` (a warning, hint, or a note's recovery)
/// names `related`.
fn names_related(data: &Value) -> bool {
    let said = |key: &str| {
        data[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let recoveries = data["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|note| note["recovery"].as_str())
        .map(str::to_owned);
    said("warnings")
        .into_iter()
        .chain(said("hints"))
        .chain(recoveries)
        .any(|sentence| {
            sentence
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|word| word == "related")
        })
}

/// The case holds what it tests: the row says `related`, and carries a
/// call to `tool` there.
fn assert_names_and_carries(data: &Value, tool: &str) {
    assert!(names_related(data), "the case must name related: {data}");
    assert!(
        data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|lead| lead["tool"] == tool),
        "a related {tool} call: {data}"
    );
}

fn github(workspace: &Workspace, server: &MockServer) -> ToolRuntime {
    workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))])
}

async fn search_server(route: &str, items: Value) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/search/{route}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": PAST_EVERY_WINDOW, "incomplete_results": false, "items": items
        })))
        .mount(&server)
        .await;
    server
}

/// GitHub: a search with more matches than the provider window answers
/// with its rows, says the window ends, and carries the search that goes on
/// past it.
#[tokio::test]
async fn a_capped_github_search_carries_the_call_past_its_window() {
    let workspace = Workspace::new();
    let issues = search_server(
        "issues",
        json!([{"number": 1, "title": "t", "state": "open", "user": {"login": "x"},
            "labels": [], "pull_request": {"url": "u"},
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z"}]),
    )
    .await;
    let runtime = github(&workspace, &issues);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json_fixture(
            r#"{"operation": "pullRequest", "owner": "a", "repo": "b", "keywords": ["fix"]}"#,
        ),
    )
    .await
    .expect("history search");
    assert_eq!(row_status(&outcome), "success", "{}", row_data(&outcome));
    assert_names_and_carries(row_data(&outcome), "ghSearchHistory");
    runtime.close().await;

    let repositories = search_server(
        "repositories",
        json!([{"name": "b", "full_name": "a/b", "owner": {"login": "a"},
            "html_url": "https://github.com/a/b", "default_branch": "main",
            "stargazers_count": 1, "created_at": "2020-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z", "pushed_at": "2024-01-02T00:00:00Z"}]),
    )
    .await;
    let runtime = github(&workspace, &repositories);
    let outcome = call(
        &runtime,
        "ghSearchRepo",
        json_fixture(r#"{"operation": "search", "keywords": ["parser"]}"#),
    )
    .await
    .expect("repository search");
    assert_eq!(row_status(&outcome), "success", "{}", row_data(&outcome));
    assert_names_and_carries(row_data(&outcome), "ghSearchRepo");
    runtime.close().await;
}

/// LSP: a declaration deep inside one long line is shown as a window of
/// that line; the row says bytes before it were left out and carries the
/// read of them.
#[tokio::test]
async fn an_lsp_window_inside_a_long_line_carries_the_read_of_what_it_omits() {
    let workspace = Workspace::new();
    let padding = "x".repeat(64 * 1024);
    let file = workspace.write(
        "src/long.ts",
        format!("const pad = \"{padding}\"; function needle() {{ return 1; }}\nneedle();\n"),
    );
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({"path": file, "symbolName": "needle", "line": 2, "operation": "definition"}),
    )
    .await
    .expect("definition");
    let data = row_data(&outcome);
    let omitted = data.to_string().contains("bytes omitted before the window");
    assert!(omitted, "the window must open inside the line: {data}");
    assert!(
        data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|lead| lead["tool"] == "localFetch"),
        "the read of the omitted bytes: {data}"
    );
    runtime.close().await;
}

/// AST: a pattern that matches nothing while nodes of its kind and name
/// exist answers empty, and the note's recovery names the calls it carries.
#[tokio::test]
async fn an_ast_exact_miss_carries_the_calls_its_recovery_names() {
    let workspace = Workspace::new();
    let file = workspace.write(
        "src/miss.ts",
        "function target(a: number) {\n  return a;\n}\n",
    );
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "astSearch",
        json!({"path": file, "operation": "match",
            "rule": {"pattern": "function target() { return 2; }"}}),
    )
    .await
    .expect("match");
    assert_eq!(row_status(&outcome), "empty", "{}", row_data(&outcome));
    assert_names_and_carries(row_data(&outcome), "astSearch");
    runtime.close().await;
}

/// Local: a case-sensitive miss whose other-case spelling exists answers
/// empty with one hint that names its call.
#[tokio::test]
async fn a_local_miss_carries_the_call_its_hint_names() {
    let workspace = Workspace::new();
    let file = workspace.write("src/case.ts", "export const Needle = 1;\n");
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "localFetch",
        json!({"path": file, "matchString": ["needle"], "caseMode": "sensitive"}),
    )
    .await
    .expect("read");
    assert_ne!(row_status(&outcome), "success", "{}", row_data(&outcome));
    assert_names_and_carries(row_data(&outcome), "localFetch");
    runtime.close().await;
}

/// The rule itself: a sentence that names `related` or a `next` key on a
/// row without that call is caught; the same row with the call passes.
#[test]
fn a_named_call_the_row_lacks_is_caught() {
    let lead = json!({"tool": "localFetch", "input": {"queries": [{"path": "a"}]}});
    let reply = |data: Value| json!({"results": [{"index": 0, "data": data}]});
    let caught = |data: Value| {
        std::panic::catch_unwind(|| assert_named_calls_carried("localSearch", &reply(data)))
            .is_err()
    };
    for sentence in [
        "2 lines were left out; the related read shows them.",
        "See related[].",
    ] {
        assert!(caught(json!({"warnings": [sentence]})), "{sentence}");
        assert!(
            !caught(json!({"warnings": [sentence], "related": [lead.clone()]})),
            "{sentence}"
        );
    }
    let paged = "More rows: next.nextPage reads on.";
    assert!(caught(json!({"warnings": [paged]})));
    assert!(!caught(
        json!({"warnings": [paged], "next": {"nextPage": lead.clone()}})
    ));
    assert!(caught(
        json!({"hints": [paged], "next": {"restart": lead.clone()}})
    ));
    // A word that only contains the name, or evidence text, names no call.
    assert!(!caught(json!({
        "warnings": ["2 unrelated files were skipped."],
        "files": [{"path": "a", "value": "related next.nextPage"}]
    })));
}
