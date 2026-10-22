//! What a fresh text-search walk searched, recorded as it walks, so a stored
//! scan is served only while none of it changed (see [`super::manifest`]).
//! Every searched file and every walked directory is stamped: a new file
//! changes its directory, and an existing file that gains a match changes
//! its own stamp. With ignore rules on, each walked directory's ignore files,
//! the rules outside the walk and the repository markers are watched too.
use crate::policy::discovery::SearchWalk;
use crate::policy::path::PathPolicy;
use octocode_engine::search::text_search::TextSearchPathFilter;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

/// What one walk searched and what its ignore rules depend on.
pub struct Walked {
    pub(super) root: PathBuf,
    /// Searched files, walked directories, and their ignore files.
    pub(super) entries: Vec<PathBuf>,
    /// Ignore inputs outside the walk.
    pub(super) rules: Vec<PathBuf>,
    /// Repository markers whose presence decides the rules.
    pub(super) markers: Vec<PathBuf>,
    /// An ignore dependency no stamp can watch (a link, an unreadable or
    /// withheld entry): the scan is not stored.
    pub(super) untracked: bool,
    /// The recorded paths alone outweighed what a stored scan may keep, so
    /// the walk stopped keeping them: the scan is not stored.
    pub(super) oversize: bool,
    /// When the walk started: an entry written within the settle window
    /// before it could race the walk's reads, so the scan is not stored.
    pub(super) started: SystemTime,
}

impl Walked {
    /// Heap bytes of the recorded paths, toward a stored scan's weight.
    pub(super) fn weight(&self) -> usize {
        self.entries
            .iter()
            .chain(&self.rules)
            .chain(&self.markers)
            .map(|path| path_weight(path))
            .sum()
    }

    /// A walk of `entries` under `root` with no ignore inputs, started past
    /// the settle window of every write so far.
    #[cfg(test)]
    pub(super) fn settled(root: PathBuf, entries: Vec<PathBuf>) -> Self {
        Self {
            root,
            entries,
            rules: Vec::new(),
            markers: Vec::new(),
            untracked: false,
            oversize: false,
            started: SystemTime::now()
                + octocode_engine::stamp::SourceStamp::SETTLE
                + std::time::Duration::from_secs(1),
        }
    }

    /// The proof a stored scan of this walk keeps; `None` when an entry is
    /// gone or was written within the settle window before the walk.
    pub(super) fn proof(&self) -> Option<crate::tools::page_memo::Proof> {
        crate::tools::page_memo::Watch {
            root: &self.root,
            sources: &self.entries,
            rules: &self.rules,
            markers: &self.markers,
        }
        .capture(crate::tools::source::Link::Follow, Some(self.started))
    }
}

/// Heap bytes one recorded path keeps.
fn path_weight(path: &Path) -> usize {
    std::mem::size_of::<PathBuf>() + path.as_os_str().len()
}

/// The walk's path filter: [`SearchWalk`]'s verdicts, recording each
/// admitted entry.
pub(super) struct Recorder {
    walk: Arc<SearchWalk>,
    policy: PathPolicy,
    root: PathBuf,
    ignore_rules: bool,
    /// Bytes the recorded entries may weigh: past it no stored scan could
    /// keep them, so the recorder keeps none.
    max_weight: usize,
    seen: Mutex<Seen>,
    started: SystemTime,
}

#[derive(Default)]
struct Seen {
    entries: Vec<PathBuf>,
    /// The entries' heap bytes ([`path_weight`]).
    weight: usize,
    rules: BTreeSet<PathBuf>,
    untracked: bool,
    oversize: bool,
}

impl Seen {
    /// Record `path`, or drop every entry once they outweigh `max_weight`.
    fn record(&mut self, path: PathBuf, max_weight: usize) {
        if self.oversize {
            return;
        }
        self.weight += path_weight(&path);
        if self.weight > max_weight {
            self.oversize = true;
            self.entries = Vec::new();
            return;
        }
        self.entries.push(path);
    }
}

impl Recorder {
    /// A recorder of `walk` over `root`; `ignore_rules` when the walk
    /// applies ignore files. It keeps at most `max_weight` bytes of entries.
    pub(super) fn new(
        walk: Arc<SearchWalk>,
        policy: PathPolicy,
        root: PathBuf,
        ignore_rules: bool,
        max_weight: usize,
    ) -> Self {
        let rules = if ignore_rules {
            octocode_engine::search::walk::external_ignore_inputs(&root)
                .into_iter()
                .collect()
        } else {
            BTreeSet::new()
        };
        let recorder = Self {
            walk,
            policy,
            root,
            ignore_rules,
            max_weight,
            seen: Mutex::new(Seen {
                rules,
                ..Seen::default()
            }),
            started: octocode_engine::stamp::settle_clock(),
        };
        // The root's own ignore files decide the walk whether or not the
        // walk reports the root as an entry.
        if ignore_rules && recorder.root.is_dir() {
            let mut seen = recorder
                .seen
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            recorder.observe_directory(&recorder.root, &mut seen);
        }
        recorder
    }

