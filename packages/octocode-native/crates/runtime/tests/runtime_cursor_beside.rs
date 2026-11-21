//! A cursor call that also sends query fields: the cursor restores its whole
//! saved query, so a field at the value that query holds reads the cursor's
//! page and is said once, naming the field; a field at another value, or
//! one the saved query does not hold, asks for another query and is refused,
//! the refusal leading with that field; a private progress field is refused
//! whatever its value. Each call is read as an MCP client reads it, from the
//! one JSON text block.
use crate::support::{self, Run as _, Workspace};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Repositories the mock search index holds.
const REPOSITORIES: usize = 20;

/// GitHub's repository search over [`REPOSITORIES`], answering
/// `page`/`per_page` as GitHub does.
struct Index;

impl Respond for Index {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let param = |name: &str, default: usize| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<usize>().ok())
                .unwrap_or(default)
        };
        let (page, per) = (param("page", 1).max(1), param("per_page", 30));
        let items = (0..REPOSITORIES)
            .skip((page - 1) * per)
            .take(per)
            .map(|n| {
                let name = format!("r{n:02}");
                json!({
                    "name": name, "full_name": format!("o/{name}"),
                    "html_url": format!("https://github.com/o/{name}"), "default_branch": "main",
                    "stargazers_count": 1, "pushed_at": "2024-01-01T00:00:00Z"
                })
            })
            .collect::<Vec<_>>();
        ResponseTemplate::new(200).set_body_json(json!({
            "total_count": REPOSITORIES, "incomplete_results": false, "items": items
        }))
    }
}

async fn search_index() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(Index)
        .mount(&server)
        .await;
    server
}

/// The MCP result of one call, and its JSON payload.
async fn mcp(runtime: &ToolRuntime, id: &str, tool: &str, input: Value) -> (bool, Value) {
    let result = runtime
        .run_mcp(id.into(), tool.into(), input)
        .await
        .expect("an MCP result");
    (result["isError"] == true, support::mcp_payload(&result))
}

/// `cursor` with `fields` sent beside it.
fn beside(cursor: &Value, fields: Value) -> Value {
    let mut input = cursor.clone();
    for (field, value) in fields.as_object().expect("fields") {
        input[field] = value.clone();
    }
    input
}

