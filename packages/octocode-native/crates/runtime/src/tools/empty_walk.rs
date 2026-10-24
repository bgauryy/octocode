//! Why a local walk came back empty: the one diagnosis `localSearch`,
//! `structureSearch` and `astSearch` share.
//!
//! A walk skips three kinds of path: `.gitignore`d ones, hidden (dot) ones and
//! default-excluded ones (`node_modules/`, `build/`, …). An empty result may
//! only mean the thing sits behind one of them. [`diagnose`] asks a
//! tool-supplied probe (the tool's own walk, files only, relaxed) which of the
//! three held matches, and returns the *minimal* set to relax: a cause is in
//! the answer only when relaxing it finds something, never all three because
//! one did. The [`Diagnosis`] names the deciding ignore rule, hidden entry or
//! excluded directory, and sets exactly those flags on the tool's lead query.
//! Each tool builds the lead itself through its own continuation builder.

use octocode_engine::error::Error;
use octocode_engine::search::walk::{IgnorePolicy, Prune, walk_builder};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

mod diagnosis;
mod reasons;
mod tagged;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tool_tests;

pub(crate) use diagnosis::{diagnose, diagnose_shown};
pub(crate) use reasons::{ignore_rule, ignored_directories};
pub(crate) use tagged::{FileProbe, Tagged};

/// Wall-clock bound of one relaxed probe walk behind a local result (a
/// [`FileProbe`], localSearch's re-walks, structureSearch's listing probes):
/// past it the probe is cut and absence behind the skipped paths unproven.
/// The one budget every local tool's probes share.
pub(crate) const PROBE_BUDGET: std::time::Duration = std::time::Duration::from_millis(400);

#[cfg(test)]
thread_local! {
    static TEST_PROBE_BUDGET: std::cell::Cell<Option<std::time::Duration>> =
        const { std::cell::Cell::new(None) };
}

/// [`PROBE_BUDGET`], or a test's own on this thread ([`with_probe_budget`]).
pub(crate) fn probe_budget() -> std::time::Duration {
    #[cfg(test)]
    if let Some(budget) = TEST_PROBE_BUDGET.with(std::cell::Cell::get) {
        return budget;
    }
    PROBE_BUDGET
}

/// Runs `body` with [`probe_budget`] set to `budget` on this thread.
#[cfg(test)]
pub(crate) fn with_probe_budget<T>(budget: std::time::Duration, body: impl FnOnce() -> T) -> T {
    TEST_PROBE_BUDGET.with(|cell| cell.set(Some(budget)));
    let out = body();
    TEST_PROBE_BUDGET.with(|cell| cell.set(None));
    out
}

/// Files of a probe's finding kept to name skip reasons.
pub(crate) const SAMPLE_FILES: usize = 8;
/// Distinct skip reasons one hint names; the rest is a count.
const NAMED_REASONS: usize = 3;

/// Why a path is out of a walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    /// A `.gitignore`, `.ignore` or git exclude rule.
    Ignored,
    /// A dot entry, walked only with `includeHidden`.
    Hidden,
    /// A directory (or generated file) the default excludes prune.
    DefaultExcluded,
}

impl Cause {
    /// This cause's bit in a file's [`FileProbe`] tag.
    fn bit(self) -> u8 {
        match self {
            Self::Ignored => 1,
            Self::Hidden => 2,
            Self::DefaultExcluded => 4,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Ignored => "ignored",
            Self::Hidden => "hidden",
            Self::DefaultExcluded => "default-excluded",
        }
    }

    /// The query field that lifts this cause, and its value.
    pub(crate) fn flag(self) -> (&'static str, Value) {
        match self {
            Self::Ignored => ("includeIgnored", json!(true)),
            Self::Hidden => ("includeHidden", json!(true)),
            Self::DefaultExcluded => ("defaultExcludes", json!(false)),
        }
    }
}

/// Which paths a walk takes: the request's flags, or a relaxed copy of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Walk {
    /// `includeIgnored`: `.gitignore`d paths are walked.
    pub(crate) ignored: bool,
    /// `includeHidden`: dot paths are walked.
    pub(crate) hidden: bool,
    /// `defaultExcludes`: the default directory prune applies.
    pub(crate) default_excludes: bool,
}

impl Walk {
    /// The causes this walk skips by.
    pub(crate) fn skipping(self) -> Vec<Cause> {
        [
            (!self.ignored, Cause::Ignored),
            (!self.hidden, Cause::Hidden),
            (self.default_excludes, Cause::DefaultExcluded),
        ]
        .into_iter()
        .filter_map(|(skips, cause)| skips.then_some(cause))
        .collect()
    }

    /// The engine ignore policy of this walk.
    pub(crate) fn policy(self) -> IgnorePolicy {
        IgnorePolicy {
            include_ignored: self.ignored,
            include_hidden: self.hidden,
        }
    }

