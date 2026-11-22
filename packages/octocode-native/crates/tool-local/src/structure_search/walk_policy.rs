//! What a listing walk leaves out (default prune, policy)
//! and how it counts it.
use super::probe::{FilterProbe, Uncovered};
use super::reports_withheld;
use super::walked::WalkMeta;
use octocode_engine::types::TestCodeExcluded;
use octocode_runtime_base::response::result::ToolError;
use serde_json::{Value, json};

/// What a `testCode` scope states on a listing window.
#[derive(Clone)]
pub(super) struct TestCodeWindow {
    pub(super) excluded: TestCodeExcluded,
    /// Rust files a verdict needed and could not read.
    pub(super) unread: Vec<std::path::PathBuf>,
}

/// What a listing walk's policy callback leaves out, counted as the walk
/// runs: entries the path policy withheld, dot entries skipped without
/// `includeHidden`, and the default-pruned directories. The callback decides
/// each entry by the shared walk admission ([`WalkAdmission`]: the cached
/// path policy, the rooted default prune with include lifting), so a
/// directory with a pruned name that git tracks stays listed. A listing is
/// an inventory: it lists generated files. What it left out is counted by
/// walk position, so a resumed window counts only its own entries.
///
/// [`WalkAdmission`]: octocode_runtime_base::policy::discovery::WalkAdmission
pub(super) struct WalkLeft<'a> {
    pub(super) admission: octocode_runtime_base::policy::discovery::WalkAdmission<'a>,
    /// Entries the path policy withheld, by the walk position of the entry
    /// (and whether it is a directory).
    pub(super) withheld: std::sync::Mutex<Vec<(u32, std::path::PathBuf)>>,
    /// The dot entries skipped without `includeHidden`, by walk position,
    /// root-relative (a directory's with a trailing `/`).
    pub(super) hidden: std::sync::Mutex<Vec<(u32, String)>>,
    pub(super) root: std::path::PathBuf,
    /// Pruned directories, root-relative, in walk order, by walk position.
    pub(super) pruned: std::sync::Mutex<Vec<(u32, String)>>,
}

/// What the windows of a walk-order listing left out through where it stands,
/// which the walk that continues from there reports with its own.
#[derive(Clone, Default)]
pub(super) struct LeftSoFar {
    pub(super) ignored: Vec<String>,
    withheld: Vec<(u32, std::path::PathBuf)>,
    hidden: Vec<(u32, String)>,
    pruned: Vec<(u32, String)>,
    pub(super) unreadable: (u32, u32),
    /// The walk positions a `testCode` scope left out.
    pub(super) test_code: Vec<u32>,
}

impl LeftSoFar {
    /// The heap this holds.
    pub(super) fn weight(&self) -> usize {
        self.ignored.iter().map(String::capacity).sum::<usize>()
            + self
                .withheld
                .iter()
                .map(|(_, path)| path.capacity())
                .sum::<usize>()
            + self
                .hidden
                .iter()
                .chain(&self.pruned)
                .map(|(_, name)| name.capacity())
                .sum::<usize>()
            + self.test_code.capacity() * std::mem::size_of::<u32>()
    }
}

impl<'a> WalkLeft<'a> {
    /// The listing walk of `root` under `paths`, with the default prune
    /// when `defaults` (less what the `include` globs name).
    pub(super) fn new(
        paths: &'a octocode_runtime_base::policy::path::PathPolicy,
        root: &std::path::Path,
        defaults: bool,
        include: &[String],
    ) -> Self {
        Self {
            admission: octocode_runtime_base::policy::discovery::WalkAdmission::new(
                paths.discovery_walk(),
                root,
                octocode_runtime_base::policy::discovery::WalkRules {
                    defaults,
                    include,
                    generated: false,
                    engine_prunes: false,
                },
            ),
            withheld: std::sync::Mutex::default(),
            hidden: std::sync::Mutex::default(),
            root: root.to_path_buf(),
            pruned: std::sync::Mutex::default(),
        }
    }

    /// Report what earlier windows left out along with this walk's.
    pub(super) fn carry(&mut self, so_far: &LeftSoFar) {
        *self
            .withheld
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = so_far.withheld.clone();
        *self
            .hidden
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = so_far.hidden.clone();
        *self
            .pruned
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = so_far.pruned.clone();
    }

