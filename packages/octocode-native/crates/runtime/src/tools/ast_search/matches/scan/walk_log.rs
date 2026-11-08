//! What a directory scan's walk saw besides the files it parsed: the
//! directories a continuation page re-checks, the files no grammar parses, and
//! the per-extension census of the files it admitted.
use super::super::types::{Scope, Unparsed};
use crate::policy::discovery::WalkAdmission;
use crate::policy::path::PathPolicy;
use crate::tools::ast_rule::has_extension_in;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

/// The walk's record, filled from its admission callback (on the engine's
/// walker threads, so behind locks).
pub(super) struct WalkLog<'a> {
    scope: &'a Scope,
    paths: &'a PathPolicy,
    admission: &'a WalkAdmission<'a>,
    include_hidden: bool,
    grammars: BTreeSet<String>,
    unparsed: Mutex<Unparsed>,
    observed: Mutex<BTreeSet<PathBuf>>,
    census: Mutex<BTreeMap<String, usize>>,
    present: Mutex<BTreeMap<String, usize>>,
    /// Files already counted: the engine asks again for each file it reads.
    counted: Mutex<HashSet<PathBuf>>,
}

/// What the walk saw, once it ended.
pub(super) struct Logged {
    pub(super) unparsed: Unparsed,
    pub(super) discovery_sources: Vec<PathBuf>,
    pub(super) census: BTreeMap<String, usize>,
    pub(super) present: Option<BTreeMap<String, usize>>,
}

impl<'a> WalkLog<'a> {
    pub(super) fn new(
        scope: &'a Scope,
        paths: &'a PathPolicy,
        admission: &'a WalkAdmission<'a>,
        include_hidden: bool,
    ) -> Self {
        Self {
            scope,
            paths,
            admission,
            include_hidden,
            grammars: super::super::scope::structural_extensions(),
            unparsed: Mutex::new(Unparsed::default()),
            observed: Mutex::new(BTreeSet::new()),
            census: Mutex::new(BTreeMap::new()),
            present: Mutex::new(BTreeMap::new()),
            counted: Mutex::new(HashSet::new()),
        }
    }

    /// Whether the walk takes `path`, a `file_type` entry when known, recording
    /// what a continuation page re-checks and what the scope's coverage
    /// reports.
    pub(super) fn admit(&self, path: &Path, file_type: Option<FileType>) -> bool {
        let scope = self.scope;
        let met = file_type.is_some_and(|kind| kind.is_file()) && self.meet(path);
        // Only a directory or a file of the scanned grammar could have
        // matched, so only those leave absence unproven when the policy
        // withholds them or they are links (the walk follows none).
        // A dot entry out of view (no `hidden`) is neither walked nor
        // disclosed, as structureSearch tree does.
        let in_view = self.include_hidden || !super::super::super::is_dot_entry(path);
        if met && scope.language.is_some() && self.admission.permits(path, file_type) {
            crate::tools::ast_rule::count_present(
                &mut self.present.lock().unwrap_or_else(PoisonError::into_inner),
                path,
                &scope.parsers,
            );
        }
        if !self.admission.admit(path, file_type, |is_dir| {
            in_view && (is_dir || super::parses(scope, path))
        }) {
            return false;
        }
        let (is_dir, is_file) = file_type.map_or_else(
            || (path.is_dir(), path.is_file()),
            |kind| (kind.is_dir(), kind.is_file()),
        );
        if is_dir {
            self.observed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(path.to_path_buf());
        }
        let unparsed_file = !super::parses(scope, path) && is_file;
        // A file no grammar parses is disclosed, not silently dropped, with
        // the type its name is of, if any: the page decides by the rule's
        // language which of them leave absence unproven. A file of another
        // grammar under `parser` is that grammar's, never this scope's.
        if unparsed_file && !has_extension_in(path, &self.grammars) {
            // Named as a row path reads after the response stage: from
            // the workspace root, else absolute.
            self.unparsed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .record(
                    self.paths
                        .workspace_relative(path)
                        .unwrap_or_else(|| path.to_string_lossy().into_owned()),
                    known_type(path),
                );
        }
        let admitted = !unparsed_file;
        if admitted && met {
            self.count(path);
        }
        // Grammar filtering is not an explicit include override: ignored
        // source files remain ignored unless the caller overrides them.
        admitted
    }

    /// Whether this is the first the log hears of the file `path`.
    fn meet(&self, path: &Path) -> bool {
        self.counted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(path.to_path_buf())
    }

    /// Count an admitted file in the census by the parser it reads with.
    fn count(&self, path: &Path) {
        let extension = super::parser_extension(self.scope, path);
        *self
            .census
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(extension)
            .or_default() += 1;
    }

    pub(super) fn finish(self) -> Logged {
        Logged {
            unparsed: self
                .unparsed
                .into_inner()
                .unwrap_or_else(PoisonError::into_inner),
            discovery_sources: self
                .observed
                .into_inner()
                .unwrap_or_else(PoisonError::into_inner)
                .into_iter()
                .collect(),
            census: self
                .census
                .into_inner()
                .unwrap_or_else(PoisonError::into_inner),
            present: self.scope.language.is_some().then(|| {
                self.present
                    .into_inner()
                    .unwrap_or_else(PoisonError::into_inner)
            }),
        }
    }
}

/// The type `path`'s name is of in ripgrep's default type table (the one
/// localSearch's `fileType` selects from), if any; of several
/// (`CMakeLists.txt` is `cmake` and `txt`), the first by name, so the count
/// is the same in every process. The table is the `ignore` crate's constant
/// and its globs compile; a glob that did not would only leave its names of
/// no known type, so they warn, never the reverse.
fn known_type(path: &Path) -> Option<&'static str> {
    static TYPES: std::sync::LazyLock<(Vec<String>, Vec<usize>, globset::GlobSet)> =
        std::sync::LazyLock::new(|| {
            let mut table = ignore::types::TypesBuilder::new();
            table.add_defaults();
            let (mut names, mut owners, mut set) = (vec![], vec![], globset::GlobSetBuilder::new());
            // Sorted by name, so the lowest owner is the first name.
            for (index, def) in table.definitions().into_iter().enumerate() {
                for glob in def.globs() {
                    if let Ok(glob) = globset::GlobBuilder::new(glob)
                        .literal_separator(true)
                        .build()
                    {
                        set.add(glob);
                        owners.push(index);
                    }
                }
                names.push(def.name().to_owned());
            }
            let set = set.build().unwrap_or_else(|_| globset::GlobSet::empty());
            (names, owners, set)
        });
    let (names, owners, set) = &*TYPES;
    let name = path.file_name()?;
    let owner = set
        .matches(name)
        .into_iter()
        .filter_map(|glob| owners.get(glob).copied())
        .min()?;
    names.get(owner).map(String::as_str)
}
