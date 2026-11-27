//! What a directory scan's walk saw besides the files it parsed: the
//! directories a continuation page re-checks, the files no grammar parses, and
//! the per-extension census of the files it admitted.
use super::super::types::{Scope, Unparsed};
use octocode_runtime_base::policy::discovery::WalkAdmission;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_tools_shared::ast_rule::has_extension_in;
use octocode_tools_shared::leads::unparsed::Passed;
use octocode_tools_shared::test_code::Scoped;
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
    /// The `testCode` scope: a file on the side it leaves out is no file of
    /// the scope's, in the census or among the unparsed.
    scoped: Option<&'a Scoped>,
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
        scoped: Option<&'a Scoped>,
    ) -> Self {
        Self {
            scope,
            paths,
            admission,
            include_hidden,
            scoped,
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
            octocode_tools_shared::ast_rule::count_present(
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
        // grammar under `parser` is that grammar's, never this scope's, and
        // one where a `testCode` scope would leave it out is no gap of it
        // (asked uncounted: only a file the scan parses counts as left out).
        if unparsed_file
            && !has_extension_in(path, &self.grammars)
            && self.scoped.is_none_or(|scoped| scoped.lists(path, false))
        {
            // Named as a row path reads after the response stage: from
            // the workspace root, else absolute. Judged (its head read)
            // before the lock, so walker threads do not queue on the read.
            let shown = self
                .paths
                .workspace_relative(path)
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            if let Some(passed) = Passed::of(path, shown) {
                self.unparsed
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .add(passed);
            }
        }
        // A file the scan parses is read unless the `testCode` scope leaves
        // it out by where it lies (counted there): the census counts only
        // the files read, so the grammars an answer names are the ones it
        // searched. A directory is always walked.
        let admitted = !unparsed_file
            && (is_dir || self.scoped.is_none_or(|scoped| scoped.reads(path, false)));
        if admitted && met {
            self.count(path);
        }
        // Grammar filtering is not an explicit include override: ignored
        // source files remain ignored unless the caller overrides them.
        admitted
    }

    /// Whether the scan reads the file `path`: a grammar's (under `parser`,
    /// the requested one's, or a `parserGlobs` entry's).
    pub(super) fn reads(&self, path: &Path) -> bool {
        self.scope.parsers.extension(path).is_some()
            || (super::parses(self.scope, path) && has_extension_in(path, &self.grammars))
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
