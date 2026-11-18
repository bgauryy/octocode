//! The source sizes a read is judged at, carried as a value: the scanner in
//! production holds the real ones, and a test builds one with small ones, so a
//! fixture past a limit is sized from the limit instead of being megabytes.

/// Byte sizes that decide how a source is read and judged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceLimits {
    /// The longest source read and judged whole. A longer one streams in
    /// windows, and a text past it is judged in one pass over its spans.
    pub whole_source: usize,
    /// Bytes one streamed window of lines holds.
    pub window: usize,
    /// Bytes of a file a search re-reads for its private-key block scan; a
    /// larger file falls back to the per-match window sanitizer.
    pub key_scan: u64,
}

impl SourceLimits {
    /// The limits every build serves.
    pub const REAL: Self = Self {
        whole_source: octocode_engine::security::MAX_CONTENT_SIZE,
        window: 4 * 1024 * 1024,
        key_scan: octocode_engine::security::MAX_CONTENT_SIZE as u64,
    };

    /// [`Self::whole_source`] as a file length.
    pub fn whole_source_bytes(self) -> u64 {
        self.whole_source as u64
    }
}

impl Default for SourceLimits {
    fn default() -> Self {
        Self::REAL
    }
}

#[cfg(any(test, feature = "test-support"))]
impl SourceLimits {
    /// Limits small enough that a fixture past each one is a fraction of a
    /// megabyte, in the same proportions as [`Self::REAL`]: the window is a
    /// part of a whole source, and the key-block scan reads what is judged whole.
    pub const SMALL: Self = Self {
        whole_source: 1024 * 1024,
        window: 512 * 1024,
        key_scan: 1024 * 1024,
    };
}
