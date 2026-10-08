//! The bounded read of a local source file a tool parses or outlines: the
//! engine's regular-file read (non-blocking open, size from the opened
//! handle, read capped at the limit), so a FIFO, a device or a file grown
//! past the limit is never read whole.
use std::path::Path;

use crate::security::ContentSecurity;
pub(crate) use octocode_engine::lsp::BoundedRead;

/// Metadata identity for cache reuse. A missing change time cannot prove that
/// a write preserving size and mtime left the source unchanged.
pub(crate) fn cache_stamp(meta: &std::fs::Metadata) -> Option<octocode_engine::graph::SourceStamp> {
    let stamp = octocode_engine::graph::SourceStamp::of(meta)?;
    (stamp.changed_ns != 0).then_some(stamp)
}

/// Bytes of the regular file at `path`, at most `max_bytes`.
pub(crate) fn read_bounded(path: &Path, max_bytes: usize) -> Result<Vec<u8>, BoundedRead> {
    octocode_engine::lsp::read_regular_bounded(path, u64::try_from(max_bytes).unwrap_or(u64::MAX))
}

/// [`read_bounded`] decoded by the content policy; `None` when the file is
/// larger, unreadable or binary.
pub(crate) fn read_text(
    path: &Path,
    max_bytes: usize,
    security: &ContentSecurity,
) -> Option<String> {
    let bytes = read_bounded(path, max_bytes).ok()?;
    security
        .decode_source_bytes(&bytes, max_bytes)
        .ok()
        .map(std::borrow::Cow::into_owned)
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
            Err(BoundedRead::TooLarge(65))
        ));
        assert_eq!(read_text(&path, 64, &ContentSecurity), None);
        assert_eq!(read_text(&path, 65, &ContentSecurity), Some("x".repeat(65)));
    }

    #[test]
    fn binary_and_non_regular_sources_yield_no_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bin.rs");
        std::fs::write(&path, b"fn a() {}\0").expect("write");
        assert_eq!(read_text(&path, 1024, &ContentSecurity), None);
        assert!(read_bounded(dir.path(), 1024).is_err());
    }
}
