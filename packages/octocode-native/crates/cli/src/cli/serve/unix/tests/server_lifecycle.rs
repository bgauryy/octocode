use super::*;

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
    let row = json!({"owner": "o", "repo": "r", "path": "README", "refresh": true});
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
            let input = json!({"queries": [{"matchString": ["alpha"], "path": "."}]});
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
            let input = json!({"queries": [{"matchString": ["alpha"], "path": "."}]});
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
                    server_left.set(true);
                })
                .await;
            });
            let input = json!({"queries": [{"matchString": ["alpha"], "path": "."}]});
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
    super::super::daemon::handle(&daemon, server, "stop-test".into()).await;
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