    /// What the policy left out through walk position `through`; the engine
    /// counts the rest ([`LeftSoFar::ignored`], [`LeftSoFar::unreadable`]).
    pub(super) fn so_far(&self, through: u32) -> LeftSoFar {
        fn through_position<T: Clone>(
            kept: &std::sync::Mutex<Vec<(u32, T)>>,
            through: u32,
        ) -> Vec<(u32, T)> {
            kept.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .filter(|(position, _)| *position <= through)
                .cloned()
                .collect()
        }
        LeftSoFar {
            withheld: through_position(&self.withheld, through),
            hidden: through_position(&self.hidden, through),
            pruned: through_position(&self.pruned, through),
            ..LeftSoFar::default()
        }
    }

    /// Whether the default prune leaves out `path`, a directory below the
    /// root.
    pub(super) fn excludes(
        &self,
        path: &std::path::Path,
        file_type: Option<std::fs::FileType>,
    ) -> bool {
        let is_dir = file_type.map_or_else(|| path.is_dir(), |kind| kind.is_dir());
        is_dir && self.admission.excludes(path, true)
    }

    /// [`Self::excludes`], recording a fresh visit (a replayed one was
    /// recorded by the window it resumes).
    pub(super) fn prunes(
        &self,
        path: &std::path::Path,
        file_type: Option<std::fs::FileType>,
        visit: octocode_engine::search::fs_query::Visit,
    ) -> bool {
        let pruned = self.excludes(path, file_type);
        if let Some(position) = visit.fresh_at().filter(|_| pruned) {
            let relative = path.strip_prefix(&self.root).unwrap_or(path);
            self.pruned
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((position, octocode_engine::text::slash_path(relative)));
        }
        pruned
    }

    /// The policy callback of a listing walk (`tree` and `files` alike):
    /// whether the walk may enter or list `path`, counting what it leaves
    /// out. The root itself stays admitted: a probe out of time lists
    /// nothing under it, never fails. Dot entries out of view (no
    /// `includeHidden`) are neither listed nor counted as withheld; the ones
    /// `includeHidden:true` would walk (permitted, not ignored, not a pruned
    /// directory) are counted as hidden, and the walk's own dot rule skips
    /// them. Only an entry the filters could list makes a withheld one worth
    /// naming, and a replayed entry was counted by the window this one
    /// resumes.
    pub(super) fn admit(
        &self,
        policy: &WalkPolicy,
        path: &std::path::Path,
        file_type: Option<std::fs::FileType>,
        visit: octocode_engine::search::fs_query::Visit,
    ) -> bool {
        let dot = path
            .file_name()
            .is_some_and(|name| name.as_encoded_bytes().starts_with(b"."));
        let allowed = self.admission.permits(path, file_type);
        let fresh = visit.fresh_at();
        let unseen = if allowed {
            Self::unfollowed(path, file_type, policy.filtered)
        } else {
            policy.probe.could_list(path)
        };
        if let Some(position) = fresh
            && unseen
            && (policy.show_hidden || !dot)
            && reports_withheld(path)
        {
            self.record_withheld(position, path);
        }
        if !allowed {
            return false;
        }
        if dot && !policy.show_hidden {
            if let Some(position) = fresh
                && !self.excludes(path, file_type)
            {
                self.count_hidden(position, path, file_type);
            }
            return true;
        }
        !self.prunes(path, file_type, visit)
    }

    pub(super) fn count_hidden(
        &self,
        position: u32,
        path: &std::path::Path,
        file_type: Option<std::fs::FileType>,
    ) {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        let mut relative = octocode_engine::text::slash_path(relative);
        if file_type.map_or_else(|| path.is_dir(), |kind| kind.is_dir()) {
            relative.push('/');
        }
        self.hidden
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((position, relative));
    }

    pub(super) fn record_withheld(&self, position: u32, path: &std::path::Path) {
        self.withheld
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((position, path.to_path_buf()));
    }

    /// Whether a `filtered` listing walked past an admitted link to a
    /// directory: the walk follows no link, so the entries there are on no
    /// page, and a filter hides the link row too. It is disclosed like a
    /// withheld entry (`N symlinks not followed`).
    pub(super) fn unfollowed(
        path: &std::path::Path,
        file_type: Option<std::fs::FileType>,
        filtered: bool,
    ) -> bool {
        filtered
            && file_type.map_or_else(|| path.is_symlink(), |kind| kind.is_symlink())
            && path.is_dir()
    }

