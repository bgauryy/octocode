use crate::support::Workspace;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(15);

struct Viewer(Child);

impl Drop for Viewer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn request(origin: &str, path: &str, token: &str, body: &str, extra: &str) -> String {
    let address = origin.trim_start_matches("http://");
    let mut stream = std::net::TcpStream::connect(address).expect("connect config view");
    stream
        .set_read_timeout(Some(DEADLINE))
        .expect("read deadline");
    stream
        .set_write_timeout(Some(DEADLINE))
        .expect("write deadline");
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nX-Octocode-Session: {token}\r\nConnection: close\r\nContent-Length: {}\r\n{extra}\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).expect("write request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    response
}

fn start(workspace: &Workspace) -> (Viewer, String, String) {
    let mut child = Viewer(
        workspace
            .cli()
            .args(["config", "view", "--no-open"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("start config view"),
    );
    let stdout = child.0.stdout.take().expect("stdout");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = sender.send(result);
    });
    let line = receiver
        .recv_timeout(DEADLINE)
        .expect("URL deadline")
        .expect("read URL");
    let url = line
        .trim()
        .strip_prefix("Octocode config view: ")
        .expect("URL");
    let (origin, bootstrap) = url.split_once("/#").expect("fragment credential");
    (child, origin.to_owned(), bootstrap.to_owned())
}

#[test]
fn config_view_uses_loopback_one_use_credentials_and_rejects_cross_site_requests() {
    let workspace = Workspace::new();
    let (mut child, origin, bootstrap) = start(&workspace);
    let session_response = request(&origin, "/api/session", &bootstrap, "{}", "");
    assert!(
        session_response.starts_with("HTTP/1.1 200"),
        "{session_response}"
    );
    let session = session_response
        .split("\r\n\r\n")
        .nth(1)
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|value| value["token"].as_str().map(str::to_owned))
        .expect("session token");
    let replay = request(&origin, "/api/session", &bootstrap, "{}", "");
    assert!(replay.starts_with("HTTP/1.1 401"), "{replay}");
    let cross_site = request(
        &origin,
        "/api/request",
        &session,
        r#"{"operation":"inspect"}"#,
        "Sec-Fetch-Site: cross-site\r\n",
    );
    assert!(cross_site.starts_with("HTTP/1.1 403"), "{cross_site}");
    let close = request(&origin, "/api/close", &session, "{}", "");
    assert!(close.starts_with("HTTP/1.1 200"), "{close}");
    let deadline = Instant::now() + DEADLINE;
    loop {
        if let Some(status) = child.0.try_wait().expect("viewer status") {
            assert!(status.success(), "{status}");
            break;
        }
        assert!(Instant::now() < deadline, "viewer did not exit after close");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// One raw request, for headers a browser sends and the page's own helper does not.
fn raw(origin: &str, head: &str) -> String {
    let address = origin.trim_start_matches("http://");
    let mut stream = std::net::TcpStream::connect(address).expect("connect config view");
    stream
        .set_read_timeout(Some(DEADLINE))
        .expect("read deadline");
    stream
        .write_all(format!("{head}\r\nConnection: close\r\n\r\n").as_bytes())
        .expect("write request");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read response");
    String::from_utf8_lossy(&response).into_owned()
}

fn header_lines(response: &str) -> String {
    response
        .split("\r\n\r\n")
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// CORS stays closed: no response grants another origin anything, a preflight
/// from one is refused, and a rebinding host or cross-site fetch never reaches
/// the page. A valid session token does not change any of that.
#[test]
fn config_view_grants_no_cross_origin_access_and_isolates_its_page() {
    let workspace = Workspace::new();
    let (_child, origin, bootstrap) = start(&workspace);
    let address = origin.trim_start_matches("http://");
    let session = request(&origin, "/api/session", &bootstrap, "{}", "");
    let session = session
        .split("\r\n\r\n")
        .nth(1)
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|value| value["token"].as_str().map(str::to_owned))
        .expect("session token");

    for asset in ["/", "/app.js", "/core.js", "/style.css", "/logo.png"] {
        let page = raw(&origin, &format!("GET {asset} HTTP/1.1\r\nHost: {address}"));
        assert!(page.starts_with("HTTP/1.1 200"), "{asset}: {page}");
        let headers = header_lines(&page);
        for required in [
            "content-security-policy: default-src 'none'",
            "cross-origin-opener-policy: same-origin",
            "cross-origin-embedder-policy: require-corp",
            "cross-origin-resource-policy: same-origin",
            "origin-agent-cluster: ?1",
            "x-content-type-options: nosniff",
            "referrer-policy: no-referrer",
            "cache-control: no-store",
        ] {
            assert!(headers.contains(required), "{asset}: {required}\n{headers}");
        }
    }

    let foreign = "https://evil.example";
    let refused = [
        format!(
            "OPTIONS /api/request HTTP/1.1\r\nHost: {address}\r\nOrigin: {foreign}\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: x-octocode-session"
        ),
        format!("GET / HTTP/1.1\r\nHost: {address}\r\nOrigin: {foreign}"),
        format!("GET / HTTP/1.1\r\nHost: {address}\r\nSec-Fetch-Site: cross-site"),
        format!(
            "GET / HTTP/1.1\r\nHost: rebound.example:{}",
            address.rsplit(':').next().unwrap()
        ),
        format!(
            "POST /api/request HTTP/1.1\r\nHost: {address}\r\nOrigin: {foreign}\r\nContent-Type: application/json\r\nX-Octocode-Session: {session}\r\nContent-Length: 2\r\n\r\n{{}}"
        ),
    ];
    for head in &refused {
        let response = raw(&origin, head);
        assert!(response.starts_with("HTTP/1.1 403"), "{head}\n{response}");
        assert!(
            !header_lines(&response).contains("access-control-"),
            "{response}"
        );
    }

    let same_origin_preflight = raw(
        &origin,
        &format!(
            "OPTIONS /api/request HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nAccess-Control-Request-Method: POST"
        ),
    );
    assert!(
        !same_origin_preflight.starts_with("HTTP/1.1 2"),
        "{same_origin_preflight}"
    );
    assert!(!header_lines(&same_origin_preflight).contains("access-control-"));
}

/// Typing keeps a session open, and only its own session can say so.
#[test]
fn config_view_activity_needs_the_session_token() {
    let workspace = Workspace::new();
    let (_child, origin, bootstrap) = start(&workspace);
    let session = request(&origin, "/api/session", &bootstrap, "{}", "");
    let session = session
        .split("\r\n\r\n")
        .nth(1)
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|value| value["token"].as_str().map(str::to_owned))
        .expect("session token");
    let anonymous = request(&origin, "/api/activity", "not-the-token", "{}", "");
    assert!(anonymous.starts_with("HTTP/1.1 401"), "{anonymous}");
    let active = request(&origin, "/api/activity", &session, "{}", "");
    assert!(active.starts_with("HTTP/1.1 200"), "{active}");
}