    /// What the walk recorded.
    pub(super) fn walked(&self) -> Walked {
        let seen =
            std::mem::take(&mut *self.seen.lock().unwrap_or_else(|error| error.into_inner()));
        Walked {
            markers: if self.ignore_rules {
                crate::tools::page_memo::repository_markers(&self.root)
            } else {
                Vec::new()
            },
            root: self.root.clone(),
            entries: seen.entries,
            rules: seen.rules.into_iter().collect(),
            untracked: seen.untracked,
            oversize: seen.oversize,
            started: self.started,
        }
    }

    /// Record a walked directory's ignore inputs
    /// ([`crate::tools::page_memo::directory_ignore_inputs`]).
    fn observe_directory(&self, dir: &Path, seen: &mut Seen) {
        let inputs = crate::tools::page_memo::directory_ignore_inputs(dir, &self.root, |ignore| {
            self.policy.validate_read(ignore).is_ok()
        });
        seen.rules.extend(inputs.external);
        seen.untracked |= inputs.untracked;
        for ignore in inputs.files {
            seen.record(ignore, self.max_weight);
        }
    }
}

impl TextSearchPathFilter for Recorder {
    fn allows(&self, path: &Path, is_dir: bool) -> bool {
        let allowed = self.walk.allows(path, is_dir);
        if allowed {
            let mut seen = self.seen.lock().unwrap_or_else(|error| error.into_inner());
            seen.record(path.to_path_buf(), self.max_weight);
            if is_dir && self.ignore_rules && path != self.root {
                self.observe_directory(path, &mut seen);
            }
        }
        allowed
    }

    fn skipped_hidden(&self, path: &Path, is_dir: bool) {
        self.walk.skipped_hidden(path, is_dir);
    }

    fn skipped_ignored(&self, path: &Path, is_dir: bool) {
        self.walk.skipped_ignored(path, is_dir);
    }

    fn skipped_symlink(&self, path: &Path) {
        self.walk.skipped_symlink(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recorder of a walk of `root` whose entries may weigh `max_weight`.
    fn recorder(root: &Path, max_weight: usize) -> Recorder {
        let policy = crate::tools::test_support::workspace_policy(root);
        let walk = Arc::new(SearchWalk::new(
            policy.clone(),
            root.to_path_buf(),
            true,
            &[],
        ));
        Recorder::new(walk, policy, root.to_path_buf(), true, max_weight)
    }

    /// Entries are kept up to the weight a stored scan may keep; past it
    /// the recorder drops them all and the walk is marked unstorable, so a
    /// large walk holds no memory proportional to its entries.
    #[test]
    fn a_walk_past_the_stored_weight_keeps_no_entries() {
        let dir = tempfile::tempdir().expect("fixture");
        let files = (0..20)
            .map(|index| {
                let path = dir.path().join(format!("f{index:02}.txt"));
                std::fs::write(&path, "x").expect("fixture");
                path
            })
            .collect::<Vec<_>>();
        let weight = files.iter().map(|path| path_weight(path)).sum::<usize>();
        let walk = |max_weight| {
            let recording = recorder(dir.path(), max_weight);
            for file in &files {
                assert!(recording.allows(file, false));
            }
            recording.walked()
        };
        let kept = walk(weight);
        assert_eq!(kept.entries, files);
        assert!(!kept.oversize);
        let dropped = walk(weight - 1);
        assert!(dropped.entries.is_empty());
        assert!(dropped.oversize);
        assert_eq!(
            dropped.weight(),
            dropped
                .rules
                .iter()
                .chain(&dropped.markers)
                .map(|path| path_weight(path))
                .sum::<usize>()
        );
    }

    /// A walked directory's ignore files are entries; a linked one is a
    /// dependency no stamp watches.
    #[cfg(unix)]
    #[test]
    fn a_walked_directory_records_its_ignore_inputs() {
        let dir = tempfile::tempdir().expect("fixture");
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).expect("fixture");
        std::fs::write(nested.join(".gitignore"), "*.log\n").expect("fixture");
        let walk = || {
            let recording = recorder(dir.path(), usize::MAX);
            assert!(recording.allows(&nested, true));
            recording.walked()
        };
        let walked = walk();
        assert!(walked.entries.contains(&nested.join(".gitignore")));
        assert!(!walked.untracked);
        std::os::unix::fs::symlink(nested.join(".gitignore"), nested.join(".ignore"))
            .expect("link");
        assert!(walk().untracked);
    }
}
