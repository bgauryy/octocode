//! Type-hierarchy and implementation answers when the server cannot give them:
//! the labelled heritage-clause scan.

use super::*;

/// A server with no `typeHierarchyProvider` still answers supertypes and
/// subtypes: subtypes are its implementation answer (never the queried
/// declaration itself), supertypes are the definitions of the heritage
/// clause, and each row names its source.
#[test]
fn type_hierarchy_falls_back_when_the_server_has_no_type_hierarchy() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(root.join("b.ts"), "export interface Base {}\n").expect("base");
    std::fs::write(root.join("a.ts"), "export class Child extends Base {}\n").expect("child");
    let server = FAKE_SERVER
        .replace(
            "hoverProvider: true",
            "implementationProvider: true, definitionProvider: true",
        )
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/implementation') {
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    const at = (name, character, length) => ({ uri: dir + name, range: { start: { line: 0, character }, end: { line: 0, character: character + length } } });
    send({ jsonrpc: '2.0', id: msg.id, result: [at('a.ts', 13, 5), at('b.ts', 17, 4)] });
  } else if (msg.method === 'textDocument/definition') {
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    send({ jsonrpc: '2.0', id: msg.id, result: [{ uri: dir + 'b.ts', range: { start: { line: 0, character: 17 }, end: { line: 0, character: 21 } } }] });"#,
        );
    crate::tools::lsp_search::tests::write_executable(&script, &server);
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        for (operation, source, file, symbol, character, expected) in [
            ("subtypes", "implementation", "b.ts", "Base", 17, "a.ts"),
            ("supertypes", "heritage", "a.ts", "Child", 13, "b.ts"),
        ] {
            let path = root.join(file).to_string_lossy().into_owned();
            let query = serde_json::from_value(json!({
                "operation":operation, "path":path, "symbolName":symbol, "line":1
            }))
            .expect("query");
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
                character,
                language_id: Some("typescript"),
                budget: 16_000,
                security: &crate::security::ContentSecurity::new(),
            }
            .run()
            .await
            .unwrap_or_else(|error| panic!("{operation} fails without a hierarchy: {error:?}"));
            assert_eq!(row["lsp"]["source"], source, "{row}");
            let paths = location_rows(&row)
                .iter()
                .map(|found| found["path"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>();
            assert!(
                paths.iter().all(|found| found.ends_with(expected)),
                "{operation} lists only other declarations: {row}"
            );
            assert_eq!(paths.len(), 1, "{row}");
            assert!(row.get("isPartial").is_none(), "{row}");
        }
        client.stop().await.expect("fixture server closed");
    });
}

/// A server that prepares the type but lists no subtypes while its
/// implementation answer names one (jdtls on guava's `Function`): the empty
/// hierarchy is not absence; the implementation path answers, labelled.
#[test]
fn empty_server_subtypes_fall_back_to_the_implementation_answer() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(root.join("b.ts"), "export interface Base {}\n").expect("base");
    std::fs::write(root.join("a.ts"), "export class Child implements Base {}\n").expect("child");
    let server = FAKE_SERVER
        .replace(
            "hoverProvider: true",
            "implementationProvider: true, definitionProvider: true, typeHierarchyProvider: true",
        )
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/prepareTypeHierarchy') {
    const uri = msg.params.textDocument.uri;
    const range = { start: { line: 0, character: 17 }, end: { line: 0, character: 21 } };
    send({ jsonrpc: '2.0', id: msg.id, result: [{ name: 'Base', kind: 11, uri, range, selectionRange: range, data: { element: '=project/src<pkg{b.ts[Base' } }] });
  } else if (msg.method === 'typeHierarchy/subtypes') {
    send({ jsonrpc: '2.0', id: msg.id, result: [] });
  } else if (msg.method === 'textDocument/implementation') {
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    send({ jsonrpc: '2.0', id: msg.id, result: [{ uri: dir + 'a.ts', range: { start: { line: 0, character: 13 }, end: { line: 0, character: 18 } } }] });
  } else if (msg.method === 'textDocument/definition') {
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    send({ jsonrpc: '2.0', id: msg.id, result: [{ uri: dir + 'b.ts', range: { start: { line: 0, character: 17 }, end: { line: 0, character: 21 } } }] });"#,
        );
    crate::tools::lsp_search::tests::write_executable(&script, &server);
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("b.ts").to_string_lossy().into_owned();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        let query = serde_json::from_value(json!({
            "operation":"subtypes", "path":path, "symbolName":"Base", "line":1
        }))
        .expect("query");
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
            character: 17,
            language_id: Some("typescript"),
            budget: 16_000,
            security: &crate::security::ContentSecurity::new(),
        }
        .run()
        .await
        .unwrap_or_else(|error| panic!("empty subtypes fall back: {error:?}"));
        assert_eq!(row["lsp"]["source"], "implementation", "{row}");
        let found = location_rows(&row)
            .iter()
            .map(|found| found["path"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "{row}");
        assert!(found[0].ends_with("a.ts"), "{row}");
        assert!(
            row["warnings"].to_string().contains("listed no subtypes"),
            "{row}"
        );
        client.stop().await.expect("fixture server closed");
    });
}

