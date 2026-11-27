//! What a server's start waits for: `initialize` outwaits a server that keeps
//! working, a start no caller waits for any more stops its server, and the
//! start's disk reads leave the runtime serving everything else.

use super::document_sync::{read_client_frame, send_server_frame};
use super::*;
use crate::lsp::transport::ClientRequestContext;
use serde_json::json;
use tokio::io::{BufReader, DuplexStream};

/// The base `initialize` waits a server that shows no sign of work.
fn base() -> Duration {
    Duration::from_millis(u64::from(REQUEST_TIMEOUT_MS))
}

/// A connection on `clock` wired to an in-memory server's two pipe ends.
fn connection_to_fake(
    clock: &TestClock,
) -> (
    Arc<JsonRpcConnection>,
    BufReader<DuplexStream>,
    DuplexStream,
) {
    let (client_w, server_r) = tokio::io::duplex(64 * 1024);
    let (server_w, client_r) = tokio::io::duplex(64 * 1024);
    let connection = JsonRpcConnection::new(
        client_r,
        client_w,
        ClientRequestContext {
            configuration: json!({}),
            section_root: None,
            workspace_folders: json!([]),
        },
        ProgressTracker::new(clock.lsp()),
        None,
        clock.lsp(),
    );
    (Arc::new(connection), BufReader::new(server_r), server_w)
}

/// `initialize` sent the way a supervised start sends it: in a task of its
/// own, outside every caller's scope.
fn supervised_initialize(
    connection: &Arc<JsonRpcConnection>,
    clock: &TestClock,
) -> tokio::task::JoinHandle<crate::error::Result<Value>> {
    let connection = Arc::clone(connection);
    let clock = clock.lsp();
    tokio::spawn(async move {
        super::super::lifecycle::initialize(&connection, &ts_config(), &clock).await
    })
}

/// A server that works inside `initialize` (jdtls or Metals importing a large
/// build) and keeps saying so is waited for past twice the base: the start
/// has no fixed ceiling of its own.
#[test]
fn initialize_outwaits_a_server_that_keeps_working() {
    paused(|clock| async move {
        let (connection, mut reader, mut writer) = connection_to_fake(&clock);
        let started = clock.now();
        let initialize = supervised_initialize(&connection, &clock);
        let request = read_client_frame(&mut reader).await;
        assert_eq!(request["method"], "initialize");
        while clock.elapsed(started) < base() * 3 {
            clock.sleep(base() / 30).await;
            send_server_frame(
                &mut writer,
                json!({"jsonrpc":"2.0","method":"window/logMessage","params":{"type":4,"message":"importing the build"}}),
            )
            .await;
        }
        send_server_frame(
            &mut writer,
            json!({"jsonrpc":"2.0","id":request["id"],"result":{"capabilities":{"hoverProvider":true}}}),
        )
        .await;
        let answer = initialize
            .await
            .expect("join")
            .expect("a server that keeps working is waited for");
        assert_eq!(answer["capabilities"]["hoverProvider"], true);
        assert!(clock.elapsed(started) >= base() * 3);
        assert!(connection.is_alive());
    });
}

/// A server that shows no sign of work still fails `initialize` at the base.
#[test]
fn initialize_fails_at_the_base_when_the_server_shows_nothing() {
    paused(|clock| async move {
        let (connection, mut reader, _writer) = connection_to_fake(&clock);
        let started = clock.now();
        let initialize = supervised_initialize(&connection, &clock);
        assert_eq!(read_client_frame(&mut reader).await["method"], "initialize");
        let error = initialize
            .await
            .expect("join")
            .expect_err("an idle server fails");
        assert_eq!(error.kind(), &crate::error::ErrorKind::Timeout);
        assert_eq!(clock.elapsed(started), base());
    });
}

