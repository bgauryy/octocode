//! The first answer on a document this request opened states the server's
//! diagnostics for it, or that none arrived: it never reads as a document
//! without errors only because the server had not reported yet.

use super::*;

/// `/*SCENARIO*/` is replaced by `const scenario = '…';`.
const ANSWERS: &str = r#"
  /*SCENARIO*/
  const anchor = file('src/core.ts');
  const site = { uri: anchor, range: at(0, 16, 4) };
  const error = { range: at(1, 9, 5), severity: 1, source: 'fake', message: "Cannot find name 'value'." };
  switch (msg.method) {
    case 'initialize': {
      const capabilities = { textDocumentSync: 1, definitionProvider: true, referencesProvider: true, documentSymbolProvider: true };
      if (scenario === 'pull') capabilities.diagnosticProvider = { interFileDependencies: true, workspaceDiagnostics: false };
      return { capabilities };
    }
    case 'textDocument/definition':
      return [site];
    case 'textDocument/references':
      // A push server reports a while after it answers, as tsserver does.
      if (scenario === 'push') setTimeout(() => send({ jsonrpc: '2.0', method: 'textDocument/publishDiagnostics', params: { uri: anchor, diagnostics: [error] } }), 100);
      return [site];
    case 'textDocument/diagnostic':
      return { kind: 'full', items: [error] };
    case 'textDocument/documentSymbol':
      return [];
    default:
      return null;
  }
"#;

/// `references` on `src/core.ts` with a server playing `scenario`, on a
/// fresh pool: the request opens the document.
fn first_references(scenario: &str) -> serde_json::Value {
    let answers = ANSWERS.replace("/*SCENARIO*/", &format!("const scenario = '{scenario}';"));
    let (_dir, root, _opened, execution) = repository_fixture(
        &[
            ("src/tsconfig.json", "{}\n"),
            (
                "src/core.ts",
                "export function core() {\n  return value + 1;\n}\n",
            ),
        ],
        &answers,
        (&[".ts"], "typescript"),
    );
    run_once(
        &root,
        &execution,
        serde_json::json!({
            "operation": "references", "path": root.join("src/core.ts").to_string_lossy(),
            "symbolName": "core", "line": 1
        }),
    )
}

/// The error counts a row's analysis context states.
fn error_counts(row: &serde_json::Value) -> (Option<&str>, Option<u64>) {
    let context = &row["lsp"]["analysisContext"];
    (
        context["diagnostics"].as_str(),
        context["errorCount"].as_u64(),
    )
}

/// A server that publishes a moment after its answer: the first answer
/// waits for that report and states its error.
#[test]
fn a_first_answer_states_the_diagnostics_published_after_it() {
    if !require_node() {
        return;
    }
    let row = first_references("push");
    assert_eq!(error_counts(&row), (Some("errors"), Some(1)), "{row}");
    assert!(
        row["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&serde_json::json!("documentAnalysisErrors"))),
        "{row}"
    );
}

/// A server that never reports: the row keeps its `lsp` field and says the
/// diagnostics are unavailable, so it does not read as free of errors.
#[test]
fn an_unpublished_document_reads_unavailable_not_clean() {
    if !require_node() {
        return;
    }
    let row = first_references("silent");
    assert_eq!(error_counts(&row), (Some("unavailable"), None), "{row}");
}

/// A server that answers pulls but pushes nothing: the listed answer asks it.
#[test]
fn a_pull_server_without_a_push_is_pulled() {
    if !require_node() {
        return;
    }
    let row = first_references("pull");
    assert_eq!(error_counts(&row), (Some("errors"), Some(1)), "{row}");
}