fn session_token(origin: &str, bootstrap: &str) -> String {
    let session = request(origin, "/api/session", bootstrap, "{}", "");
    session
        .split("\r\n\r\n")
        .nth(1)
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|value| value["token"].as_str().map(str::to_owned))
        .expect("session token")
}

fn raw_post(origin: &str, token: &str, kind: &str, body: &[u8]) -> String {
    let address = origin.trim_start_matches("http://");
    let mut stream = std::net::TcpStream::connect(address).expect("connect config view");
    stream
        .set_read_timeout(Some(DEADLINE))
        .expect("read deadline");
    let head = format!(
        "POST /api/request HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nContent-Type: {kind}\r\nX-Octocode-Session: {token}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).expect("write head");
    // A server may refuse before reading it all; that is a pass, not a failure.
    let _ = stream.write_all(body);
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    String::from_utf8_lossy(&response).into_owned()
}

/// The page takes small JSON from its own session and nothing else: no file
/// upload, form post or oversized body reaches the configuration.
#[test]
fn config_view_refuses_uploads_forms_and_oversized_bodies() {
    let workspace = Workspace::new();
    let (_child, origin, bootstrap) = start(&workspace);
    let token = session_token(&origin, &bootstrap);
    for kind in [
        "multipart/form-data; boundary=x",
        "application/x-www-form-urlencoded",
        "text/plain",
        "application/octet-stream",
    ] {
        let response = raw_post(&origin, &token, kind, b"--x\r\n\r\nKEY=value\r\n--x--");
        assert!(response.starts_with("HTTP/1.1 415"), "{kind}: {response}");
    }
    let big = vec![b'a'; 65 * 1024];
    let response = raw_post(&origin, &token, "application/json", &big);
    assert!(response.starts_with("HTTP/1.1 413"), "{response}");
    let bad_json = raw_post(&origin, &token, "application/json", b"{not json");
    assert!(bad_json.starts_with("HTTP/1.1 400"), "{bad_json}");
    let polluted = raw_post(
        &origin,
        &token,
        "application/json",
        br#"{"operation":"inspect","__proto__":{"admin":true}}"#,
    );
    assert!(polluted.starts_with("HTTP/1.1 400"), "{polluted}");
    for path in [
        "/../../etc/passwd",
        "/.env",
        "/%2e%2e/%2e%2e/etc/passwd",
        "/api/config",
    ] {
        let address = origin.trim_start_matches("http://");
        let response = raw(&origin, &format!("GET {path} HTTP/1.1\r\nHost: {address}"));
        assert!(response.starts_with("HTTP/1.1 404"), "{path}: {response}");
    }
    assert!(!workspace.home.join(".env").exists(), "nothing was stored");
}

/// Without its session token nothing is read or written, and a wrong token
/// reveals nothing about the configuration.
#[test]
fn config_view_needs_its_token_for_every_read_and_write() {
    let workspace = Workspace::new();
    std::fs::write(workspace.home.join(".env"), "TAVILY_API_KEY=home-secret\n").unwrap();
    let (_child, origin, _bootstrap) = start(&workspace);
    for token in ["", "wrong-token"] {
        for body in [
            r#"{"operation":"inspect"}"#,
            r#"{"operation":"setEnv","scope":"home","key":"X","value":"y","revision":"r"}"#,
        ] {
            let response = request(&origin, "/api/request", token, body, "");
            assert!(response.starts_with("HTTP/1.1 401"), "{response}");
            assert!(!response.contains("home-secret"), "{response}");
            assert!(!response.contains("TAVILY"), "{response}");
        }
    }
    assert_eq!(
        std::fs::read_to_string(workspace.home.join(".env")).unwrap(),
        "TAVILY_API_KEY=home-secret\n"
    );
}

/// What the page sends is checked as hard as what a stranger sends: a line break
/// cannot smuggle a second variable into `.env`, a key cannot name a path, and
/// a process-only variable cannot be saved at all.
#[test]
fn config_view_refuses_values_that_would_write_more_than_they_say() {
    let workspace = Workspace::new();
    let (_child, origin, bootstrap) = start(&workspace);
    let token = session_token(&origin, &bootstrap);
    let inspect = request(
        &origin,
        "/api/request",
        &token,
        r#"{"operation":"inspect"}"#,
        "",
    );
    let inspected: serde_json::Value =
        serde_json::from_str(inspect.split("\r\n\r\n").nth(1).expect("body")).expect("JSON");
    let revision = inspected["files"]["homeEnv"]["revision"].clone();
    let save = |key: &str, value: &str| {
        let body = serde_json::json!({"operation":"setEnv","scope":"home","key":key,"value":value,"revision":revision});
        request(&origin, "/api/request", &token, &body.to_string(), "")
    };
    for (key, value) in [
        ("TAVILY_API_KEY", "x\nGITHUB_API_URL=https://evil.example"),
        ("TAVILY_API_KEY", "x\r\nPATH=/evil"),
        ("TAVILY_API_KEY", "x\u{0}y"),
        ("../../outside", "x"),
        ("A=B", "x"),
        ("", "x"),
        ("PATH", "/evil"),
        ("NODE_OPTIONS", "--require=/evil.js"),
        ("HTTPS_PROXY", "http://evil.example"),
        ("SSL_CERT_FILE", "/evil.pem"),
        ("OCTOCODE_HOME", "/evil"),
        ("TAVILY_BASE_URL", "https://user:secret@evil.example"),
    ] {
        let response = save(key, value);
        assert!(response.starts_with("HTTP/1.1 400"), "{key}: {response}");
        assert!(!response.contains("evil"), "{key}: {response}");
    }
    assert!(!workspace.home.join(".env").exists(), "nothing was stored");
    // A key a workspace may not set is refused in that scope, with no file written.
    let workspace_revision = inspected["files"]["workspaceEnv"]["revision"].clone();
    let body = serde_json::json!({"operation":"setEnv","scope":"workspace","key":"TAVILY_BASE_URL","value":"https://evil.example","revision":workspace_revision});
    let response = request(&origin, "/api/request", &token, &body.to_string(), "");
    assert!(response.starts_with("HTTP/1.1 400"), "{response}");
    assert!(!workspace.workspace.join(".octocode/.env").exists());
}

/// The server listens on loopback only, and its assets name nothing outside themselves.
#[test]
fn config_view_listens_on_loopback_and_loads_nothing_from_elsewhere() {
    let workspace = Workspace::new();
    let (_child, origin, _bootstrap) = start(&workspace);
    assert!(origin.starts_with("http://127.0.0.1:"), "{origin}");
    let address = origin.trim_start_matches("http://");
    for asset in ["/", "/app.js", "/core.js", "/style.css"] {
        let page = raw(&origin, &format!("GET {asset} HTTP/1.1\r\nHost: {address}"));
        let body = page.split("\r\n\r\n").nth(1).unwrap_or_default();
        for external in [
            "http://",
            "https://",
            "//cdn",
            "@import",
            "eval(",
            "new Function",
        ] {
            assert!(!body.contains(external), "{asset} holds {external}");
        }
    }
    let page = raw(&origin, &format!("GET / HTTP/1.1\r\nHost: {address}"));
    assert!(
        header_lines(&page).contains(
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'"
        ),
        "{page}"
    );
}
