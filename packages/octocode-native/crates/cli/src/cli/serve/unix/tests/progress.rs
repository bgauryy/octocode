use super::*;
use crate::cli::serve::unix::daemon::request::attended;
use crate::cli::serve::unix::frames::{
    ALIVE, BACKLOG, Backlog, Frames, PROGRESS, Pace, progress_scope,
};
use octocode_test_support::json_fixture;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn reply() -> Value {
    json_fixture(r#"{"exit": 0, "body": "{}", "json": true}"#)
}

/// A pace whose heartbeat never comes within a test and whose silence ends
/// a stalled write quickly.
const QUICK: Pace = Pace {
    heartbeat: Duration::from_secs(3600),
    silence: Duration::from_millis(100),
};

/// How long a test waits for what only a stalled server never does.
const BOUND: Duration = Duration::from_secs(5);

/// Raise `line` as the running call's progress.
fn say(line: &str) {
    octocode_io::notice::progress_sink().expect("a listener")(line);
}

/// Every line `read` has, parsed, until it closes.
async fn lines(read: impl tokio::io::AsyncRead + Unpin) -> Vec<Value> {
    let mut read = BufReader::new(read);
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        if read.read_line(&mut line).await.expect("read") == 0 {
            return lines;
        }
        lines.push(serde_json::from_str(&line).expect("each line is whole JSON"));
    }
}

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
        let (read, write) = server.into_split();
        let mut request = String::new();
        BufReader::new(read)
            .read_line(&mut request)
            .await
            .expect("request");
        let (scope, backlog) = progress_scope();
        let call = scope.run(async {
            say("waiting 3s for quota");
            // The call cannot go on until the client has shown the line.
            shown_first.await.expect("client showed the first line");
            say("quota restored");
        });
        let mut frames = Frames::new(write, Pace::SERVED);
        let mut call = std::pin::pin!(call);
        frames.run(call.as_mut(), &backlog).await.expect("ran");
        frames.drain(&backlog).await.expect("drained");
        assert!(
            scope.notices().is_empty(),
            "progress is not also relayed as a notice"
        );
        frames.send(&reply()).await.expect("reply");
    };
    let input = json!({});
    let request = request(&input, Path::new("/w"));
    let asking = exchange(
        &mut client,
        &request,
        reply_deadline(),
        octocode_io::http::READ_STALL,
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
    let (write, read) = tokio::io::duplex(1024);
    let (_scope, backlog) = progress_scope();
    let mut frames = Frames::new(write, Pace::SERVED);
    let work = std::pin::pin!(async { 7 });
    assert_eq!(frames.run(work, &backlog).await, Ok(7));
    frames.drain(&backlog).await.expect("nothing to drain");
    drop(frames);
    assert!(lines(read).await.is_empty(), "nothing was written");
}

/// A client that stops reading fills the socket, but the call still runs to
/// its end: a write that waits for the client never stops the call being
/// polled. Then the frame in flight, the rest of the progress and the
/// reply reach the client whole and in order once it reads.
#[tokio::test]
async fn a_client_that_stops_reading_does_not_stop_the_call() {
    let (write, read) = tokio::io::duplex(1);
    let (scope, backlog) = progress_scope();
    let line = "x".repeat(64);
    let finished = std::cell::Cell::new(false);
    let call = scope.run(async {
        say(&line);
        tokio::time::sleep(Duration::from_millis(10)).await;
        say("done");
        finished.set(true);
        7
    });
    let mut frames = Frames::new(write, Pace::SERVED);
    let mut call = std::pin::pin!(call);
    let ran = tokio::time::timeout(BOUND, frames.run(call.as_mut(), &backlog))
        .await
        .expect("a full socket held up the call");
    assert_eq!(ran, Ok(7));
    assert!(finished.get());
    let reading = tokio::spawn(lines(read));
    frames.drain(&backlog).await.expect("drained");
    frames.send(&reply()).await.expect("reply");
    drop(frames);
    let heard = reading.await.expect("reader");
    assert_eq!(
        heard,
        [
            json!({ PROGRESS: line }),
            json!({ PROGRESS: "done" }),
            reply()
        ]
    );
}

/// A client slower than the call's progress gets the newest lines, at most
/// [`BACKLOG`] beside the one in flight, in order, and the reply last:
/// progress queued for it neither grows without bound nor blocks the call.
#[tokio::test]
async fn progress_for_a_slow_client_keeps_the_newest_lines() {
    let (write, read) = tokio::io::duplex(1);
    let (scope, backlog) = progress_scope();
    let raised = 3 * BACKLOG;
    let call = scope.run(async {
        for at in 0..raised {
            say(&format!("line {at}"));
            tokio::task::yield_now().await;
        }
    });
    let mut frames = Frames::new(write, Pace::SERVED);
    let mut call = std::pin::pin!(call);
    tokio::time::timeout(BOUND, frames.run(call.as_mut(), &backlog))
        .await
        .expect("a slow client held up the call")
        .expect("ran");
    let reading = tokio::spawn(lines(read));
    frames.drain(&backlog).await.expect("drained");
    frames.send(&reply()).await.expect("reply");
    drop(frames);
    let mut heard = reading.await.expect("reader");
    assert_eq!(heard.pop(), Some(reply()), "the reply comes last");
    let numbers: Vec<usize> = heard
        .iter()
        .map(|frame| {
            let text = frame[PROGRESS].as_str().expect("a progress frame");
            text.trim_start_matches("line ").parse().expect("number")
        })
        .collect();
    assert!(numbers.len() <= BACKLOG + 1, "{} frames", numbers.len());
    assert!(numbers.windows(2).all(|pair| pair[0] < pair[1]), "in order");
    assert_eq!(
        numbers.last(),
        Some(&(raised - 1)),
        "the newest line is kept"
    );
}

