//! The bounded read of a local source file a tool parses or outlines: the
//! engine's regular-file read (non-blocking open, size from the opened
//! handle, read capped at the limit), so a FIFO, a device or a file grown
//! past the limit is never read whole.
use std::path::Path;

use octocode_engine::num::saturating_u64;
use octocode_engine::text::ReadError;

/// Bytes of the regular file at `path`, at most `max_bytes`.
pub fn read_bounded(path: &Path, max_bytes: usize) -> Result<Vec<u8>, ReadError> {
    octocode_engine::text::read_source(path, saturating_u64(max_bytes)).map(|source| source.bytes)
}

#[cfg(test)]
mod tests;
