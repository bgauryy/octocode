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

/// Word-bounded files-only scan of `root` (family `include`, read policy,
/// ignore rules honored), observing `cancel` and `budget`. `None` when the
/// scan failed or stopped at its bound.
pub(super) async fn scan_text(
    root: &str,
    include: &[String],
    symbol: &str,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
    budget: Duration,
) -> Result<Option<BTreeSet<String>>, LspFailure> {
    if symbol.trim().is_empty() {
        return Ok(None);
    }
    let options = TextSearchOptions {
        path: root.to_owned(),
        pattern: word_pattern(symbol),
        output: octocode_engine::types::SearchOutput::FilesWithMatch,
        include: (!include.is_empty()).then(|| include.to_vec()),
        ..TextSearchOptions::default()
    };
    let filter: Arc<dyn octocode_engine::search::text_search::TextSearchPathFilter> =
        Arc::new(policy.clone());
    let base = root.to_owned();
    let scanned = blocking_cancellable(cancel, move |stopped| {
        let started = Instant::now();
        let bounded = || stopped() || started.elapsed() > budget;
        search_cancellable(options, filter, &bounded)
            .ok()
            .filter(|parsed| {
                !parsed
                    .stats
                    .cap_reasons
                    .contains(&octocode_engine::types::CapReason::Cancelled)
            })
            .map(|parsed| {
                parsed
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
                    .collect::<BTreeSet<_>>()
            })
    })
    .await?
    .flatten();
    Ok(scanned)
}

/// The digest of every family and project file under `root`, with the
/// entries it covers (relative path → stamp). Ignored files are walked too:
/// a generated source a `.gitignore` hides is still one the server compiles.
pub(super) fn fingerprint(
    root: &Path,
    include: &[String],
    stopped: &(dyn Fn() -> bool + Sync),
    budget: Duration,
) -> Option<(String, Arc<BTreeMap<String, String>>)> {
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
    crate::tools::pruned_walk_with(
        root,
        [],
        octocode_engine::search::walk::IgnorePolicy {
            include_ignored: true,
            ..Default::default()
        },
    )
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
    let digest = octocode_engine::digest::sha256(entries.join("\n").as_bytes());
    let stamps = entries
        .into_iter()
        .filter_map(|line| {
            let (path, stamp) = line.split_once('\u{0}')?;
            Some((path.to_owned(), stamp.to_owned()))
        })
        .collect::<BTreeMap<_, _>>();
    Some((digest, Arc::new(stamps)))
}
