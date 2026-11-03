//! Progress frames: what a server says on the socket while a call runs.
//!
//! A reply line is the call as printed. Before it, the server may write any
//! number of frame lines, `{"progress":"<line>"}`, one per progress line
//! the call raised ([`octocode_io::notice::progress_sink`]) in the order
//! raised; the client prints each on stderr as it arrives, as an in-process
//! call does, and ends on the first line that is not a frame. A server and
//! client of different builds never meet: the socket name hashes the
//! executable's identity (`exe_identity`), so a client that knows frames
//! reaches a server that sends them.

use octocode_io::notice::Scope;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// The key of a frame line.
pub(super) const PROGRESS: &str = "progress";

/// A scope for one call whose progress lines queue on the returned
/// receiver, for [`forward`] to write.
pub(super) fn progress_scope() -> (Scope, UnboundedReceiver<String>) {
    let (queue, lines) = unbounded_channel::<String>();
    let scope = Scope::with_progress(move |line| {
        let _ = queue.send(line.to_owned());
    });
    (scope, lines)
}

/// Await `work`, writing each queued progress line to `write` as a frame
/// while it runs, then the lines it left queued: all before the caller
/// writes the reply. A failed write is the client having left, which the
/// caller's own read of the socket finds.
pub(super) async fn forward<T>(
    work: impl Future<Output = T>,
    lines: &mut UnboundedReceiver<String>,
    write: &mut (impl AsyncWrite + Unpin),
) -> T {
    tokio::pin!(work);
    let result = loop {
        tokio::select! {
            result = &mut work => break result,
            Some(line) = lines.recv() => frame(write, &line).await,
        }
    };
    while let Ok(line) = lines.try_recv() {
        frame(write, &line).await;
    }
    result
}

async fn frame(write: &mut (impl AsyncWrite + Unpin), line: &str) {
    let mut frame = serde_json::json!({ PROGRESS: line }).to_string();
    frame.push('\n');
    let _ = write.write_all(frame.as_bytes()).await;
}
