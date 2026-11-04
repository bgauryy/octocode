//! The files a scope's text scan reads, kept across requests.
//!
//! What a walk lists depends on the scope's directories and ignore rules, not
//! on any listed file's text, so a walk is kept with a proof of exactly
//! those inputs: each directory it entered, each ignore file there, the ignore
//! inputs outside the walk, and the repository markers. While the proof holds,
//! a later request (for any name) searches the listed files as they are now
//! ([`super::spelling_index`]) instead of walking again; a file created, removed or renamed changes its
//! directory's stamp, and an edited ignore file changes its own. A scope
//! written within the settle window could change unseen in one clock tick, so
//! its walk is not kept ([`Watch::capture`]).
use super::spelling_index::WordSearch;
use super::*;
use crate::policy::path::DiscoveryWalk;
use crate::tools::page_memo::{
    PageMemo, Proven, Watch, directory_ignore_inputs, repository_markers,
};
use octocode_engine::search::text_search::TextSearchPathFilter;
use octocode_engine::stamp::Link;

/// Kept walks, by root and family globs, against the one process budget.
static WALKS: PageMemo<Proven<[PathBuf]>> = PageMemo::new();

fn key(root: &str, include: &[String]) -> String {
    format!("{root}\u{0}{}", include.join("\u{0}"))
}

/// The files the kept walk of `root` listed, while its proof holds.
pub(super) fn current(
    root: &str,
    include: &[String],
    policy: &PathPolicy,
) -> Option<Arc<[PathBuf]>> {
    WALKS.current(&key(root, include), &policy.identity())
}

/// What one walk admitted, recorded as it walks.
#[derive(Default)]
struct Seen {
    files: Vec<PathBuf>,
    /// Walked directories and their ignore files.
    watched: Vec<PathBuf>,
    /// Ignore inputs a nested repository reads from outside the walk.
    rules: Vec<PathBuf>,
    /// An ignore input no stamp can watch: the walk is not kept.
    untracked: bool,
}

/// The walk's path filter: the discovery walk's verdicts, recording what it
/// admits, so the walk that answers a request can serve the next.
pub(super) struct Recorder {
    walk: DiscoveryWalk<'static>,
    policy: PathPolicy,
    root: PathBuf,
    seen: Mutex<Seen>,
}

impl Recorder {
    pub(super) fn new(policy: &PathPolicy, root: &str) -> Self {
        let recorder = Self {
            walk: DiscoveryWalk::owned(policy.clone()),
            policy: policy.clone(),
            root: PathBuf::from(root),
            seen: Mutex::default(),
        };
        recorder.record_dir(&recorder.root);
        recorder
    }

    fn seen(&self) -> std::sync::MutexGuard<'_, Seen> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn record_dir(&self, dir: &Path) {
        let inputs = directory_ignore_inputs(dir, &self.root, |ignore| {
            self.policy.validate_read(ignore).is_ok()
        });
        let mut seen = self.seen();
        seen.watched.push(dir.to_path_buf());
        seen.watched.extend(inputs.files);
        seen.rules.extend(inputs.external);
        seen.untracked |= inputs.untracked;
    }

    /// Keep this finished walk under `include`, when it can be proven later:
    /// nothing it depends on was written within the settle window before
    /// `started`, the walk's start on the settle clock.
    pub(super) fn keep(&self, include: &[String], started: SystemTime) {
        let (policy, root) = (&self.policy, &self.root);
        let seen = std::mem::take(&mut *self.seen());
        if seen.untracked {
            return;
        }
        let mut rules = octocode_engine::search::walk::external_ignore_inputs(root);
        rules.extend(seen.rules);
        let Some(proof) = (Watch {
            root,
            sources: &seen.watched,
            rules: &rules,
            markers: &repository_markers(root),
        })
        .capture(Link::Follow, Some(started)) else {
            return;
        };
        let bytes = seen
            .files
            .iter()
            .chain(&seen.watched)
            .chain(&rules)
            .map(|path| std::mem::size_of::<PathBuf>() + path.as_os_str().len())
            .sum();
        WALKS.put_proven(
            key(&root.to_string_lossy(), include),
            policy.identity(),
            bytes,
            proof,
            seen.files.into(),
        );
    }
}

impl TextSearchPathFilter for Recorder {
    fn allows(&self, path: &Path, is_dir: bool) -> bool {
        self.walk.allows(path, is_dir)
    }

    fn allows_entry(&self, path: &Path, file_type: Option<std::fs::FileType>) -> bool {
        let allowed = self.walk.allows_entry(path, file_type);
        if allowed {
            if file_type.is_some_and(|kind| kind.is_dir()) {
                if path != self.root {
                    self.record_dir(path);
                }
            } else if file_type.is_some_and(|kind| kind.is_file()) {
                self.seen().files.push(path.to_path_buf());
            }
        }
        allowed
    }
}

/// The listed `files` that spell `symbol` as a word, as they are now, as a
/// walk's search would find them: a file too large to scan or unreadable is
/// not listed by either. `None` when `stopped` ended the search. `clock` is the
/// request's settle clock.
pub(super) fn spelling(
    files: &[PathBuf],
    symbol: &str,
    clock: SystemTime,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<BTreeSet<String>> {
    let search = WordSearch::new(
        symbol,
        Some(octocode_engine::search::text_search::DEFAULT_MAX_LINE_SEARCH_FILE_BYTES),
        clock,
    );
    let (found, read_all) =
        crate::tools::lsp_search::words::files_spelling(files, stopped, |file| {
            search.file_spells(file).unwrap_or(false)
        });
    read_all.then(|| {
        found
            .into_iter()
            .map(|file| file.to_string_lossy().into_owned())
            .collect()
    })
}
