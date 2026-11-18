//! LSP base-protocol framing (`Content-Length: N\r\n\r\n<N bytes>`).
//!
//! Reading is bounded before allocating: one header line, the whole header
//! block, and the body each have a cap. Every malformed input that can desync
//! the stream (an oversized or unparsable `Content-Length`, an unterminated
//! header, EOF mid-body) is a fatal [`FrameError`]; the connection must then be
//! failed, never resumed. Encoding produces one contiguous header+body buffer,
//! serialized once, so a single writer can put a whole frame on the wire.

use crate::error::{Error, Result};
use serde_json::Value;
use std::fmt::{self, Display, Formatter};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};

/// Largest body we accept from a server. Checked before the body is allocated.
pub(super) const MAX_CONTENT_LENGTH: usize = 64 * 1024 * 1024;
/// Upper bound on a single header line (e.g. `Content-Length: <n>`). A
/// well-formed LSP header line is a few dozen bytes; anything approaching this
/// cap is a server streaming an unterminated line to exhaust memory.
const MAX_HEADER_LINE_BYTES: u64 = 8 * 1024;
/// Upper bound on the whole header block (all lines up to the blank separator).
const MAX_HEADER_BLOCK_BYTES: usize = 64 * 1024;

/// One decoded frame.
#[derive(Debug)]
pub(super) enum Frame {
    /// The stream closed cleanly between frames.
    Eof,
    /// A header block with no `Content-Length` (or `Content-Length: 0`). Such a
    /// frame carries no body, so skipping it cannot desync the stream; it is
    /// tolerated rather than tearing the connection down.
    Empty,
    Body(Vec<u8>),
}

/// A framing fault after which the stream position is unknown. Fatal.
#[derive(Debug)]
pub(super) struct FrameError(pub(super) String);

impl Display for FrameError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Reads one frame. Not cancel-safe (`read_line`/`read_exact`): call it only
/// from a task that owns the reader and is never raced in `select!`.
pub(super) async fn read_frame<R>(
    reader: &mut BufReader<R>,
) -> std::result::Result<Frame, FrameError>
where
    R: AsyncRead + Unpin,
{
    let content_length = match read_headers(reader).await? {
        None => return Ok(Frame::Eof),
        Some(0) => return Ok(Frame::Empty),
        Some(length) => length,
    };
    if content_length > MAX_CONTENT_LENGTH {
        return Err(FrameError(format!(
            "LSP response exceeded maximum JSON-RPC frame size: {content_length} bytes"
        )));
    }
    let mut body = vec![0u8; content_length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|error| FrameError(format!("LSP connection closed mid-frame: {error}")))?;
    Ok(Frame::Body(body))
}

/// Reads one header block. `Ok(None)` = clean EOF before any header byte;
/// `Ok(Some(len))` = the declared body length (0 when absent).
async fn read_headers<R>(
    reader: &mut BufReader<R>,
) -> std::result::Result<Option<usize>, FrameError>
where
    R: AsyncRead + Unpin,
{
    let mut content_length = None;
    let mut header_bytes = 0usize;
    loop {
        let mut line = String::new();
        // `take` caps how many bytes `read_line` may pull for one line.
        let bytes = (&mut *reader)
            .take(MAX_HEADER_LINE_BYTES)
            .read_line(&mut line)
            .await
            .map_err(|error| FrameError(format!("LSP header read failed: {error}")))?;
        if bytes == 0 {
            if header_bytes == 0 {
                return Ok(None);
            }
            return Err(FrameError("LSP connection closed mid-header".to_owned()));
        }
        if bytes as u64 == MAX_HEADER_LINE_BYTES && !line.ends_with('\n') {
            return Err(FrameError(
                "LSP header line exceeded maximum size".to_owned(),
            ));
        }
        header_bytes = header_bytes.saturating_add(bytes);
        if header_bytes > MAX_HEADER_BLOCK_BYTES {
            return Err(FrameError(
                "LSP header block exceeded maximum size".to_owned(),
            ));
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            return Ok(Some(content_length.unwrap_or(0)));
        }
        // Header names are case-insensitive; unknown headers are ignored.
        if let Some((name, value)) = trimmed.split_once(':')
            && name.trim().eq_ignore_ascii_case("Content-Length")
        {
            // An unparsable value (garbage, negative, overflow) means we cannot
            // know where the body ends: resuming would parse the body as headers.
            let value = value.trim();
            let length = value
                .parse::<usize>()
                .ok()
                .filter(|_| value.bytes().all(|byte| byte.is_ascii_digit()))
                .ok_or_else(|| {
                    FrameError(format!(
                        "LSP frame has an invalid Content-Length: {value:?}"
                    ))
                })?;
            content_length = Some(length);
        }
    }
}

/// Serializes `message` once into a complete frame (header and body in one
/// buffer), ready for a single `write_all`.
pub(super) fn encode_frame(message: &Value) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(message)
        .map_err(|error| Error::new(format!("Serialize JSON-RPC failed: {error}")))?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut frame = Vec::with_capacity(header.len() + body.len());
    frame.extend_from_slice(header.as_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

#[cfg(test)]
mod tests;
