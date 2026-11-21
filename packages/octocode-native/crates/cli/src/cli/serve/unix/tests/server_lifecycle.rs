use super::*;
use octocode_test_support::json_fixture;

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
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
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
    // Two rows: they run on threads of their own, each reading GitHub.
    let row = json_fixture(r#"{"owner": "o", "repo": "r", "path": "README"}"#);
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
            let input = json_fixture(r#"{"queries": [{"matchString": ["alpha"], "path": "."}]}"#);
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
                // The root is the client's directory: no root note, though
                // the server runs elsewhere.
                let notes = served.notes.unwrap_or_default();
                assert!(!notes.contains("# root:"), "{notes}");
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
            let input = json_fixture(r#"{"queries": [{"matchString": ["alpha"], "path": "."}]}"#);
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

/// A server's idle expiry counts requests only. A connection that asks
/// nothing (a starting server probing for stale sockets) neither keeps it
/// alive nor, arriving more often than the idle check, hides its silence.
#[tokio::test]
async fn connections_that_ask_nothing_do_not_keep_a_server_alive() {
    let fixture = Fixture::new();
    let socket = fixture.home.path().join("serve-idle.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let daemon = std::rc::Rc::new(Daemon::new(socket.clone()));
    tokio::task::LocalSet::new()
        .run_until(async {
            let server_daemon = std::rc::Rc::clone(&daemon);
            let server = tokio::task::spawn_local(async move {
                serve_on(listener, &server_daemon, Duration::from_millis(300), || {}).await;
            });
            let probing = async {
                loop {
                    drop(tokio::net::UnixStream::connect(&socket).await);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            };
            tokio::select! {
                left = server => left.expect("server"),
                () = probing => unreachable!("probing never ends"),
                () = tokio::time::sleep(Duration::from_secs(5)) => {
                    panic!("probes kept an idle server alive");
                }
            }
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
                    server_left.set(true);
                })
                .await;
            });
            let input = json_fixture(r#"{"queries": [{"matchString": ["alpha"], "path": "."}]}"#);
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

#[tokio::test]
async fn drain_acknowledgement_is_sent_only_after_shutdown() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (mut client, server) = tokio::net::UnixStream::pair().unwrap();
    let daemon = Daemon::new(std::path::PathBuf::new());
    client
        .write_all(b"{\"stop\":true,\"drain\":true}\n")
        .await
        .unwrap();
    super::super::daemon::handle(&daemon, server, "stop-test".into(), || {}).await;
    let mut byte = [0];
    assert!(
        tokio::time::timeout(Duration::from_millis(20), client.read(&mut byte))
            .await
            .is_err()
    );
    shut_down(&daemon).await;
    let mut reply = String::new();
    client.read_to_string(&mut reply).await.unwrap();
    assert_eq!(reply, "{\"stopped\":true}\n");
}

/// A server answers a call only under its own process environment (the
/// variables the engine reads from it, which no call can bring): a request
/// made under another is refused with a reason, and the same request under
/// the server's own is served.
#[tokio::test]
async fn a_request_made_under_another_process_environment_falls_back() {
    let fixture = Fixture::new();
    let socket = fixture.home.path().join("p.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let daemon = std::rc::Rc::new(Daemon::new(socket.clone()));
    tokio::task::LocalSet::new()
        .run_until(async {
            let server_daemon = std::rc::Rc::clone(&daemon);
            let server = tokio::task::spawn_local(async move {
                serve_on(listener, &server_daemon, Duration::from_millis(300), || {}).await;
            });
            let input = json_fixture(r#"{"queries": [{"matchString": ["alpha"], "path": "."}]}"#);
            let cwd = fixture.dir("a");
            let env = served_env(fixture.env.clone());
            let identity = fixture.identity(&cwd);
            let own = own_process_identity();
            for (process, served) in [("another", false), (own.as_str(), true)] {
                let mut stream = connect(&socket).await.expect("connect");
                let request = Request {
                    tool: "localSearch",
                    input: &input,
                    json_out: true,
                    cwd: &cwd,
                    env: &env,
                    identity: &identity,
                    process,
                    surface: Surface::Cli,
                };
                let reply = exchange_reply(&mut stream, &request, reply_deadline()).await;
                if served {
                    reply.expect("served under its own process environment");
                } else {
                    assert!(
                        matches!(&reply, Err(Failed::Fallback(reason)) if reason.contains("another process environment")),
                        "{reply:?}"
                    );
                }
            }
            server.await.expect("server");
        })
        .await;
    shut_down(&daemon).await;
}

/// An MCP request gets a runtime of its own beside the CLI's for one
/// directory, answers as an MCP result carrying the bytes an in-process MCP
/// server sends, and keeps MCP's policy (`DISABLE_TOOLS` removes a tool the
/// CLI still runs).
#[tokio::test]
async fn an_mcp_request_is_its_own_runtime_with_mcp_policy_and_bytes() {
    let mut fixture = Fixture::new();
    let socket = fixture.home.path().join("m.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let daemon = std::rc::Rc::new(Daemon::new(socket.clone()));
    tokio::task::LocalSet::new()
        .run_until(async {
            let input = json_fixture(r#"{"queries": [{"matchString": ["alpha"], "path": "."}]}"#);
            let cwd = fixture.dir("a");
            // The in-process answers come first: a server idle for its
            // 300 ms while one runs would stop before the next call.
            let (expected, _) = fixture
                .in_process_mcp(&cwd, "localSearch", input.clone())
                .await;
            fixture
                .env
                .insert("DISABLE_TOOLS".into(), "localSearch".into());
            let (refused, refused_error) = fixture
                .in_process_mcp(&cwd, "localSearch", input.clone())
                .await;
            assert!(
                refused_error && refused.contains("DISABLE_TOOLS"),
                "{refused}"
            );
            fixture.env.remove("DISABLE_TOOLS");
            let server_daemon = std::rc::Rc::clone(&daemon);
            let server = tokio::task::spawn_local(async move {
                serve_on(listener, &server_daemon, Duration::from_millis(300), || {}).await;
            });
            let served = fixture
                .ask_mcp(&socket, &cwd, "localSearch", &input)
                .await
                .expect("served");
            assert_eq!(served["mcp"]["text"], json!(expected), "the bytes");
            assert_eq!(served["mcp"]["isError"], false);
            // The CLI of that directory is another runtime on the server.
            fixture
                .ask(&socket, &cwd, "localSearch", &input, true)
                .await
                .expect("the CLI");
            assert_eq!(daemon.runtimes.borrow().entries.len(), 2, "one per surface");
            // MCP's policy holds on the server; the CLI ignores it.
            fixture
                .env
                .insert("DISABLE_TOOLS".into(), "localSearch".into());
            let served = fixture
                .ask_mcp(&socket, &cwd, "localSearch", &input)
                .await
                .expect("served");
            assert_eq!(served["mcp"]["isError"], true, "{served}");
            assert_eq!(served["mcp"]["text"], json!(refused), "the same refusal");
            let cli = fixture
                .ask(&socket, &cwd, "localSearch", &input, true)
                .await
                .expect("the CLI")
                .printed;
            assert_eq!(cli.exit, 0, "{}", cli.body);
            server.await.expect("server");
        })
        .await;
    shut_down(&daemon).await;
}

/// Under `output.mcpResult: text` the server answers an MCP request with
/// the text an in-process MCP server sends, from the same function: the
/// setting is the session's, which the request's environment carries.
#[tokio::test]
async fn an_mcp_request_is_answered_in_the_sessions_result_format() {
    let mut fixture = Fixture::new();
    fixture
        .env
        .insert("OCTOCODE_MCP_RESULT".into(), "text".into());
    let socket = fixture.home.path().join("t.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let daemon = std::rc::Rc::new(Daemon::new(socket.clone()));
    tokio::task::LocalSet::new()
        .run_until(async {
            let input = json_fixture(r#"{"queries": [{"matchString": ["alpha"], "path": "."}]}"#);
            let cwd = fixture.dir("a");
            // The in-process answer comes first: a server idle for its
            // 300 ms while one runs would stop before the call.
            let runtime = octocode_native::runtime::ToolRuntime::from_host(
                octocode_native::runtime::HostOptions {
                    cwd: Some(cwd.clone()),
                    env: Some(served_env(fixture.env.clone())),
                    ..crate::cli::mcp_server::mcp_host()
                },
            )
            .expect("runtime");
            let format = runtime.config().resolved.output.mcp_result;
            assert_eq!(format, octocode_runtime_base::config::OutputMcpResult::Text);
            let admission = runtime.admit_with("mcp-1".into(), None).expect("admit");
            let result = runtime
                .execute(
                    admission,
                    "localSearch".into(),
                    input.clone(),
                    octocode_native::runtime::Channel::Mcp,
                )
                .await;
            runtime.close().await;
            let expected = crate::cli::mcp_server::reply("localSearch", result, format, &input)
                .expect("answered");
            assert!(
                serde_json::from_str::<Value>(&expected.text).is_err(),
                "the text view, not JSON: {}",
                expected.text
            );
            let server_daemon = std::rc::Rc::clone(&daemon);
            let server = tokio::task::spawn_local(async move {
                serve_on(listener, &server_daemon, Duration::from_millis(300), || {}).await;
            });
            let served = fixture
                .ask_mcp(&socket, &cwd, "localSearch", &input)
                .await
                .expect("served");
            assert_eq!(served["mcp"]["text"], json!(expected.text), "the bytes");
            assert_eq!(served["mcp"]["isError"], json!(expected.is_error));
            server.await.expect("server");
        })
        .await;
    shut_down(&daemon).await;
}