/// Rust `subtypes` on a trait whose file expands a `macro_rules!` inside an
/// impl (tokio's `AsyncRead`): the macro leaves only a linking caveat, so
/// the trait is the one anchor its implementation candidates belong to,
/// never "an ambiguous type anchor".
#[test]
fn rust_subtypes_anchor_a_trait_in_a_file_with_an_unexpanded_macro() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(
        root.join("a.rs"),
        "pub trait AsyncRead {}\nmacro_rules! deref_async_read { () => { fn f() {} }; }\nimpl<T: AsyncRead> AsyncRead for Box<T> { deref_async_read!(); }\n",
    )
    .expect("trait");
    std::fs::write(
        root.join("b.rs"),
        "pub struct Foo;\nimpl AsyncRead for Foo {}\n",
    )
    .expect("impl");
    let server = FAKE_SERVER
        .replace(
            "hoverProvider: true",
            "implementationProvider: true, definitionProvider: true",
        )
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/implementation') {
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    const at = (name, line, character, length) => ({ uri: dir + name, range: { start: { line, character }, end: { line, character: character + length } } });
    send({ jsonrpc: '2.0', id: msg.id, result: [at('a.rs', 2, 33, 3), at('b.rs', 1, 19, 3)] });
  } else if (msg.method === 'textDocument/definition') {
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    send({ jsonrpc: '2.0', id: msg.id, result: [{ uri: dir + 'a.rs', range: { start: { line: 0, character: 10 }, end: { line: 0, character: 19 } } }] });"#,
        );
    crate::tools::lsp_search::tests::write_executable(&script, &server);
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("a.rs").to_string_lossy().into_owned();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        let query = serde_json::from_value(json!({
            "operation":"subtypes", "path":path, "symbolName":"AsyncRead", "line":1
        }))
        .expect("query");
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
            character: 10,
            language_id: Some("rust"),
            budget: 16_000,
            security: &crate::security::ContentSecurity::new(),
        }
        .run()
        .await
        .unwrap_or_else(|error| panic!("the trait anchors its subtypes: {error:?}"));
        assert_eq!(row["lsp"]["source"], "implementation", "{row}");
        let mut found = location_rows(&row)
            .iter()
            .map(|found| {
                Path::new(found["path"].as_str().unwrap_or_default())
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        found.sort();
        assert_eq!(found, ["a.rs", "b.rs"], "{row}");
        client.stop().await.expect("fixture server closed");
    });
}

/// A Rust server whose `implementation` answer lists `listed` (JS location
/// literals built with `at(name, line, character, length)`) and whose
/// `definition` names the declaration at `a.rs` `declared` (line, start,
/// end): rust-analyzer without a type hierarchy.
fn rust_implementation_server(listed: &str, declared: (u32, u32, u32)) -> String {
    let (line, start, end) = declared;
    FAKE_SERVER
        .replace(
            "hoverProvider: true",
            "implementationProvider: true, definitionProvider: true",
        )
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            &format!(
                r"  }} else if (msg.method === 'textDocument/implementation') {{
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    const at = (name, line, character, length) => ({{ uri: dir + name, range: {{ start: {{ line, character }}, end: {{ line, character: character + length }} }} }});
    send({{ jsonrpc: '2.0', id: msg.id, result: [{listed}] }});
  }} else if (msg.method === 'textDocument/definition') {{
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    send({{ jsonrpc: '2.0', id: msg.id, result: [{{ uri: dir + 'a.rs', range: {{ start: {{ line: {line}, character: {start} }}, end: {{ line: {line}, character: {end} }} }} }}] }});"
            ),
        )
}

/// One Rust operation at `a.rs` (zero-based `line`, `character`) against
/// `server`, over the files of `root`.
fn rust_row(
    root: &Path,
    script: &Path,
    server: &str,
    (operation, symbol, line, character): (&str, &str, u32, u32),
) -> serde_json::Value {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    crate::tools::lsp_search::tests::write_executable(script, server);
    let paths = crate::tools::test_support::workspace_policy(root);
    let path = root.join("a.rs").to_string_lossy().into_owned();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(root, script, None));
        client.start().await.expect("server starts");
        let query = serde_json::from_value(serde_json::json!({
            "operation": operation, "path": path, "symbolName": symbol, "line": line + 1
        }))
        .expect("query");
        let mut sources = SourceCache::new(&paths);
        let policy = snippet_policy(&paths);
        let answer = Operation {
            client: &client,
            query: &query,
            sources: &mut sources,
            snippet_policy: &policy,
            cancel: &NeverCancel,
            path: &path,
            scope: &crate::tools::lsp_search::scope::Scope::new(
                root.to_string_lossy().into_owned(),
                vec!["*.rs".into()],
            ),
            root_only: false,
            line,
            character,
            language_id: Some("rust"),
            budget: 16_000,
            security: &crate::security::ContentSecurity::new(),
        }
        .run()
        .await;
        assert!(
            answer.is_ok(),
            "{operation} answers: {:?}",
            answer.as_ref().err()
        );
        let row = answer.expect("checked above");
        client.stop().await.expect("fixture server closed");
        row
    })
}

