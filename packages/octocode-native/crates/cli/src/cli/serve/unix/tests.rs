use super::client::*;
use super::daemon::*;
use super::identity::*;
use super::sockets::*;
use super::*;
use crate::cli::Printed;
use crate::cli::serve::runs_on_server;
use octocode_native::id::ToolId;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[test]
fn exit_removes_only_the_socket_this_server_bound() {
    // A successor that rebound the path keeps its socket when we exit.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("s.sock");
    let first = std::os::unix::net::UnixListener::bind(&path).expect("bind");
    let ours = socket_identity(&path);
    drop(first);
    std::fs::remove_file(&path).expect("unlink");
    let _successor = std::os::unix::net::UnixListener::bind(&path).expect("rebind");
    remove_if_ours(&path, ours);
    assert!(path.exists(), "the successor's socket must survive");
    let theirs = socket_identity(&path);
    remove_if_ours(&path, theirs);
    assert!(!path.exists());
    // An unknown identity removes nothing.
    let _again = std::os::unix::net::UnixListener::bind(&path).expect("bind again");
    remove_if_ours(&path, None);
    assert!(path.exists());
}

#[tokio::test]
async fn request_line_is_read_only_up_to_its_limit() {
    // Served from a slice; the handler passes the socket's buffered reader.
    let mut ok = tokio::io::BufReader::new(&b"{\"tool\":\"x\"}\n"[..]);
    assert_eq!(
        read_line_bounded(&mut ok, 64).await,
        Some("{\"tool\":\"x\"}\n".to_owned())
    );
    // Exactly at the limit, with its newline, is still a request.
    let mut at_limit = [b'a'; 64];
    at_limit[63] = b'\n';
    let mut at_limit = tokio::io::BufReader::new(&at_limit[..]);
    assert!(read_line_bounded(&mut at_limit, 64).await.is_some());
    // A line with no newline inside the limit is refused, not buffered whole.
    let long_line = [b'a'; 100];
    let mut long = tokio::io::BufReader::new(&long_line[..]);
    assert_eq!(read_line_bounded(&mut long, 64).await, None);
    let mut empty = tokio::io::BufReader::new(&b""[..]);
    assert_eq!(read_line_bounded(&mut empty, 64).await, None);
}

static NO_ENV: BTreeMap<String, String> = BTreeMap::new();

fn request<'a>(input: &'a Value, cwd: &'a Path) -> Request<'a> {
    Request {
        tool: "localSearch",
        input,
        json_out: true,
        cwd,
        env: &NO_ENV,
        identity: "id",
    }
}

/// An exchange no interrupt reaches.
async fn exchange_plain(
    client: &mut tokio::net::UnixStream,
    request: &Request<'_>,
    deadline: Duration,
) -> Result<Printed, Failed> {
    exchange_reply(client, request, deadline)
        .await
        .map(|reply| reply.printed)
}

async fn exchange_reply(
    client: &mut tokio::net::UnixStream,
    request: &Request<'_>,
    deadline: Duration,
) -> Result<Reply, Failed> {
    exchange(
        client,
        request,
        deadline,
        std::future::pending(),
        std::future::pending::<()>(),
        || {},
    )
    .await
}

#[tokio::test]
async fn client_reads_one_reply_line_and_treats_a_silent_close_as_no_reply() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let (mut client, server) = tokio::net::UnixStream::pair().expect("pair");
    let peer = tokio::spawn(async move {
        let mut server = BufReader::new(server);
        let mut request = String::new();
        server.read_line(&mut request).await.expect("request");
        server
            .get_mut()
            .write_all(b"{\"exit\":1,\"stderr\":false,\"body\":\"{}\"}\n")
            .await
            .expect("reply");
        request
    });
    let input = json!({"q": 1});
    let request = request(&input, Path::new("/work/here"));
    let reply = exchange_plain(&mut client, &request, REPLY_DEADLINE).await;
    assert_eq!(
        reply,
        Ok(Printed {
            exit: 1,
            stream: crate::cli::Stream::Stdout,
            body: "{}".into(),
            error_text: None,
            notes: None,
        })
    );
    let sent: Value = serde_json::from_str(&peer.await.expect("peer")).expect("json");
    assert_eq!(sent["tool"], "localSearch");
    // The server renders in the caller's output mode: one channel.
    assert_eq!(sent["json"], true);
    // ...runs it as a CLI in the caller's directory, under its identity.
    assert_eq!(sent["cwd"], "/work/here");
    assert_eq!(sent["identity"], "id");
    // ...with the caller's environment.
    assert_eq!(sent["env"], json!({}));

    // A server that closes without replying sends the caller to the
    // in-process path, with a reason to state.
    let (mut client, server) = tokio::net::UnixStream::pair().expect("pair");
    drop(server);
    assert!(matches!(
        exchange_plain(&mut client, &request, REPLY_DEADLINE).await,
        Err(Failed::Fallback(_))
    ));
}

