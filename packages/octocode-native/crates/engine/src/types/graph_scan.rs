//! Graph facts scan options and diagnostics.

#[derive(Debug, Clone, Default)]
pub struct GraphFactsScanOptions {
    pub path: String,
    /// See [`FileSystemQueryOptions::default_exclude_dir`](super::FileSystemQueryOptions::default_exclude_dir).
    pub default_exclude_dir: Option<Vec<String>>,
    /// The caller's path globs to skip (see [`FileSystemQueryOptions::exclude`](super::FileSystemQueryOptions::exclude)).
    pub exclude: Option<Vec<String>>,
    pub max_files: Option<u32>,
    /// Leading files (walk order) left unscanned: an earlier scan of the
    /// same scope already read them. They still count toward `max_files`.
    pub skip_files: Option<u32>,
    /// The largest file the scan reads whole: the caller's memory bound
    /// (a graph build's budget, a whole-source read's limit). `None` reads
    /// any size; the parse's own work bound and the map budget still apply.
    pub max_file_bytes: Option<u64>,
    pub language_globs: Option<Vec<GraphLanguageGlob>>,
    /// Which ignore rules and dot entries the walk leaves out.
    pub ignore: crate::types::IgnorePolicy,
}

#[derive(Debug, Clone)]
pub struct GraphLanguageGlob {
    pub language: String,
    pub glob: String,
}

#[derive(Debug, Clone)]
pub struct GraphFactsScanDiagnostic {
    pub relative_path: String,
    pub code: String,
    pub message: String,
}
