//! The bounded text scan behind the scope, and the scope fingerprint.
use super::*;

/// Files that spell a name but that ignore rules keep out of the lexical
/// scans (see [`Scope::ignored_text_files`]).
#[derive(Debug, Default)]
pub(in super::super) struct IgnoredMentions {
    /// Canonical paths, sorted.
    pub(in super::super) files: Vec<String>,
    /// Distinct deciding rules (`.gitignore:1 'gen/'`) of the first files.
    pub(in super::super) rules: Vec<String>,
    /// The scan failed or stopped at its bound: `files` is what it reached,
    /// and ignored files may mention the name beyond it.
    pub(in super::super) unfinished: bool,
    /// Of `files`, those that import the name: the evidence that ignore
    /// rules hide an importer. A file that only spells the name (a bundled
    /// copy declaring its own) is not one.
    pub(in super::super) importing: Vec<String>,
    /// Of `files`, those the import check could not decide (unreadable,
    /// too large, unparsable, or past its file bound): not importers, and
    /// not ruled out.
    pub(in super::super) unchecked: Vec<String>,
    /// Nested repositories (root-relative) the scan did not enter
    /// ([`octocode_engine::search::walk::NestedRepositories`]): ignored
    /// files in them are not counted.
    pub(in super::super) nested_repositories: Vec<String>,
}

impl IgnoredMentions {
    /// A scan that produced nothing: no file is known, and that is not
    /// proof of none.
    pub(in super::super) fn unfinished() -> Self {
        Self {
            unfinished: true,
            ..Self::default()
        }
    }
}

/// What a text scan reached. `complete` is false when the scan stopped at
/// its bound (cancelled by the budget before the walk finished).
pub(super) struct Scanned {
    pub(super) files: BTreeSet<String>,
    pub(super) complete: bool,
    /// A lifted scan's nested repositories it did not enter, root-relative
    /// and sorted.
    pub(super) nested: Vec<String>,
}

/// The read policy of a lifted scan, which also leaves nested repositories
/// out ([`NestedRepositories`]) and the directories the one default prune
/// does (a git-tracked `build/` is scanned, as every local tool walks it).
pub(super) struct Lifted {
    pub(super) policy: PathPolicy,
    pub(super) nested: Arc<NestedRepositories>,
    pub(super) prune: crate::policy::prune::OwnWalk,
}

impl octocode_engine::search::text_search::TextSearchPathFilter for Lifted {
    fn allows(&self, path: &Path, is_dir: bool) -> bool {
        self.policy.allows(path, is_dir)
            && !self.prune.prunes(path, is_dir)
            && !self.nested.skips(path, is_dir)
    }

    fn skipped_hidden(&self, path: &Path, is_dir: bool) {
        self.policy.skipped_hidden(path, is_dir);
    }

    fn skipped_ignored(&self, path: &Path, is_dir: bool) {
        self.policy.skipped_ignored(path, is_dir);
    }
}

/// Whether a text scan honors ignore rules (every lexical scan does) or
/// lifts them to find what they hide.
#[derive(Clone, Copy)]
pub(super) enum Ignores {
    Honored,
    Lifted,
}

