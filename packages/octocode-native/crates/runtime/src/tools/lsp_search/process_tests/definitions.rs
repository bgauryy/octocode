//! Definition answers from a real provider: partial follow-ups and locations
//! outside the read roots.

use super::*;

/// A real provider answers the first definition with an alias location, then
/// fails the follow-up. Useful locations survive, but identity stays partial.
#[test]
fn nested_definition_failure_keeps_location_with_partial_provenance() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    for fail in [true, false] {
        let (_dir, root, script) = fixture();
        std::fs::write(root.join("b.ts"), "import { b } from './c';\n").expect("alias source");
        let server = FAKE_SERVER
            .replace("hoverProvider: true", "definitionProvider: true")
            .replace(
                "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
                &format!(r#"  }} else if (msg.method === 'textDocument/definition') {{
    if (msg.params.textDocument.uri.endsWith('/a.ts')) {{
      send({{jsonrpc:'2.0',id:msg.id,result:{{uri:msg.params.textDocument.uri.replace('/a.ts','/b.ts'),range:{{start:{{line:0,character:9}},end:{{line:0,character:10}}}}}}}});
    }} else if ({fail}) {{
      send({{jsonrpc:'2.0',id:msg.id,error:{{code:-32603,message:'injected follow-up failure'}}}});
    }} else {{
      send({{jsonrpc:'2.0',id:msg.id,result:[]}});
    }}"#),
            );
        std::fs::write(&script, server).expect("definition fixture server");
        let paths = crate::tools::test_support::workspace_policy(&root);
        let path = root.join("a.ts").to_string_lossy().into_owned();
        let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
        let query = serde_json::from_value(json!({
            "operation":"definition", "path":uri, "symbolName":"x", "line":1
        }))
        .expect("query");
        tokio::runtime::Runtime::new().expect("rt").block_on(async {
            let client = NativeLspClient::new(config(&root, &script, None));
            client.start().await.expect("server starts");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let row = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                scope: &crate::tools::lsp_search::scope::Scope::new(
                    root.to_string_lossy().into_owned(),
                    Vec::new(),
                ),
                root_only: false,
                line: 0,
                character: 6,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await
            .expect("definition row retains earlier evidence");
            assert!(lists_path(&row, "b.ts"), "{row}");
            if fail {
                assert_eq!(row["isPartial"], true, "{row}");
                assert!(
                    row["partialReasons"]
                        .as_array()
                        .expect("reasons")
                        .contains(&json!("definitionHopFailed")),
                    "{row}"
                );
                // The server's failure reaches the caller in a warning.
                assert!(
                    row["warnings"].as_array().is_some_and(|warnings| warnings
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .any(|warning| warning.contains("injected follow-up failure"))),
                    "{row}"
                );
                assert!(
                    !row["related"].as_array().is_some_and(|calls| calls
                        .iter()
                        .any(|call| { call["kind"] == "related" && call["tool"] == "lspSearch" })),
                    "a failed definition hop is not a retryable rerun: {row}"
                );
            } else {
                assert!(
                    !row["partialReasons"]
                        .as_array()
                        .is_some_and(|reasons| reasons.contains(&json!("definitionHopFailed"))),
                    "{row}"
                );
                assert_absent(&row, "injected follow-up failure");
            }
            client.stop().await.expect("fixture server closed");
        });
    }
}

/// A definition the server places outside the allowed read roots is
/// withheld, and the row says so instead of "no locations".
#[test]
fn a_definition_outside_the_read_roots_is_reported_withheld() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let outside = tempfile::tempdir().expect("outside");
    let target = outside.path().join("dep.ts");
    std::fs::write(&target, "export const a = 1;\n").expect("target");
    let target_uri =
        octocode_engine::lsp::uri::path_to_uri(&target.to_string_lossy()).expect("uri");
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "definitionProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            &format!(
                "  }} else if (msg.method === 'textDocument/definition') {{\n    send({{ jsonrpc: '2.0', id: msg.id, result: [{{ uri: '{target_uri}', range: {{ start: {{ line: 0, character: 13 }}, end: {{ line: 0, character: 14 }} }} }}] }});"
            ),
        );
    std::fs::write(&script, server).expect("fixture server");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("a.ts").to_string_lossy().into_owned();
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(async {
            let client = NativeLspClient::new(config(&root, &script, None));
            client.start().await.expect("server starts");
            let query = serde_json::from_value(json!({
                "operation": "definition", "path": path, "symbolName": "a", "line": 1
            }))
            .expect("query");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let outcome = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                scope: &crate::tools::lsp_search::scope::Scope::new(
                    root.to_string_lossy().into_owned(),
                    vec!["*.ts".into()],
                ),
                root_only: false,
                line: 0,
                character: 6,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await;
            client.stop().await.expect("fixture server closed");
            outcome
        })
        .unwrap_or_else(|error| panic!("the server answers: {error:?}"));
    assert_eq!(row["category"], "locationsWithheld", "{row}");
    assert!(
        row["reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with("1 location")),
        "{row}"
    );
}

