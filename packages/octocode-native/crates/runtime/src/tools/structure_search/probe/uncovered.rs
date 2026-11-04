//! What a walk left out of a listing, and which of a walk's skip causes that
//! proves.
use crate::tools::empty_walk::{Cause, Walk};

/// What a walk left out of a listing, root-relative (a directory's with a
/// trailing `/` where noted): `.gitignore`d entries (`/`), entries the path
/// policy withheld, dot entries skipped without `hidden` (`/`), the
/// directories the default prune skipped, and the entries it could not read
/// (how many, and how many of them for want of permission).
#[derive(Clone, Debug, Default)]
pub(in crate::tools::structure_search) struct Uncovered {
    pub(in crate::tools::structure_search) ignored: Vec<String>,
    pub(in crate::tools::structure_search) withheld: crate::policy::discovery::Withheld,
    pub(in crate::tools::structure_search) hidden: Vec<String>,
    pub(in crate::tools::structure_search) pruned: Vec<String>,
    pub(in crate::tools::structure_search) unreadable: (u32, u32),
}

impl Uncovered {
    /// The distinct names of the default-pruned directories, sorted.
    pub(in crate::tools::structure_search) fn pruned_names(&self) -> Option<Vec<String>> {
        if self.pruned.is_empty() {
            return None;
        }
        let mut names = self
            .pruned
            .iter()
            .map(|dir| dir.rsplit('/').next().unwrap_or(dir).to_owned())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        Some(names)
    }

    /// Whether the walk skipped anything a relaxed walk could list.
    pub(in crate::tools::structure_search) fn saw_skips(&self) -> bool {
        !self.ignored.is_empty() || !self.hidden.is_empty() || !self.pruned.is_empty()
    }

    /// The causes of `walk` this walk skipped entries by, in [`Cause`] order.
    pub(in crate::tools::structure_search) fn skipped(&self, walk: Walk) -> Vec<Cause> {
        walk.skipping()
            .into_iter()
            .filter(|cause| match cause {
                Cause::Ignored => !self.ignored.is_empty(),
                Cause::Hidden => !self.hidden.is_empty(),
                Cause::DefaultExcluded => !self.pruned.is_empty(),
            })
            .collect()
    }

    /// The causes a lead must lift to reach what this walk skipped when no
    /// probe says where entries sit: [`Self::skipped`], plus the default
    /// prune when ignored or dot entries were skipped, since a pruned
    /// directory behind them (a `.gitignore`d `target/`) is never seen as
    /// pruned.
    pub(in crate::tools::structure_search) fn unproven(&self, walk: Walk) -> Vec<Cause> {
        let behind = !self.ignored.is_empty() || !self.hidden.is_empty();
        walk.skipping()
            .into_iter()
            .filter(|cause| match cause {
                Cause::Ignored => !self.ignored.is_empty(),
                Cause::Hidden => !self.hidden.is_empty(),
                Cause::DefaultExcluded => behind || !self.pruned.is_empty(),
            })
            .collect()
    }

    /// `causes` as a hint names them: the pruned directories by name, the
    /// rest by label.
    pub(in crate::tools::structure_search) fn named(&self, causes: &[Cause]) -> String {
        causes
            .iter()
            .map(|cause| match (cause, self.pruned_names()) {
                (Cause::DefaultExcluded, Some(names)) => names
                    .iter()
                    .map(|name| format!("{name}/"))
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => cause.label().to_owned(),
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}
