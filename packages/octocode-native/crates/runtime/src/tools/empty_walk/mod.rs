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

use crate::policy::prune;
use octocode_engine::error::Error;
use octocode_engine::search::walk::{IgnorePolicy, IgnoreRules, Prune, pruned_walk_builder};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tool_tests;

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
    /// directory: ignore rules and hidden entries follow the flags, and
    /// `globs` scope it like the tool's own scan.
    pub(crate) fn walker(
        self,
        root: &Path,
        prune: &[String],
        globs: Globs<'_>,
    ) -> Result<ignore::Walk, Error> {
        let mut builder = pruned_walk_builder(root, self.policy(), None, Prune::new(prune));
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

/// Finds which of the causes `walk` skips by held matches. `probe` runs the
/// tool's own walk (files only) with the given relaxed [`Walk`] and says what
/// it found; it is called with everything lifted first (one call when
/// nothing is found), then with each smaller set of lifted causes by size
/// until one finds matches, so the answer is the smallest sets that do (the
/// union of those that reach different files).
pub(crate) fn diagnose<E>(
    root: &Path,
    walk: Walk,
    probe: impl FnMut(Walk) -> Result<Hits, E>,
) -> Result<Verdict, E> {
    diagnose_shown(root, walk, &|path| below(root, path), probe)
}

/// [`diagnose`] whose reasons spell paths with `shown` (a tool whose rows
/// name paths relative to the workspace names its rules the same way).
pub(crate) fn diagnose_shown<E>(
    root: &Path,
    walk: Walk,
    shown: &dyn Fn(&Path) -> String,
    mut probe: impl FnMut(Walk) -> Result<Hits, E>,
) -> Result<Verdict, E> {
    let off = walk.skipping();
    if off.is_empty() {
        return Ok(Verdict::Absent);
    }
    let subset = |mask: usize| -> Vec<Cause> {
        off.iter()
            .enumerate()
            .filter(|(index, _)| mask >> index & 1 == 1)
            .map(|(_, cause)| *cause)
            .collect()
    };
    let full = (1_usize << off.len()) - 1;
    let everything = probe(walk.lifting(&off))?;
    if everything.count == 0 {
        return Ok(if everything.cut {
            Verdict::Unproven
        } else {
            Verdict::Absent
        });
    }
    // Smallest sets first; a larger set is probed only while the sets found
    // so far reach fewer files than lifting everything does, and kept only
    // when it reaches more than the found sets inside it (a file behind two
    // causes, beside files behind one of them).
    let mut revealed: Vec<(usize, Hits)> = Vec::new();
    let within = |revealed: &[(usize, Hits)], mask: usize| -> usize {
        revealed
            .iter()
            .filter(|(known, _)| known & mask == *known)
            .map(|(_, found)| found.count)
            .sum()
    };
    for size in 1..off.len() {
        if within(&revealed, full) >= everything.count {
            break;
        }
        let mut level = Vec::new();
        for mask in (1..full).filter(|mask| mask.count_ones() as usize == size) {
            let found = probe(walk.lifting(&subset(mask)))?;
            if found.count > within(&revealed, mask) {
                level.push((mask, found));
            }
        }
        revealed.extend(level);
    }
    if revealed.is_empty() {
        revealed.push((full, everything));
    }
    // Causes that reach the same files are alternatives (a walk that couples
    // two flags): the first of them in cause order is enough.
    // A larger set kept above reaches files its subsets do not, whatever its
    // sample shows.
    let mut covered = std::collections::BTreeSet::new();
    let mut kept: Vec<(usize, Hits)> = Vec::new();
    for (mask, found) in revealed {
        let new = found.sample.iter().any(|file| !covered.contains(file))
            || (kept.iter().any(|(known, _)| known & mask == *known)
                && found.count > within(&kept, mask));
        covered.extend(found.sample.iter().cloned());
        if new {
            kept.push((mask, found));
        }
    }
    let revealed = kept;
    let union = revealed.iter().fold(0, |union, (mask, _)| union | mask);
    let found = match revealed.iter().find(|(mask, _)| *mask == union) {
        Some((_, found)) => found.clone(),
        None => {
            let found = probe(walk.lifting(&subset(union)))?;
            if found.count > 0 {
                found
            } else {
                revealed[0].1.clone()
            }
        }
    };
    let mut skips: Vec<Skip> = Vec::new();
    let rules = std::cell::OnceCell::new();
    for (mask, found) in &revealed {
        let causes = subset(*mask);
        for file in found.sample.iter().take(SAMPLE_FILES) {
            let rules = rules.get_or_init(|| IgnoreRules::new(root));
            if let Some(skip) = skip_of(root, rules, file, &causes, shown)
                && !skips.iter().any(|known| known.reason == skip.reason)
            {
                skips.push(skip);
            }
        }
    }
    Ok(Verdict::Skipped(Diagnosis {
        causes: subset(union),
        found,
        skips,
    }))
}

/// Every file one fully lifted walk reached, each tagged with the causes
/// that hide it ([`Cause::bit`]), and whether the walk was cut: every relaxed
/// walk is answered from the tags, so a diagnosis walks once.
pub(crate) struct Tagged {
    files: Vec<(PathBuf, u8)>,
    cut: bool,
}

impl Tagged {
    /// The files a tool's own search found under `root` with every cause
    /// lifted (or a `cut` search's first ones), each tagged by its path: an
    /// ignore rule on the way (an include glob's match aside), a dot entry,
    /// a default-pruned directory, or a generated file the default excludes
    /// skip.
    pub(crate) fn found(
        root: &Path,
        globs: Globs<'_>,
        files: impl IntoIterator<Item = PathBuf>,
        cut: bool,
    ) -> Result<Self, String> {
        let overrides = globs.overrides(root).map_err(|error| error.reason)?;
        let files = files.into_iter().map(|file| (file, false));
        Ok(Self::tag(root, Some(&overrides), files, true, cut))
    }

    /// The entries (each a path and whether it is a directory) a listing
    /// walk found under `root` with every cause lifted (or a `cut` walk's
    /// first ones), each tagged by its path like [`Self::found`]'s files: a
    /// listing walk applies no include override and skips no generated file.
    pub(crate) fn listed(
        root: &Path,
        entries: impl IntoIterator<Item = (PathBuf, bool)>,
        cut: bool,
    ) -> Self {
        Self::tag(root, None, entries, false, cut)
    }

    /// Tag each entry with the causes on its way below `root`; `overrides`
    /// whitelist ignored paths, and with `generated` a generated file is
    /// default-excluded.
    fn tag(
        root: &Path,
        overrides: Option<&ignore::overrides::Override>,
        entries: impl IntoIterator<Item = (PathBuf, bool)>,
        generated: bool,
        cut: bool,
    ) -> Self {
        let rules = IgnoreRules::new(root);
        let prune = Prune::new(prune::directories(true)).at(root);
        let files = entries
            .into_iter()
            .map(|(entry, entry_is_dir)| {
                let mut bits = 0;
                let ways = prefixes(root, &entry);
                let last = ways.len().saturating_sub(1);
                for (index, (prefix, on_the_way)) in ways.into_iter().enumerate() {
                    let is_dir = on_the_way || (index == last && entry_is_dir);
                    if bits & Cause::Ignored.bit() == 0
                        && !overrides.is_some_and(|overrides| {
                            overrides.matched(&prefix, is_dir).is_whitelist()
                        })
                        && rules.is_ignored(&prefix, is_dir)
                    {
                        bits |= Cause::Ignored.bit();
                    }
                    let name = prefix.file_name().unwrap_or_default();
                    if name.to_string_lossy().starts_with('.') {
                        bits |= Cause::Hidden.bit();
                    }
                    let generated_file = generated
                        && !is_dir
                        && crate::policy::discovery::generated_file_pattern(
                            &name.to_string_lossy(),
                        )
                        .is_some();
                    if prune.prunes(&prefix, is_dir) || generated_file {
                        bits |= Cause::DefaultExcluded.bit();
                    }
                }
                (entry, bits)
            })
            .collect();
        Self { files, cut }
    }

    /// What the relaxed `walk` finds; with `requested`, only files that walk
    /// did not reach.
    pub(crate) fn find(&self, walk: Walk, requested: Option<Walk>) -> Hits {
        let lifted = walk.lifted_bits();
        let reached = requested.map(Walk::lifted_bits);
        let mut files = self
            .files
            .iter()
            .filter(|(_, bits)| bits & !lifted == 0)
            .filter(|(_, bits)| reached.is_none_or(|reached| bits & !reached != 0))
            .map(|(path, _)| path);
        let sample = files
            .by_ref()
            .take(SAMPLE_FILES)
            .cloned()
            .collect::<Vec<_>>();
        Hits {
            count: sample.len() + files.count(),
            sample,
            cut: self.cut,
        }
    }
}

/// A file probe for a tool without a text engine of its own: the files under
/// `root` that `keep` admits, within `budget` walked files and
/// [`probe_budget`], under a relaxed [`Walk`]. One walk with every cause lifted tags
/// each file with the causes that hide it; every relaxed walk is then
/// answered from those tags. The walk is the discovery policy's (`permits`),
/// `globs`, ignore rules and prune list the tool's scan honors.
pub(crate) struct FileProbe<'a> {
    root: PathBuf,
    budget: usize,
    deadline: std::time::Instant,
    globs: Globs<'a>,
    permits: &'a dyn Fn(&Path) -> bool,
    keep: &'a dyn Fn(&Path) -> bool,
    /// The request's own walk: the files it reached are not behind anything.
    requested: Option<Walk>,
    /// The caller's cancellation: a stopped walk is cut.
    stop: Option<&'a (dyn Fn() -> bool + 'a)>,
    tagged: std::cell::OnceCell<Result<Tagged, String>>,
}

