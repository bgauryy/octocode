use super::{install_hint, omit_healthy_receipt, stamp};
use serde_json::{Value, json};

fn stamped(mut row: Value) -> Value {
    stamp(&mut row);
    row
}

/// A row a server answered, or one whose server was available but failed,
/// is a server row; a row no engine answered is a text row; a level a rung
/// set stays; a row with no receipt answered nothing and gets none.
#[test]
fn every_receipt_names_the_rung_that_answered() {
    let server = stamped(json!({"matches": [], "lsp": {"serverAvailable": true}}));
    assert_eq!(server["lsp"]["level"], "server", "{server}");
    let text = stamped(json!({
        "status": "error", "errorCode": "serverUnavailable",
        "error": "No language server configured for .rb files.",
        "lsp": {"serverAvailable": false}
    }));
    assert_eq!(text["lsp"]["level"], "text", "{text}");
    assert!(
        text.get("warnings").is_none(),
        "no command, no hint: {text}"
    );
    let syntax = stamped(json!({"lsp": {"serverAvailable": false, "level": "syntax"}}));
    assert_eq!(syntax["lsp"]["level"], "syntax");
    let restart = stamped(json!({"status": "error", "errorCode": "staleSnapshot"}));
    assert!(restart.get("lsp").is_none(), "{restart}");
}

/// A text row whose server did not start names how to install it, once:
/// not again when the start error already says it.
#[test]
fn a_text_row_names_the_install_hint_of_its_server() {
    let row = stamped(json!({
        "status": "error", "errorCode": "serverUnavailable",
        "error": "Language server failed to start.",
        "lsp": {"serverAvailable": false, "receipt": {"command": "/opt/bin/jdtls"}}
    }));
    let hint = octocode_engine::lsp::config::install_hint("jdtls").expect("a jdtls hint");
    let warnings = row["warnings"].to_string();
    assert!(warnings.contains(&hint), "{row}");
    let receipt = json!({"command": "jdtls"});
    assert_eq!(
        install_hint(Some(&receipt), &format!("not found. {hint}")),
        None
    );
    assert_eq!(
        install_hint(Some(&json!({"command": "solargraph"})), ""),
        None
    );
}

/// A receipt that only restates a healthy server answer (`server` level,
/// the operation's own provider, no errors) decides no next step: it is
/// omitted. Every deviation keeps the whole receipt.
#[test]
fn a_healthy_server_receipt_is_omitted_and_a_deviation_is_kept() {
    let clean = json!({
        "serverAvailable": true, "level": "server", "provider": "referencesProvider",
        "analysisContext": {
            "diagnostics": "noErrorsReported", "diagnosticsComplete": true,
            "errorCount": 0, "anchorErrorCount": 0
        }
    });
    let mut healthy = json!({"matches": [], "lsp": clean});
    omit_healthy_receipt(&mut healthy);
    assert!(healthy.get("lsp").is_none(), "{healthy}");

    let mut hover =
        json!({"lsp": {"serverAvailable": true, "level": "server", "provider": "hoverProvider"}});
    omit_healthy_receipt(&mut hover);
    assert!(hover.get("lsp").is_none(), "{hover}");

    let deviations = [
        (
            "errors",
            json!({"analysisContext": {"diagnostics": "errors", "diagnosticsComplete": true, "errorCount": 2, "anchorErrorCount": 0}}),
        ),
        (
            "truncated",
            json!({"analysisContext": {"diagnostics": "noErrorsReported", "diagnosticsComplete": false, "errorCount": 0, "anchorErrorCount": 0}}),
        ),
        (
            "unavailable",
            json!({"analysisContext": {"diagnostics": "unavailable"}}),
        ),
        ("source", json!({"source": "native-graph-facts"})),
        ("level", json!({"level": "syntax"})),
        ("server", json!({"serverAvailable": false})),
    ];
    for (name, change) in deviations {
        let mut lsp = clean.clone();
        for (key, value) in change.as_object().unwrap() {
            lsp[key] = value.clone();
        }
        let mut row = json!({"lsp": lsp.clone()});
        omit_healthy_receipt(&mut row);
        assert_eq!(row["lsp"], lsp, "{name} keeps its receipt");
    }
}
