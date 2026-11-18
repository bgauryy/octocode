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
use octocode_engine::search::walk::{GlobalIgnore, Prune, walk_builder_under};
use octocode_engine::types::IgnorePolicy;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub mod diagnosis;
pub mod reasons;
pub mod tagged;
#[cfg(test)]
mod tests;

pub use diagnosis::{diagnose, diagnose_shown};
pub use reasons::{ignore_rule, keeping_out};
pub use tagged::{FileProbe, Tagged};

/// The work one relaxed probe walk behind a local result may do (a
/// [`FileProbe`], localSearch's re-walks and alternate spellings,
/// structureSearch's listing probes): past it the probe is cut and absence
/// behind the skipped paths unproven. The one budget every local tool's
/// probes share.
///
/// Counted in work, never in time, so the same tree gives the same finding
/// on every surface and under any load: each probe walks in a fixed order and
/// is charged per entry it admits and per file byte it reads. No earlier
/// signal proves absence (`Absent` needs the relaxed walk to finish), and
/// finishing costs what the skipped paths hold, not what the request asked
/// for: a dependency or build tree holds far more than any request walks, so
/// the budget is sized to a small project, which a probe finishes. A cut probe
/// is disclosed (`Verdict::Unproven`, `Hits::cut`, "at least") with the lead
/// that lifts the flags, which runs the full walk under the request's deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbeLimits {
    /// Walk entries (files and directories) a probe admits.
    pub entries: usize,
    /// File bytes a probe reads.
    pub bytes: u64,
}

pub(crate) const PROBE_LIMITS: ProbeLimits = ProbeLimits {
    entries: 10_000,
    bytes: 64 * 1024 * 1024,
};

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static TEST_PROBE_LIMITS: std::cell::Cell<Option<ProbeLimits>> =
        const { std::cell::Cell::new(None) };
}

/// [`PROBE_LIMITS`], or a test's own on this thread ([`with_probe_limits`]).
pub(crate) fn probe_limits() -> ProbeLimits {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(limits) = TEST_PROBE_LIMITS.with(std::cell::Cell::get) {
        return limits;
    }
    PROBE_LIMITS
}

/// Runs `body` with [`probe_limits`] set to `limits` on this thread.
#[cfg(any(test, feature = "test-support"))]
pub fn with_probe_limits<T>(limits: ProbeLimits, body: impl FnOnce() -> T) -> T {
    TEST_PROBE_LIMITS.with(|cell| cell.set(Some(limits)));
    let out = body();
    TEST_PROBE_LIMITS.with(|cell| cell.set(None));
    out
}

/// Probe limits no test fixture reaches: the probe finishes.
#[cfg(any(test, feature = "test-support"))]
pub const FINISHES: ProbeLimits = ProbeLimits {
    entries: usize::MAX,
    bytes: u64::MAX,
};

/// Probe limits that admit nothing: the probe is cut at its first entry.
#[cfg(any(test, feature = "test-support"))]
pub const CUT_AT_ONCE: ProbeLimits = ProbeLimits {
    entries: 0,
    bytes: 0,
};

/// One probe's spend against [`probe_limits`], charged in its walk order:
/// once an entry does not fit, the probe is cut there and stays cut.
#[derive(Debug)]
pub struct ProbeWork {
    limits: ProbeLimits,
    entries: std::sync::atomic::AtomicUsize,
    bytes: std::sync::atomic::AtomicU64,
    cut: std::sync::atomic::AtomicBool,
}

impl Default for ProbeWork {
    fn default() -> Self {
        Self::new()
    }
}

