//! The bounded read of a local source file a tool parses or outlines: the
//! engine's regular-file read (non-blocking open, size from the opened
//! handle, read capped at the limit), so a FIFO, a device or a file grown
//! past the limit is never read whole.
use std::path::Path;

use octocode_engine::text::ReadError;

/// Bytes of the regular file at `path`, at most `max_bytes`.
pub(crate) fn read_bounded(path: &Path, max_bytes: usize) -> Result<Vec<u8>, ReadError> {
    octocode_engine::text::read_source(path, u64::try_from(max_bytes).unwrap_or(u64::MAX))
        .map(|source| source.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_past_the_limit_is_refused_before_it_is_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("big.rs");
        std::fs::write(&path, "x".repeat(65)).expect("write");
        assert!(matches!(
            read_bounded(&path, 64),
            Err(ReadError::TooLarge(65))
        ));
    }

    #[test]
    fn a_directory_is_no_source() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(read_bounded(dir.path(), 1024).is_err());
    }
}
