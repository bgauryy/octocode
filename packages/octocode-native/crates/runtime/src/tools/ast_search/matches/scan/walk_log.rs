//! What a directory scan's walk saw besides the files it parsed: the
//! directories and ignore inputs a continuation page re-checks, the files
//! no grammar parses, and the per-extension census of the files it admitted.
use super::super::types::{NamedFiles, Scope};
use crate::policy::discovery::WalkAdmission;
use crate::policy::path::PathPolicy;
use crate::tools::ast_rule::has_extension_in;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

/// The walk's record, filled from its admission callback (on the engine's
/// walker threads, so behind locks).
pub(super) struct WalkLog<'a> {
    scope: &'a Scope,
    paths: &'a PathPolicy,
    admission: &'a WalkAdmission<'a>,
    /// Ignore files take part in the walk (no `includeIgnored`).
    ignore_rules: bool,
    include_hidden: bool,
    grammars: BTreeSet<String>,
    unparsed: Mutex<NamedFiles>,
    observed: Mutex<BTreeSet<PathBuf>>,
    external_rules: Mutex<BTreeSet<PathBuf>>,
    untracked_ignore_dependencies: AtomicBool,
    census: Mutex<BTreeMap<String, usize>>,
    counted: Mutex<HashSet<PathBuf>>,
}

/// What the walk saw, once it ended.
pub(super) struct Logged {
    pub(super) unparsed: NamedFiles,
    pub(super) discovery_sources: Vec<PathBuf>,
    pub(super) ignore_rules: Vec<PathBuf>,
    pub(super) untracked_ignore_dependencies: bool,
    pub(super) census: BTreeMap<String, usize>,
}

impl<'a> WalkLog<'a> {
    pub(super) fn new(
        scope: &'a Scope,
        paths: &'a PathPolicy,
        admission: &'a WalkAdmission<'a>,
        ignore_rules: bool,
        include_hidden: bool,
    ) -> Self {
        let external_rules = if ignore_rules {
            octocode_engine::search::walk::external_ignore_inputs(&scope.canonical)
                .into_iter()
                .collect()
        } else {
            BTreeSet::new()
        };
        Self {
            scope,
            paths,
            admission,
            ignore_rules,
            include_hidden,
            grammars: super::super::scope::structural_extensions(),
            unparsed: Mutex::new(NamedFiles::default()),
            observed: Mutex::new(BTreeSet::new()),
            external_rules: Mutex::new(external_rules),
            untracked_ignore_dependencies: AtomicBool::new(false),
            census: Mutex::new(BTreeMap::new()),
            counted: Mutex::new(HashSet::new()),
        }
    }

    /// Whether the walk takes `path`, recording what a continuation page
    /// re-checks and what the scope's coverage reports.
    pub(super) fn admit(&self, path: &Path) -> bool {
        let scope = self.scope;
        // Only a directory or a file of the scanned grammar could have
        // matched, so only those leave absence unproven when the policy
        // withholds them or they are links (the walk follows none).
        // A dot entry out of view (no `hidden`) is neither walked nor
        // disclosed, as structureSearch tree does.
        let in_view = self.include_hidden || !super::super::super::is_dot_entry(path);
        if !self.admission.admit(path, None, |is_dir| {
            in_view && (is_dir || super::parses(scope, path))
        }) {
            return false;
        }
        if path.is_dir() {
            self.watch(path);
        }
        let unparsed_file = !super::parses(scope, path) && path.is_file();
        // A file no grammar parses is disclosed, not silently dropped, when
        // the scope is every grammar's files. Under `parser` the scope is
        // that language's files: a file of any other kind (a grammar's own
        // or none) was never in it, so it is not skipped evidence.
        if unparsed_file && scope.language.is_none() && !has_extension_in(path, &self.grammars) {
            // Named as a row path reads after the response stage: from
            // the workspace root, else absolute.
            self.unparsed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .record(
                    self.paths
                        .workspace_relative(path)
                        .unwrap_or_else(|| path.to_string_lossy().into_owned()),
                );
        }
        let admitted = !unparsed_file;
        if admitted && scope.language.is_none() && path.is_file() {
            self.count(path);
        }
        // Grammar filtering is not an explicit include override: ignored
        // source files remain ignored unless the caller overrides them.
        admitted
    }

    /// Watch a walked directory, and the ignore inputs that shape it.
    fn watch(&self, dir: &Path) {
        let mut watched = self.observed.lock().unwrap_or_else(PoisonError::into_inner);
        watched.insert(dir.to_path_buf());
        if self.ignore_rules {
            let inputs = crate::tools::page_memo::directory_ignore_inputs(
                dir,
                &self.scope.canonical,
                |ignore| self.admission.permits(ignore, None),
            );
            self.external_rules
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(inputs.external);
            if inputs.untracked {
                self.untracked_ignore_dependencies
                    .store(true, Ordering::Relaxed);
            }
            watched.extend(inputs.files);
        }
    }

    /// Count an admitted file in the census by the parser it reads with.
    /// The engine asks again for each file it reads: a path counts once.
    fn count(&self, path: &Path) {
        if !self
            .counted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(path.to_path_buf())
        {
            return;
        }
        let extension = self.scope.parsers.extension(path).map_or_else(
            || octocode_engine::text::extension_of(&path.to_string_lossy(), true, ""),
            str::to_owned,
        );
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
            ignore_rules: self
                .external_rules
                .into_inner()
                .unwrap_or_else(PoisonError::into_inner)
                .into_iter()
                .collect(),
            untracked_ignore_dependencies: self.untracked_ignore_dependencies.into_inner(),
            census: self
                .census
                .into_inner()
                .unwrap_or_else(PoisonError::into_inner),
        }
    }
}
