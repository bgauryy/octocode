//! What a listing walk leaves out (default prune, policy, the probe bound)
//! and how it counts it.

use super::*;

/// What a listing walk's policy callback leaves out, counted as the walk
/// runs: entries the path policy withheld, dot entries skipped without
/// `includeHidden`, and the default-pruned directories. The callback decides
/// each entry by the shared walk admission ([`WalkAdmission`]: the cached
/// path policy, the rooted default prune with include lifting), so a
/// directory with a pruned name that git tracks stays listed. A listing is
/// an inventory: it lists generated files. What it left out is counted by
/// walk position, so a resumed window counts only its own entries.
///
/// [`WalkAdmission`]: crate::policy::discovery::WalkAdmission
pub(super) struct WalkLeft<'a> {
    pub(super) admission: crate::policy::discovery::WalkAdmission<'a>,
    /// Entries the path policy withheld, by the walk position of the entry
    /// (and whether it is a directory).
    pub(super) withheld: std::sync::Mutex<Vec<(u32, std::path::PathBuf, bool)>>,
    /// The dot entries skipped without `includeHidden`, by walk position,
    /// root-relative (a directory's with a trailing `/`).
    pub(super) hidden: std::sync::Mutex<Vec<(u32, String)>>,
    pub(super) root: std::path::PathBuf,
    /// Pruned directories, root-relative, in walk order, by walk position.
    pub(super) pruned: std::sync::Mutex<Vec<(u32, String)>>,
}

impl<'a> WalkLeft<'a> {
    /// The listing walk of `root` under `paths`, with the default prune
    /// when `defaults` (less what the `include` globs name).
    pub(super) fn new(
        paths: &'a crate::policy::path::PathPolicy,
        root: &std::path::Path,
        defaults: bool,
        include: &[String],
    ) -> Self {
        Self {
            admission: crate::policy::discovery::WalkAdmission::new(
                paths.discovery_walk(),
                root,
                crate::policy::discovery::WalkRules {
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
                .unwrap_or_else(|error| error.into_inner())
                .push((position, relative.to_string_lossy().replace('\\', "/")));
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
        policy: &WalkPolicy<'_>,
        path: &std::path::Path,
        file_type: Option<std::fs::FileType>,
        visit: octocode_engine::search::fs_query::Visit,
    ) -> bool {
        if path != self.root
            && policy
                .bound
                .is_some_and(|bound| !bound.admits(path, file_type))
        {
            return false;
        }
        let dot = display_name(path).starts_with('.');
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
            .unwrap_or_else(|error| error.into_inner())
            .push((position, relative));
    }

    pub(super) fn record_withheld(&self, position: u32, path: &std::path::Path) {
        self.withheld
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((position, path.to_path_buf(), path.is_dir()));
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
        let behind_ignore = ignored
            .iter()
            .filter_map(|entry| entry.strip_suffix('/'))
            .filter(|dir| self.excludes(&self.root.join(dir), None))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut withheld = crate::policy::discovery::Withheld::default();
        for (_, path, is_dir) in self
            .withheld
            .into_inner()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|(position, ..)| covered(position))
        {
            withheld.record(path, *is_dir);
        }
        Uncovered {
            ignored,
            withheld,
            hidden: self
                .hidden
                .into_inner()
                .unwrap_or_else(|error| error.into_inner())
                .into_iter()
                .filter(|(position, _)| covered(position))
                .map(|(_, relative)| relative)
                .collect(),
            pruned: self
                .pruned
                .into_inner()
                .unwrap_or_else(|error| error.into_inner())
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
/// by: the probe bound (a probe walk only), the filters a
/// withheld entry is weighed against, and the hidden and filtered flags.
pub(super) struct WalkPolicy<'a> {
    pub(super) bound: Option<&'a ProbeBound>,
    pub(super) probe: FilterProbe,
    pub(super) show_hidden: bool,
    pub(super) filtered: bool,
}

impl WalkMeta {
    /// The walk's `summary` (its counts) and `skippedPaths`: what it left
    /// out of `root`, the ignored, hidden and default-pruned entries (their
    /// names stay out of the listing, but their absence is not silent), what
    /// the path policy withheld and the entries it could not read. One
    /// entry per cause, or every path with `details`; `sampled` too when the
    /// walk stopped before its end. A resumed walk (`walkOffset`) states the
    /// entries it read.
    pub(super) fn disclose(
        &self,
        out: &mut Value,
        root: &std::path::Path,
        details: bool,
    ) -> Result<(), ToolError> {
        use crate::tools::leads::skipped::{self, under};
        use octocode_contract_types::SkippedPathCause::{DefaultExcluded, Hidden, Ignored};
        out["summary"] = json!({"files": self.files, "dirs": self.dirs, "bytes": self.bytes});
        let left = &self.uncovered;
        let walked = under(root, Ignored, &left.ignored)
            .chain(under(root, Hidden, &left.hidden))
            .chain(under(root, DefaultExcluded, &left.pruned))
            .chain(skipped::unlifted(
                &root.to_string_lossy(),
                &left.withheld,
                left.unreadable.into(),
            ));
        let incomplete = self.was_capped || self.time_cut.is_some();
        if let Some(paths) = skipped::skipped_paths(walked, details, incomplete)? {
            out["skippedPaths"] = serde_json::to_value(paths).unwrap_or_default();
        }
        Ok(())
    }
}