/// A long call that says nothing still sends a heartbeat frame, which the
/// client counts as the server being alive and does not print; a server's
/// heartbeats come several times within the silence a client waits through.
#[tokio::test]
async fn a_silent_call_sends_heartbeats() {
    let served = Pace::SERVED;
    assert!(served.heartbeat * 2 <= served.silence);
    assert_eq!(served.silence, octocode_io::http::READ_STALL);
    let (write, read) = tokio::io::duplex(1024);
    let backlog = Backlog::default();
    let pace = Pace {
        heartbeat: Duration::from_millis(20),
        silence: BOUND,
    };
    let mut frames = Frames::new(write, pace);
    let work = std::pin::pin!(tokio::time::sleep(Duration::from_millis(110)));
    frames.run(work, &backlog).await.expect("ran");
    frames.drain(&backlog).await.expect("drained");
    drop(frames);
    let heard = lines(read).await;
    assert!(!heard.is_empty(), "no heartbeat");
    assert!(heard.iter().all(|frame| *frame == json!({ ALIVE: true })));
}

/// A client that leaves (Ctrl-C half-closes its side) while the server's
/// write waits for it cancels the call at once: the waiting frame is
/// dropped, the cancelled work drains, and no reply is due. A client that
/// stays connected but takes no byte is gone after the pace's silence.
#[tokio::test]
async fn a_client_that_leaves_during_a_blocked_write_cancels_its_call() {
    let (mut client, server) = tokio::io::duplex(1);
    let (mut read, write) = tokio::io::split(server);
    let (scope, backlog) = progress_scope();
    let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();
    let work = scope.run(async {
        say(&"x".repeat(64));
        let _ = cancelled.await;
        "drained"
    });
    let mut frames = Frames::new(write, Pace::SERVED);
    let mut cancel = Some(cancel);
    let left = async {
        // The frame is waiting for the client when it leaves.
        tokio::time::sleep(Duration::from_millis(20)).await;
        client.shutdown().await.expect("half-close");
        std::future::pending::<()>().await;
    };
    let attending = attended(work, &mut read, &mut frames, &backlog, || {
        let _ = cancel.take().expect("once").send(());
    });
    let result = tokio::time::timeout(BOUND, async {
        tokio::select! {
            result = attending => result,
            () = left => unreachable!("leaving never ends"),
        }
    })
    .await
    .expect("a blocked write held up the cancel");
    assert_eq!(result, None);
    assert!(cancel.is_none(), "the call was cancelled");

    // Connected, never reading: gone after the silence, then cancelled.
    let (_client, server) = tokio::io::duplex(1);
    let (mut read, write) = tokio::io::split(server);
    let (scope, backlog) = progress_scope();
    let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();
    let work = scope.run(async {
        say(&"x".repeat(64));
        let _ = cancelled.await;
    });
    let mut frames = Frames::new(write, QUICK);
    let mut cancel = Some(cancel);
    let result = tokio::time::timeout(
        BOUND,
        attended(work, &mut read, &mut frames, &backlog, || {
            let _ = cancel.take().expect("once").send(());
        }),
    )
    .await
    .expect("a client that never reads held up the call");
    assert_eq!(result, None);
    assert!(cancel.is_none(), "the call was cancelled");
}

/// A client that stays and reads gets the result; one that left before a
/// write is cancelled without one.
#[tokio::test]
async fn a_client_that_leaves_cancels_its_call() {
    let (scope, backlog) = progress_scope();
    let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();
    let execution = scope.run(async {
        let _ = cancelled.await;
        "drained"
    });
    let mut gone = &b""[..];
    let mut frames = Frames::new(tokio::io::sink(), Pace::SERVED);
    let mut cancel = Some(cancel);
    let result = attended(execution, &mut gone, &mut frames, &backlog, || {
        let _ = cancel.take().expect("once").send(());
    })
    .await;
    assert_eq!(result, None);
    assert!(cancel.is_none(), "the call was cancelled");

    let (_client, mut server) = tokio::io::duplex(8);
    let mut frames = Frames::new(tokio::io::sink(), Pace::SERVED);
    let result = attended(async { "done" }, &mut server, &mut frames, &backlog, || {
        panic!("not cancelled")
    })
    .await;
    assert_eq!(result, Some("done"));
}
