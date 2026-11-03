//! The text scan behind the scope, and the scope fingerprint.
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
/// ignore rules honored), observing `cancel`. The scan walks the whole
/// scope: only the request stops it (its cancellation or its deadline),
/// and a stopped scan keeps nothing. `None` when the scan failed.
pub(super) async fn scan_text(
    root: &str,
    include: &[String],
    symbol: &str,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Option<BTreeSet<String>>, LspFailure> {
    if symbol.trim().is_empty() {
        return Ok(None);
    }
    // The walk follows no symlink, so a file it reaches under a canonical
    // root is canonical: the root is resolved once, not each file.
    #[cfg(test)]
    if let Some(work) = super::probe::current() {
        work.canonicalized
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    let base = canonical(root);
    let options = TextSearchOptions {
        path: base.clone(),
        pattern: word_pattern(symbol),
        output: octocode_engine::types::SearchOutput::FilesWithMatch,
        include: crate::tools::variant_field::non_empty(include),
        ..TextSearchOptions::default()
    };
    let filter: Arc<dyn octocode_engine::search::text_search::TextSearchPathFilter> =
        Arc::new(crate::policy::path::DiscoveryWalk::owned(policy.clone()));
    #[cfg(test)]
    if let Some(work) = super::probe::current() {
        work.text_scans
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    let scanned = blocking_cancellable(cancel, move |stopped| {
        let parsed = search_cancellable(options, filter, stopped).ok()?;
        if parsed
            .stats
            .cap_reasons
            .contains(&octocode_engine::types::CapReason::Cancelled)
        {
            return None;
        }
        Some(
            parsed
                .files
                .into_iter()
                .map(|file| {
                    let path = Path::new(&file.path);
                    if path.is_absolute() {
                        file.path
                    } else {
                        Path::new(&base).join(path).to_string_lossy().into_owned()
                    }
                })
                .collect::<BTreeSet<_>>(),
        )
    })
    .await?
    .flatten();
    Ok(scanned)
}

/// A scope fingerprint: the digest, and the entries it covers (relative
/// path → stamp).
pub(super) type Fingerprint = (String, Arc<BTreeMap<String, String>>);

/// What a fingerprint covers: the family and project files of the lexical
/// scope `root` that its ignore rules admit (what the text scans read),
/// and every one under the language server's `server_root`, ignored files
/// included (a generated source a `.gitignore` hides is still one the
/// server compiles).
#[derive(Clone, Copy)]
pub(super) struct FingerprintScope<'a> {
    pub(super) root: &'a Path,
    pub(super) server_root: Option<&'a Path>,
    pub(super) include: &'a [String],
}

/// The digest of every file `scope` covers ([`FingerprintScope`]), with
/// the entries it covers (path relative to the scope root → stamp), as of
/// `observed`: a file modified within [`FRESH_MTIME`] before it (or after
/// it) also digests its content. The digest depends on the files and
/// `observed` alone, never on when the walk runs, so a later walk at the
/// same `observed` gives unchanged files the same digest. The walks cover
/// the whole scope, however long they take: only `stopped` (the request's
/// cancellation or deadline) ends them early. Then, or when a file cannot
/// be stamped or an unsettled one is too large to digest, there is no
/// fingerprint, and the revision covers the anchor text alone.
pub(super) fn fingerprint(
    scope: &FingerprintScope<'_>,
    observed: SystemTime,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<Fingerprint> {
    let lines = Mutex::new(Vec::new());
    let honored = octocode_engine::types::IgnorePolicy::default();
    let lifted = octocode_engine::types::IgnorePolicy {
        include_ignored: true,
        ..Default::default()
    };
    let walks = match scope.server_root {
        // The server's root is the scope: one walk, ignored files included.
        Some(server) if server == scope.root => vec![(scope.root, lifted)],
        // The server's walk (ignored files included) already reaches every
        // file under the scope with the same line: walking the scope too
        // would stamp, and hash, each of them twice.
        Some(server) if scope.root.starts_with(server) => vec![(server, lifted)],
        Some(server) => vec![(scope.root, honored), (server, lifted)],
        None => vec![(scope.root, honored)],
    };
    for (dir, policy) in walks {
        if !walk_lines(scope, dir, policy, observed, stopped, &lines) {
            return None;
        }
    }
    let mut entries = lines.into_inner().ok()?;
    entries.sort_unstable();
    // A file both walks reach is one entry.
    entries.dedup();
    // The digest of the lines joined by newlines, hashed without the join.
    let mut hasher = octocode_engine::digest::HashWriter::default();
    for (index, line) in entries.iter().enumerate() {
        if index > 0 {
            hasher.update(b"\n");
        }
        hasher.update(line.as_bytes());
    }
    let digest = hex::encode(hasher.finalize());
    let stamps = entries
        .into_iter()
        .filter_map(|line| {
            let (path, stamp) = line.split_once('\u{0}')?;
            Some((path.to_owned(), stamp.to_owned()))
        })
        .collect::<BTreeMap<_, _>>();
    Some((digest, Arc::new(stamps)))
}

/// Whether `before` and `after`, two stamps of the file at `path` from walks
/// observed at different instants, describe the same file. A stamp carries
/// the content hash only when its mtime was near its walk's instant, so a
/// file that settled between the walks lost its hash without changing: the
/// metadata decides, and a hash it had is checked against its content now
/// (a same-tick edit on a coarse-mtime filesystem keeps the metadata).
pub(in super::super) fn same_file(path: &Path, before: &str, after: &str) -> bool {
    let ((metadata, hashed), (current, hash)) = (split_stamp(before), split_stamp(after));
    if metadata != current {
        return false;
    }
    match (hashed.is_empty(), hash.is_empty()) {
        (false, false) => hashed == hash,
        (false, true) => crate::tools::source::read_bounded(path, FRESH_CONTENT_MAX_BYTES)
            .is_ok_and(|bytes| octocode_engine::digest::sha256(&bytes) == hashed),
        (true, _) => true,
    }
}

/// Whether any file of a walk was last written after `started`: a page
/// that began then may have read it before the write, so a revision built
/// from this walk cannot prove the page's rows. A file's last write is the
/// later of its mtime and ctime. A timestamp of whole seconds comes from a
/// coarse clock, where a write after `started` can carry a time before it:
/// the write must then be [`SourceStamp::SETTLE`] older than `started`, as
/// for any settled stamp. A stamp that does not parse proves nothing.
pub(in super::super) fn written_since(
    entries: &BTreeMap<String, String>,
    started: SystemTime,
) -> bool {
    const SECOND_NS: u64 = 1_000_000_000;
    let Some(started_ns) = started
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| u64::try_from(since.as_nanos()).ok())
    else {
        return true;
    };
    let settle =
        u64::try_from(octocode_engine::stamp::SourceStamp::SETTLE.as_nanos()).unwrap_or(u64::MAX);
    entries.values().any(|stamp| {
        let mut fields = stamp.split('\u{0}').skip(1).map(str::parse::<u64>);
        let (Some(Ok(modified)), Some(Ok(changed))) = (fields.next(), fields.next()) else {
            return true;
        };
        let last_write = modified.max(changed);
        let coarse = last_write % SECOND_NS == 0;
        last_write.saturating_add(if coarse { settle } else { 0 }) > started_ns
    })
}

/// A stamp's metadata and its content hash (empty for a settled file).
fn split_stamp(stamp: &str) -> (&str, &str) {
    stamp.rsplit_once('\u{0}').unwrap_or((stamp, ""))
}

/// Add one digest line per family or project file under `dir` (walked
/// under `policy`) to `lines`: path relative to the scope root, length,
/// mtime, and the content hash of a file modified near `observed`. False
/// when the walk was stopped or a file could not be stamped.
fn walk_lines(
    scope: &FingerprintScope<'_>,
    dir: &Path,
    policy: octocode_engine::types::IgnorePolicy,
    observed: SystemTime,
    stopped: &(dyn Fn() -> bool + Sync),
    lines: &Mutex<Vec<String>>,
) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    let root = scope.root;
    let extensions = scope
        .include
        .iter()
        .filter_map(|glob| glob.strip_prefix("*."))
        .collect::<Vec<_>>();
    let abandoned = AtomicBool::new(false);
    // One digest line per family or project file: relative path, length,
    // mtime, and the content hash of a file modified near `observed`.
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
        let fresh = modified.is_none_or(|time| {
            !observed
                .duration_since(time)
                .is_ok_and(|age| age >= FRESH_MTIME)
        });
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
    crate::tools::pruned_walk_with(dir, [], policy)
        .build_parallel()
        .run(|| {
            Box::new(|entry| {
                if abandoned.load(Ordering::Relaxed) {
                    return ignore::WalkState::Quit;
                }
                if stopped() {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                }
                let Some(line) = entry.ok().and_then(line) else {
                    return ignore::WalkState::Continue;
                };
                let Ok(mut entries) = lines.lock() else {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                };
                entries.push(line);
                ignore::WalkState::Continue
            })
        });
    !abandoned.load(Ordering::Relaxed)
}

#[cfg(test)]
mod walks {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Entries a fingerprint of `root` (server at `server`) visits.
    fn visited(root: &Path, server: &Path) -> (usize, Vec<String>) {
        let include = ["*.rs".to_owned()];
        let scope = FingerprintScope {
            root,
            server_root: Some(server),
            include: &include,
        };
        let visits = AtomicUsize::new(0);
        let stopped = || {
            visits.fetch_add(1, Ordering::Relaxed);
            false
        };
        let (_, entries) = fingerprint(&scope, SystemTime::now(), &stopped).expect("fingerprint");
        (visits.into_inner(), entries.keys().cloned().collect())
    }

    /// A scope inside the server's root is covered by the server's walk:
    /// each of its files is visited, stamped and hashed once per request.
    #[test]
    fn a_scope_under_the_server_root_is_walked_once() {
        let dir = tempfile::tempdir().expect("dir");
        let server = dir.path();
        for file in ["a.rs", "sub/b.rs", "sub/deep/c.rs"] {
            let path = server.join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(path, "pub fn f() {}\n").expect("file");
        }
        let (server_walk, _) = visited(server, server);
        let (visits, entries) = visited(&server.join("sub"), server);
        assert_eq!(visits, server_walk, "one walk of the server's root");
        assert!(
            ["b.rs", "deep/c.rs"]
                .iter()
                .all(|file| entries.iter().any(|entry| entry == file)),
            "{entries:?}"
        );
    }
}