impl<'a> FileProbe<'a> {
    pub(crate) fn new(
        root: &Path,
        budget: usize,
        permits: &'a dyn Fn(&Path) -> bool,
        keep: &'a dyn Fn(&Path) -> bool,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            budget,
            deadline: std::time::Instant::now() + probe_budget(),
            globs: Globs::default(),
            permits,
            keep,
            requested: None,
            stop: None,
            tagged: std::cell::OnceCell::new(),
        }
    }

    /// Scopes the probe to the request's `include`/`exclude` globs.
    pub(crate) fn within(mut self, globs: Globs<'a>) -> Self {
        self.globs = globs;
        self
    }

    /// Counts only files the request's own `walk` did not reach: the probe's
    /// finding is then what relaxing skipped paths adds, not what the scan
    /// already saw (an empty result with other causes, such as a name filter).
    pub(crate) fn beyond(mut self, walk: Walk) -> Self {
        self.requested = Some(walk);
        self
    }

    /// Stops walking once `stop` is true: what was not walked is cut.
    pub(crate) fn stopping(mut self, stop: &'a (dyn Fn() -> bool + 'a)) -> Self {
        self.stop = Some(stop);
        self
    }

    /// The one walk with every cause lifted, each kept file tagged with the
    /// causes on its way: an ignore rule (an include glob's match aside), a
    /// dot entry, or a default-pruned directory.
    fn tag(&self) -> Result<Tagged, Error> {
        let overrides = self.globs.overrides(&self.root)?;
        let mut builder =
            octocode_engine::search::walk::walk_builder(&self.root, IgnorePolicy::ALL, None);
        builder
            .overrides(overrides.clone())
            .sort_by_file_name(std::ffi::OsStr::cmp);
        let rules = IgnoreRules::new(&self.root);
        let prune = Prune::new(prune::directories(true));
        // Tag bits of each directory on the way to the current entry, by depth.
        let mut inherited: Vec<u8> = vec![0];
        let (mut files, mut walked) = (Vec::new(), 0_usize);
        for entry in builder.build() {
            let Ok(entry) = entry else { continue };
            let depth = entry.depth();
            if depth == 0 {
                continue;
            }
            if std::time::Instant::now() >= self.deadline || self.stop.is_some_and(|stop| stop()) {
                return Ok(Tagged { files, cut: true });
            }
            let path = entry.path();
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            inherited.truncate(depth);
            let parent = inherited.last().copied().unwrap_or(0);
            let mut bits = parent;
            if parent & Cause::Ignored.bit() == 0
                && !overrides.matched(path, is_dir).is_whitelist()
                && rules.is_ignored(path, is_dir)
            {
                bits |= Cause::Ignored.bit();
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                bits |= Cause::Hidden.bit();
            }
            if prune.prunes_entry(&entry) {
                bits |= Cause::DefaultExcluded.bit();
            }
            if is_dir {
                inherited.push(bits);
                continue;
            }
            if !entry.file_type().is_some_and(|kind| kind.is_file()) || !(self.permits)(path) {
                continue;
            }
            walked += 1;
            if walked > self.budget {
                return Ok(Tagged { files, cut: true });
            }
            if (self.keep)(path) {
                files.push((path.to_path_buf(), bits));
            }
        }
        Ok(Tagged { files, cut: false })
    }

    /// What the relaxed `walk` finds.
    pub(crate) fn find(&self, walk: Walk) -> Result<Hits, String> {
        let tagged = self
            .tagged
            .get_or_init(|| self.tag().map_err(|error| error.reason))
            .as_ref()
            .map_err(Clone::clone)?;
        Ok(tagged.find(walk, self.requested))
    }
}

