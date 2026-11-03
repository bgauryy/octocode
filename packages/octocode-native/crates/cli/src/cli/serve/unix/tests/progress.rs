use super::*;
use crate::cli::serve::unix::frames::{forward, progress_scope};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const REPLY: &[u8] = b"{\"exit\":0,\"stderr\":false,\"body\":\"{}\",\"json\":true}\n";

/// A call that raises two progress lines, the second only after the client
/// has shown the first, reaches the client as two frames in order, both
/// before the reply that ends the exchange: the server writes them while
/// the call runs, and the client prints them as they arrive.
#[tokio::test]
async fn progress_frames_reach_the_client_while_the_call_runs_and_before_its_reply() {
    let (mut client, server) = tokio::net::UnixStream::pair().expect("pair");
    let (shown, shown_first) = tokio::sync::oneshot::channel::<()>();
    let mut shown = Some(shown);
    let mut heard: Vec<String> = Vec::new();

    let daemon = async move {
        let (read, mut write) = server.into_split();
        let mut request = String::new();
        BufReader::new(read)
            .read_line(&mut request)
            .await
            .expect("request");
        let (scope, mut lines) = progress_scope();
        let call = scope.run(async {
            let say = octocode_io::notice::progress_sink().expect("a listener");
            say("waiting 3s for quota");
            // The call cannot go on until the client has shown the line.
            shown_first.await.expect("client showed the first line");
            say("quota restored");
        });
        forward(call, &mut lines, &mut write).await;
        assert!(
            scope.notices().is_empty(),
            "progress is not also relayed as a notice"
        );
        write.write_all(REPLY).await.expect("reply");
    };
    let input = json!({});
    let request = request(&input, Path::new("/w"));
    let asking = exchange(
        &mut client,
        &request,
        REPLY_DEADLINE,
        std::future::pending(),
        std::future::pending::<()>(),
        || {},
        |line| {
            heard.push(line.to_owned());
            if let Some(shown) = shown.take() {
                let _ = shown.send(());
            }
        },
    );
    let (reply, ()) = tokio::join!(asking, daemon);
    let reply = reply.expect("the reply ends the exchange");
    assert_eq!(reply.printed.exit, 0);
    assert!(reply.notices.is_empty());
    assert_eq!(heard, ["waiting 3s for quota", "quota restored"]);
}

/// A call that says nothing sends the reply alone: no frame, same reply.
#[tokio::test]
async fn a_call_without_progress_sends_the_reply_alone() {
    let (mut write, mut read) = tokio::io::duplex(1024);
    let (_scope, mut lines) = progress_scope();
    let result = forward(async { 7 }, &mut lines, &mut write).await;
    assert_eq!(result, 7);
    drop(write);
    let mut sent = String::new();
    BufReader::new(&mut read)
        .read_line(&mut sent)
        .await
        .expect("read");
    assert_eq!(sent, "", "nothing was written before the reply");
}