/// A server that refuses sends the caller in process with its reason;
/// one that never replies ends the call at the deadline instead of
/// running it twice.
#[tokio::test]
async fn a_refusal_falls_back_and_a_silent_server_ends_at_the_deadline() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let input = json!({});
    let request = request(&input, Path::new("/w"));
    let (mut client, server) = tokio::net::UnixStream::pair().expect("pair");
    let peer = tokio::spawn(async move {
        let mut server = BufReader::new(server);
        let mut line = String::new();
        server.read_line(&mut line).await.expect("request");
        server
            .get_mut()
            .write_all(b"{\"fallback\":\"it runs under another configuration\"}\n")
            .await
            .expect("reply");
    });
    assert_eq!(
        exchange_plain(&mut client, &request, REPLY_DEADLINE).await,
        Err(Failed::Fallback(
            "it runs under another configuration".into()
        ))
    );
    peer.await.expect("peer");

    let (mut client, _silent) = tokio::net::UnixStream::pair().expect("pair");
    assert_eq!(
        exchange_plain(&mut client, &request, Duration::from_millis(50)).await,
        Err(Failed::Deadline)
    );
    // The deadline outlasts the server's own request budget.
    assert!(REPLY_DEADLINE > octocode_native::execution::DEFAULT_TIMEOUT);
}

/// Ctrl-C on the client closes its side, which the server reads as a
/// cancel; the client then waits for the server to finish, or stops
/// waiting at a second Ctrl-C.
#[tokio::test]
async fn ctrl_c_closes_the_request_side_and_drains_the_server() {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    let input = json!({});
    let request = request(&input, Path::new("/w"));
    let (mut client, server) = tokio::net::UnixStream::pair().expect("pair");
    let peer = tokio::spawn(async move {
        let mut server = BufReader::new(server);
        let mut line = String::new();
        server.read_line(&mut line).await.expect("request");
        // The client's close reaches the server as end of stream.
        let mut rest = Vec::new();
        server.read_to_end(&mut rest).await.expect("read");
        rest.is_empty()
    });
    let noticed = std::cell::Cell::new(false);
    let drained = exchange(
        &mut client,
        &request,
        REPLY_DEADLINE,
        std::future::ready(()),
        std::future::pending::<()>(),
        || noticed.set(true),
    )
    .await;
    assert_eq!(
        drained,
        Err(Failed::Interrupted(crate::cli::Interrupted::Drained))
    );
    assert!(noticed.get(), "the cancel notice is printed");
    assert!(peer.await.expect("peer"), "the server saw only the close");

    // A server that keeps working is abandoned at the second Ctrl-C.
    let (mut client, _busy) = tokio::net::UnixStream::pair().expect("pair");
    let forced = exchange(
        &mut client,
        &request,
        REPLY_DEADLINE,
        std::future::ready(()),
        std::future::ready(()),
        || {},
    )
    .await;
    assert_eq!(
        forced,
        Err(Failed::Interrupted(crate::cli::Interrupted::Forced))
    );
}

/// The server cancels a call whose client left, waits for the cancelled
/// work, and replies nothing; a client that stays gets the result.
#[tokio::test]
async fn a_client_that_leaves_cancels_its_call() {
    let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();
    let execution = async {
        let _ = cancelled.await;
        "drained"
    };
    let mut gone = &b""[..];
    let mut cancel = Some(cancel);
    let result = unless_left(execution, &mut gone, || {
        let _ = cancel.take().expect("once").send(());
    })
    .await;
    assert_eq!(result, None);
    assert!(cancel.is_none(), "the call was cancelled");

    let (_client, mut server) = tokio::io::duplex(8);
    let result = unless_left(async { "done" }, &mut server, || panic!("not cancelled")).await;
    assert_eq!(result, Some("done"));
}

