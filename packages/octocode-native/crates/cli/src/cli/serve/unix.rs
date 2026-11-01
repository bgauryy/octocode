//! The warm server on Unix sockets; see the parent module for its promises.

mod client;
mod daemon;
mod identity;
mod served;
mod sockets;
#[cfg(test)]
mod tests;

pub(in crate::cli) use client::call;
pub(in crate::cli) use daemon::run;
pub(in crate::cli) use sockets::{stop_all, stop_all_and_wait};

use std::path::{Path, PathBuf};

/// Read one request line of at most `limit` bytes from `reader`, leaving the
/// rest buffered; `None` at end of stream, on error, or when the line is longer
/// than the limit.
pub(super) async fn read_line_bounded<R>(reader: &mut R, limit: u64) -> Option<String>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    use tokio::io::{AsyncBufReadExt, AsyncReadExt};
    let mut bounded = (&mut *reader).take(limit + 1);
    let mut line = String::new();
    let read = bounded.read_line(&mut line).await.ok()?;
    (read > 0 && read as u64 <= limit).then_some(line)
}

/// The form two spellings of one directory share (symlinks resolved), or
/// the path as given when it cannot be resolved.
pub(super) fn same_dir_form(dir: &Path) -> PathBuf {
    std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
}
