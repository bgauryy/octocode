use super::*;
use std::{net::TcpListener, sync::Mutex};

struct Server {
    endpoint: String,
    state: Arc<Mutex<String>>,
    workspace: Arc<Mutex<String>>,
    requests: Arc<Mutex<Vec<Value>>>,
    worker: thread::JoinHandle<Result<()>>,
}
impl Server {
    fn start(status: &str, failure: &str) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let endpoint = format!("ws://{}/", listener.local_addr()?);
        let state = Arc::new(Mutex::new(status.to_owned()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let workspace = Arc::new(Mutex::new(String::new()));
        let cwd = workspace.clone();
        let (current, seen, failure) = (state.clone(), requests.clone(), failure.to_owned());
        let worker = thread::spawn(move || -> Result<()> {
            let (stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(10)))?;
            let mut socket = tungstenite::accept(stream)?;
            loop {
                let frame = match socket.read() {
                    Ok(tungstenite::Message::Text(frame)) => frame,
                    Ok(tungstenite::Message::Close(_))
                    | Err(tungstenite::Error::ConnectionClosed) => break,
                    Err(tungstenite::Error::Protocol(
                        tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                    )) => break,
                    Err(error) => return Err(error.into()),
                    _ => continue,
                };
                let request: Value = serde_json::from_str(&frame)?;
                seen.lock()
                    .map_err(|_| anyhow!("Request mutex poisoned"))?
                    .push(request.clone());
                if request.get("id").is_none() {
                    continue;
                }
                let method = request["method"].as_str().unwrap_or("");
                if method == "turn/start" && failure == "drop" {
                    break;
                }
                let response = if method == "turn/start" && failure == "race" {
                    json!({"id":request["id"],"error":{"code":-32000,"message":"Active turn cannot accept steering"}})
                } else {
                    let result = match method {
                        "thread/read" => {
                            json!({"thread":{"cwd": if failure == "wrong-workspace" { json!("/") } else if failure == "missing-workspace" { Value::Null } else { json!(cwd.lock().map_err(|_| anyhow!("Workspace mutex poisoned"))?.clone()) },"id": if failure == "wrong-thread" {"another-thread"} else {"existing"},"status":{"type":current.lock().map_err(|_| anyhow!("State mutex poisoned"))?.clone()}}})
                        }
                        "turn/start" if failure == "bad-receipt" => json!({}),
                        "turn/start" => {
                            json!({"turn":{"id":"accepted-turn","status":"inProgress","items":[],"error":null}})
                        }
                        _ => json!({}),
                    };
                    json!({"id":request["id"],"result":result})
                };
                socket.send(tungstenite::Message::Text(response.to_string().into()))?;
            }
            Ok(())
        });
        Ok(Self {
            endpoint,
            state,
            workspace,
            requests,
            worker,
        })
    }
    fn methods(&self) -> Result<Vec<String>> {
        Ok(self
            .requests
            .lock()
            .map_err(|_| anyhow!("Request mutex poisoned"))?
            .iter()
            .filter_map(|r| r["method"].as_str().map(str::to_owned))
            .collect())
    }
    fn stop(self, client: DeliveryClients) -> Result<()> {
        drop(client);
        self.worker
            .join()
            .map_err(|_| anyhow!("Fixture server panicked"))??;
        Ok(())
    }
}
fn fixture(endpoint: &str, wake: &str) -> Result<(tempfile::TempDir, Store, String, i64)> {
    let temp = tempfile::tempdir()?;
    let store = Store::open(temp.path().join("audit.sqlite"), temp.path(), false, true)?;
    let sender = store.call("", "join", &json!({"name":"sender","vendor":"raw"}))?;
    let receiver = store.call("", "join", &json!({"name":"receiver","vendor":"codex"}))?;
    let recipient = catalog::text(&receiver, "id")?.to_owned();
    store.attach(
        &recipient,
        &json!({"transport":"codex","endpoint":endpoint,"vendorSession":"existing"}),
    )?;
    let sent = store.call(catalog::text(&sender,"id")?, "send_message", &json!({"to":recipient,"body":"exact peer body","wake":wake,"reasoning":"Verify wake without duplicate context"}))?;
    Ok((
        temp,
        store,
        recipient,
        sent["id"]
            .as_i64()
            .ok_or_else(|| anyhow!("No message id"))?,
    ))
}

