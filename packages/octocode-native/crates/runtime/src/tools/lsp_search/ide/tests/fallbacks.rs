//! Where the rung does not apply (silently) and where it fails (with why).
use super::{Bridge, Editor, location, result};
use serde_json::{Value, json};

/// The first page of an outline of the editor's file, answered by the
/// server rung (which does not exist for `.zzz` files).
async fn outline(editor: &Editor) -> Value {
    editor.without_server();
    editor.ask(json!({"operation": "documentSymbols"})).await
}

fn assert_fell_back(row: &Value, reason: &str) {
    assert_eq!(row["lsp"]["fellBackFrom"]["level"], "ide", "{row}");
    let said = row["lsp"]["fellBackFrom"]["reason"]
        .as_str()
        .unwrap_or_default();
    assert!(said.contains(reason), "{said} lacks {reason}");
    assert_ne!(row["lsp"]["level"], "ide", "{row}");
    assert!(!said.contains(super::TOKEN), "the token leaked: {said}");
}

fn silent(row: &Value, bridge: &Bridge) {
    assert!(row["lsp"]["fellBackFrom"].is_null(), "{row}");
    assert!(bridge.methods().is_empty(), "{:?}", bridge.methods());
}

#[tokio::test(flavor = "current_thread")]
async fn a_rejected_token_falls_back_with_the_reason() {
    let editor = Editor::with_file("notes.zzz");
    let bridge = Bridge::serve(Box::new(|_| {
        (401, json!({"error": "unauthorized", "reason": "bad token"}))
    }));
    editor.open(&bridge);
    assert_fell_back(&outline(&editor).await, "401");
}

#[tokio::test(flavor = "current_thread")]
async fn an_unsupported_language_falls_back() {
    let editor = Editor::with_file("notes.zzz");
    let bridge = Bridge::serve(Box::new(|_| (200, json!({"unsupported": true}))));
    editor.open(&bridge);
    assert_fell_back(&outline(&editor).await, "no provider");
}

#[tokio::test(flavor = "current_thread")]
async fn a_failed_provider_falls_back() {
    let editor = Editor::with_file("notes.zzz");
    let bridge = Bridge::serve(Box::new(|_| {
        (500, json!({"error": "provider failed", "reason": "boom"}))
    }));
    editor.open(&bridge);
    assert_fell_back(&outline(&editor).await, "500");
}

#[tokio::test(flavor = "current_thread")]
async fn a_refused_connection_falls_back() {
    let editor = Editor::with_file("notes.zzz");
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.local_addr().expect("addr").port()
    };
    editor.lock(std::process::id(), port, &[&editor.root], 0o600);
    assert_fell_back(&outline(&editor).await, "could not be reached");
}

/// A lock a group or others can read is refused, and its token never sent.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_group_readable_lock_is_refused() {
    let editor = Editor::with_file("notes.zzz");
    let bridge = Bridge::serve(Box::new(|_| result(json!([]))));
    editor.lock(std::process::id(), bridge.port, &[&editor.root], 0o640);
    let row = outline(&editor).await;
    assert_fell_back(&row, "group or others");
    assert!(bridge.methods().is_empty(), "{:?}", bridge.methods());
}

/// A lock whose process is gone is skipped, with no trace on the row.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_dead_pid_is_skipped_silently() {
    let editor = Editor::with_file("notes.zzz");
    let bridge = Bridge::serve(Box::new(|_| result(json!([]))));
    let mut child = std::process::Command::new("true").spawn().expect("spawn");
    child.wait().expect("wait");
    editor.lock(child.id(), bridge.port, &[&editor.root], 0o600);
    silent(&outline(&editor).await, &bridge);
}

/// A file outside every workspace folder of the editor is not its to answer.
#[tokio::test(flavor = "current_thread")]
async fn a_path_outside_the_workspace_folders_is_skipped() {
    let editor = Editor::with_file("notes.zzz");
    let elsewhere = crate::tools::test_support::Fixture::new();
    let elsewhere_root = elsewhere.0.canonicalize().expect("elsewhere");
    let bridge = Bridge::serve(Box::new(|_| result(json!([]))));
    editor.lock(std::process::id(), bridge.port, &[&elsewhere_root], 0o600);
    silent(&outline(&editor).await, &bridge);
}

