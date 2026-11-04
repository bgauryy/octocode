//! Detects changes to the per-project configuration a language server reads
//! once per project and never again for a document it already holds.
//!
//! clangd resolves a file's compile command from `compile_commands.json`,
//! `compile_flags.txt` and `.clangd` in the file's directory and every
//! ancestor. It caches that lookup (a found database is revalidated at most
//! every 5 s, a missing one every 30 s), re-resolves it only when a document
//! is opened or edited, and ignores `workspace/didChangeWatchedFiles` for
//! these files and registers no watchers of its own
//! (`ClangdLSPServer::onFileEvent`, FIXME "observe config files"). A pooled
//! server therefore keeps the project setup it saw at its first `didOpen`
//! until the process is replaced. [`ProjectConfigWatch`] is how the pool
//! notices that and replaces it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// What one config file looked like: `None` for a file that does not exist.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FileStamp(Option<(Option<SystemTime>, u64)>);

impl FileStamp {
    fn of(path: &Path) -> Self {
        Self(
            std::fs::metadata(path)
                .ok()
                .map(|meta| (meta.modified().ok(), meta.len())),
        )
    }
}

/// The config files of every directory a server resolved a document
/// through, as they were when that directory was first observed.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectConfigWatch {
    /// File names relative to a directory (`build/compile_commands.json`);
    /// empty for a server that re-reads its own configuration.
    names: &'static [&'static str],
    seen: BTreeMap<PathBuf, Vec<FileStamp>>,
    /// Absolute files the server reads whatever the document (a clangd
    /// `--compile-commands-dir` database, see `lsp::clangd_database`).
    fixed: Vec<PathBuf>,
    /// Their stamps at the first observation; `None` until then.
    fixed_baseline: Option<Vec<FileStamp>>,
}