/// Word-bounded files-only scan of `root` (family `include`, read policy),
/// observing `cancel` and the scan bounds. A scan that lifts ignore rules
/// still skips the default-excluded directories and nested repositories.
pub(super) async fn scan_text(
    root: &str,
    include: &[String],
    symbol: &str,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
    ignores: Ignores,
    budget: Duration,
) -> Result<Option<Scanned>, LspFailure> {
    if symbol.trim().is_empty() {
        return Ok(None);
    }
    let lifted = matches!(ignores, Ignores::Lifted);
    let options = TextSearchOptions {
        path: root.to_owned(),
        pattern: word_pattern(symbol),
        output: octocode_engine::types::SearchOutput::FilesWithMatch,
        include: (!include.is_empty()).then(|| include.to_vec()),
        ignore: octocode_engine::search::walk::IgnorePolicy {
            include_ignored: lifted,
            include_hidden: false,
        },
        walk_threads: lifted.then_some(LIFTED_SCAN_THREADS),
        ..TextSearchOptions::default()
    };
    let nested = Arc::new(NestedRepositories::new(Path::new(root)));
    let filter: Arc<dyn octocode_engine::search::text_search::TextSearchPathFilter> = if lifted {
        Arc::new(Lifted {
            policy: policy.clone(),
            nested: Arc::clone(&nested),
            prune: crate::policy::prune::OwnWalk::new(Path::new(root), []),
        })
    } else {
        Arc::new(policy.clone())
    };
    let base = root.to_owned();
    let scanned = blocking_cancellable(cancel, move |stopped| {
        let started = Instant::now();
        let bounded = || stopped() || started.elapsed() > budget;
        search_cancellable(options, filter, &bounded)
            .ok()
            .map(|parsed| Scanned {
                nested: nested.skipped(),
                complete: !parsed
                    .stats
                    .cap_reasons
                    .contains(&octocode_engine::types::CapReason::Cancelled),
                files: parsed
                    .files
                    .into_iter()
                    .map(|file| {
                        let path = Path::new(&file.path);
                        if path.is_absolute() {
                            canonical(&file.path)
                        } else {
                            canonical(&Path::new(&base).join(path).to_string_lossy())
                        }
                    })
                    .collect::<BTreeSet<_>>(),
            })
    })
    .await?
    .flatten();
    Ok(scanned)
}

pub(super) fn fingerprint(
    root: &Path,
    include: &[String],
    stopped: &(dyn Fn() -> bool + Sync),
    budget: Duration,
) -> Option<String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let extensions = include
        .iter()
        .filter_map(|glob| glob.strip_prefix("*."))
        .collect::<Vec<_>>();
    let started = Instant::now();
    let now = SystemTime::now();
    let entries = Mutex::new(Vec::new());
    let abandoned = AtomicBool::new(false);
    // One digest line per family or project file: relative path, length,
    // mtime, and the content hash of a freshly modified file.
    let line = |entry: ignore::DirEntry| -> Option<String> {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            return None;
        }
        let path = entry.path();
        let name = path.file_name().map(|name| name.to_string_lossy());
        let family = path.extension().is_some_and(|extension| {
            extensions.is_empty() || extensions.contains(&extension.to_string_lossy().as_ref())
        });
        if !family && !name.is_some_and(|name| PROJECT_FILES.contains(&name.as_ref())) {
            return None;
        }
        let meta = entry.metadata().ok()?;
        let Some(stamp) = crate::tools::source::cache_stamp(&meta) else {
            abandoned.store(true, Ordering::Relaxed);
            return None;
        };
        let modified = meta.modified().ok();
        let fresh = modified
            .is_none_or(|time| !now.duration_since(time).is_ok_and(|age| age >= FRESH_MTIME));
        let content = if fresh {
            match crate::tools::source::read_bounded(path, FRESH_CONTENT_MAX_BYTES) {
                Ok(bytes) => octocode_engine::digest::sha256(&bytes),
                Err(_) => {
                    abandoned.store(true, Ordering::Relaxed);
                    return None;
                }
            }
        } else {
            String::new()
        };
        let relative = path.strip_prefix(root).unwrap_or(path);
        Some(format!(
            "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{content}",
            relative.to_string_lossy(),
            stamp.size,
            stamp.modified_ns,
            stamp.changed_ns,
            stamp.inode,
        ))
    };
    // A parallel walk: the digest is order-free (lines are sorted).
    crate::tools::pruned_walk(root, [])
        .build_parallel()
        .run(|| {
            Box::new(|entry| {
                if abandoned.load(Ordering::Relaxed) {
                    return ignore::WalkState::Quit;
                }
                if stopped() || started.elapsed() > budget {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                }
                let Some(line) = entry.ok().and_then(line) else {
                    return ignore::WalkState::Continue;
                };
                let Ok(mut entries) = entries.lock() else {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                };
                entries.push(line);
                if entries.len() > FINGERPRINT_MAX_FILES {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                }
                ignore::WalkState::Continue
            })
        });
    if abandoned.load(Ordering::Relaxed) {
        return None;
    }
    let mut entries = entries.into_inner().ok()?;
    entries.sort_unstable();
    Some(octocode_engine::digest::sha256(
        entries.join("\n").as_bytes(),
    ))
}