/// A location outside the allowed roots is dropped as a server's would be.
#[tokio::test(flavor = "current_thread")]
async fn a_location_outside_the_allowed_roots_is_filtered() {
    let editor = Editor::new();
    let outside = crate::tools::test_support::Fixture::new();
    let secret = outside.0.canonicalize().expect("outside").join("secret.ts");
    std::fs::write(&secret, "export const secret = 1;\n").expect("secret");
    let secret_uri =
        octocode_engine::lsp::uri::path_to_uri(&secret.to_string_lossy()).expect("uri");
    let uri = editor.uri();
    let bridge = Bridge::serve(Box::new(move |_| {
        result(json!([
            location(&secret_uri, 0, 13, 19),
            location(&uri, 0, 16, 22)
        ]))
    }));
    editor.open(&bridge);
    let row = editor
        .ask(json!({"operation": "definition", "symbolName": "target", "line": 1}))
        .await;
    assert_eq!(row["lsp"]["level"], "ide", "{row}");
    assert!(!row.to_string().contains("secret"), "{row}");
    assert!(row.to_string().contains("target"), "{row}");
}

/// An editor that accepts the call and never answers (its extension host is
/// wedged) is given half of what the request has left, then the server and
/// syntax rungs answer in the other half: the call does not fail.
#[tokio::test(flavor = "current_thread")]
async fn a_hung_editor_leaves_the_rest_of_the_request_to_the_next_rung() {
    let editor = Editor::with_file("notes.zzz");
    editor.without_server();
    // Connections queue in the backlog; nothing ever reads them.
    let hung = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = hung.local_addr().expect("addr").port();
    editor.lock(std::process::id(), port, &[&editor.root], 0o600);
    let started = std::time::Instant::now();
    let budget = super::Budget::within(std::time::Duration::from_secs(4));
    let row = editor
        .ask_under(json!({"operation": "documentSymbols"}), &budget)
        .await;
    assert_fell_back(&row, "ide timed out");
    // Half of 4 s, not the whole of it.
    assert!(started.elapsed() < std::time::Duration::from_secs(4));
    drop(hung);
}

/// The same hang, with a grammar to answer: the syntax rung answers and the
/// row says the editor timed out.
#[tokio::test(flavor = "current_thread")]
async fn a_hung_editor_falls_through_to_the_syntax_rung() {
    let editor = Editor::with_file("lib.rs");
    std::fs::write(&editor.file, "fn target() {}\n").expect("source");
    editor.without_server_for(".rs");
    let hung = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = hung.local_addr().expect("addr").port();
    editor.lock(std::process::id(), port, &[&editor.root], 0o600);
    let budget = super::Budget::within(std::time::Duration::from_secs(4));
    let row = editor
        .ask_under(json!({"operation": "documentSymbols"}), &budget)
        .await;
    assert_eq!(row["lsp"]["level"], "syntax", "{row}");
    assert!(row.to_string().contains("ide timed out"), "{row}");
    assert!(row.to_string().contains("target"), "{row}");
    drop(hung);
}

/// A service that redirects the bridge's address is not followed: the
/// request's paths and symbols go nowhere else.
#[tokio::test(flavor = "current_thread")]
async fn a_redirect_is_not_followed() {
    let editor = Editor::with_file("notes.zzz");
    let elsewhere = Bridge::serve(Box::new(|_| result(json!([]))));
    let target = format!("http://127.0.0.1:{}/lsp", elsewhere.port);
    let bridge = Bridge::serve(Box::new(move |_| {
        (307, json!({"location": target, "reason": "moved"}))
    }));
    editor.open(&bridge);
    assert_fell_back(&outline(&editor).await, "307");
    assert!(elsewhere.methods().is_empty(), "{:?}", elsewhere.methods());
    assert!(elsewhere.violations.lock().expect("violations").is_empty());
}