/// A server that logs every definition position it is asked and answers
/// from `FAKE_LSP_GRAPH` (`"file:line:character"` → locations); a key
/// listed under `early` answers itself the first time it is asked (an
/// alias binding the server has not resolved yet).
fn logging_definition_server(script: &Path) {
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "definitionProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/definition') {
    const graph = JSON.parse(process.env.FAKE_LSP_GRAPH);
    const uri = msg.params.textDocument.uri;
    const p = msg.params.position;
    const key = `${require('path').basename(new URL(uri).pathname)}:${p.line}:${p.character}`;
    fs.appendFileSync(process.env.FAKE_LSP_LOG, key + '\n');
    const asked = fs.readFileSync(process.env.FAKE_LSP_LOG, 'utf8').split('\n').filter((line) => line === key).length;
    const dir = uri.slice(0, uri.lastIndexOf('/') + 1);
    const at = ([file, line, start, end]) => ({ uri: dir + file, range: { start: { line, character: start }, end: { line, character: end } } });
    const early = graph.early[key];
    send({ jsonrpc: '2.0', id: msg.id, result: (early && asked === 1 ? [early] : graph.answers[key] || []).map(at) });"#,
        );
    std::fs::write(script, server).expect("definition fixture server");
}

/// The definition chain from `file` at zero-based `(line, character)`
/// against [`logging_definition_server`] answering `graph`: the row and
/// every position the server was asked.
fn definition_chain(
    files: &[(&str, &str)],
    graph: serde_json::Value,
    (file, line, character): (&str, u32, u32),
) -> (serde_json::Value, Vec<String>) {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    let (_dir, root, script) = fixture();
    for (name, content) in files {
        std::fs::write(root.join(name), content).expect("source");
    }
    logging_definition_server(&script);
    let log = root.join("definitions.log");
    std::fs::write(&log, "").expect("log");
    let mut config = config(&root, &script, None);
    config.env = Some(
        [
            ("FAKE_LSP_GRAPH".to_owned(), graph.to_string()),
            (
                "FAKE_LSP_LOG".to_owned(),
                log.to_string_lossy().into_owned(),
            ),
        ]
        .into_iter()
        .collect(),
    );
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join(file).to_string_lossy().into_owned();
    let query = serde_json::from_value(serde_json::json!({
        "operation": "definition", "path": path, "symbolName": "foo", "line": line + 1
    }))
    .expect("query");
    let row = tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config);
        client.start().await.expect("server starts");
        let mut sources = SourceCache::new(&paths);
        let policy = snippet_policy(&paths);
        let row = Operation {
            client: &client,
            query: &query,
            sources: &mut sources,
            snippet_policy: &policy,
            cancel: &NeverCancel,
            path: &path,
            scope: &crate::tools::lsp_search::scope::Scope::new(
                root.to_string_lossy().into_owned(),
                Vec::new(),
            ),
            root_only: false,
            line,
            character,
            language_id: Some("typescript"),
            budget: 16_000,
            response_window: None,
        }
        .run()
        .await
        .expect("definition row");
        client.stop().await.expect("fixture server closed");
        row
    });
    let asked = std::fs::read_to_string(&log)
        .expect("log")
        .lines()
        .map(str::to_owned)
        .collect();
    (row, asked)
}