#[test]
fn every_read_only_tool_runs_on_the_server() {
    for tool in ToolId::ALL {
        let mutates = matches!(tool, ToolId::GhCloneRepo);
        assert_eq!(runs_on_server(tool), !mutates, "{tool}");
    }
}

/// A synthetic configuration: `env` (with `OCTOCODE_HOME` = `home`) for
/// a CLI in `cwd`.
fn identity(
    home: &Path,
    cwd: &Path,
    env: &[(&str, &str)],
) -> (octocode_native::config::ConfigOutput, String) {
    use octocode_native::config::{RuntimeSurface, acquire_config_input, resolve_config};
    let mut vars: BTreeMap<String, String> = env
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    vars.entry("OCTOCODE_HOME".into())
        .or_insert_with(|| home.to_string_lossy().into_owned());
    let vars = served_env(vars);
    let input = acquire_config_input(
        vars.clone(),
        cwd.to_path_buf(),
        home.to_path_buf(),
        RuntimeSurface::Cli,
    );
    let config = resolve_config(&input);
    let identity = call_identity(&config, &vars);
    (config, identity)
}

/// A `cd` changes the shell's directory variables only: the call keeps
/// its runtime. Any other variable is another runtime.
#[test]
fn a_cd_keeps_the_runtime_of_a_directory() {
    let runtime = |env: &[(&str, &str)]| {
        let env = env
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        env_digest(&served_env(env))
    };
    let base = runtime(&[("GH_CONFIG_DIR", "/a"), ("PWD", "/w"), ("OLDPWD", "/x")]);
    assert_eq!(
        runtime(&[("GH_CONFIG_DIR", "/a"), ("PWD", "/w/sub"), ("OLDPWD", "/w")]),
        base
    );
    assert_eq!(runtime(&[("GH_CONFIG_DIR", "/a")]), base);
    assert_ne!(runtime(&[("GH_CONFIG_DIR", "/b"), ("PWD", "/w")]), base);
}

/// Every input that changes what a call does changes the server key;
/// the shell's `PWD` and unrelated variables do not, and two
/// directories of one repository share a server.
#[test]
fn the_server_key_covers_every_identity_input() {
    let home = tempfile::tempdir().expect("home");
    let repo = tempfile::tempdir().expect("repo");
    std::fs::create_dir_all(repo.path().join(".git")).expect("git");
    let (a, b) = (repo.path().join("a"), repo.path().join("b"));
    std::fs::create_dir_all(&a).expect("a");
    std::fs::create_dir_all(&b).expect("b");
    let key = |cwd: &Path, env: &[(&str, &str)]| {
        server_key(
            "exe|1|2",
            &workspace_of(cwd),
            &identity(home.path(), cwd, env).1,
        )
    };
    let base = key(&a, &[("GITHUB_TOKEN", "t1")]);
    assert_eq!(base.len(), 16);
    assert!(!base.contains("t1"));
    assert_eq!(key(&a, &[("GITHUB_TOKEN", "t1")]), base);
    // One server for two directories of one repository.
    assert_eq!(key(&b, &[("GITHUB_TOKEN", "t1")]), base);
    assert_eq!(workspace_of(&a), repo.path());
    for (what, env) in [
        ("PWD", vec![("GITHUB_TOKEN", "t1"), ("PWD", "/elsewhere")]),
        (
            "an unrelated variable",
            vec![("GITHUB_TOKEN", "t1"), ("TERM_SESSION_ID", "x")],
        ),
    ] {
        assert_eq!(key(&a, &env), base, "{what}");
    }
    let other_home = tempfile::tempdir().expect("other home");
    let other_home = other_home.path().to_string_lossy().into_owned();
    for (what, env) in [
        ("token", vec![("GITHUB_TOKEN", "t2")]),
        ("token source", vec![("GH_TOKEN", "t1")]),
        ("no token", vec![]),
        (
            "OCTOCODE_BETA",
            vec![("GITHUB_TOKEN", "t1"), ("OCTOCODE_BETA", "true")],
        ),
        (
            "OCTOCODE_HOME",
            vec![("GITHUB_TOKEN", "t1"), ("OCTOCODE_HOME", &other_home)],
        ),
        (
            "an undeclared OCTOCODE_ variable",
            vec![("GITHUB_TOKEN", "t1"), ("OCTOCODE_X", "1")],
        ),
        (
            "proxy",
            vec![("GITHUB_TOKEN", "t1"), ("HTTPS_PROXY", "http://proxy:3128")],
        ),
        (
            "trust store",
            vec![("GITHUB_TOKEN", "t1"), ("SSL_CERT_FILE", "/ca.pem")],
        ),
        ("PATH", vec![("GITHUB_TOKEN", "t1"), ("PATH", "/opt/bin")]),
    ] {
        assert_ne!(key(&a, &env), base, "{what}");
    }
    // Every variable the engine reads from the server's own process: a
    // call cannot bring its own value, so another value is another server.
    for name in octocode_engine::process_env::PROCESS_ENV
        .iter()
        .chain(octocode_io::http::NETWORK_ENV)
    {
        assert_ne!(
            key(&a, &[("GITHUB_TOKEN", "t1"), (name, "/elsewhere")]),
            base,
            "{name}"
        );
    }
    // The home `.env`, the home and workspace `.octocoderc` files.
    let env = [("GITHUB_TOKEN", "t1")];
    std::fs::write(home.path().join(".env"), "OCTOCODE_REDACT_EMAILS=true\n").expect(".env");
    let with_env = key(&a, &env);
    assert_ne!(with_env, base, "home .env");
    std::fs::write(
        home.path().join(".octocoderc"),
        "{\"local\":{\"enabled\":false}}",
    )
    .expect("rc");
    let with_rc = key(&a, &env);
    assert_ne!(with_rc, with_env, "home .octocoderc");
    std::fs::create_dir_all(a.join(".octocode")).expect("ws");
    std::fs::write(
        a.join(".octocode/.octocoderc"),
        "{\"lsp\":{\"prewarm\":\"off\"}}",
    )
    .expect("ws rc");
    assert_ne!(key(&a, &env), with_rc, "workspace .octocoderc");
    // Another repository, another build.
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    assert_ne!(key(elsewhere.path(), &env), key(&b, &env), "workspace");
    let identity = identity(home.path(), &b, &env).1;
    assert_ne!(
        server_key("exe|1|3", &workspace_of(&b), &identity),
        server_key("exe|1|2", &workspace_of(&b), &identity),
        "build"
    );
}

