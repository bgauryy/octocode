use super::*;
use octocode_test_support::json_fixture;

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
    deadline: Option<Duration>,
) -> Result<Printed, Failed> {
    exchange_reply(client, request, deadline)
        .await
        .map(|reply| reply.printed)
}

pub(super) async fn exchange_reply(
    client: &mut tokio::net::UnixStream,
    request: &Request<'_>,
    deadline: Option<Duration>,
) -> Result<Reply, Failed> {
    exchange(
        client,
        request,
        deadline,
        octocode_io::http::READ_STALL,
        std::future::pending(),
        std::future::pending::<()>(),
        || {},
        |_| {},
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
            .write_all(b"{\"exit\":1,\"body\":\"{}\",\"json\":true}\n")
            .await
            .expect("reply");
        request
    });
    let input = json_fixture(r#"{"q": 1}"#);
    let request = request(&input, Path::new("/work/here"));
    let reply = exchange_plain(&mut client, &request, reply_deadline()).await;
    assert_eq!(
        reply,
        Ok(Printed {
            exit: 1,
            body: "{}".into(),
            json: true,
            error_text: None,
            notes: None,
            continued: None,
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
        exchange_plain(&mut client, &request, reply_deadline()).await,
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
        exchange_plain(&mut client, &request, reply_deadline()).await,
        Err(Failed::Fallback(
            "it runs under another configuration".into()
        ))
    );
    peer.await.expect("peer");

    let (mut client, _silent) = tokio::net::UnixStream::pair().expect("pair");
    assert_eq!(
        exchange_plain(&mut client, &request, Some(Duration::from_millis(50))).await,
        Err(Failed::Deadline)
    );
    // B75: the deadline follows the server's request budget, past it when
    // the server has one; the CLI's servers have none, so neither does the
    // client.
    let budget = Duration::from_secs(60);
    assert!(reply_deadline_for(Some(budget)).is_some_and(|wait| wait > budget));
    assert_eq!(reply_deadline_for(None), None);
    assert_eq!(reply_deadline(), None);
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
        reply_deadline(),
        octocode_io::http::READ_STALL,
        std::future::ready(()),
        std::future::pending::<()>(),
        || noticed.set(true),
        |_| {},
    )
    .await;
    assert_eq!(
        drained,
        Err(Failed::Interrupted(
            crate::cli::tool_call::Interrupted::Drained
        ))
    );
    assert!(noticed.get(), "the cancel notice is printed");
    assert!(peer.await.expect("peer"), "the server saw only the close");

    // A server that keeps working is abandoned at the second Ctrl-C.
    let (mut client, _busy) = tokio::net::UnixStream::pair().expect("pair");
    let forced = exchange(
        &mut client,
        &request,
        reply_deadline(),
        octocode_io::http::READ_STALL,
        std::future::ready(()),
        std::future::ready(()),
        || {},
        |_| {},
    )
    .await;
    assert_eq!(
        forced,
        Err(Failed::Interrupted(
            crate::cli::tool_call::Interrupted::Forced
        ))
    );
}

/// B75: with no reply deadline, a silent server is still left at Ctrl-C:
/// the first closes the request side, the second stops waiting.
#[tokio::test]
async fn ctrl_c_ends_a_reply_wait_that_has_no_deadline() {
    let input = json!({});
    let request = request(&input, Path::new("/w"));
    let (mut client, _silent) = tokio::net::UnixStream::pair().expect("pair");
    let ended = tokio::time::timeout(
        Duration::from_secs(5),
        exchange(
            &mut client,
            &request,
            None,
            octocode_io::http::READ_STALL,
            std::future::ready(()),
            tokio::time::sleep(Duration::from_millis(50)),
            || {},
            |_| {},
        ),
    )
    .await
    .expect("Ctrl-C ends the wait");
    assert_eq!(
        ended,
        Err(Failed::Interrupted(
            crate::cli::tool_call::Interrupted::Forced
        ))
    );
}

/// A tool the MCP listing marks read-only runs on the warm server; one that
/// writes the workspace (`readOnlyHint: false`) stays in the invoking
/// process. Both follow the contract's `mutatesWorkspace`.
#[test]
fn tool_transport_keeps_writing_tools_in_the_invoking_process() {
    let presentation = octocode_runtime_base::contracts::cli_presentation().expect("presentation");
    let listed = presentation["tools"]
        .as_array()
        .expect("presentation tools");
    let mut writers = 0;
    for tool in ToolId::ALL {
        let entry = listed
            .iter()
            .find(|entry| entry["name"] == tool.as_str())
            .unwrap_or_else(|| panic!("{tool} has a presentation entry"));
        let read_only = entry["mcp"]["annotations"]["readOnlyHint"]
            .as_bool()
            .unwrap_or_else(|| panic!("{tool} declares readOnlyHint"));
        assert_eq!(runs_on_server(tool), read_only, "{tool}");
        writers += usize::from(!read_only);
    }
    assert!(writers > 0, "some tool writes the workspace");
}

/// With no reply deadline, a server that says nothing for the silence is
/// gone and the call falls back; one that keeps sending heartbeat or
/// progress frames for longer than the silence is waited for, and its
/// heartbeats are not printed.
#[tokio::test]
async fn the_reply_wait_ends_on_silence_but_not_while_frames_arrive() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let silence = Duration::from_millis(100);
    let input = json!({});
    let request = &request(&input, Path::new("/w"));
    let ask = |client| async move {
        let mut client = client;
        let mut heard = Vec::new();
        let reply = exchange(
            &mut client,
            request,
            None,
            silence,
            std::future::pending(),
            std::future::pending::<()>(),
            || {},
            |line| heard.push(line.to_owned()),
        )
        .await;
        (reply, heard)
    };

    let (client, _silent) = tokio::net::UnixStream::pair().expect("pair");
    let (reply, _) = tokio::time::timeout(Duration::from_secs(5), ask(client))
        .await
        .expect("a silent server held the client");
    assert!(
        matches!(&reply, Err(Failed::Stalled(waited)) if *waited == silence),
        "{reply:?}"
    );

    let (client, server) = tokio::net::UnixStream::pair().expect("pair");
    let peer = async move {
        let mut server = BufReader::new(server);
        let mut line = String::new();
        server.read_line(&mut line).await.expect("request");
        // Four silences long, never silent for one.
        for at in 0..16 {
            tokio::time::sleep(silence / 4).await;
            let frame = if at == 8 {
                "{\"progress\":\"halfway\"}\n"
            } else {
                "{\"alive\":true}\n"
            };
            server
                .get_mut()
                .write_all(frame.as_bytes())
                .await
                .expect("frame");
        }
        server
            .get_mut()
            .write_all(b"{\"exit\":0,\"body\":\"{}\",\"json\":true}\n")
            .await
            .expect("reply");
    };
    let ((reply, heard), ()) = tokio::join!(ask(client), peer);
    assert_eq!(reply.expect("the reply").printed.exit, 0);
    assert_eq!(heard, ["halfway"]);
}
