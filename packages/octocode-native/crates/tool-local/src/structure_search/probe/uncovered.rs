//! What a walk left out of a listing, and which of a walk's skip causes that
//! proves.
use octocode_tools_shared::empty_walk::{Cause, Walk};

/// What a walk left out of a listing, root-relative (a directory's with a
/// trailing `/` where noted): `.gitignore`d entries (`/`), entries the path
/// policy withheld, dot entries skipped without `hidden` (`/`), the
/// directories the default prune skipped, and the entries it could not read
/// (how many, and how many of them for want of permission).
#[derive(Clone, Debug, Default)]
pub(in crate::structure_search) struct Uncovered {
    pub(in crate::structure_search) ignored: Vec<String>,
    pub(in crate::structure_search) withheld: octocode_runtime_base::policy::discovery::Withheld,
    pub(in crate::structure_search) hidden: Vec<String>,
    pub(in crate::structure_search) pruned: Vec<String>,
    pub(in crate::structure_search) unreadable: (u32, u32),
}

impl Uncovered {
    /// Whether the walk skipped anything a relaxed walk could list.
    pub(in crate::structure_search) fn saw_skips(&self) -> bool {
        !self.ignored.is_empty() || !self.hidden.is_empty() || !self.pruned.is_empty()
    }

    /// The causes a lead must lift to reach what this walk skipped:
    /// [`Self::skipped`], plus the dot rule for an ignored dot entry, and the
    /// default prune when ignored or dot entries were skipped, since a pruned
    /// directory behind them (a `.gitignore`d `target/`) is never seen as
    /// pruned.
    pub(in crate::structure_search) fn unproven(&self, walk: Walk) -> Vec<Cause> {
        let behind = !self.ignored.is_empty() || !self.hidden.is_empty();
        let ignored_dot = self.ignored.iter().any(|entry| {
            entry
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with('.'))
        });
        walk.skipping()
            .into_iter()
            .filter(|cause| match cause {
                Cause::Ignored => !self.ignored.is_empty(),
                Cause::Hidden => !self.hidden.is_empty() || ignored_dot,
                Cause::DefaultExcluded => behind || !self.pruned.is_empty(),
            })
            .collect()
    }
}
