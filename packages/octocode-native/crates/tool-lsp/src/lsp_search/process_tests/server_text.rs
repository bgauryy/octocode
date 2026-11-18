//! What a language server says is untrusted text: hover, symbol names and
//! details, its error messages, its logs and its stderr. Whatever reaches a row
//! passes the output gate; logs and stderr reach no row at all.

use super::*;
use crate::lsp_search::ops::Operation;
use crate::lsp_search::source::{SourceCache, snippet_policy};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::security::Sanitized;
use serde_json::{Value, json};

const WG: &str = "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=";
const SCALAR: &str = "Fake0Scalar9Not4Real8Value7Long6Enough5Xy";
/// A secret only its well-known key name makes one.
const AWS: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
const BODY: &str = "MIIEpAIBAAKCAQEAfakebodylinefakebodylinefakebodyline00000";

/// One text each secret form reads as in a hover, and the piece it must hide.
fn texts() -> Vec<(String, &'static str)> {
    vec![
        (format!("PrivateKey = {WG}\nVISIBLE"), WG),
        (
            format!("{{\"region\":\"eu\",\"aws_secret_access_key\":\"{AWS}\"}}\nVISIBLE"),
            AWS,
        ),
        (
            format!("-----BEGIN RSA PRIVATE KEY-----\n{BODY}\nVISIBLE"),
            BODY,
        ),
        // A hover that cuts its text off: the line is no longer only base64.
        (
            format!("const k = \"-----BEGIN RSA PRIVATE KEY-----\n{BODY}…\"\nVISIBLE"),
            BODY,
        ),
        (
            format!("pem=-----BEGIN%20RSA%20PRIVATE%20KEY-----%0A{BODY}%0A\nVISIBLE"),
            BODY,
        ),
        (
            format!("untrusted comment: minisign encrypted secret key\n{SCALAR}{SCALAR}\nVISIBLE"),
            SCALAR,
        ),
    ]
}

fn server(text: &str) -> String {
    let text = serde_json::to_string(text).expect("text");
    let range = "{start:{line:0,character:6},end:{line:0,character:7}}";
    FAKE_SERVER
        .replace(
            "capabilities: { hoverProvider: true, textDocumentSync: 1 }",
            "capabilities: { hoverProvider: true, documentSymbolProvider: true, workspaceSymbolProvider: true, textDocumentSync: 1 }",
        )
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            &format!(
                r#"  }} else if (msg.method === 'initialized') {{
    process.stderr.write(TEXT + '\n');
    send({{jsonrpc:'2.0',method:'window/logMessage',params:{{type:1,message:TEXT}}}});
    send({{jsonrpc:'2.0',method:'window/showMessage',params:{{type:1,message:TEXT}}}});
  }} else if (msg.method === 'textDocument/hover') {{
    send({{jsonrpc:'2.0',id:msg.id,result:{{contents:{{kind:'markdown',value:TEXT}},range:{range}}}}});
  }} else if (msg.method === 'textDocument/documentSymbol') {{
    send({{jsonrpc:'2.0',id:msg.id,result:[{{name:TEXT,detail:TEXT,kind:13,range:{range},selectionRange:{range},children:[{{name:'a',detail:TEXT,kind:13,range:{range},selectionRange:{range}}}]}}]}});
  }} else if (msg.method === 'workspace/symbol') {{
    send({{jsonrpc:'2.0',id:msg.id,result:[{{name:TEXT,containerName:TEXT,kind:13,location:{{uri:URI,range:{range}}}}}]}});"#
            ),
        )
        .replacen(
            "let buf = Buffer.alloc(0);",
            &format!("const TEXT = {text};\nconst URI = 'file://' + process.cwd() + '/a.ts';\nlet buf = Buffer.alloc(0);"),
            1,
        )
}

/// The row `row` asks for, as the output gate leaves it.
fn sealed(root: &Path, script: &Path, row: Value) -> Value {
    let paths = octocode_runtime_base::test_support::workspace_policy(root);
    let path = root.join("a.ts").to_string_lossy().into_owned();
    let query: LspSearchQuery = serde_json::from_value(row).expect("query");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(root, script, None));
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
            scope: &crate::lsp_search::scope::Scope::new(
                root.to_string_lossy().into_owned(),
                vec!["*.ts".into()],
            ),
            root_only: false,
            line: 0,
            character: 6,
            language_id: Some("typescript"),
            budget: 16_000,
            security: &octocode_runtime_base::security::ContentSecurity::new(),
            anchor_opened: false,
        }
        .run()
        .await
        .expect("provider answer");
        let _ = client.stop().await;
        Sanitized::sealed(row).into_value()
    })
}

