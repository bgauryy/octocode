//! An IDE answer takes the post-processing a server answer takes: the
//! empty-answer diagnostics note, the implementation heritage scan and the
//! TypeScript alias naming, so one query reads the same whichever rung answered.
use super::{Bridge, Editor, location, result};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

fn context(row: &Value) -> &Value {
    &row["lsp"]["analysisContext"]
}

/// An empty references answer says what the document's diagnostics held, as a
/// server's does: errors at the anchor explain the emptiness.
#[tokio::test(flavor = "current_thread")]
async fn an_empty_ide_answer_carries_the_diagnostics_note() {
    let editor = Editor::new();
    let uri = editor.uri();
    let range = location(&uri, 0, 16, 22)["range"].clone();
    let bridge = Bridge::serve(Box::new(move |body| match body["method"].as_str() {
        Some("references") => result(json!([])),
        Some("diagnostics") => result(json!([
            {"range": range, "severity": 1, "message": "cannot resolve", "source": "ts"}
        ])),
        _ => (
            404,
            json_fixture(r#"{"error": "unknown", "reason": "method"}"#),
        ),
    }));
    editor.open(&bridge);
    let row = editor
        .ask(json_fixture(
            r#"{
                "operation": "references",
                "symbolName": "target",
                "line": 1
            }"#,
        ))
        .await;
    assert_eq!(row["lsp"]["level"], "ide", "{row}");
    assert_eq!(row["status"], "empty", "{row}");
    assert_eq!(context(&row)["diagnostics"], "errors", "{row}");
    assert_eq!(context(&row)["anchorErrorCount"], 1, "{row}");
    bridge.assert_authentic();
}

/// A clean document says so, and an editor that cannot report says that.
#[tokio::test(flavor = "current_thread")]
async fn an_empty_ide_answer_names_what_diagnostics_could_not_show() {
    let editor = Editor::new();
    let clean = Bridge::serve(Box::new(|body| match body["method"].as_str() {
        Some("references") | Some("diagnostics") => result(json!([])),
        _ => (
            404,
            json_fixture(r#"{"error": "unknown", "reason": "method"}"#),
        ),
    }));
    editor.open(&clean);
    let row = editor
        .ask(json_fixture(
            r#"{
                "operation": "references",
                "symbolName": "target",
                "line": 1
            }"#,
        ))
        .await;
    assert_eq!(context(&row)["diagnostics"], "noErrorsReported", "{row}");
    let silent = Bridge::serve(Box::new(|body| match body["method"].as_str() {
        Some("references") => result(json!([])),
        _ => (
            404,
            json_fixture(r#"{"error": "unknown", "reason": "method"}"#),
        ),
    }));
    editor.open(&silent);
    let row = editor
        .ask(json_fixture(
            r#"{
                "operation": "references",
                "symbolName": "target",
                "line": 1
            }"#,
        ))
        .await;
    assert_eq!(context(&row)["diagnostics"], "unavailable", "{row}");
}

/// A type alias the editor reports as a variable is named a type, as the
/// server rung names it.
#[tokio::test(flavor = "current_thread")]
async fn an_ide_outline_names_a_typescript_alias_a_type() {
    let editor = Editor::new();
    std::fs::write(&editor.file, "export type Id = string;\n").expect("source");
    let range = json_fixture(
        r#"{
            "start": {"line": 0, "character": 12},
            "end": {"line": 0, "character": 14}
        }"#,
    );
    let bridge = Bridge::serve(Box::new(move |_| {
        result(json!([{"name": "Id", "kind": 13, "range": range, "selectionRange": range}]))
    }));
    editor.open(&bridge);
    let row = editor
        .ask(json_fixture(r#"{"operation": "documentSymbols"}"#))
        .await;
    assert_eq!(row["lsp"]["level"], "ide", "{row}");
    assert_eq!(row["symbols"][0]["kind"], "type", "{row}");
}

/// `implementation` on a type lists the heritage-scan declarations the
/// editor left out, each labelled and the row partial, as a server answer does.
#[tokio::test(flavor = "current_thread")]
async fn an_ide_implementation_lists_what_the_heritage_scan_found_beside_it() {
    let editor = Editor::new();
    std::fs::write(
        &editor.file,
        "export interface Base {}\nexport class A implements Base {}\n",
    )
    .expect("source");
    std::fs::write(
        editor.root.join("b.ts"),
        "import { Base } from './a';\nexport class B implements Base {}\n",
    )
    .expect("sibling");
    let uri = editor.uri();
    let bridge = Bridge::serve(Box::new(move |body| match body["method"].as_str() {
        Some("implementation") => result(json!([location(&uri, 1, 13, 14)])),
        Some("diagnostics") => result(json!([])),
        _ => (
            404,
            json_fixture(r#"{"error": "unknown", "reason": "method"}"#),
        ),
    }));
    editor.open(&bridge);
    let row = editor
        .ask(json_fixture(
            r#"{
                "operation": "implementation",
                "symbolName": "Base",
                "line": 1
            }"#,
        ))
        .await;
    assert_eq!(row["lsp"]["level"], "ide", "{row}");
    let text = row.to_string();
    assert!(
        text.contains("b.ts"),
        "the scan's declaration is listed: {row}"
    );
    assert!(text.contains("native-graph-facts"), "and labelled: {row}");
    assert_eq!(row["isPartial"], true, "{row}");
}
