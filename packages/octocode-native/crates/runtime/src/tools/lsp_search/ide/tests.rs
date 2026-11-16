//! The IDE rung through the public `lsp_search` entry, against an in-process
//! fake bridge on loopback: it asserts the `Host` and bearer token of every
//! request and answers canned LSP JSON. No language server is spawned.
use crate::id::ToolId;
use crate::tools::test_support::checks::{Page, Walker};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

mod answers;
mod fallbacks;
mod locks;
mod parity;

/// `(status, body)` for a `POST /lsp` body.
type Handler = Box<dyn Fn(&Value) -> (u16, Value) + Send + Sync>;

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The fake editor bridge.
struct Bridge {
    port: u16,
    bodies: Arc<Mutex<Vec<Value>>>,
    /// Requests whose `Host`, bearer token or content type was wrong.
    violations: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl Bridge {
    fn serve(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let bridge = Self {
            port,
            bodies: Arc::default(),
            violations: Arc::default(),
            stop: Arc::default(),
        };
        let (bodies, violations, stop) = (
            Arc::clone(&bridge.bodies),
            Arc::clone(&bridge.violations),
            Arc::clone(&bridge.stop),
        );
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(stream) = stream else { continue };
                answer(stream, port, &handler, &bodies, &violations);
            }
        });
        bridge
    }

    /// The methods asked, in order.
    fn methods(&self) -> Vec<String> {
        self.bodies
            .lock()
            .expect("bodies")
            .iter()
            .map(|body| body["method"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    fn asked(&self, method: &str) -> Vec<Value> {
        self.bodies
            .lock()
            .expect("bodies")
            .iter()
            .filter(|body| body["method"] == method)
            .cloned()
            .collect()
    }

    /// Every request carried `Host: 127.0.0.1:<port>` and the bearer token.
    fn assert_authentic(&self) {
        let violations = self.violations.lock().expect("violations");
        assert!(violations.is_empty(), "{violations:?}");
        assert!(
            !self.bodies.lock().expect("bodies").is_empty(),
            "never asked"
        );
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], self.port)));
    }
}

/// Serve one connection: parse the request, check it, answer it.
fn answer(
    mut stream: TcpStream,
    port: u16,
    handler: &Handler,
    bodies: &Mutex<Vec<Value>>,
    violations: &Mutex<Vec<String>>,
) {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    let (head_end, length) = loop {
        let read = stream.read(&mut chunk).unwrap_or(0);
        raw.extend_from_slice(&chunk[..read]);
        if let Some(end) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..end]).to_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            break (end + 4, length);
        }
        if read == 0 {
            return;
        }
    };
    while raw.len() < head_end + length {
        let read = stream.read(&mut chunk).unwrap_or(0);
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
    }
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let header = |name: &str| {
        head.lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_owned())
            })
            .unwrap_or_default()
    };
    let mut wrong = Vec::new();
    if header("host") != format!("127.0.0.1:{port}") {
        wrong.push(format!("host {:?}", header("host")));
    }
    if header("authorization") != format!("Bearer {TOKEN}") {
        wrong.push("authorization".to_owned());
    }
    if !header("content-type").starts_with("application/json") {
        wrong.push("content-type".to_owned());
    }
    let body: Value = serde_json::from_slice(&raw[head_end..]).unwrap_or(Value::Null);
    let (status, reply) = if wrong.is_empty() {
        bodies.lock().expect("bodies").push(body.clone());
        handler(&body)
    } else {
        violations.lock().expect("violations").extend(wrong);
        (
            401,
            json!({"error": "unauthorized", "reason": "bad credentials"}),
        )
    };
    let text = reply.to_string();
    // A reply with a `location` field sends it as the `Location` header.
    let location = reply
        .get("location")
        .and_then(Value::as_str)
        .map(|target| format!("Location: {target}\r\n"))
        .unwrap_or_default();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\n{location}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
}

/// A workspace, and a home whose `ide/` holds a lock for it.
struct Editor {
    _root: crate::tools::test_support::Fixture,
    _home: crate::tools::test_support::Fixture,
    root: PathBuf,
    home: PathBuf,
    file: PathBuf,
}

const SOURCE: &str = "export function target() {}\nexport function caller() { target(); }\nexport function top() { caller(); }\n";

impl Editor {
    fn new() -> Self {
        Self::with_file("a.ts")
    }

    /// A workspace holding `name` (the TypeScript `SOURCE`).
    fn with_file(name: &str) -> Self {
        let dir = crate::tools::test_support::Fixture::new();
        let root = dir.0.canonicalize().expect("canonical root");
        let file = root.join(name);
        std::fs::write(&file, SOURCE).expect("source");
        let home = crate::tools::test_support::Fixture::new();
        let home_path = home.0.canonicalize().expect("home");
        Self {
            _root: dir,
            _home: home,
            root,
            home: home_path,
            file,
        }
    }

    fn uri(&self) -> String {
        octocode_engine::lsp::uri::path_to_uri(&self.file.to_string_lossy()).expect("uri")
    }