    /// The engine walk of this scope that never descends into a `prune`d
    /// directory, rooted as every local walk roots its default prune
    /// ([`Prune::at`]: inside a git work tree only where git ignores it):
    /// ignore rules and hidden entries follow the flags, and `globs` scope
    /// it like the tool's own scan.
    pub(crate) fn walker(
        self,
        root: &Path,
        prune: &[String],
        globs: Globs<'_>,
    ) -> Result<ignore::Walk, Error> {
        let mut builder = walk_builder(root, self.policy(), None);
        let prune = Prune::new(prune).at(root);
        builder.filter_entry(move |entry| {
            entry.depth() == 0
                || !prune.prunes(
                    entry.path(),
                    entry.file_type().is_some_and(|kind| kind.is_dir()),
                )
        });
        builder.overrides(globs.overrides(root)?);
        Ok(builder.build())
    }

    /// The tag bits of the causes this walk lifts.
    fn lifted_bits(self) -> u8 {
        [Cause::Ignored, Cause::Hidden, Cause::DefaultExcluded]
            .into_iter()
            .filter(|cause| !self.skipping().contains(cause))
            .fold(0, |bits, cause| bits | cause.bit())
    }

    /// This walk with `causes` lifted.
    pub(crate) fn lifting(mut self, causes: &[Cause]) -> Self {
        for cause in causes {
            match cause {
                Cause::Ignored => self.ignored = true,
                Cause::Hidden => self.hidden = true,
                Cause::DefaultExcluded => self.default_excludes = false,
            }
        }
        self
    }
}

/// A request's `include` and `exclude` globs: the scope its own scan applies,
/// so a probe never reports files the request filtered out.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Globs<'a> {
    pub(crate) include: &'a [String],
    pub(crate) exclude: &'a [String],
}

impl Globs<'_> {
    /// The override set the engine's walks apply, rooted at `root`.
    fn overrides(self, root: &Path) -> Result<ignore::overrides::Override, Error> {
        Ok(octocode_engine::search::glob::compile(root, self.include, self.exclude)?.overrides())
    }
}

/// Sets the lifting flag of each of `causes` on a lead query that spells walk
/// flags like the contract does (`includeIgnored`, `includeHidden`,
/// `defaultExcludes`), and nothing else.
pub(crate) fn lift(query: &mut Value, causes: &[Cause]) {
    if let Some(fields) = query.as_object_mut() {
        for cause in causes {
            let (field, value) = cause.flag();
            fields.insert(field.to_owned(), value);
        }
    }
}

/// What a probe's relaxed walk found.
#[derive(Clone, Debug, Default)]
pub(crate) struct Hits {
    /// Files (or matches) found.
    pub(crate) count: usize,
    /// The first of them, absolute, at most [`SAMPLE_FILES`]: what names the
    /// skip reasons.
    pub(crate) sample: Vec<PathBuf>,
    /// The probe stopped early (deadline or cap): there may be more, or some.
    pub(crate) cut: bool,
}

/// One skipped path's reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Skip {
    pub(crate) cause: Cause,
    /// The topmost skipped entry on the way to the file: the directory (or
    /// file) whose own scope shows what the walk left out.
    pub(crate) entry: PathBuf,
    /// The deciding ignore rule (`gen/.gitignore:2 '*'`), the hidden entry
    /// (`hidden .notes/`) or the excluded directory (`build/`).
    pub(crate) reason: String,
}

/// The skip causes whose relaxation found matches.
#[derive(Clone, Debug)]
pub(crate) struct Diagnosis {
    causes: Vec<Cause>,
    found: Hits,
    skips: Vec<Skip>,
}

impl Diagnosis {
    /// The minimal causes to lift, in [`Cause`] order.
    pub(crate) fn causes(&self) -> &[Cause] {
        &self.causes
    }

    /// What the probe found with exactly [`Self::causes`] lifted.
    pub(crate) fn found(&self) -> &Hits {
        &self.found
    }

    /// The distinct skipped entries behind the sampled files, first-found first.
    pub(crate) fn skips(&self) -> &[Skip] {
        &self.skips
    }

    /// The hint's reason: the named rules, entries and directories (the first
    /// few, then a count), else the causes.
    pub(crate) fn because(&self) -> String {
        if self.skips.is_empty() {
            return self.labels();
        }
        let shown = crate::response::pages::first_named(
            self.skips.iter().map(|skip| skip.reason.clone()),
            NAMED_REASONS,
        );
        // Every lifted cause is named: one no sampled file's reason states
        // (a file behind two causes, past the sample) by its label.
        let unnamed = self
            .causes
            .iter()
            .filter(|cause| !self.skips.iter().any(|skip| skip.cause == **cause))
            .map(|cause| cause.label())
            .collect::<Vec<_>>();
        if unnamed.is_empty() {
            shown
        } else {
            format!("{shown}; also {}", unnamed.join(", "))
        }
    }

    /// The causes, `ignored, hidden`.
    pub(crate) fn labels(&self) -> String {
        self.causes
            .iter()
            .map(|cause| cause.label())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// What an empty walk's probes concluded.
#[derive(Debug)]
pub(crate) enum Verdict {
    /// Nothing sits behind the skipped paths (or nothing was skipped).
    Absent,
    /// The probe was cut before it found anything: absence is unproven.
    Unproven,
    /// These causes held matches.
    Skipped(Diagnosis),
}
