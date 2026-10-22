//! What a listing walk leaves out (default prune, policy, the probe bound)
//! and how it counts it.

use super::*;

/// What a listing walk's policy callback leaves out, counted as the walk
/// runs: entries the path policy withheld, dot entries skipped without
/// `includeHidden`, and the default-pruned directories. The callback applies
/// the default prune itself ([`Prune::at`]), so a directory with a pruned
/// name that git tracks stays listed.
///
/// [`Prune::at`]: octocode_engine::search::walk::Prune::at
pub(super) struct WalkLeft {
    /// Entries the path policy withheld, by the walk position of the entry
    /// (and whether it is a directory).
    pub(super) withheld: std::sync::Mutex<Vec<(u32, std::path::PathBuf, bool)>>,
    /// Walk positions of the dot entries skipped without `includeHidden`.
    pub(super) hidden: std::sync::Mutex<Vec<u32>>,
    pub(super) root: std::path::PathBuf,
    /// The default prune; `None` with `defaultExcludes:false`.
    pub(super) prune: Option<octocode_engine::search::walk::RootedPrune>,
    /// Pruned directories, root-relative, in walk order, by walk position.
    pub(super) pruned: std::sync::Mutex<Vec<(u32, String)>>,
}

impl WalkLeft {
    pub(super) fn new(root: &std::path::Path, defaults: bool) -> Self {
        Self {
            withheld: std::sync::Mutex::default(),
            hidden: std::sync::Mutex::default(),
            root: root.to_path_buf(),
            prune: defaults.then(|| {
                octocode_engine::search::walk::Prune::new(prune::directories(true)).at(root)
            }),
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
        path != self.root
            && self
                .prune
                .as_ref()
                .is_some_and(|prune| prune.prunes(path, is_dir))
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
        let allowed = policy.discovery.permits_entry(path, file_type);
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
                self.count_hidden(position);
            }
            return true;
        }
        !self.prunes(path, file_type, visit)
    }

    pub(super) fn count_hidden(&self, position: u32) {
        self.hidden
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(position);
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
    /// the rest.
    pub(super) fn into_uncovered(self, ignored: usize, through: u32) -> Uncovered {
        let covered = |position: &u32| *position <= through;
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
                .iter()
                .filter(|position| covered(position))
                .count(),
            pruned: self
                .pruned
                .into_inner()
                .unwrap_or_else(|error| error.into_inner())
                .into_iter()
                .filter(|(position, _)| covered(position))
                .map(|(_, relative)| relative)
                .collect(),
        }
    }
}

/// What one listing walk's policy callback ([`WalkLeft::admit`]) decides
/// by: the probe bound (a probe walk only), the path policy, the filters a
/// withheld entry is weighed against, and the hidden and filtered flags.
pub(super) struct WalkPolicy<'a> {
    pub(super) bound: Option<&'a ProbeBound>,
    pub(super) discovery: crate::policy::path::DiscoveryWalk<'a>,
    pub(super) probe: FilterProbe,
    pub(super) show_hidden: bool,
    pub(super) filtered: bool,
}