    /// Write `<home>/ide/<pid>.json` for `pid` with `mode`, for `folders`.
    fn lock(&self, pid: u32, port: u16, folders: &[&Path], mode: u32) {
        let dir = self.home.join("ide");
        std::fs::create_dir_all(&dir).expect("ide dir");
        private_directory(&dir);
        let file = dir.join(format!("{pid}.json"));
        std::fs::write(
            &file,
            json!({
                "version": 1, "pid": pid, "ide": "Visual Studio Code", "port": port,
                "token": TOKEN, "workspaceFolders": folders
            })
            .to_string(),
        )
        .expect("lock");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).expect("mode");
        }
        let _ = mode;
    }

    /// A live lock for this workspace on `bridge`.
    fn open(&self, bridge: &Bridge) {
        self.lock(std::process::id(), bridge.port, &[&self.root], 0o600);
    }

    /// Route `.zzz` files to a server that does not exist, with no grammar to
    /// answer instead: the server rung's own row is then the final one.
    fn without_server(&self) {
        self.without_server_for(".zzz");
    }

    /// Route `extension` files to a server that does not exist.
    fn without_server_for(&self, extension: &str) {
        std::fs::write(
            self.home.join("lsp-servers.json"),
            json!({"languageServers": {extension: {
                "command": self.home.join("missing-server").to_string_lossy(), "args": [],
                "languageId": "plaintext"
            }}})
            .to_string(),
        )
        .expect("config");
    }

    fn execution(&self) -> crate::tools::lsp_search::LspExecutionConfig {
        crate::tools::lsp_search::LspExecutionConfig {
            octocode_home: Some(self.home.clone()),
            ..crate::tools::lsp_search::LspExecutionConfig::with_path()
        }
    }

    /// Every page of `start`, walked through its `next` leads (each once).
    async fn walk(&self, start: Value, budget: Option<usize>) -> Vec<Value> {
        let paths = crate::tools::test_support::workspace_policy(&self.root);
        let pool = crate::tools::lsp_search::tests::fake_server_pool();
        let execution = self.execution();
        let mut walker = Walker::new(ToolId::LspSearch.as_str(), json!({ "queries": [start] }));
        let mut pages = Vec::new();
        while let Some(call) = walker.next_query() {
            let row = crate::tools::test_support::opened_input(ToolId::LspSearch, &call)["queries"]
                [0]
            .clone();
            let result = crate::tools::lsp_search::execute(
                serde_json::from_value(row).expect("query"),
                &crate::execution::cancel::NeverCancel,
                &pool,
                &paths,
                &execution,
                budget,
                crate::tools::lsp_search::test_answers(),
            )
            .await
            .expect("row");
            walker.accept(Page {
                output: crate::tools::test_support::published(ToolId::LspSearch, &result),
                rows: Vec::new(),
            });
            pages.push(result);
        }
        walker.finish();
        pages
    }

    /// The first page of `query` on the editor's file, under `cancel`.
    async fn ask_under(
        &self,
        mut query: Value,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Value {
        query["path"] = json!(self.file.to_string_lossy());
        let paths = crate::tools::test_support::workspace_policy(&self.root);
        let pool = crate::tools::lsp_search::tests::fake_server_pool();
        crate::tools::lsp_search::execute(
            serde_json::from_value(query).expect("query"),
            cancel,
            &pool,
            &paths,
            &self.execution(),
            None,
            crate::tools::lsp_search::test_answers(),
        )
        .await
        .expect("row")
    }

    /// The first page of one query on `a.ts`.
    async fn ask(&self, mut query: Value) -> Value {
        query["path"] = json!(self.file.to_string_lossy());
        self.walk(query, None).await.remove(0)
    }
}

/// A `{result}` answer.
fn result(value: Value) -> (u16, Value) {
    (200, json!({ "result": value }))
}

/// A `Location` on line `line` (zero-based) of `uri`.
fn location(uri: &str, line: u32, start: u32, end: u32) -> Value {
    json!({"uri": uri, "range": {
        "start": {"line": line, "character": start},
        "end": {"line": line, "character": end}}})
}

/// A call hierarchy item of function `name` declared on `line` of `uri`.
fn item(uri: &str, name: &str, line: u32) -> Value {
    json!({"name": name, "kind": 12, "uri": uri,
        "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 30}},
        "selectionRange": {"start": {"line": line, "character": 16}, "end": {"line": line, "character": 22}},
        "data": format!("handle-{name}")})
}

/// `dir` owned by the user, closed to everyone else (as the extension makes it).
fn private_directory(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).expect("mode");
    }
    let _ = dir;
}

/// A request that runs out of time `within` from now, as a call's does.
struct Budget(std::time::Instant);

impl Budget {
    fn within(span: std::time::Duration) -> Self {
        Self(std::time::Instant::now() + span)
    }
}

impl crate::execution::cancel::CancellationCheck for Budget {
    fn check(&self) -> Result<(), String> {
        if std::time::Instant::now() >= self.0 {
            return Err("Timeout".into());
        }
        Ok(())
    }

    fn deadline(&self) -> Option<std::time::Instant> {
        Some(self.0)
    }
}