#[test]
fn no_operation_shows_a_secret_the_server_returns() {
    if !require_node() {
        return;
    }
    for (text, secret) in texts() {
        let (_dir, root, script) = fixture();
        crate::lsp_search::tests::write_executable(&script, server(&text));
        let path = root.join("a.ts").to_string_lossy().into_owned();
        let hover = sealed(
            &root,
            &script,
            json!({"operation":"hover","path":path,"symbolName":"a","line":1}),
        );
        assert!(hover.to_string().contains("VISIBLE"), "{hover}");
        for (name, row) in [
            ("hover", hover),
            (
                "documentSymbols",
                sealed(
                    &root,
                    &script,
                    json!({"operation":"documentSymbols","path":path}),
                ),
            ),
            (
                "workspaceSymbol",
                sealed(
                    &root,
                    &script,
                    json!({"operation":"workspaceSymbol","path":path,"symbolFuzzy":"a"}),
                ),
            ),
        ] {
            let shown = row.to_string();
            assert!(!shown.contains(secret), "{name} {secret}: {shown}");
        }
    }
}

/// A server that logs a secret and fails: its message passes the gate, and
/// nothing it wrote to stderr or its log channel is in the row.
#[test]
fn a_failing_server_shows_neither_its_stderr_nor_a_secret_in_its_error() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let token = format!("ghp_{}", "a1B2c3D4".repeat(5));
    let server = FAKE_SERVER
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            &format!(
                r#"  }} else if (msg.method === 'textDocument/hover') {{
    process.stderr.write('STDERR-ONLY {token}\n');
    send({{jsonrpc:'2.0',method:'window/logMessage',params:{{type:1,message:'LOG-ONLY {token}'}}}});
    send({{jsonrpc:'2.0',id:msg.id,error:{{code:-32603,message:'failed {token}',data:'{token}'}}}});"#
            ),
        );
    crate::lsp_search::tests::write_executable(&script, &server);
    let path = root.join("a.ts").to_string_lossy().into_owned();
    let query: LspSearchQuery =
        serde_json::from_value(json!({"operation":"hover","path":path,"symbolName":"a","line":1}))
            .expect("query");
    let paths = octocode_runtime_base::test_support::workspace_policy(&root);
    let error = tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        let mut sources = SourceCache::new(&paths);
        let policy = snippet_policy(&paths);
        let outcome = Operation {
            client: &client,
            query: &query,
            sources: &mut sources,
            snippet_policy: &policy,
            cancel: &NeverCancel,
            path: &path,
            scope: &crate::lsp_search::scope::Scope::new(
                root.to_string_lossy().into_owned(),
                Vec::new(),
            ),
            root_only: false,
            line: 0,
            character: 6,
            language_id: Some("typescript"),
            budget: 16_000,
            security: &octocode_runtime_base::security::ContentSecurity::new(),
            anchor_opened: false,
        }
        .run()
        .await;
        let _ = client.stop().await;
        outcome.expect_err("the server failed the request")
    });
    let row = Sanitized::sealed(json!({"error": format!("{error:?}")})).into_value();
    let shown = row.to_string();
    assert!(!shown.contains(&token), "{shown}");
    assert!(!shown.contains("STDERR-ONLY"), "{shown}");
    assert!(!shown.contains("LOG-ONLY"), "{shown}");
}

/// A location row quotes the file, not the server: a server pointing into a key
/// block or at a private member gets a row, with those lines hidden and the
/// lines around them shown, by default and with `contextLines`.
#[test]
fn a_location_row_hides_the_source_lines_only_the_file_can_judge() {
    if !require_node() {
        return;
    }
    let source = format!(
        "const before = 1;\n-----BEGIN RSA PRIVATE KEY-----\n{BODY}\n{BODY}\n-----END RSA PRIVATE KEY-----\nconst after = 2;\nconst jwk = {{\n  kty: 'EC',\n  x: 'plainPublicMember',\n  aws_secret_access_key: '{AWS}',\n}};\n"
    );
    for (line, secret) in [(2, BODY), (9, AWS)] {
        let (_dir, root, script) = fixture();
        std::fs::write(root.join("a.ts"), &source).expect("source");
        let server = FAKE_SERVER
            .replace(
                "capabilities: { hoverProvider: true, textDocumentSync: 1 }",
                "capabilities: { definitionProvider: true, referencesProvider: true, textDocumentSync: 1 }",
            )
            .replace(
                "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
                &format!(
                    r#"  }} else if (msg.method === 'textDocument/definition' || msg.method === 'textDocument/references') {{
    send({{jsonrpc:'2.0',id:msg.id,result:[{{uri:msg.params.textDocument.uri,range:{{start:{{line:{line},character:0}},end:{{line:{line},character:3}}}}}}]}});"#
                ),
            );
        crate::lsp_search::tests::write_executable(&script, &server);
        let path = root.join("a.ts").to_string_lossy().into_owned();
        for row in [
            json!({"operation":"references","path":path,"symbolName":"before","line":1}),
            json!({"operation":"definition","path":path,"symbolName":"before","line":1}),
            json!({"operation":"definition","path":path,"symbolName":"before","line":1,"contextLines":{"before":10,"after":10}}),
        ] {
            let context = row.get("contextLines").is_some();
            let out = sealed(&root, &script, row);
            let shown = out.to_string();
            assert!(!shown.contains(secret), "{secret}: {shown}");
            assert!(shown.contains("REDACTED"), "{shown}");
            if context {
                assert!(shown.contains("const before = 1;"), "{shown}");
                assert!(shown.contains("const after = 2;"), "{shown}");
                assert!(shown.contains("plainPublicMember"), "{shown}");
            }
        }
    }
}