impl ProjectConfigWatch {
    pub(crate) fn new(names: &'static [&'static str]) -> Self {
        Self {
            names,
            seen: BTreeMap::new(),
            fixed: Vec::new(),
            fixed_baseline: None,
        }
    }

    /// Also watch `files`, absolute paths every document resolves through.
    pub(crate) fn with_files(mut self, files: Vec<PathBuf>) -> Self {
        self.fixed = files;
        self
    }

    /// `false` for a server with nothing to watch.
    pub(crate) fn is_active(&self) -> bool {
        !self.names.is_empty()
    }

    /// Forget every baseline (a new server process starts from a clean one).
    pub(crate) fn clear(&mut self) {
        self.seen.clear();
        self.fixed_baseline = None;
    }

    /// Record `path` (a file or a directory): its directory and every
    /// ancestor become watched, each with the stamps it has now. A directory
    /// already watched keeps its older baseline, so a change made after the
    /// server first looked there is never absorbed by a later observation.
    pub(crate) fn observe(&mut self, path: &Path) {
        if !self.is_active() {
            return;
        }
        if self.fixed_baseline.is_none() {
            self.fixed_baseline = Some(self.fixed.iter().map(|file| FileStamp::of(file)).collect());
        }
        let start = if path.is_dir() {
            Some(path)
        } else {
            path.parent()
        };
        for dir in start.into_iter().flat_map(Path::ancestors) {
            if !self.seen.contains_key(dir) {
                self.seen.insert(dir.to_path_buf(), self.stamps(dir));
            }
        }
    }

    /// Whether any watched config file was created, changed or deleted since
    /// its directory was first observed.
    pub(crate) fn changed(&self) -> bool {
        let fixed_changed = self.fixed_baseline.as_ref().is_some_and(|baseline| {
            self.fixed
                .iter()
                .map(|file| FileStamp::of(file))
                .ne(baseline.iter().cloned())
        });
        fixed_changed
            || self
                .seen
                .iter()
                .any(|(dir, baseline)| self.stamps(dir) != *baseline)
    }

    fn stamps(&self, dir: &Path) -> Vec<FileStamp> {
        self.names
            .iter()
            .map(|name| FileStamp::of(&dir.join(name)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &["compile_flags.txt", "build/compile_commands.json"];

    fn watch_of(file: &Path) -> ProjectConfigWatch {
        let mut watch = ProjectConfigWatch::new(NAMES);
        watch.observe(file);
        watch
    }

    #[test]
    fn creating_changing_and_deleting_a_config_file_is_noticed_in_any_ancestor() {
        let root = tempfile::tempdir().expect("tempdir");
        let leaf = root.path().join("a/b");
        std::fs::create_dir_all(&leaf).expect("dirs");
        let source = leaf.join("x.cu");
        std::fs::write(&source, "").expect("source");
        let watch = watch_of(&source);
        assert!(!watch.changed(), "nothing changed yet");

        let flags = root.path().join("compile_flags.txt");
        std::fs::write(&flags, "-xcuda\n").expect("create");
        assert!(watch.changed(), "a config file created in an ancestor");

        let mut after_create = ProjectConfigWatch::new(NAMES);
        after_create.observe(&source);
        assert!(!after_create.changed());
        std::fs::write(&flags, "-xcuda\n-std=c++17\n").expect("edit");
        assert!(
            after_create.changed(),
            "a config file whose content changed"
        );

        let mut after_edit = ProjectConfigWatch::new(NAMES);
        after_edit.observe(&source);
        std::fs::remove_file(&flags).expect("delete");
        assert!(after_edit.changed(), "a config file deleted");
    }

    #[test]
    fn a_nested_config_path_and_unrelated_files_are_told_apart() {
        let root = tempfile::tempdir().expect("tempdir");
        let source = root.path().join("x.cu");
        std::fs::write(&source, "").expect("source");
        let watch = watch_of(&source);
        std::fs::write(root.path().join("notes.txt"), "x").expect("unrelated");
        assert!(!watch.changed(), "a file the server does not read");
        std::fs::create_dir(root.path().join("build")).expect("build dir");
        std::fs::write(root.path().join("build/compile_commands.json"), "[]").expect("db");
        assert!(watch.changed(), "build/compile_commands.json");
    }

    #[test]
    fn a_later_observation_keeps_the_older_baseline() {
        let root = tempfile::tempdir().expect("tempdir");
        let first = root.path().join("x.cu");
        std::fs::write(&first, "").expect("source");
        let mut watch = watch_of(&first);
        std::fs::write(root.path().join("compile_flags.txt"), "-xcuda\n").expect("create");
        watch.observe(&first);
        watch.observe(root.path());
        assert!(
            watch.changed(),
            "re-observing must not absorb a change the server has not seen"
        );
    }

    #[test]
    fn a_new_directory_is_watched_from_the_moment_it_is_first_observed() {
        let root = tempfile::tempdir().expect("tempdir");
        let other = root.path().join("other");
        std::fs::create_dir(&other).expect("dir");
        let first = root.path().join("x.cu");
        std::fs::write(&first, "").expect("source");
        let mut watch = watch_of(&first);
        std::fs::write(other.join("compile_flags.txt"), "-xcuda\n").expect("existing");
        watch.observe(&other.join("y.cu"));
        assert!(
            !watch.changed(),
            "a file already there when its directory is first observed is the baseline"
        );
    }

    #[test]
    fn a_server_without_config_files_watches_nothing() {
        let root = tempfile::tempdir().expect("tempdir");
        let source = root.path().join("x.ts");
        std::fs::write(&source, "").expect("source");
        let mut watch = ProjectConfigWatch::new(&[]);
        watch.observe(&source);
        assert!(!watch.is_active());
        assert!(!watch.changed());
    }

    /// Issue #482: clangd started with `--compile-commands-dir=build/debug`
    /// reads that database for every file; rebuilding it reloads clangd.
    #[test]
    fn a_configured_database_directory_is_watched() {
        let root = tempfile::tempdir().expect("tempdir");
        let debug = root.path().join("build/debug");
        std::fs::create_dir_all(&debug).expect("debug");
        let source = root.path().join("src/a.cpp");
        std::fs::create_dir_all(source.parent().expect("src")).expect("src");
        std::fs::write(&source, "").expect("source");
        let config = crate::lsp::types::LspLanguageServerConfig {
            command: "clangd".into(),
            args: Some(vec!["--compile-commands-dir=build/debug".into()]),
            workspace_root: root.path().to_string_lossy().into_owned(),
            language_id: Some("cpp".into()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
            source: crate::lsp::types::ServerSource::Path,
        };
        let mut watch = ProjectConfigWatch::new(NAMES)
            .with_files(crate::lsp::clangd_database::fixed_database_files(&config));
        watch.observe(&source);
        assert!(!watch.changed(), "nothing changed yet");
        std::fs::write(debug.join("compile_commands.json"), "[]").expect("database");
        assert!(watch.changed(), "the configured database was created");
        watch.clear();
        watch.observe(&source);
        assert!(!watch.changed(), "a new baseline after clear");
    }

    /// Rebuilding an existing configured database (not only creating one)
    /// reloads clangd.
    #[test]
    fn modifying_a_configured_database_is_a_change() {
        let root = tempfile::tempdir().expect("tempdir");
        let debug = root.path().join("build/debug");
        std::fs::create_dir_all(&debug).expect("debug");
        let database = debug.join("compile_commands.json");
        std::fs::write(&database, "[]").expect("database");
        let source = root.path().join("src/a.cpp");
        std::fs::create_dir_all(source.parent().expect("src")).expect("src");
        std::fs::write(&source, "").expect("source");
        let config = crate::lsp::types::LspLanguageServerConfig {
            command: "clangd".into(),
            args: Some(vec!["--compile-commands-dir=build/debug".into()]),
            workspace_root: root.path().to_string_lossy().into_owned(),
            language_id: Some("cpp".into()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
            source: crate::lsp::types::ServerSource::Path,
        };
        let mut watch = ProjectConfigWatch::new(NAMES)
            .with_files(crate::lsp::clangd_database::fixed_database_files(&config));
        watch.observe(&source);
        assert!(!watch.changed(), "an unchanged database is no change");
        std::fs::write(&database, "[{\"file\": \"a.cpp\"}]").expect("rebuild");
        assert!(watch.changed(), "the rebuilt database is a change");
    }
}