impl ProbeWork {
    pub fn new() -> Self {
        Self {
            limits: probe_limits(),
            entries: std::sync::atomic::AtomicUsize::new(0),
            bytes: std::sync::atomic::AtomicU64::new(0),
            cut: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Charges one entry that reads `bytes` of file content (0 for a
    /// directory or an entry not read): whether it fits.
    pub fn charge(&self, bytes: u64) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        if self.cut.load(Relaxed) {
            return false;
        }
        let entries = self.entries.fetch_add(1, Relaxed).saturating_add(1);
        let read = self.bytes.fetch_add(bytes, Relaxed).saturating_add(bytes);
        let fits = entries <= self.limits.entries && read <= self.limits.bytes;
        if !fits {
            self.cut.store(true, Relaxed);
        }
        fits
    }

    /// Whether an entry did not fit: what was not walked is unproven.
    pub fn cut(&self) -> bool {
        self.cut.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Files of a probe's finding kept to name skip reasons.
pub const SAMPLE_FILES: usize = 8;
/// Distinct skip reasons one hint names; the rest is a count.
const NAMED_REASONS: usize = 3;

/// Why a path is out of a walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
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

    pub fn label(self) -> &'static str {
        match self {
            Self::Ignored => "ignored",
            Self::Hidden => "hidden",
            Self::DefaultExcluded => "default-excluded",
        }
    }

    /// The query field that lifts this cause, and its value.
    pub fn flag(self) -> (&'static str, Value) {
        match self {
            Self::Ignored => ("includeIgnored", json!(true)),
            Self::Hidden => ("includeHidden", json!(true)),
            Self::DefaultExcluded => ("defaultExcludes", json!(false)),
        }
    }
}

/// Which paths a walk takes: the request's flags, or a relaxed copy of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Walk {
    /// `includeIgnored`: `.gitignore`d paths are walked.
    pub ignored: bool,
    /// `includeHidden`: dot paths are walked.
    pub hidden: bool,
    /// `defaultExcludes`: the default directory prune applies.
    pub default_excludes: bool,
    /// The global ignore rules of the call the walk serves.
    pub global: GlobalIgnore,
}

impl Walk {
    /// This walk under the global ignore rules of a call.
    pub fn under(self, global: GlobalIgnore) -> Self {
        Self { global, ..self }
    }

    /// The causes this walk skips by.
    pub fn skipping(self) -> Vec<Cause> {
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
    pub fn policy(self) -> IgnorePolicy {
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
    pub fn walker(
        self,
        root: &Path,
        prune: &[String],
        globs: Globs<'_>,
    ) -> Result<ignore::Walk, Error> {
        let (mut builder, global) = walk_builder_under(root, self.policy(), self.global, None);
        let prune = Prune::new(prune).at_with(root, self.global);
        builder.filter_entry(move |entry| {
            entry.depth() == 0
                || (!global.hides_entry(entry)
                    && !prune.prunes(
                        entry.path(),
                        entry.file_type().is_some_and(|kind| kind.is_dir()),
                    ))
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
    pub fn lifting(mut self, causes: &[Cause]) -> Self {
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
pub struct Globs<'a> {
    pub include: &'a [String],
    pub exclude: &'a [String],
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
pub struct Hits {
    /// Files (or matches) found.
    pub count: usize,
    /// The first of them, absolute, at most [`SAMPLE_FILES`]: what names the
    /// skip reasons.
    pub sample: Vec<PathBuf>,
    /// The probe stopped early (deadline or cap): there may be more, or some.
    pub cut: bool,
}

/// One skipped path's reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skip {
    pub cause: Cause,
    /// The topmost skipped entry on the way to the file: the directory (or
    /// file) whose own scope shows what the walk left out.
    pub entry: PathBuf,
    /// The deciding ignore rule (`gen/.gitignore:2 '*'`), the hidden entry
    /// (`hidden .notes/`) or the excluded directory (`build/`).
    pub reason: String,
}

/// The skip causes whose relaxation found matches.
#[derive(Clone, Debug)]
pub struct Diagnosis {
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
    pub fn skips(&self) -> &[Skip] {
        &self.skips
    }

    /// The hint's reason: the named rules, entries and directories (the first
    /// few, then a count), else the causes.
    pub(crate) fn because(&self) -> String {
        if self.skips.is_empty() {
            return self.labels();
        }
        let shown = octocode_runtime_base::response::pages::first_named(
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
pub enum Verdict {
    /// Nothing sits behind the skipped paths (or nothing was skipped).
    Absent,
    /// The probe was cut before it found anything: absence is unproven.
    Unproven,
    /// These causes held matches.
    Skipped(Diagnosis),
}