    /// What the walk left out through the entry at walk position `through`:
    /// the window ends there, and the window that resumes after it counts
    /// the rest. `ignored` names the ignored entries (a directory's with a
    /// trailing `/`): a directory the default prune also skips (in a git
    /// work tree it skips only ignored ones, `target/`) is named with the
    /// pruned directories too, since lifting the ignore rules alone does
    /// not reach it. `unreadable`: the entries the walk could not read, and
    /// how many of them for want of permission.
    pub(super) fn into_uncovered(
        self,
        ignored: Vec<String>,
        through: u32,
        unreadable: (u32, u32),
    ) -> Uncovered {
        let covered = |position: &u32| *position <= through;
        // The path policy comes before the ignore rules: an ignored entry it
        // denies is withheld (counted under its holding directory), never
        // named as ignored.
        let (ignored, denied): (Vec<String>, Vec<String>) =
            ignored.into_iter().partition(|entry| {
                let path = self.root.join(entry.trim_end_matches('/'));
                self.admission.permits(&path, None) || !reports_withheld(&path)
            });
        let behind_ignore = ignored
            .iter()
            .filter_map(|entry| entry.strip_suffix('/'))
            .filter(|dir| self.admission.excludes(&self.root.join(dir), true))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut withheld = octocode_runtime_base::policy::discovery::Withheld::default();
        for entry in &denied {
            withheld.record(&self.root.join(entry.trim_end_matches('/')));
        }
        for (_, path) in self
            .withheld
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|(position, ..)| covered(position))
        {
            withheld.record(path);
        }
        Uncovered {
            ignored,
            withheld,
            hidden: self
                .hidden
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .into_iter()
                .filter(|(position, _)| covered(position))
                .map(|(_, relative)| relative)
                .collect(),
            pruned: self
                .pruned
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .into_iter()
                .filter(|(position, _)| covered(position))
                .map(|(_, relative)| relative)
                .chain(behind_ignore)
                .collect(),
            unreadable,
        }
    }
}

/// What one listing walk's policy callback ([`WalkLeft::admit`]) decides
/// by: the filters a withheld entry is weighed against, and the hidden and
/// filtered flags.
pub(super) struct WalkPolicy {
    pub(super) probe: FilterProbe,
    pub(super) show_hidden: bool,
    pub(super) filtered: bool,
}

impl WalkMeta {
    /// The walk's `summary` (its counts) and `skippedPaths`: what it left
    /// out of `root`, the ignored, hidden and default-pruned entries (their
    /// names stay out of the listing, but their absence is not silent), what
    /// the path policy withheld and the entries it could not read. One
    /// counted entry per cause and rule, its first path named; `sampled` too when the
    /// walk stopped before its end. A resumed walk (`walkOffset`) states the
    /// entries it read. The summary counts the rows `pagination.total` counts:
    /// every ranked row when the count is `complete` (`whole`), else this
    /// window's.
    pub(super) fn disclose(
        &self,
        out: &mut Value,
        root: &std::path::Path,
        whole: bool,
    ) -> Result<(), ToolError> {
        use octocode_contract_types::SkippedPathCause::{Hidden, Ignored};
        use octocode_tools_shared::leads::skipped::{self, under};
        let tally = self.ranked.filter(|_| whole).unwrap_or(self.window);
        out["summary"] = json!({"fileCount": tally.files, "dirCount": tally.dirs});
        // A walk that read no sizes (a view that shows only names) counts no
        // bytes.
        if !self.names_only {
            out["summary"]["bytes"] = json!(tally.bytes);
        }
        if let Some(window) = &self.test_code {
            if let Some(excluded) = window.excluded.stated() {
                out["testCodeExcluded"] = serde_json::to_value(excluded).unwrap_or_default();
            }
            if let Some(note) = octocode_tools_shared::test_code::unread_note(&window.unread) {
                octocode_runtime_base::response::paging::mark_partial(
                    out,
                    octocode_runtime_base::response::paging::PartialReason::FilesUnevaluated,
                    &[note],
                );
            }
        }
        let left = &self.uncovered;
        let walked = under(root, Ignored, &left.ignored)
            .chain(under(root, Hidden, &left.hidden))
            .chain(skipped::default_excluded(
                root,
                &left.pruned,
                std::iter::empty(),
            ))
            .chain(skipped::unlifted(
                &root.to_string_lossy(),
                &left.withheld,
                left.unreadable.into(),
            ));
        let incomplete = self.was_capped || self.time_cut.is_some();
        if let Some(paths) = skipped::skipped_paths(walked, false, incomplete)? {
            out["skippedPaths"] = serde_json::to_value(paths).unwrap_or_default();
        }
        Ok(())
    }
}