#[test]
fn the_least_recently_used_runtime_leaves_at_the_cap() {
    let mut recent = Recent::new(2);
    assert_eq!(recent.insert("a", 1), None);
    assert_eq!(recent.insert("b", 2), None);
    assert_eq!(recent.get(&"a"), Some(&1));
    // `b` is now the least recently used.
    assert_eq!(recent.insert("c", 3), Some(2));
    assert_eq!(recent.get(&"b"), None);
    // Replacing a key keeps one entry for it.
    assert_eq!(recent.insert("a", 4), None);
    assert_eq!(recent.get(&"a"), Some(&4));
    assert_eq!(recent.drain(), vec![3, 4]);
}

#[test]
fn stale_sockets_are_pruned_and_live_ones_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let own = dir.path().join("serve-own.sock");
    let live = dir.path().join("serve-live.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&live).expect("bind");
    let stale = dir.path().join("serve-stale.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale).expect("bind"));
    let other = dir.path().join("notes.txt");
    std::fs::write(&other, "x").expect("write");
    prune_siblings(&own);
    assert!(live.exists() && other.exists());
    assert!(!stale.exists());
}

/// What a server listening on `listener` was sent by one connection.
fn received(listener: &std::os::unix::net::UnixListener) -> Vec<u8> {
    use std::io::Read as _;
    let (mut stream, _) = listener.accept().expect("accept");
    let mut sent = Vec::new();
    stream.read_to_end(&mut sent).expect("read");
    sent
}

/// A starting server keeps [`MAX_SERVERS`] in all: the least recently
/// asked live ones past the cap are asked to stop, the others are left
/// alone.
#[test]
fn a_starting_server_stops_the_least_recently_asked_past_the_cap() {
    let dir = tempfile::tempdir().expect("dir");
    let own = dir.path().join("serve-own.sock");
    let servers: Vec<_> = (0..=MAX_SERVERS)
        .map(|at| {
            let path = dir.path().join(format!("serve-{at}.sock"));
            let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
            std::thread::sleep(Duration::from_millis(20));
            (path, listener)
        })
        .collect();
    // The first one bound was asked last, so it is the most recent.
    touch(&servers[0].0);
    prune_siblings(&own);
    // Kept: the first, then the newest bound, up to the cap with `own`;
    // asked to stop: the oldest bound after the first.
    let stopped = servers.len() - (MAX_SERVERS - 1);
    for (at, (path, listener)) in servers.iter().enumerate() {
        let asked_to_stop = (1..=stopped).contains(&at);
        assert_eq!(
            received(listener) == STOP,
            asked_to_stop,
            "{}",
            path.display()
        );
    }
    // Stop requests go to every live server, and none is removed.
    stop_in(dir.path());
    for (path, listener) in &servers {
        assert_eq!(received(listener), STOP, "{}", path.display());
        assert!(path.exists());
    }
}

/// The socket's place does not depend on the Octocode home, so a
/// long home (a macOS `$TMPDIR` one) still gets a warm server; a
/// location too long for a socket says so instead of being skipped.
#[test]
fn the_socket_lives_under_the_temp_dir_whatever_the_home() {
    let temp = tempfile::tempdir().expect("temp");
    let socket = socket_in(temp.path(), "0123456789abcdef").expect("socket");
    assert!(socket.starts_with(temp.path()), "{}", socket.display());
    assert!(
        std::os::unix::net::SocketAddr::from_pathname(&socket).is_ok(),
        "{}",
        socket.display()
    );
    let deep = temp.path().join("d".repeat(120));
    std::fs::create_dir_all(&deep).expect("deep");
    let refused = socket_in(&deep, "0123456789abcdef").expect_err("too long");
    assert!(refused.contains("too long"), "{refused}");
}

#[test]
fn socket_directory_must_be_private() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let temp = tempfile::tempdir().expect("temp");
    let uid = std::fs::metadata(temp.path()).expect("meta").uid();
    let sockets = temp.path().join(format!("octocode-{uid}"));
    assert!(private_dir(&sockets, uid).is_some());
    assert!(private_dir(&sockets, uid + 1).is_none());
    std::fs::set_permissions(&sockets, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    assert!(private_dir(&sockets, uid).is_none());
}

/// A workspace with one source file, a private home, and the
/// environment both the server and the in-process call run with.
struct Fixture {
    home: tempfile::TempDir,
    repo: tempfile::TempDir,
    env: BTreeMap<String, String>,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().expect("home");
        let repo = tempfile::tempdir().expect("repo");
        std::fs::create_dir_all(repo.path().join(".git")).expect("git");
        for dir in ["a", "b"] {
            let dir = repo.path().join(dir);
            std::fs::create_dir_all(&dir).expect("dir");
            std::fs::write(
                dir.join("lib.rs"),
                "fn alpha() {}\nfn beta() { alpha(); }\nfn gamma() { alpha(); }\n",
            )
            .expect("source");
        }
        let env = BTreeMap::from([
            (
                "OCTOCODE_HOME".to_owned(),
                home.path().to_string_lossy().into_owned(),
            ),
            ("OCTOCODE_ENABLE_LOCAL".to_owned(), "true".to_owned()),
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
        ]);
        Self { home, repo, env }
    }

    fn dir(&self, name: &str) -> PathBuf {
        same_dir_form(&self.repo.path().join(name))
    }

    /// A directory, with the same source, whose runtime builds slowly
    /// ([`slow_build`]).
    fn slow_dir(&self) -> PathBuf {
        let slow = self.repo.path().join("slow");
        std::fs::create_dir_all(&slow).expect("slow");
        std::fs::copy(self.repo.path().join("a/lib.rs"), slow.join("lib.rs")).expect("source");
        same_dir_form(&slow)
    }

    /// The identity a client in `cwd` sends.
    fn identity(&self, cwd: &Path) -> String {
        use octocode_native::config::{RuntimeSurface, acquire_config_input, resolve_config};
        let os_home = octocode_io::env::home_dir().expect("home");
        let env = served_env(self.env.clone());
        let input =
            acquire_config_input(env.clone(), cwd.to_path_buf(), os_home, RuntimeSurface::Cli);
        call_identity(&resolve_config(&input), &env)
    }

    /// Send one call from `cwd`, with this fixture's environment and
    /// the identity a client there sends.
    async fn ask(
        &self,
        socket: &Path,
        cwd: &Path,
        tool: &str,
        input: &Value,
        json_out: bool,
    ) -> Result<Reply, Failed> {
        self.ask_as(socket, cwd, tool, input, json_out, &self.identity(cwd))
            .await
    }

    async fn ask_as(
        &self,
        socket: &Path,
        cwd: &Path,
        tool: &str,
        input: &Value,
        json_out: bool,
        identity: &str,
    ) -> Result<Reply, Failed> {
        let mut stream = connect(socket).await.expect("connect");
        let env = served_env(self.env.clone());
        let request = Request {
            tool,
            input,
            json_out,
            cwd,
            env: &env,
            identity,
        };
        exchange_reply(&mut stream, &request, REPLY_DEADLINE).await
    }

    /// The call as an in-process CLI in `cwd` prints it.
    async fn in_process(&self, cwd: &Path, tool: &str, input: Value, json_out: bool) -> Printed {
        let runtime = octocode_native::runtime::ToolRuntime::from_host(
            octocode_native::runtime::HostOptions {
                cwd: Some(cwd.to_path_buf()),
                env: Some(served_env(self.env.clone())),
                ..crate::cli::cli_host()
            },
        )
        .expect("runtime");
        let admission = runtime.admit("cli-1".into()).expect("admit");
        let query = input.clone();
        let result = runtime
            .execute(admission, tool.into(), input, crate::cli::channel(json_out))
            .await;
        runtime.close().await;
        Printed::finish(tool, result, json_out, &query)
    }
}

