//! Connections that stall: a peer that never asks, never reads, or leaves
//! while a write waits holds up neither the call, the server's exit nor its
//! shutdown.

use super::*;

/// How long a test waits for what only a stalled server never does.
const BOUND: Duration = Duration::from_secs(5);

/// A server that is asked to stop leaves although a client connected and
/// never sent its request: a connection that asked nothing has no call to
/// drain.
#[tokio::test]
async fn a_stop_ends_a_server_whose_client_never_asked() {
    let fixture = Fixture::new();
    let socket = fixture.home.path().join("serve-stall.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let daemon = std::rc::Rc::new(Daemon::new(socket.clone()));
    tokio::task::LocalSet::new()
        .run_until(async {
            let server_daemon = std::rc::Rc::clone(&daemon);
            let server = tokio::task::spawn_local(async move {
                serve_on(listener, &server_daemon, IDLE_EXIT, || {}).await;
            });
            // Connected, silent, and kept open for the whole test.
            let _silent = tokio::net::UnixStream::connect(&socket)
                .await
                .expect("connect");
            tokio::time::sleep(Duration::from_millis(50)).await;
            stop_in(socket.parent().expect("dir"));
            tokio::time::timeout(BOUND, server)
                .await
                .expect("a silent connection held up the stop")
                .expect("server");
        })
        .await;
    shut_down(&daemon).await;
}

/// A server with nothing asked of it leaves at its idle expiry although a
/// client stays connected without sending a request.
#[tokio::test]
async fn a_connection_that_never_asks_does_not_keep_a_server_alive() {
    let fixture = Fixture::new();
    let socket = fixture.home.path().join("serve-silent.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let daemon = std::rc::Rc::new(Daemon::new(socket.clone()));
    tokio::task::LocalSet::new()
        .run_until(async {
            let server_daemon = std::rc::Rc::clone(&daemon);
            let server = tokio::task::spawn_local(async move {
                serve_on(listener, &server_daemon, Duration::from_millis(300), || {}).await;
            });
            let _silent = tokio::net::UnixStream::connect(&socket)
                .await
                .expect("connect");
            tokio::time::timeout(BOUND, server)
                .await
                .expect("a silent connection kept an idle server alive")
                .expect("server");
        })
        .await;
    shut_down(&daemon).await;
}

/// A server draining its calls tells the client waiting for it (`cache
/// clear`) how many it waits for, then says it is alive for as long as the
/// drain runs, then that it stopped: the frames come whole and in order.
#[tokio::test]
async fn a_draining_server_keeps_telling_its_waiter_it_is_alive() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (mut client, server) = tokio::net::UnixStream::pair().expect("pair");
    let mut daemon = Daemon::new(PathBuf::new());
    daemon.pace.heartbeat = Duration::from_millis(20);
    client
        .write_all(b"{\"stop\":true,\"drain\":true}\n")
        .await
        .expect("stop");
    assert!(handle(&daemon, server, "stop".into(), || {}).await);
    daemon.tell_draining(1).await;
    daemon
        .telling_waiters(tokio::time::sleep(Duration::from_millis(150)))
        .await;
    shut_down(&daemon).await;
    let mut heard = String::new();
    client.read_to_string(&mut heard).await.expect("read");
    let lines: Vec<&str> = heard.lines().collect();
    assert_eq!(lines.first(), Some(&"{\"draining\":1}"));
    assert_eq!(lines.last(), Some(&"{\"stopped\":true}"));
    let between = lines.get(1..lines.len() - 1).unwrap_or_default();
    assert!(
        !between.is_empty(),
        "no heartbeat during the drain: {heard}"
    );
    assert!(
        between.iter().all(|line| *line == "{\"alive\":true}"),
        "{heard}"
    );
}

/// A server that accepts a request and never answers is left behind by the
/// call that waited on it: the next call finds no socket there and starts a
/// new server, instead of waiting out the same silence again. The wedged
/// process is not touched.
#[tokio::test]
async fn a_server_that_went_silent_is_not_reconnected_to() {
    let fixture = Fixture::new();
    let socket = fixture.home.path().join("serve-wedged.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind");
    // Accepts, reads nothing, writes nothing, stays open.
    let wedged = std::thread::spawn(move || {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            held.push(stream);
        }
    });
    let silence = Duration::from_millis(100);
    let input = json!({});
    let bound = socket_identity(&socket);
    let mut stream = connect(&socket).await.expect("first call connects");
    let first = exchange(
        &mut stream,
        &request(&input, Path::new("/w")),
        None,
        silence,
        std::future::pending(),
        std::future::pending::<()>(),
        || {},
        |_| {},
    )
    .await;
    assert_eq!(first, Err(Failed::Stalled(silence)));
    super::client::discard_stalled(&socket, bound);
    assert!(
        connect(&socket).await.is_none(),
        "the second call does not reconnect to the wedged server"
    );
    // A successor that took the path keeps it.
    let successor = std::os::unix::net::UnixListener::bind(&socket).expect("rebind");
    // (A filesystem may reuse the inode: then the two are not told apart.)
    if socket_identity(&socket) != bound {
        super::client::discard_stalled(&socket, bound);
        assert!(connect(&socket).await.is_some(), "a live successor stays");
    }
    drop(successor);
    drop(stream);
    // The wedged thread ends when its listener drops with the process; it holds
    // nothing this test must stop.
    drop(wedged);
}
