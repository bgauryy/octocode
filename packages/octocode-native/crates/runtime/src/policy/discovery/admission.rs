use std::path::Path;

use super::{Withheld, generated_file_pattern};

/// What a [`WalkAdmission`] left out: the one skip ledger of a local walk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WalkSkips {
    pub withheld: Withheld,
    /// Default-pruned directories, each by its path under the walk root.
    pub pruned: std::collections::BTreeSet<String>,
    /// Default-excluded files (`*.min.js`, `.DS_Store`), each by its path
    /// under the walk root.
    pub generated: std::collections::BTreeSet<String>,
    /// Dot entries left out without `includeHidden`, each by its path under
    /// the walk root, a directory's with a trailing `/` (its contents were
    /// never walked). One the path policy withholds is not here: no flag
    /// reaches it.
    pub hidden: std::collections::BTreeSet<String>,
    /// Some dot entry would also be default-excluded (`.github/`, `.cache/`,
    /// `.DS_Store`): reaching it lifts both.
    pub(crate) hidden_excluded: bool,
    /// Entries an ignore rule left out without `includeIgnored`, each by its
    /// path under the walk root, a directory's with a trailing `/` (its
    /// contents were never walked). One the path policy withholds is not
    /// here: no flag reaches it.
    pub ignored: std::collections::BTreeSet<String>,
    /// Some ignored entry would also be default-excluded (an ignored
    /// `node_modules/`) or is a dot entry: reaching it lifts those too.
    pub(crate) ignored_excluded: bool,
    pub(crate) ignored_hidden: bool,
}

impl WalkSkips {
    /// The skipped paths counted per cause (`2 ignored, 1 hidden`).
    pub(crate) fn counts(&self) -> String {
        [
            (self.ignored.len(), "ignored"),
            (self.hidden.len(), "hidden"),
            (self.pruned.len() + self.generated.len(), "default-excluded"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, cause)| format!("{count} {cause}"))
        .collect::<Vec<_>>()
        .join(", ")
    }
}

/// What one local walk skips by default, as its request spells it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WalkRules<'r> {
    /// `defaultExcludes`: the default prune (and generated files) apply.
    pub(crate) defaults: bool,
    /// The include globs as the walk applies them
    /// ([`crate::policy::include::include_globs`]): a glob that names a pruned
    /// directory lifts its prune, and only skipped entries one of them could
    /// reach are recorded.
    pub(crate) include: &'r [String],
    /// Generated files (`*.min.js`, `*.map`, `.DS_Store`, archives) are
    /// default-excluded with the directories. Off only for an inventory
    /// listing (`structureSearch`), which lists what exists.
    pub(crate) generated: bool,
    /// The engine walk prunes the default directories itself (its
    /// `default_exclude_dir` is [`crate::policy::prune::walk_names`] of these same
    /// rules) and reports what it pruned: [`WalkAdmission::admit`] then
    /// leaves directories to it.
    pub(crate) engine_prunes: bool,
}

/// The one admission of a local walk (`localSearch`, `structureSearch`,
/// `astSearch` symbols and match): the cached path-policy decision
/// ([`DiscoveryWalk`]), links never followed, the rooted default prune with
/// include lifting ([`crate::policy::prune::rooted`]), the generated-file excludes,
/// and one skip ledger ([`WalkSkips`]) of what it left out. A tool supplies
/// only its own relevance: which denied entries are worth disclosing.
///
/// [`DiscoveryWalk`]: crate::policy::path::DiscoveryWalk
pub(crate) struct WalkAdmission<'a> {
    discovery: crate::policy::path::DiscoveryWalk<'a>,
    /// The walked root: a file named as the root is always walked.
    root: std::path::PathBuf,
    /// The default prune; `None` with `defaultExcludes:false`.
    prune: Option<octocode_engine::search::walk::RootedPrune>,
    /// Generated files are default-excluded ([`WalkRules::generated`]).
    generated: bool,
    /// [`WalkRules::engine_prunes`].
    engine_prunes: bool,
    /// The entries that could hold a path the include globs match: only
    /// those are recorded as skipped.
    scope: octocode_engine::search::glob::IncludeScope,
    skipped: std::sync::Mutex<WalkSkips>,
}

/// Whether a walked entry is a link (by its listing type when the walk has
/// it): no local walk follows one.
fn is_link(path: &Path, file_type: Option<std::fs::FileType>) -> bool {
    file_type.map_or_else(|| path.is_symlink(), |kind| kind.is_symlink())
}

impl<'a> WalkAdmission<'a> {
    /// The admission of a walk of `root` under `discovery` by `rules`.
    pub(crate) fn new(
        discovery: crate::policy::path::DiscoveryWalk<'a>,
        root: &Path,
        rules: WalkRules<'_>,
    ) -> Self {
        Self {
            prune: crate::policy::prune::rooted(root, rules.defaults, rules.include),
            generated: rules.defaults && rules.generated,
            engine_prunes: rules.engine_prunes,
            scope: octocode_engine::search::glob::IncludeScope::new(root, rules.include),
            discovery,
            root: root.to_path_buf(),
            skipped: std::sync::Mutex::new(WalkSkips::default()),
        }
    }

    /// Whether the path policy admits `path` ([`DiscoveryWalk::permits_entry`]).
    ///
    /// [`DiscoveryWalk::permits_entry`]: crate::policy::path::DiscoveryWalk::permits_entry
    pub(crate) fn permits(&self, path: &Path, file_type: Option<std::fs::FileType>) -> bool {
        self.discovery.permits_entry(path, file_type)
    }

