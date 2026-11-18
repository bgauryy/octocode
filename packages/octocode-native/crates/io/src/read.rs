//! The one bounded read of a stream, sync ([`read_limited`]) and async
//! ([`read_limited_async`], a child's pipe): at most `limit` bytes, and an
//! input that holds more is reported, never silently cut to a prefix.
//! (The HTTP body reader is [`retry::read_bounded`](crate::retry::read_bounded);
//! the engine, which depends on this crate, keeps its own cancellable source
//! read.)
use std::io::{self, Read};
use tokio::io::{AsyncRead, AsyncReadExt};

/// Read at most `limit` bytes from `reader`; `Ok(None)` when it holds more.
/// The read stops one byte past `limit`, so an endless reader cannot grow
/// the buffer beyond it.
pub fn read_limited(reader: impl Read, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok(within(bytes, limit))
}

/// [`read_limited`] for an async reader.
pub async fn read_limited_async(
    reader: impl AsyncRead + Unpin,
    limit: u64,
) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .await?;
    Ok(within(bytes, limit))
}

fn within(bytes: Vec<u8>, limit: u64) -> Option<Vec<u8>> {
    (bytes.len() as u64 <= limit).then_some(bytes)
}

#[cfg(test)]
mod tests;