/// Why the walk skipped `file`, by the first of `causes` that applies;
/// paths in the reason spelled by `shown`.
fn skip_of(
    root: &Path,
    rules: &IgnoreRules,
    file: &Path,
    causes: &[Cause],
    shown: &dyn Fn(&Path) -> String,
) -> Option<Skip> {
    [Cause::Ignored, Cause::Hidden, Cause::DefaultExcluded]
        .into_iter()
        .filter(|cause| causes.contains(cause))
        .find_map(|cause| {
            let (entry, reason) = match cause {
                Cause::Ignored => ignored_entry(rules, file, shown),
                Cause::Hidden => hidden_entry(root, file, shown),
                Cause::DefaultExcluded => excluded_entry(root, file, shown),
            }?;
            Some(Skip {
                cause,
                entry,
                reason,
            })
        })
}

/// The topmost directories under `root` that ignore rules keep `walk` out of:
/// a probe that ran out of time can still name what it could not search.
/// Directories only, found as the children the ignore-aware walk did not
/// enter, within `stop`.
pub(crate) fn ignored_directories(root: &Path, walk: Walk, stop: &dyn Fn() -> bool) -> Vec<Skip> {
    if walk.ignored {
        return Vec::new();
    }
    let pruned = prune::directories(walk.default_excludes);
    let Ok(walker) = walk.walker(root, &pruned, Globs::default()) else {
        return Vec::new();
    };
    let rules = IgnoreRules::new(root);
    let mut walked = HashSet::new();
    for entry in walker {
        if stop() {
            break;
        }
        if let Ok(entry) = entry
            && entry.file_type().is_some_and(|kind| kind.is_dir())
        {
            walked.insert(entry.into_path());
        }
    }
    let mut dirs: Vec<&PathBuf> = walked.iter().collect();
    dirs.sort();
    let mut skips = Vec::new();
    for dir in dirs {
        let Ok(children) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut children: Vec<_> = children.flatten().collect();
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            if stop() {
                return skips;
            }
            let path = child.path();
            let name = child.file_name().to_string_lossy().into_owned();
            let candidate = child.file_type().is_ok_and(|kind| kind.is_dir())
                && !walked.contains(&path)
                && !pruned.contains(&name)
                && (walk.hidden || !name.starts_with('.'));
            if candidate
                && let Some(reason) = rule_shown(&rules, &path, true, &|path| below(root, path))
            {
                skips.push(Skip {
                    cause: Cause::Ignored,
                    entry: path,
                    reason,
                });
            }
        }
    }
    skips
}