/// Whether process `pid` still runs.
#[cfg(unix)]
fn running(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// A start whose caller left while the server still worked inside
/// `initialize` ends there: the pool stops the server instead of waiting on
/// it for no one.
#[cfg(unix)]
#[test]
fn a_start_no_caller_waits_for_stops_its_server() {
    if !node_available() {
        return;
    }
    let tmp = TmpDir::new();
    let pid_file = tmp.0.join("server.pid");
    let (_server_dir, _root, config) = configurable_server(
        "abandoned-start",
        &[
            ("FAKE_INIT_DELAY_MS", "600000".to_owned()),
            ("FAKE_INIT_CHATTER_MS", "50".to_owned()),
            ("FAKE_PID_FILE", pid_file.to_string_lossy().into_owned()),
        ],
        None,
    );
    tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(async {
            let pool = Arc::new(crate::lsp::pool::LspClientPool::default());
            let caller = tokio::spawn({
                let pool = Arc::clone(&pool);
                async move { pool.acquire(config).await }
            });
            let began = std::time::Instant::now();
            let pid = loop {
                if let Ok(pid) = std::fs::read_to_string(&pid_file)
                    && !pid.is_empty()
                {
                    break pid;
                }
                assert!(
                    began.elapsed() < Duration::from_secs(60),
                    "the server never started"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            };
            assert!(running(&pid));
            caller.abort();
            let _ = caller.await;
            let left = std::time::Instant::now();
            while running(&pid) {
                assert!(
                    left.elapsed() < Duration::from_secs(20),
                    "the server still runs {:?} after its caller left",
                    left.elapsed()
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            pool.clear_all().await;
        });
}

/// The start reads clangd's compilation database (here a pipe the test
/// writes only once another task of the runtime has run) on a blocking
/// thread: a large database never stalls the reads, writes and timers of
/// every other connection on the runtime thread.
#[cfg(unix)]
#[test]
fn reading_the_compile_database_leaves_the_runtime_serving() {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    if !node_available() {
        return;
    }
    let (_tmp, root, mut config) = configurable_server("database-read", &[], None);
    let database = root.join("compile_commands.json");
    let made = std::process::Command::new("mkfifo")
        .arg(&database)
        .status()
        .expect("mkfifo");
    assert!(made.success());
    config.args = Some(vec![
        format!("--compile-commands-dir={}", root.join("mirror").display()),
        "--background-index".to_owned(),
    ]);
    config.env.get_or_insert_with(Default::default).insert(
        "CLANGD_MIRRORED_DATABASE".to_owned(),
        database.to_string_lossy().into_owned(),
    );
    let reading = Arc::new(AtomicBool::new(false));
    let (ran, ran_rx) = std::sync::mpsc::channel::<()>();
    let writer = std::thread::spawn({
        let reading = Arc::clone(&reading);
        move || {
            let began = std::time::Instant::now();
            // Opens once the start has the database open to read it.
            let mut pipe = loop {
                match std::fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&database)
                {
                    Ok(pipe) => break pipe,
                    Err(error) => {
                        assert!(
                            began.elapsed() < Duration::from_secs(60),
                            "the start never read the database: {error}"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            };
            reading.store(true, Ordering::SeqCst);
            // The start now waits for the database: another task of its
            // runtime runs meanwhile, or never does until it is written.
            let served = ran_rx.recv_timeout(Duration::from_secs(30)).is_ok();
            pipe.write_all(b"[]").expect("database written");
            served
        }
    });
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let client = NativeLspClient::new(config);
            let start = tokio::spawn({
                let client = client.clone();
                async move { client.start().await }
            });
            let other = tokio::spawn(async move {
                while !reading.load(Ordering::SeqCst) {
                    tokio::task::yield_now().await;
                }
                let _ = ran.send(());
            });
            start.await.expect("join").expect("the server starts");
            other.await.expect("join");
            let _ = client.stop().await;
        });
    assert!(
        writer.join().expect("writer"),
        "no other task of the runtime ran while the start read the compile database"
    );
}