fn warnings(payload: &Value) -> Vec<String> {
    payload["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The notes that say a field beside the cursor was not applied.
fn beside_notes(payload: &Value) -> Vec<String> {
    warnings(payload)
        .into_iter()
        .filter(|note| note.contains("beside `cursor`"))
        .collect()
}

fn repositories(payload: &Value) -> Vec<Value> {
    payload["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| row["data"]["repositories"].as_array().cloned())
        .flatten()
        .collect()
}

/// A ghSearchRepo runtime over the mock index, and the cursor of its first
/// page.
async fn first_repository_page(workspace: &Workspace, server: &MockServer) -> (ToolRuntime, Value) {
    let runtime = workspace.runtime(&support::github_api(server));
    let (failed, first) = mcp(
        &runtime,
        "first",
        "ghSearchRepo",
        json!({"queries": [{"operation": "search", "keywords": ["x"], "limit": 3}]}),
    )
    .await;
    assert!(!failed, "{first}");
    let next = support::next_input(&first).expect("a second page");
    support::assert_cursor_only(&next);
    (runtime, next)
}

/// The query fields the cursor's saved query holds, sent beside it at the
/// same values, read the page the cursor alone reads, with one note naming
/// each field; the walk goes on from the same next cursor, so the saved
/// query is unchanged.
#[tokio::test]
async fn fields_beside_a_cursor_at_its_saved_values_read_its_page_and_are_said_once() {
    let server = search_index().await;
    let workspace = Workspace::new();
    let (runtime, next) = first_repository_page(&workspace, &server).await;
    let (failed, alone) = mcp(&runtime, "alone", "ghSearchRepo", next.clone()).await;
    assert!(!failed, "{alone}");
    assert!(!repositories(&alone).is_empty(), "{alone}");
    assert!(beside_notes(&alone).is_empty(), "{alone}");
    for (sent, fields) in [
        (json!({"operation": "search"}), vec!["`operation`"]),
        (json!({"keywords": ["x"]}), vec!["`keywords`"]),
        (
            json!({"operation": "search", "keywords": ["x"]}),
            vec!["`operation`", "`keywords`"],
        ),
    ] {
        let (failed, page) = mcp(
            &runtime,
            "beside",
            "ghSearchRepo",
            beside(&next, sent.clone()),
        )
        .await;
        assert!(!failed, "{sent}: {page}");
        assert_eq!(repositories(&page), repositories(&alone), "{sent}");
        assert_eq!(
            support::next_input(&page),
            support::next_input(&alone),
            "{sent}: the walk goes on from the saved query"
        );
        let notes = beside_notes(&page);
        assert!(
            matches!(notes.as_slice(), [note]
                if fields.iter().all(|field| note.contains(field))
                    && note.contains(r#"{"cursor":"…"} alone"#)),
            "{sent}: {notes:?}"
        );
        let rest: Vec<String> = warnings(&page)
            .into_iter()
            .filter(|note| !note.contains("beside `cursor`"))
            .collect();
        assert_eq!(rest, warnings(&alone), "{sent}");
    }
    runtime.close().await;
}

/// A query field beside the cursor at another value than its saved query
/// holds asks for another query: refused, naming the value the cursor
/// holds. A field the saved query does not hold is refused too, and so is a
/// cursor inside a row.
#[tokio::test]
async fn a_field_beside_a_cursor_at_another_value_is_refused_naming_the_saved_value() {
    let server = search_index().await;
    let workspace = Workspace::new();
    let (runtime, next) = first_repository_page(&workspace, &server).await;
    for (sent, saved) in [
        (json!({"operation": "list"}), r#"`operation: "search"`"#),
        (json!({"keywords": ["y"]}), r#"`keywords: ["x"]`"#),
        (
            json!({"operation": "search", "keywords": ["y"]}),
            r#"`keywords: ["x"]`"#,
        ),
    ] {
        let (failed, refused) = mcp(
            &runtime,
            "differs",
            "ghSearchRepo",
            beside(&next, sent.clone()),
        )
        .await;
        assert!(failed, "{sent}: {refused}");
        assert_eq!(refused["errorCode"], "invalidInput", "{sent}: {refused}");
        let error = refused["error"].as_str().unwrap_or_default();
        assert!(error.contains("beside cursor"), "{sent}: {error}");
        assert!(
            error.contains("The cursor's query holds") && error.contains(saved),
            "{sent}: {error}"
        );
    }
    // A field the saved query does not hold, and a cursor sent inside a row
    // (a shape no tool's contract takes, the cursor alone included).
    for sent in [
        beside(&next, json!({"owner": "o"})),
        json!({"queries": [beside(&next, json!({"operation": "search"}))]}),
    ] {
        let (failed, refused) = mcp(&runtime, "unheld", "ghSearchRepo", sent.clone()).await;
        assert!(failed, "{sent}: {refused}");
        assert_eq!(refused["errorCode"], "invalidInput", "{sent}: {refused}");
    }
    runtime.close().await;
}

/// A private progress field beside the cursor is refused before any value
/// is read, the value its saved query holds included, and no saved value
/// is named.
#[tokio::test]
async fn a_private_field_beside_a_cursor_is_refused_at_any_value() {
    let server = search_index().await;
    let workspace = Workspace::new();
    let (runtime, next) = first_repository_page(&workspace, &server).await;
    for page in 1..=4 {
        for sent in [
            json!({"page": page}),
            json!({"operation": "search", "page": page}),
        ] {
            let (failed, refused) = mcp(
                &runtime,
                "private",
                "ghSearchRepo",
                beside(&next, sent.clone()),
            )
            .await;
            assert!(failed, "{sent}: {refused}");
            assert_eq!(refused["errorCode"], "invalidInput", "{sent}: {refused}");
            let error = refused["error"].as_str().unwrap_or_default();
            assert!(
                !error.contains("The cursor's query holds"),
                "{sent}: {error}"
            );
        }
    }
    runtime.close().await;
}

/// A refusal's error, then its details: what a caller reads, in order.
fn refusal_text(refused: &Value) -> String {
    std::iter::once(&refused["error"])
        .chain(refused["details"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The field of `sent` a refusal's error names first, however it quotes it.
fn named_first<'a>(refused: &Value, sent: &[&'a str]) -> Option<&'a str> {
    let error = refused["error"].as_str().unwrap_or_default();
    sent.iter()
        .filter_map(|field| {
            [
                format!("'{field}'"),
                format!("`{field}`"),
                format!("`{field}:"),
            ]
            .iter()
            .filter_map(|spelling| error.find(spelling.as_str()))
            .min()
            .map(|at| (at, *field))
        })
        .min()
        .map(|(_, field)| field)
}

/// A refused cursor call leads with the field that refuses it, in whichever
/// order the fields were sent: a private one, one at another value than the
/// saved query holds, or one it does not hold. A field at its saved value
/// beside such a field is no cause: validation does not refuse it, and the
/// refusal names it as one to drop. Beside a private field nothing is
/// compared, so no saved value is named.
#[tokio::test]
async fn a_refusal_leads_with_the_field_that_causes_it() {
    let server = search_index().await;
    let workspace = Workspace::new();
    let (runtime, next) = first_repository_page(&workspace, &server).await;
    for (cause, value, compared) in [
        ("page", json!(2), false),
        ("keywords", json!(["y"]), true),
        ("owner", json!("o"), true),
    ] {
        let mut cause_first = serde_json::Map::new();
        cause_first.insert(cause.to_owned(), value.clone());
        cause_first.insert("operation".to_owned(), json!("search"));
        let mut held_first = serde_json::Map::new();
        held_first.insert("operation".to_owned(), json!("search"));
        held_first.insert(cause.to_owned(), value.clone());
        for sent in [Value::Object(cause_first), Value::Object(held_first)] {
            let (failed, refused) = mcp(
                &runtime,
                "cause",
                "ghSearchRepo",
                beside(&next, sent.clone()),
            )
            .await;
            assert!(failed, "{sent}: {refused}");
            assert_eq!(refused["errorCode"], "invalidInput", "{sent}: {refused}");
            assert_eq!(
                named_first(&refused, &[cause, "operation"]),
                Some(cause),
                "{sent}: {refused}"
            );
            let text = refusal_text(&refused);
            assert_eq!(
                text.contains("The cursor's query holds"),
                cause == "keywords",
                "{sent}: {text}"
            );
            if compared {
                assert!(!text.contains("'operation'"), "{sent}: {text}");
                assert!(
                    refused["error"]
                        .as_str()
                        .is_some_and(|error| error.contains("`operation`")),
                    "{sent}: {text}"
                );
            }
        }
    }
    runtime.close().await;
}

/// A page size beside a cursor keeps its own note, alone or beside a query
/// field at its saved value, and the page read is the cursor's.
#[tokio::test]
async fn a_page_size_beside_a_cursor_keeps_its_own_note() {
    let server = search_index().await;
    let workspace = Workspace::new();
    let (runtime, next) = first_repository_page(&workspace, &server).await;
    let (_, alone) = mcp(&runtime, "alone", "ghSearchRepo", next.clone()).await;
    for (sent, held_note) in [
        (json!({"responseChars": 900}), false),
        (json!({"responseChars": 900, "operation": "search"}), true),
    ] {
        let (failed, page) = mcp(
            &runtime,
            "sized",
            "ghSearchRepo",
            beside(&next, sent.clone()),
        )
        .await;
        assert!(!failed, "{sent}: {page}");
        assert_eq!(repositories(&page), repositories(&alone), "{sent}");
        let notes = beside_notes(&page);
        let size = notes
            .iter()
            .filter(|note| {
                note.contains("`responseChars`") && note.contains("page size it was issued with")
            })
            .count();
        let held = notes
            .iter()
            .filter(|note| note.contains("`operation`") && note.contains("already holds"))
            .count();
        assert_eq!(
            (size, held),
            (1, usize::from(held_note)),
            "{sent}: {notes:?}"
        );
        assert_eq!(notes.len(), size + held, "{sent}: {notes:?}");
    }
    runtime.close().await;
}

/// The same for a response page of a local search: its `path`, at the value
/// sent, is dropped and said; another path is refused naming the saved one.
#[tokio::test]
async fn a_path_beside_a_local_search_page_cursor_is_read_against_its_saved_query() {
    let workspace = Workspace::new();
    let file = workspace.write(
        "captured.txt",
        (0..120)
            .map(|index| format!("needle captured {index:03}\n"))
            .collect::<String>(),
    );
    let other = workspace.write("other.txt", "needle\n");
    let runtime = workspace.runtime(&[]);
    let (failed, first) = mcp(
        &runtime,
        "first",
        "localSearch",
        json!({"queries": [{"path": file, "matchString": ["needle"]}], "responseChars": 1200}),
    )
    .await;
    assert!(!failed, "{first}");
    let next = first
        .pointer("/next/input")
        .cloned()
        .expect("a response page");
    support::assert_cursor_only(&next);
    let (_, alone) = mcp(&runtime, "alone", "localSearch", next.clone()).await;
    let (failed, page) = mcp(
        &runtime,
        "path",
        "localSearch",
        beside(&next, json!({"path": file, "matchString": ["needle"]})),
    )
    .await;
    assert!(!failed, "{page}");
    assert_eq!(page["results"], alone["results"]);
    assert_eq!(page.pointer("/next"), alone.pointer("/next"));
    let notes = beside_notes(&page);
    assert!(
        matches!(notes.as_slice(), [note] if note.contains("`path`") && note.contains("`matchString`")),
        "{notes:?}"
    );
    let (failed, refused) = mcp(
        &runtime,
        "other",
        "localSearch",
        beside(&next, json!({"path": other})),
    )
    .await;
    assert!(failed, "{refused}");
    assert_eq!(refused["errorCode"], "invalidInput", "{refused}");
    let error = refused["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("The cursor's query holds `path: "),
        "{error}"
    );
    runtime.close().await;
}