/// The path's ancestors below `root` and itself, shortest first, each with
/// whether it is a directory on the way to `file`.
fn prefixes(root: &Path, file: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(relative) = file.strip_prefix(root) else {
        return Vec::new();
    };
    let count = relative.components().count();
    let mut prefix = root.to_path_buf();
    relative
        .components()
        .enumerate()
        .map(|(index, part)| {
            prefix.push(part);
            (prefix.clone(), index + 1 < count)
        })
        .collect()
}

/// The topmost ignored entry on the way to `file`, and its rule.
fn ignored_entry(
    rules: &IgnoreRules,
    file: &Path,
    shown: &dyn Fn(&Path) -> String,
) -> Option<(PathBuf, String)> {
    let reason = rules.why(file, false)?;
    let entry = reason.entry.clone();
    Some((entry, spell(&reason, shown)))
}

fn hidden_entry(
    root: &Path,
    file: &Path,
    shown: &dyn Fn(&Path) -> String,
) -> Option<(PathBuf, String)> {
    prefixes(root, file)
        .into_iter()
        .find_map(|(prefix, is_dir)| {
            let name = prefix.file_name()?.to_string_lossy();
            let slash = if is_dir { "/" } else { "" };
            name.starts_with('.')
                .then(|| (prefix.clone(), format!("hidden {}{slash}", shown(&prefix))))
        })
}

