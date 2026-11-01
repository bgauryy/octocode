use crate::policy::prune;
use octocode_engine::error::Error;
use octocode_engine::search::walk::{IgnoreRules, Prune};
use octocode_engine::types::IgnorePolicy;
use std::path::{Path, PathBuf};

use super::reasons::prefixes;
use super::{Cause, Globs, Hits, SAMPLE_FILES, Walk, probe_budget};

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
    /// dot entry, a default-pruned directory, or a generated file.
    fn tag(&self) -> Result<Tagged, Error> {
        let overrides = self.globs.overrides(&self.root)?;
        let mut builder =
            octocode_engine::search::walk::walk_builder(&self.root, IgnorePolicy::ALL, None);
        builder
            .overrides(overrides.clone())
            .sort_by_file_name(std::ffi::OsStr::cmp);
        let rules = IgnoreRules::new(&self.root);
        // The default excludes as the tool's own walk applies them: rooted,
        // less what the include globs name, generated files included.
        let prune = Prune::new(prune::walk_names(true, self.globs.include)).at(&self.root);
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
            let excluded = if is_dir {
                prune.prunes(path, true)
            } else {
                crate::policy::discovery::generated_file_pattern(
                    &entry.file_name().to_string_lossy(),
                )
                .is_some()
            };
            if excluded {
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
