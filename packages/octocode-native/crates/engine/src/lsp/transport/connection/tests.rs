mod messages;
mod patience;
mod stream_failures;
mod timeouts;

use super::*;
use crate::error::{ErrorCode, ErrorKind};
use crate::lsp::transport::codec::{Frame, encode_frame, read_frame};
use crate::lsp::transport::patience::{DEADLINE_RESERVE, with_caller_deadline};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadBuf, duplex};

/// The fake server's two pipe ends.
struct Server {
    reader: BufReader<DuplexStream>,
    writer: DuplexStream,
}

impl Server {
    async fn read_message(&mut self) -> Value {
        match read_frame(&mut self.reader).await {
            Ok(Frame::Body(body)) => serde_json::from_slice(&body).expect("client sent valid JSON"),
            other => panic!("expected a client frame, got {other:?}"),
        }
    }

    async fn send(&mut self, message: Value) {
        let frame = encode_frame(&message).expect("encode");
        self.writer.write_all(&frame).await.expect("server write");
        self.writer.flush().await.expect("server flush");
    }

    async fn send_raw(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).await.expect("server write");
    }
}

fn context() -> ClientRequestContext {
    ClientRequestContext {
        configuration: json!({"gopls": {"a": 1}}),
        section_root: None,
        workspace_folders: json!([{ "uri": "file:///w", "name": "workspace" }]),
    }
}

fn connect_with_buffer(client_to_server: usize) -> (JsonRpcConnection, Server) {
    let (client_w, server_r) = duplex(client_to_server);
    let (server_w, client_r) = duplex(64 * 1024);
    let connection =
        JsonRpcConnection::new(client_r, client_w, context(), ProgressTracker::new(), None);
    (
        connection,
        Server {
            reader: BufReader::new(server_r),
            writer: server_w,
        },
    )
}

fn connect() -> (JsonRpcConnection, Server) {
    connect_with_buffer(64 * 1024)
}

fn pending_len(connection: &JsonRpcConnection) -> Option<usize> {
    connection
        .shared
        .pending
        .lock()
        .expect("pending lock")
        .as_ref()
        .map(HashMap::len)
}

async fn wait_until_dead(connection: &JsonRpcConnection) {
    for _ in 0..100 {
        if !connection.is_alive() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("connection should be marked dead");
}

/// A writer whose pipe never drains: models a server that stopped reading stdin.
struct StalledWriter;

impl AsyncWrite for StalledWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Pending
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
}

/// A reader that never yields bytes.
struct SilentReader;

impl AsyncRead for SilentReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
}
