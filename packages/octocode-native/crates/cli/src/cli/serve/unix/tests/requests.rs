use super::*;

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

pub(super) async fn exchange_reply(
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
            .write_all(b"{\"exit\":1,\"stderr\":false,\"body\":\"{}\",\"json\":true}\n")
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
            stream: crate::cli::output::Stream::Stdout,
            body: "{}".into(),
            json: true,
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
fn tool_transport_keeps_writing_tools_in_the_invoking_process() {
    for tool in ToolId::ALL {
        let process_local = matches!(tool, ToolId::GhCloneRepo | ToolId::GraphBuild);
        assert_eq!(runs_on_server(tool), !process_local, "{tool}");
    }
}