/// A warning a call says on stderr outside its result (here: the shared
/// GitHub rate-limit directory cannot be created) reaches the client of
/// every call that raises it, as an in-process call prints it, though
/// the server's process says it on its own stderr only once. A call in
/// flight that raised nothing relays nothing, though the raising rows
/// ran on other threads.
#[tokio::test]
async fn notices_reach_the_client_as_an_in_process_call_says_them() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
    let github = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&github)
        .await;
    let mut fixture = Fixture::new();
    fixture
        .env
        .insert("GITHUB_API_URL".into(), format!("{}/api/v3", github.uri()));
    fixture
        .env
        .insert("GITHUB_TOKEN".into(), "fixture-token".into());
    // A file where the shared rate-limit directory goes.
    std::fs::create_dir_all(fixture.home.path().join("tmp")).expect("tmp");
    std::fs::write(fixture.home.path().join("tmp/ratelimit"), "").expect("blocker");
    let cwd = fixture.dir("a");
    // Two rows: they run on threads of their own.
    let row = json!({"owner": "o", "repo": "r", "path": "README", "forceRefresh": true});
    let input = json!({"queries": [row, row]});
    // A call in flight the whole time, such as a localSearch, that
    // raises nothing itself.
    let bystander = octocode_io::notice::Scope::default();
    let scope = octocode_io::notice::Scope::default();
    let expected = scope
        .run(fixture.in_process(&cwd, "ghGetFileContent", input.clone(), true))
        .await;
    let said = scope.notices();
    assert!(
        said.iter()
            .any(|notice| notice.contains("rate-limit state")),
        "{said:?}"
    );

    let socket = fixture.home.path().join("n.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let daemon = std::rc::Rc::new(Daemon::new(socket.clone()));
    tokio::task::LocalSet::new()
        .run_until(async {
            let server_daemon = std::rc::Rc::clone(&daemon);
            let server = tokio::task::spawn_local(async move {
                serve_on(listener, &server_daemon, Duration::from_millis(300), || {}).await;
            });
            for _ in 0..2 {
                let reply = fixture
                    .ask(&socket, &cwd, "ghGetFileContent", &input, true)
                    .await
                    .expect("served");
                assert_eq!(reply.printed, expected);
                assert_eq!(reply.notices, said, "every call relays what it said");
            }
            server.await.expect("server");
        })
        .await;
    shut_down(&daemon).await;
    let home = fixture.home.path().display().to_string();
    let relayed: Vec<_> = bystander
        .notices()
        .into_iter()
        .filter(|notice| notice.contains(&home))
        .collect();
    assert!(relayed.is_empty(), "never another call's: {relayed:?}");
}