    /// Whether the default excludes leave out `path`: a pruned directory or
    /// a generated file below the root (the root itself is walked).
    pub(crate) fn excludes(&self, path: &Path, is_dir: bool) -> bool {
        if path == self.root {
            return false;
        }
        if is_dir {
            self.prune
                .as_ref()
                .is_some_and(|prune| prune.prunes(path, true))
        } else {
            self.generated
                && path
                    .file_name()
                    .is_some_and(|name| generated_file_pattern(&name.to_string_lossy()).is_some())
        }
    }

    /// [`Self::excludes`], recording a left-out entry the include globs
    /// could reach.
    pub(crate) fn exclude(&self, path: &Path, is_dir: bool) -> bool {
        if !self.excludes(path, is_dir) {
            return false;
        }
        if self.in_scope(path, is_dir) {
            let shown = self.shown(path);
            self.note(|skips| {
                if is_dir {
                    skips.pruned.insert(shown);
                } else {
                    skips.generated.insert(shown);
                }
            });
        }
        true
    }

    /// Count one entry the path policy withheld, or a link the walk did
    /// not follow ([`Withheld::record`]).
    pub(crate) fn withhold(&self, path: &Path, is_dir: bool) {
        self.note(|skips| skips.withheld.record(path, is_dir));
    }

    /// The verdict on one walked entry: a link is never followed and a
    /// denied entry never walked (each recorded when `relevant` says it
    /// could have mattered: a directory, or a file the tool reads), a
    /// default-excluded one is recorded and left out; anything else is
    /// walked.
    pub(crate) fn admit(
        &self,
        path: &Path,
        file_type: Option<std::fs::FileType>,
        relevant: impl FnOnce(bool) -> bool,
    ) -> bool {
        let is_dir = || file_type.map_or_else(|| path.is_dir(), |kind| kind.is_dir());
        if is_link(path, file_type) || !self.permits(path, file_type) {
            let is_dir = path.is_dir();
            if relevant(is_dir) {
                self.withhold(path, is_dir);
            }
            return false;
        }
        let is_dir = is_dir();
        (is_dir && self.engine_prunes) || !self.exclude(path, is_dir)
    }

    /// What the walk left out so far.
    pub(crate) fn skipped(&self) -> WalkSkips {
        self.skipped
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn note(&self, record: impl FnOnce(&mut WalkSkips)) {
        record(
            &mut self
                .skipped
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
    }

    /// An entry a walk rule (hidden, ignored) left out, as the skips record
    /// it: its path under the root (a directory's with a trailing `/`) and
    /// whether the default excludes would leave it out too; `None` for one
    /// the path policy withholds, which no flag reaches.
    fn skipped_entry(&self, path: &Path, is_dir: bool) -> Option<(String, bool)> {
        if !self.permits(path, None) || !self.in_scope(path, is_dir) {
            return None;
        }
        let excluded = self.excludes(path, is_dir);
        let mut shown = self.shown(path);
        if is_dir {
            shown.push('/');
        }
        Some((shown, excluded))
    }

    /// Whether a skipped `path` could hold a path the include globs match.
    fn in_scope(&self, path: &Path, is_dir: bool) -> bool {
        self.scope
            .reaches(path, path.strip_prefix(&self.root).unwrap_or(path), is_dir)
    }

    /// `path` under the walk root, with `/` separators.
    fn shown(&self, path: &Path) -> String {
        octocode_engine::text::slash_path(path.strip_prefix(&self.root).unwrap_or(path))
    }
}

impl WalkAdmission<'static> {
    /// A text search's admission of `root` under `policy` (generated files
    /// default-excluded with the directories when `defaults`), shared by the
    /// walk's threads.
    pub(crate) fn search(
        policy: crate::policy::path::PathPolicy,
        root: std::path::PathBuf,
        defaults: bool,
        include: &[String],
    ) -> Self {
        Self::new(
            crate::policy::path::DiscoveryWalk::owned(policy),
            &root,
            WalkRules {
                defaults,
                include,
                generated: true,
                engine_prunes: false,
            },
        )
    }
}

/// A text search walk's path filter: every entry is admitted by
/// [`WalkAdmission::admit`] (every denied entry and unfollowed link
/// recorded), and the dot and ignored entries the walk's own rules left out
/// are recorded too.
impl octocode_engine::search::text_search::TextSearchPathFilter for WalkAdmission<'static> {
    fn allows(&self, path: &Path, _is_dir: bool) -> bool {
        self.allows_entry(
            path,
            std::fs::symlink_metadata(path)
                .ok()
                .map(|meta| meta.file_type()),
        )
    }

    fn allows_entry(&self, path: &Path, file_type: Option<std::fs::FileType>) -> bool {
        self.admit(path, file_type, |_| true)
    }

    fn skipped_hidden(&self, path: &Path, is_dir: bool) {
        if let Some((shown, excluded)) = self.skipped_entry(path, is_dir) {
            self.note(|skips| {
                skips.hidden.insert(shown);
                skips.hidden_excluded |= excluded;
            });
        }
    }

    fn skipped_ignored(&self, path: &Path, is_dir: bool) {
        if let Some((shown, excluded)) = self.skipped_entry(path, is_dir) {
            let hidden = path
                .file_name()
                .is_some_and(|name| name.as_encoded_bytes().first() == Some(&b'.'));
            self.note(|skips| {
                skips.ignored.insert(shown);
                skips.ignored_excluded |= excluded;
                skips.ignored_hidden |= hidden;
            });
        }
    }

    fn skipped_symlink(&self, path: &Path) {
        self.withhold(path, false);
    }
}
