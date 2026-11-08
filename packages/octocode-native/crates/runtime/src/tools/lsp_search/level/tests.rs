use super::{install_hint, stamp};
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
