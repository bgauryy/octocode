//! A text search scoped to production code or to test code.

/// The 1-based line span of a file that is test code whole.
pub const WHOLE_FILE: [u32; 2] = [1, u32::MAX];

/// Whether a file is test code by where it lies, or may hold any.
pub type PathTest = dyn Fn(&std::path::Path) -> bool + Send + Sync;
/// A file's 1-based test-only line spans.
pub type LineSpans = dyn Fn(&std::path::Path) -> std::sync::Arc<Vec<[u32; 2]>> + Send + Sync;

/// Which side of the test-code line a search keeps, and how it tells: a
/// file is test code whole by where it lies or by its spans; only a file
/// that may hold test code has its spans read, once, on its first hit (or
/// when a files-without view would list it).
#[derive(Clone)]
pub struct TestCodeScope {
    /// Keep only test code (`true`) or only production code (`false`).
    pub keep_test: bool,
    /// Whether a file is test code whole by where it lies (its path, or a
    /// test-only mount), decided without reading it.
    pub test_path: std::sync::Arc<PathTest>,
    /// Whether a file that is not a test path may still hold test code.
    pub may_hold: std::sync::Arc<PathTest>,
    /// A file's 1-based test-only line spans.
    pub spans: std::sync::Arc<LineSpans>,
}

impl std::fmt::Debug for TestCodeScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestCodeScope")
            .field("keep_test", &self.keep_test)
            .finish_non_exhaustive()
    }
}

/// Whether a file with test-only `spans` lies wholly on the side a scope
/// leaves out: all test code when production is kept (`keep_test` false),
/// no test code when test code is kept. Such a file is left out as a file,
/// never listed and never counted by its lines.
pub fn wholly_left_out(keep_test: bool, spans: &[[u32; 2]]) -> bool {
    if keep_test {
        spans.is_empty()
    } else {
        spans == [WHOLE_FILE]
    }
}

/// What a [`TestCodeScope`] left out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TestCodeExcluded {
    /// Files left out whole: unread by where they lie, or read and wholly
    /// on the other side by their spans.
    pub files: u32,
    /// Matched lines left out inside searched files.
    pub lines: u32,
}