/// A server on a temporary socket answers as an in-process call would,
/// keeps one runtime per directory and environment on one pool, and
/// refuses a call it cannot run as the client would.
#[tokio::test]
async fn the_server_answers_byte_for_byte_like_an_in_process_call() {
    let mut fixture = Fixture::new();
    let socket = fixture.home.path().join("s.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let daemon = std::rc::Rc::new(Daemon::new(socket.clone()));
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let input = json!({"queries": [{"matchString": "alpha", "path": "."}]});
            // The in-process answers come first: a server idle for its
            // 300 ms while one runs would stop before the next call.
            let mut calls = Vec::new();
            for (dir, json_out) in [("a", true), ("a", false), ("b", true)] {
                let cwd = fixture.dir(dir);
                let expected = fixture.in_process(&cwd, "localSearch", input.clone(), json_out).await;
                calls.push((dir, json_out, cwd, expected));
            }
            let server_daemon = std::rc::Rc::clone(&daemon);
            let server = tokio::task::spawn_local(async move {
                serve_on(listener, &server_daemon, Duration::from_millis(300), || {}).await;
            });
            for (dir, json_out, cwd, expected) in calls {
                let served = fixture
                    .ask(&socket, &cwd, "localSearch", &input, json_out)
                    .await
                    .expect("served")
                    .printed;
                assert_eq!(served, expected, "{dir} json={json_out}");
                assert_eq!(served.exit, 0, "{}", served.body);
            }
            {
                let runtimes = daemon.runtimes.borrow();
                assert_eq!(runtimes.entries.len(), 2, "one runtime per directory");
                // Each runtime holds the server's one pool.
                assert!(std::sync::Arc::strong_count(&daemon.lsp_pool) > 2);
            }
            // Another environment (one no identity covers) is another
            // runtime, built with that environment, as in process.
            let cwd = fixture.dir("a");
            fixture.env.insert("GH_CONFIG_DIR".into(), "/accounts/b".into());
            fixture
                .ask(&socket, &cwd, "localSearch", &input, true)
                .await
                .expect("served");
            {
                let runtimes = daemon.runtimes.borrow();
                assert_eq!(runtimes.entries.len(), 3, "one runtime per environment");
                let newest = &runtimes.entries.last().expect("runtime").1;
                assert_eq!(
                    newest.runtime.config().env_value("GH_CONFIG_DIR"),
                    Some("/accounts/b")
                );
            }
            // A call under another identity (a config the call's
            // environment does not give) is refused, not run.
            let refused = fixture
                .ask_as(&socket, &cwd, "localSearch", &json!({}), true, "other")
                .await;
            assert!(
                matches!(&refused, Err(Failed::Fallback(reason)) if reason.contains("another configuration")),
                "{refused:?}"
            );
            // So is a tool that changes files.
            let refused = fixture.ask(&socket, &cwd, "ghCloneRepo", &json!({}), true).await;
            assert!(
                matches!(&refused, Err(Failed::Fallback(reason)) if reason.contains("ghCloneRepo")),
                "{refused:?}"
            );
            // Idle, the server stops serving.
            server.await.expect("server");
        })
        .await;
    shut_down(&daemon).await;
    assert!(daemon.runtimes.borrow().entries.is_empty());
}