/// A declaration answers its own definition: the chain stops at that first
/// answer, without asking the same symbol again (nor again after a settle).
#[test]
fn a_declaration_anchor_asks_its_definition_once() {
    if !require_node() {
        return;
    }
    let (row, asked) = definition_chain(
        &[("a.ts", "export function foo() {}\n")],
        serde_json::json!({"early": {}, "answers": {
            "a.ts:0:16": [["a.ts", 0, 16, 19]], "a.ts:0:17": [["a.ts", 0, 16, 19]]
        }}),
        ("a.ts", 0, 17),
    );
    assert!(lists_path(&row, "a.ts"), "{row}");
    assert_eq!(asked, ["a.ts:0:17"], "{row}");
}

/// An import binding a server answers with itself before it resolves the
/// alias is asked again after the settle, and the chain reaches the
/// declaration it names.
#[test]
fn an_unresolved_import_binding_is_asked_again_and_reaches_its_declaration() {
    if !require_node() {
        return;
    }
    let (row, asked) = definition_chain(
        &[
            ("a.ts", "export function foo() {}\n"),
            ("b.ts", "import { foo } from './a';\nfoo();\n"),
        ],
        serde_json::json!({
            "early": {"b.ts:0:9": ["b.ts", 0, 9, 12]},
            "answers": {
                "b.ts:1:0": [["b.ts", 0, 9, 12]],
                "b.ts:0:9": [["a.ts", 0, 16, 19]],
                "a.ts:0:16": [["a.ts", 0, 16, 19]]
            }
        }),
        ("b.ts", 1, 0),
    );
    assert!(lists_path(&row, "a.ts"), "the alias resolves: {row}");
    assert!(!lists_path(&row, "b.ts"), "{row}");
    assert_eq!(
        asked.iter().filter(|key| *key == "b.ts:0:9").count(),
        2,
        "the binding is asked again after its early answer: {asked:?}"
    );
}

/// A re-export chain longer than any fixed hop count reaches its
/// declaration: the chain ends where no hop finds a new location, and a
/// cycle ends because every location is followed once.
#[test]
fn a_long_reexport_chain_reaches_its_declaration_and_a_cycle_ends() {
    if !require_node() {
        return;
    }
    let links = 8;
    let names = (0..=links).map(|n| format!("f{n}.ts")).collect::<Vec<_>>();
    let mut files = vec![(
        "use.ts".to_owned(),
        "import { foo } from './f0';\nfoo();\n".to_owned(),
    )];
    let mut answers = serde_json::Map::new();
    answers.insert(
        "use.ts:1:0".into(),
        serde_json::json!([["f0.ts", 0, 9, 12]]),
    );
    for n in 0..links {
        files.push((
            names[n].clone(),
            format!("export {{ foo }} from './f{}';\n", n + 1),
        ));
        answers.insert(
            format!("{}:0:9", names[n]),
            serde_json::json!([[names[n + 1].clone(), 0, 9, 12]]),
        );
    }
    files.push((
        names[links].clone(),
        "export { foo };\nfunction foo() {}\n".to_owned(),
    ));
    answers.insert(
        format!("{}:0:9", names[links]),
        serde_json::json!([[names[links].clone(), 1, 9, 12]]),
    );
    answers.insert(
        format!("{}:1:9", names[links]),
        serde_json::json!([[names[links].clone(), 1, 9, 12]]),
    );
    let borrowed = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let (row, _) = definition_chain(
        &borrowed,
        serde_json::json!({"early": {}, "answers": answers}),
        ("use.ts", 1, 0),
    );
    assert!(lists_path(&row, &names[links]), "{row}");
    assert!(!lists_path(&row, "f3.ts"), "{row}");

    // a.ts and b.ts answer each other forever: the chain still ends.
    let (row, asked) = definition_chain(
        &[
            ("a.ts", "export { foo } from './b';\nfoo();\n"),
            ("b.ts", "export { foo } from './a';\n"),
        ],
        serde_json::json!({"early": {}, "answers": {
            "a.ts:1:0": [["a.ts", 0, 9, 12]],
            "a.ts:0:9": [["b.ts", 0, 9, 12]],
            "b.ts:0:9": [["a.ts", 0, 9, 12]]
        }}),
        ("a.ts", 1, 0),
    );
    assert!(
        lists_path(&row, "a.ts") || lists_path(&row, "b.ts"),
        "{row}"
    );
    assert!(
        asked.len() < 10,
        "each location is followed once: {asked:?}"
    );
}