/// `(file name, one-based line, source label)` of each location row.
fn located(row: &serde_json::Value) -> Vec<(String, u64, Option<String>)> {
    let mut found = location_rows(row)
        .iter()
        .map(|found| {
            (
                Path::new(found["path"].as_str().unwrap_or_default())
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                found["line"].as_u64().unwrap_or(0),
                found["source"].as_str().map(str::to_owned),
            )
        })
        .collect::<Vec<_>>();
    found.sort();
    found
}

/// Rust `subtypes` on a trait: the server's implementation answer leaves
/// out an `impl Trait for X` declared inside a function body. The heritage
/// scan's declaration is a candidate like the server's own, and the walk
/// checks it against the trait its clause names: both impls are subtypes,
/// and the row is complete.
#[test]
fn rust_subtypes_list_an_impl_declared_inside_a_function_body() {
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(root.join("a.rs"), "pub trait AsyncRead {}\n").expect("trait");
    std::fs::write(
        root.join("b.rs"),
        "pub struct Foo;\nimpl AsyncRead for Foo {}\nfn local() {\n    struct Bar;\n    impl AsyncRead for Bar {}\n}\n",
    )
    .expect("impls");
    octocode_engine::stamp::settle_written_so_far();
    let server = rust_implementation_server("at('b.rs', 1, 19, 3)", (0, 10, 19));
    let row = rust_row(&root, &script, &server, ("subtypes", "AsyncRead", 0, 10));
    assert_eq!(row["lsp"]["source"], "implementation", "{row}");
    let lines = located(&row)
        .into_iter()
        .map(|(file, line, _)| (file, line))
        .collect::<Vec<_>>();
    assert_eq!(
        lines,
        [("b.rs".to_owned(), 2), ("b.rs".to_owned(), 5)],
        "{row}"
    );
    assert!(row.get("isPartial").is_none(), "{row}");
}

/// Rust `supertypes` on a struct: the struct names no trait itself, its
/// `impl Trait for Type` blocks do. The server's implementation answer at
/// the struct lists its impl blocks; the trait implementation among them
/// names the supertype, and the inherent one names none.
#[test]
fn rust_supertypes_of_a_struct_are_the_traits_its_impls_name() {
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(
        root.join("a.rs"),
        "pub trait Shape {}\npub struct Square;\nimpl Shape for Square {}\nimpl Square {}\n",
    )
    .expect("source");
    octocode_engine::stamp::settle_written_so_far();
    let server =
        rust_implementation_server("at('a.rs', 2, 15, 6), at('a.rs', 3, 5, 6)", (0, 10, 15));
    let row = rust_row(&root, &script, &server, ("supertypes", "Square", 1, 11));
    assert_eq!(row["lsp"]["source"], "heritage", "{row}");
    let lines = located(&row)
        .into_iter()
        .map(|(file, line, _)| (file, line))
        .collect::<Vec<_>>();
    assert_eq!(lines, [("a.rs".to_owned(), 1)], "{row}");
    assert!(row.get("isPartial").is_none(), "{row}");
}

/// `implementation` on a trait method: the server lists the method of the
/// top-level impl and leaves out the one of an impl declared inside a
/// function body. The scan reads the methods of that name in the trait's
/// impl blocks: the one the server left out follows its row, labelled, and
/// the row says scan rows stand beside the answer.
#[test]
fn implementation_of_a_trait_method_lists_the_impl_inside_a_function_body() {
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(
        root.join("a.rs"),
        "pub trait Check {\n    fn check(&self) -> bool;\n}\n",
    )
    .expect("trait");
    std::fs::write(
        root.join("b.rs"),
        "struct A;\nimpl Check for A {\n    fn check(&self) -> bool { true }\n}\nfn local() {\n    struct B;\n    impl Check for B {\n        fn check(&self) -> bool { false }\n    }\n}\nstruct C;\nimpl C {\n    fn check(&self) {}\n}\n",
    )
    .expect("impls");
    octocode_engine::stamp::settle_written_so_far();
    let server = rust_implementation_server("at('b.rs', 2, 7, 5)", (1, 7, 12));
    let row = rust_row(&root, &script, &server, ("implementation", "check", 1, 7));
    assert_eq!(
        located(&row),
        [
            ("b.rs".to_owned(), 3, None),
            (
                "b.rs".to_owned(),
                8,
                Some(crate::tools::lsp_search::ops::SCAN_SOURCE.to_owned())
            ),
        ],
        "{row}"
    );
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["scanOnlyImplementations"]),
        "{row}"
    );
}

mod scan;