/// The `slow` directories [`slow_build`] started a runtime build for.
static SLOW_BUILDS: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// The `slow` directories whose builds a test let finish
/// ([`finish_slow_builds`]), and the signal that wakes those builds.
static FINISHED_SLOW_BUILDS: (std::sync::Mutex<Vec<PathBuf>>, std::sync::Condvar) =
    (std::sync::Mutex::new(Vec::new()), std::sync::Condvar::new());

/// How long a slow build, or a test, waits for an event only a broken
/// server never brings, so the test fails instead of hanging.
const SLOW_BUILD_BOUND: Duration = Duration::from_secs(60);

fn slow_builds(dir: &Path) -> usize {
    let built = SLOW_BUILDS.lock().expect("builds");
    built.iter().filter(|built| *built == dir).count()
}

/// Let the builds of `dir` finish.
fn finish_slow_builds(dir: &Path) {
    let (finished, signal) = &FINISHED_SLOW_BUILDS;
    finished.lock().expect("finished").push(dir.to_path_buf());
    signal.notify_all();
}

/// Wait until `ready` holds, polling; panics past [`SLOW_BUILD_BOUND`].
async fn until(what: &str, ready: impl Fn() -> bool) {
    let started = std::time::Instant::now();
    while !ready() {
        assert!(started.elapsed() < SLOW_BUILD_BOUND, "never {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A build that, in a directory named `slow`, runs until the test lets
/// it finish ([`finish_slow_builds`]).
fn slow_build(
    host: octocode_native::runtime::HostOptions,
    pool: std::sync::Arc<octocode_engine::lsp::pool::LspClientPool>,
) -> Result<octocode_native::runtime::ToolRuntime, octocode_native::runtime::RuntimeError> {
    if let Some(cwd) = host.cwd.as_ref().filter(|cwd| cwd.ends_with("slow")) {
        SLOW_BUILDS.lock().expect("builds").push(cwd.clone());
        let (finished, signal) = &FINISHED_SLOW_BUILDS;
        let finished = finished.lock().expect("finished");
        let (_finished, waited) = signal
            .wait_timeout_while(finished, SLOW_BUILD_BOUND, |finished| {
                !finished.contains(cwd)
            })
            .expect("finished");
        assert!(!waited.timed_out(), "no test let the {cwd:?} build finish");
    }
    octocode_native::runtime::ToolRuntime::from_host_with_lsp_pool(host, pool)
}

/// A runtime builds off the thread that serves every connection: a call
/// whose runtime is ready answers while another directory's build (its
/// config, a due cache sweep) is still running, and two first calls of
/// one directory share one build. The build runs until the ready call
/// has answered, so the order holds however slow the runner is.
#[tokio::test]
async fn a_slow_runtime_build_holds_up_no_other_call() {
    let fixture = Fixture::new();
    let slow = fixture.slow_dir();
    let socket = fixture.home.path().join("b.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let daemon = std::rc::Rc::new(Daemon::with_build(socket.clone(), slow_build));
    tokio::task::LocalSet::new()
        .run_until(async {
            let server_daemon = std::rc::Rc::clone(&daemon);
            let server = tokio::task::spawn_local(async move {
                serve_on(listener, &server_daemon, Duration::from_millis(300), || {}).await;
            });
            let input = json!({"queries": [{"matchString": "alpha", "path": "."}]});
            let warm = fixture.dir("a");
            fixture
                .ask(&socket, &warm, "localSearch", &input, true)
                .await
                .expect("warm");
            let (first, second, ready) = tokio::join!(
                fixture.ask(&socket, &slow, "localSearch", &input, true),
                fixture.ask(&socket, &slow, "localSearch", &input, true),
                // A ready call that waited for the build would hold the
                // build past its bound: the slow calls then fail.
                async {
                    until("started the slow build", || slow_builds(&slow) == 1).await;
                    let reply = fixture
                        .ask(&socket, &warm, "localSearch", &input, true)
                        .await;
                    finish_slow_builds(&slow);
                    reply
                },
            );
            ready.expect("ready directory");
            first.expect("slow first");
            second.expect("slow second");
            assert_eq!(slow_builds(&slow), 1, "one build for both first calls");
            server.await.expect("server");
        })
        .await;
    shut_down(&daemon).await;
}

/// A stop request ends the accept loop: the socket goes at once (the
/// next call starts another server), and a call already taken finishes
/// with its reply.
#[tokio::test]
async fn a_stopped_server_finishes_the_calls_it_took() {
    let fixture = Fixture::new();
    let slow = fixture.slow_dir();
    // Named as a server's socket, so a stop request reaches it.
    let socket = fixture.home.path().join("serve-t.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let daemon = std::rc::Rc::new(Daemon::with_build(socket.clone(), slow_build));
    let left = std::rc::Rc::new(std::cell::Cell::new(false));
    tokio::task::LocalSet::new()
        .run_until(async {
            let (server_daemon, server_left) =
                (std::rc::Rc::clone(&daemon), std::rc::Rc::clone(&left));
            let server = tokio::task::spawn_local(async move {
                serve_on(listener, &server_daemon, IDLE_EXIT, || {
                    server_left.set(true)
                })
                .await;
            });
            let input = json!({"queries": [{"matchString": "alpha", "path": "."}]});
            let (taken, ()) = tokio::join!(
                fixture.ask(&socket, &slow, "localSearch", &input, true),
                async {
                    // The call is taken (its build runs) before the stop.
                    until("started the slow build", || slow_builds(&slow) == 1).await;
                    stop_in(socket.parent().expect("dir"));
                    until("left while its call was running", || left.get()).await;
                    finish_slow_builds(&slow);
                },
            );
            assert_eq!(taken.expect("taken call").printed.exit, 0);
            server.await.expect("server");
        })
        .await;
    shut_down(&daemon).await;
}
