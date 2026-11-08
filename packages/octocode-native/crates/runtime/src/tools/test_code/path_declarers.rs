//! The Rust files that spell a `#[path]`, and so can mount a file under any name:
//! those directly in one directory, at or above it, and in its whole crate. Each
//! directory is read once whoever asks.
use super::rust_source::spells_path_attribute;
use super::{TestCodeIndex, lock};
use crate::policy::discovery::EntryKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

impl TestCodeIndex {
    /// The Rust files at or above `dir` that spell a `#[path]`: those can mount
    /// a file below them under any name.
    pub(super) fn path_declarers(&self, dir: &Path) -> Arc<[PathBuf]> {
        self.declarers.of(dir, || {
            let mut declarers = self.spelling_path_in(dir).to_vec();
            if let Some(up) = self.level_above(dir) {
                declarers.extend(self.path_declarers(up).iter().cloned());
            }
            declarers.into()
        })
    }

    /// The Rust files of `dir`'s crate that spell a `#[path]`, whatever directory
    /// holds them, each directory read once whoever asks; of its test directories,
    /// only the Cargo test targets directly in `tests/`.
    pub(super) fn path_declarers_of_crate(&self, dir: &Path) -> Arc<[PathBuf]> {
        let Some(top) = self.crate_dir(dir) else {
            return Arc::default();
        };
        self.crate_declarers.of(&top, || {
            let mut declarers = Vec::new();
            let mut pending = vec![top.clone()];
            let targets = top.join("tests");
            while let Some(dir) = pending.pop() {
                declarers.extend(self.spelling_path_in(&dir).iter().cloned());
                // Test code's own tree is most of many crates and rarely reaches
                // out: its target roots are read, and nothing below them.
                if !self.test_dir(&dir) {
                    pending.extend(
                        self.subdirectories_of_crate(&dir)
                            .into_iter()
                            .filter(|sub| *sub == targets || !self.test_dir(sub)),
                    );
                }
            }
            declarers.into()
        })
    }

    /// The directories of `dir` that belong to its crate: no other crate's, and
    /// none a walk leaves out (build output, hidden, ignored by policy).
    fn subdirectories_of_crate(&self, dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                !name.starts_with('.') && name != "target" && name != "node_modules"
            })
            .map(|entry| entry.path())
            .filter(|sub| {
                !self.paths.withholds(sub, EntryKind::Dir) && !self.is_file(&sub.join("Cargo.toml"))
            })
            .collect()
    }

    /// The Rust files directly in `dir` that spell a `#[path]`. Every one of a
    /// walked tree's files lands here, so the directory is resolved once: an
    /// entry listed as a regular file resolves to its name in it, and the read
    /// refuses one swapped for a link since.
    fn spelling_path_in(&self, dir: &Path) -> Arc<[PathBuf]> {
        self.spelling.of(dir, || {
            lock(&self.read).insert(dir.to_path_buf());
            self.read_spelling_path_in(dir).into()
        })
    }

    /// Whether the file `path` (resolved to `canonical`) spells a `#[path]`.
    /// Its text is the one every verdict shares ([`Self::source`]): a read
    /// by a parent or a span before or after this scan is the same read.
    fn spells_path(&self, path: &Path, canonical: &Path) -> bool {
        let text = self.texts.of(path, || {
            let bytes = self.bounded(canonical, octocode_engine::text::read_regular)?;
            String::from_utf8(bytes).ok().map(Arc::from)
        });
        text.is_some_and(|text| {
            if memchr::memmem::find(text.as_bytes(), b"cfg").is_none() {
                lock(&self.never_cfg).insert(path.to_path_buf());
            }
            spells_path_attribute(text.as_bytes())
        })
    }

    fn read_spelling_path_in(&self, dir: &Path) -> Vec<PathBuf> {
        let Ok(resolved) = self.paths.validate(dir) else {
            return Vec::new();
        };
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .map(|entry| (entry.path(), resolved.canonical.join(entry.file_name())))
            .filter(|(path, _)| Self::may_hold_test_code(path))
            .filter(|(path, canonical)| {
                lock(&self.read).extend([path.clone(), canonical.clone()]);
                !self.paths.withholds(path, EntryKind::File)
                    && !self.paths.withholds(canonical, EntryKind::File)
                    && self.spells_path(path, canonical)
            })
            .map(|(path, _)| path)
            .collect()
    }
}
