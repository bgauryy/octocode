//! Server receipts: none on a row the server answered; on a row where the
//! server's state decides the next step, the facts that step needs.

use super::*;
use octocode_engine::lsp::types::{LspLanguageServerConfig, ServerSource};
use serde_json::json;

fn server(args: &[&str], source: ServerSource) -> LspLanguageServerConfig {
    LspLanguageServerConfig {
        command: "rust-analyzer".into(),
        args: Some(args.iter().map(|arg| (*arg).to_owned()).collect()),
        workspace_root: "/repo".into(),
        language_id: Some("rust".into()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source,
    }
}

/// An answered operation was supported and its server is named by
/// `provider`: the row carries no receipt. Every row, anchored or not,
/// names the root the server searched.
#[test]
fn success_rows_carry_no_receipt_and_name_their_workspace_root() {
    let config = server(&["--stdio"], ServerSource::Path);
    for (row, provider) in [
        (
            json!({"operation":"callers","path":"/repo/src/a.rs","symbolName":"run","line":3}),
            "callHierarchyProvider",
        ),
        (
            json!({"operation":"documentSymbols","path":"/repo/src/a.rs"}),
            "documentSymbolProvider",
        ),
        (
            json!({"operation":"workspaceSymbol","workspaceRoot":"/repo","symbolQuery":"run"}),
            "workspaceSymbolProvider",
        ),
    ] {
        let query = query(row);
        let mut value = json!({"kind": lsp_search_query::operation(&query)});
        attach_provider_context(&mut value, &query, "file:///repo/src/a.rs", None, &config);
        assert!(!value.to_string().contains("receipt"), "{value}");
        assert_eq!(value["lsp"]["serverAvailable"], true, "{value}");
        assert_eq!(value["lsp"]["provider"], provider, "{value}");
        assert_eq!(value["workspaceRoot"], "/repo", "{value}");
    }
}

/// A failure receipt names what the user configures: the command, its
/// argv when it has one, and where discovery found the server. The source
/// is discovery's, not a guess from the argv.
#[test]
fn a_server_receipt_names_the_invocation_and_omits_empty_argv() {
    assert_eq!(
        server_receipt(&server(&["--stdio"], ServerSource::Path), true),
        json!({"command":"rust-analyzer","argv":["--stdio"],"source":"path"})
    );
    assert_eq!(
        server_receipt(&server(&["--stdio"], ServerSource::Path), false),
        json!({"command":"rust-analyzer","source":"path"})
    );
    assert_eq!(
        server_receipt(&server(&[], ServerSource::Path), true),
        json!({"command":"rust-analyzer","source":"path"})
    );
    for (source, argv, expected) in [
        (
            ServerSource::User,
            "/x/node_modules/typescript-language-server/lib/cli.mjs",
            "user",
        ),
        (ServerSource::Bundled, "--lsp", "bundled"),
    ] {
        assert_eq!(
            server_receipt(&server(&[argv], source), true)["source"],
            expected
        );
    }
}

/// The operations a capability failure lists are exactly the ones
/// admission lets through: a fallback provider admits its operation, and
/// `diagnostic` needs no capability.
#[test]
fn advertised_operations_follow_capability_admission() {
    let only = |names: &'static [&'static str]| move |capability: &str| names.contains(&capability);
    assert_eq!(
        advertised_operations(&only(&["hoverProvider"])),
        ["hover", "diagnostic"]
    );
    assert_eq!(
        advertised_operations(&only(&["definitionProvider"])),
        ["definition", "supertypes", "diagnostic"]
    );
    assert_eq!(advertised_operations(&only(&[])), ["diagnostic"]);
    let all = only(&[
        "definitionProvider",
        "referencesProvider",
        "hoverProvider",
        "typeDefinitionProvider",
        "implementationProvider",
        "documentSymbolProvider",
        "workspaceSymbolProvider",
        "callHierarchyProvider",
        "typeHierarchyProvider",
    ]);
    assert_eq!(
        advertised_operations(&all),
        Op::ALL.map(Op::name).to_vec(),
        "every operation, each once, in declaration order"
    );
    for op in Op::ALL {
        let hover = only(&["hoverProvider"]);
        assert_eq!(
            admits(op.name(), &hover),
            advertised_operations(&hover).contains(&op.name()),
            "{}",
            op.name()
        );
    }
}

/// A server stopped for its memory cannot answer again: its row leads to a
/// text search and names the server that was killed. A crash a retry
/// recovers from stays a request-level failure.
#[test]
fn a_crash_the_server_cannot_recover_from_names_the_server_on_its_row() {
    let config = server(&["--stdio"], ServerSource::User);
    let query =
        query(json!({"operation":"callers","path":"/repo/src/a.rs","symbolName":"run","line":3}));
    let killed = LspFailure {
        code: ErrorCode::ServerCrashed,
        message: "killed for memory".into(),
        retryable: false,
        lexical_fallback: true,
    };
    let Exit::Row(row) = crashed(&query, "file:///repo/src/a.rs", &config, killed) else {
        panic!("a memory kill is a row");
    };
    assert_eq!(row["errorCode"], "serverCrashed", "{row}");
    assert_eq!(row["path"], "/repo/src/a.rs", "{row}");
    assert_eq!(
        row["lsp"],
        json!({"serverAvailable": false,
            "receipt": {"command":"rust-analyzer","argv":["--stdio"],"source":"user"}})
    );
    let closed = LspFailure {
        code: ErrorCode::ServerCrashed,
        message: "connection closed".into(),
        retryable: true,
        lexical_fallback: false,
    };
    assert!(matches!(
        crashed(&query, "file:///repo/src/a.rs", &config, closed.clone()),
        Exit::Failed(failure) if failure == closed
    ));
}