#[test]
fn codex_idle_action_starts_once_without_injecting_duplicate_context() -> Result<()> {
    let server = Server::start("idle", "")?;
    let (_temp, store, recipient, _) = fixture(&server.endpoint, "action")?;
    *server
        .workspace
        .lock()
        .map_err(|_| anyhow!("Workspace mutex poisoned"))? = store.workspace.clone();
    let mut client = DeliveryClients::default();
    let result = once(&store, &recipient, &mut client)?;
    assert_eq!(result["recipientTurnRequested"], true);
    assert_eq!(result["modelCalls"], 0);
    assert_eq!(result["submitted"], 1);
    assert_eq!(once(&store, &recipient, &mut client)?["submitted"], 0);
    assert_eq!(
        server.methods()?,
        ["initialize", "initialized", "thread/read", "turn/start"]
    );
    let requests = server
        .requests
        .lock()
        .map_err(|_| anyhow!("Request mutex poisoned"))?;
    let start = requests.last().ok_or_else(|| anyhow!("No turn request"))?;
    assert_eq!(
        start["params"].as_object().map(|v| v.len()),
        Some(2),
        "Do not override host model, policy, effort or permissions"
    );
    assert_eq!(
        start["params"]["input"][0]["text"]
            .as_str()
            .map(|s| s.matches("exact peer body").count()),
        Some(1)
    );
    drop(requests);
    assert_eq!(
        store.call(&recipient, "inbox", &json!({}))?["items"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    server.stop(client)
}

#[test]
fn codex_passive_injects_without_starting_turn() -> Result<()> {
    let server = Server::start("idle", "")?;
    let (_temp, store, recipient, _) = fixture(&server.endpoint, "passive")?;
    *server
        .workspace
        .lock()
        .map_err(|_| anyhow!("Workspace mutex poisoned"))? = store.workspace.clone();
    let mut client = DeliveryClients::default();
    let result = once(&store, &recipient, &mut client)?;
    assert_eq!(result["recipientTurnRequested"], false);
    assert_eq!(
        server.methods()?,
        [
            "initialize",
            "initialized",
            "thread/read",
            "thread/inject_items"
        ]
    );
    server.stop(client)
}

#[test]
fn codex_busy_and_unloaded_threads_keep_mail_unstaged_until_idle() -> Result<()> {
    for status in ["active", "notLoaded", "systemError"] {
        let server = Server::start(status, "")?;
        let (_temp, store, recipient, _) = fixture(&server.endpoint, "action")?;
        *server
            .workspace
            .lock()
            .map_err(|_| anyhow!("Workspace mutex poisoned"))? = store.workspace.clone();
        let mut client = DeliveryClients::default();
        let result = once(&store, &recipient, &mut client)?;
        assert_eq!(result["submitted"], 0);
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM dispatches", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        *server
            .state
            .lock()
            .map_err(|_| anyhow!("State mutex poisoned"))? = "idle".into();
        assert_eq!(
            once(&store, &recipient, &mut client)?["recipientTurnRequested"],
            true
        );
        server.stop(client)?;
    }
    Ok(())
}

#[test]
fn codex_races_dropped_connections_and_bad_receipts_are_uncertain_without_replay() -> Result<()> {
    for failure in ["race", "drop", "bad-receipt"] {
        let server = Server::start("idle", failure)?;
        let (_temp, store, recipient, _) = fixture(&server.endpoint, "action")?;
        *server
            .workspace
            .lock()
            .map_err(|_| anyhow!("Workspace mutex poisoned"))? = store.workspace.clone();
        let mut client = DeliveryClients::default();
        assert!(once(&store, &recipient, &mut client).is_err(), "{failure}");
        assert!(client.native.is_none(), "Failed connection must be cleared");
        assert_eq!(
            store
                .db
                .query_row("SELECT state FROM dispatches", [], |r| r
                    .get::<_, String>(0))?,
            "uncertain"
        );
        assert_eq!(
            once(&store, &recipient, &mut client)?["submitted"],
            0,
            "Uncertain attempt must not reconnect or replay"
        );
        assert_eq!(
            server
                .methods()?
                .iter()
                .filter(|m| *m == "turn/start")
                .count(),
            1
        );
        server.stop(client)?;
    }
    Ok(())
}

#[test]
fn codex_wrong_thread_receipt_fails_before_staging() -> Result<()> {
    let server = Server::start("idle", "wrong-thread")?;
    let (_temp, store, recipient, _) = fixture(&server.endpoint, "action")?;
    *server
        .workspace
        .lock()
        .map_err(|_| anyhow!("Workspace mutex poisoned"))? = store.workspace.clone();
    let mut client = DeliveryClients::default();
    assert!(once(&store, &recipient, &mut client).is_err());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM dispatches", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    server.stop(client)
}

#[test]
fn codex_wrong_or_missing_workspace_never_stages_or_injects() -> Result<()> {
    for failure in ["wrong-workspace", "missing-workspace"] {
        let server = Server::start("idle", failure)?;
        let (_temp, store, recipient, _) = fixture(&server.endpoint, "passive")?;
        let mut client = DeliveryClients::default();
        let error = once(&store, &recipient, &mut client)
            .err()
            .ok_or_else(|| anyhow!("Expected workspace rejection"))?;
        assert!(error.to_string().contains("workspace"));
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM dispatches", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        assert!(
            !server
                .methods()?
                .iter()
                .any(|method| method == "thread/inject_items" || method == "turn/start")
        );
        server.stop(client)?;
    }
    Ok(())
}
