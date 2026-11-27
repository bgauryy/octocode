//! A private key whose every line stands behind a lead (a comment mark, a
//! bullet, a box-drawing tree, an ellipsis) is served as a commented one is:
//! its body hidden in every localFetch and localSearch row, the row marked
//! `redacted`, and prose and file trees beside it as written. All key bytes
//! are fake.

use crate::support::{Workspace, call, query_path, row_data};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

const BODY: [&str; 3] = [
    "WzHvuy3fEWJr5musirUr5JYe5CMpm8DI2mKvqGsxgG5oDTudHZGh0Z7tcHL5kVeq",
    "c3BsaXRLZXlCb2R5VHdvQkFCQUJBQkFCQUJBQkFCQUJBQkFCQUJBQkFCQUJBQkFC",
    "Rk9vYmFyQmF6UXV4MTIzNDU2Nzg5MEFiY0RlZkdoaUprbE1ub1BxclN0dVZ3eFl6",
];

const LEADS: [&str; 10] = [
    "• ",
    "│ ",
    "…",
    "▸ ",
    "★ ",
    "├── ",
    "# ",
    "// ",
    "> ",
    " * ",
];

const PROSE: &str = "• This release improves the parser and keeps every option readable.\n• Another bullet explains the configuration file layout in plain words.\n★ Highlight: nothing secret here at all\n";

const TREE: &str = "src\n├── main.rs\n├── lib.rs\n│   ├── security.rs\n└── README.md\n";

fn key_behind(lead: &str) -> String {
    let body: String = BODY.iter().map(|line| format!("{lead}{line}\n")).collect();
    format!(
        "{PROSE}{lead}-----BEGIN RSA PRIVATE KEY-----\n{body}{lead}-----END RSA PRIVATE KEY-----\n{TREE}"
    )
}

/// Every string in `value`, one per line.
fn strings(value: &Value) -> String {
    match value {
        Value::String(text) => format!("{text}\n"),
        Value::Array(items) => items.iter().map(strings).collect(),
        Value::Object(fields) => fields.values().map(strings).collect(),
        _ => String::new(),
    }
}

fn assert_body_hidden(shown: &Value, lead: &str) {
    let text = strings(shown);
    for line in BODY {
        assert!(!text.contains(&line[..16]), "{lead:?}: {shown}");
    }
}

/// The text of each line a read's numbered content shows.
fn shown_lines(data: &Value) -> Vec<&str> {
    data["content"]
        .as_str()
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('\t').map(|(_, text)| text))
        .collect()
}

/// Each read and each search row of the key shows no body byte, shows its
/// armor lines and the prose and tree around it as written, and is marked
/// `redacted`.
#[tokio::test]
async fn a_key_behind_any_lead_is_hidden_in_every_read_and_search_row() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (index, lead) in LEADS.iter().enumerate() {
        let path = workspace.write(&format!("fx/notes{index}.md"), key_behind(lead));
        for extra in [
            json_fixture("{}"),
            json_fixture(r#"{"ranges": [{"startLine": 5, "endLine": 7}]}"#),
            json_fixture(
                r#"{"matchString": ["PRIVATE KEY"], "contextLines": {"before": 2, "after": 2}}"#,
            ),
        ] {
            let read = call(&runtime, "localFetch", query_path(&path, extra.clone()))
                .await
                .expect("read");
            let data = row_data(&read);
            assert_body_hidden(&data["content"], lead);
            assert!(
                shown_lines(data)
                    .iter()
                    .any(|line| line.contains("[REDACTED-PRIVATEKEYFRAGMENT]")),
                "{lead:?} {extra}: {data}"
            );
            assert_eq!(
                data["redacted"],
                Value::Bool(true),
                "{lead:?} {extra}: {data}"
            );
        }
        let whole = call(
            &runtime,
            "localFetch",
            query_path(&path, json_fixture("{}")),
        )
        .await
        .expect("read");
        let shown = shown_lines(row_data(&whole));
        let begin = format!("{lead}-----BEGIN RSA PRIVATE KEY-----");
        let end = format!("{lead}-----END RSA PRIVATE KEY-----");
        for kept in PROSE
            .lines()
            .chain(TREE.lines())
            .chain([begin.as_str(), end.as_str()])
        {
            assert!(shown.contains(&kept), "{lead:?}: {kept:?} not in {shown:?}");
        }
        let around = call(
            &runtime,
            "localSearch",
            json!({"path": path, "matchString": ["PRIVATE KEY"], "view": "content", "contextLines": {"before": 3, "after": 3}}),
        )
        .await
        .expect("search");
        let data = row_data(&around);
        assert_body_hidden(&data["files"], lead);
        assert!(
            strings(&data["files"]).contains("[REDACTED-PRIVATEKEYFRAGMENT]"),
            "{lead:?}: {data}"
        );
        assert_eq!(data["redacted"], Value::Bool(true), "{lead:?}: {data}");
        for view in ["content", "matchOnly"] {
            let found = call(
                &runtime,
                "localSearch",
                json!({"path": path, "matchString": [&BODY[1][..20]], "view": view}),
            )
            .await
            .expect("search");
            assert_body_hidden(&row_data(&found)["files"], lead);
        }
    }
    runtime.close().await;
}

/// Bulleted prose and a box-drawing tree of file names, with no key beside
/// them, are served as written and no row is marked `redacted`.
#[tokio::test]
async fn bulleted_prose_and_a_file_tree_are_served_as_written() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let source = format!("{PROSE}{TREE}");
    let path = workspace.write("fx/README.md", &source);
    let read = call(
        &runtime,
        "localFetch",
        query_path(&path, json_fixture("{}")),
    )
    .await
    .expect("read");
    let data = row_data(&read);
    assert_eq!(
        shown_lines(data),
        source.lines().collect::<Vec<_>>(),
        "{data}"
    );
    assert!(data.get("redacted").is_none(), "{data}");
    let found = call(
        &runtime,
        "localSearch",
        json!({"path": path, "matchString": ["•", "├──"], "view": "content"}),
    )
    .await
    .expect("search");
    let data = row_data(&found);
    let values = strings(&data["files"]);
    for line in PROSE.lines().chain(TREE.lines()) {
        if line.contains('•') || line.contains("├──") {
            assert!(values.contains(line), "{line:?}: {data}");
        }
    }
    assert!(data.get("redacted").is_none(), "{data}");
    runtime.close().await;
}