/// The first pruned directory on the way to `file`, else the file itself
/// when it is a generated one the default excludes skip (`*.min.js`).
fn excluded_entry(
    root: &Path,
    file: &Path,
    shown: &dyn Fn(&Path) -> String,
) -> Option<(PathBuf, String)> {
    let prune = Prune::new(prune::directories(true));
    let directory = prefixes(root, file)
        .into_iter()
        .filter(|(prefix, is_dir)| {
            *is_dir && prefix.file_name().is_some_and(|name| prune.names(name))
        })
        .map(|(prefix, _)| {
            let reason = format!("{}/", shown(&prefix));
            (prefix, reason)
        })
        .next();
    directory.or_else(|| {
        let name = file.file_name()?.to_string_lossy();
        let pattern = crate::policy::discovery::generated_file_pattern(&name)?;
        Some((file.to_path_buf(), pattern))
    })
}

/// The ignore rule that keeps `path` out of a walk under `root`: the deciding
/// ignore file, line and pattern, the file relative to `root` (through `../`
/// for one above it). `None` when no rule applies.
pub(crate) fn ignore_rule(root: &Path, path: &Path, is_dir: bool) -> Option<String> {
    rule_shown(&IgnoreRules::new(root), path, is_dir, &|path| {
        below(root, path)
    })
}

/// [`ignore_rule`] under prepared rules, the ignore file named by `shown`.
fn rule_shown(
    rules: &IgnoreRules,
    path: &Path,
    is_dir: bool,
    shown: &dyn Fn(&Path) -> String,
) -> Option<String> {
    rules.why(path, is_dir).map(|reason| spell(&reason, shown))
}

/// `file:line 'pattern'`, the file named by `shown`.
fn spell(
    reason: &octocode_engine::search::walk::IgnoreReason,
    shown: &dyn Fn(&Path) -> String,
) -> String {
    let line = reason
        .line
        .map(|line| format!(":{line}"))
        .unwrap_or_default();
    let source = reason.source.as_deref().map(shown).unwrap_or_default();
    format!("{source}{line} '{}'", reason.pattern)
}

/// `path` relative to `root`, through `../` for a path above it: a row
/// never prints an absolute path.
fn below(root: &Path, path: &Path) -> String {
    root.ancestors()
        .enumerate()
        .find_map(|(up, base)| {
            let rest = path.strip_prefix(base).ok()?;
            Some("../".repeat(up) + &octocode_engine::text::slash_path(rest))
        })
        .unwrap_or_else(|| octocode_engine::text::slash_path(path))
}
