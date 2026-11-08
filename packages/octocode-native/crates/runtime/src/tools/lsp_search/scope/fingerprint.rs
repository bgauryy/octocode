//! Scope fingerprints, their invalidation proof, and file stamp comparison.
use super::*;

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
///
/// A fingerprint none of whose files was fresh is kept across requests with
/// a proof of what the walks read: each directory they entered (an entry
/// added, removed or renamed changes its stamp), the ignore files there and
/// outside them, the repository markers, and every file they stamped. While
/// it holds, every file is as it was and none is fresh at a later instant,
/// so a fresh walk would give the same lines: the kept fingerprint is
/// served without walking. A scope written within the settle window before
/// the walk began is not kept ([`Watch::capture`]).
pub(super) fn fingerprint(
    scope: &FingerprintScope<'_>,
    observed: SystemTime,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<Fingerprint> {
    let key = [
        scope.root.to_string_lossy().as_ref(),
        &scope
            .server_root
            .map(|root| root.to_string_lossy().into_owned())
            .unwrap_or_default(),
        &scope.include.join("\u{0}"),
    ]
    .join("\u{1}");
    if let Some(kept) = FINGERPRINTS.current(&key, ANY_POLICY) {
        return Some((kept.0.clone(), Arc::clone(&kept.1)));
    }
    #[cfg(test)]
    fingerprints::note(scope.root);
    // Before the walks' first read, on the request's own clock.
    let started = octocode_engine::stamp::settle_clock();
    let read = Mutex::new(Read::default());
    let found = walked_fingerprint(scope, observed, stopped, &read)?;
    let read = read.into_inner().ok()?;
    let any_fresh = found
        .1
        .values()
        .any(|stamp| !split_stamp(stamp).1.is_empty());
    if !any_fresh && let Some(proof) = read.proof(scope.root, started) {
        let bytes = found.0.len()
            + found
                .1
                .iter()
                .map(|(path, stamp)| path.len() + stamp.len() + 2 * std::mem::size_of::<String>())
                .sum::<usize>()
            + proof.weight();
        FINGERPRINTS.put_proven(
            key,
            ANY_POLICY.to_owned(),
            bytes,
            proof,
            Arc::new(found.clone()),
        );
    }
    Some(found)
}

/// Kept fingerprints, by scope root, server root and family globs, against
/// the one process budget.
static FINGERPRINTS: crate::tools::page_memo::PageMemo<
    crate::tools::page_memo::Proven<Fingerprint>,
> = crate::tools::page_memo::PageMemo::new();

/// The key already says what a fingerprint depends on.
const ANY_POLICY: &str = "";

/// What the fingerprint walks read.
#[derive(Default)]
struct Read {
    /// Entered directories, their ignore files, and the stamped files.
    watched: Vec<PathBuf>,
    /// Ignore inputs outside the walks: the walked roots' own and nested
    /// repositories'.
    rules: Vec<PathBuf>,
    /// An ignore input no stamp can watch: the fingerprint is not kept.
    untracked: bool,
}

impl Read {
    fn enter(&mut self, dir: &Path, walk_root: &Path) {
        let inputs = crate::tools::page_memo::directory_ignore_inputs(dir, walk_root, |_| true);
        self.watched.push(dir.to_path_buf());
        self.watched.extend(inputs.files);
        self.rules.extend(inputs.external);
        self.untracked |= inputs.untracked;
    }

    fn proof(self, root: &Path, started: SystemTime) -> Option<octocode_engine::stamp::Proof> {
        if self.untracked {
            return None;
        }
        crate::tools::page_memo::Watch {
            root,
            sources: &self.watched,
            rules: &self.rules,
            markers: &crate::tools::page_memo::repository_markers(root),
        }
        .capture(octocode_engine::stamp::Link::Itself, Some(started))
    }
}

/// Test-only count of the fingerprint walks of each scope root.
#[cfg(test)]
pub(super) mod fingerprints {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{LazyLock, Mutex, PoisonError};

    static WALKS: LazyLock<Mutex<HashMap<PathBuf, usize>>> = LazyLock::new(Mutex::default);

    pub(super) fn note(root: &Path) {
        *WALKS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(root.to_path_buf())
            .or_default() += 1;
    }

    pub(in super::super) fn of(root: &Path) -> usize {
        WALKS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(root)
            .copied()
            .unwrap_or(0)
    }
}

/// [`fingerprint`] by walking, recording what the walks read in `read`.
fn walked_fingerprint(
    scope: &FingerprintScope<'_>,
    observed: SystemTime,
    stopped: &(dyn Fn() -> bool + Sync),
    read: &Mutex<Read>,
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
        if !walk_lines(scope, dir, policy, observed, stopped, &lines, read) {
            return None;
        }
        if let Ok(mut read) = read.lock() {
            read.rules
                .extend(octocode_engine::search::walk::external_ignore_inputs(dir));
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
/// the write must then be its [`SourceStamp::settle_for`] window older than
/// `started`, as for any settled stamp. A stamp that does not parse proves nothing.
pub(in super::super) fn written_since(
    entries: &BTreeMap<String, String>,
    started: SystemTime,
) -> bool {
    let Some(started_ns) = started
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| u64::try_from(since.as_nanos()).ok())
    else {
        return true;
    };
    entries.values().any(|stamp| {
        let mut fields = stamp.split('\u{0}').skip(1).map(str::parse::<u64>);
        let (Some(Ok(modified_ns)), Some(Ok(changed_ns))) = (fields.next(), fields.next()) else {
            return true;
        };
        let stamp = octocode_engine::stamp::SourceStamp {
            modified_ns,
            changed_ns,
            ..Default::default()
        };
        !stamp.settled_by(started_ns)
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
    read: &Mutex<Read>,
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
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            match read.lock() {
                Ok(mut read) => read.enter(entry.path(), dir),
                Err(_) => abandoned.store(true, Ordering::Relaxed),
            }
            return None;
        }
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
        let Some(stamp) = octocode_engine::stamp::cache_stamp(&meta) else {
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
        match read.lock() {
            Ok(mut read) => read.watched.push(path.to_path_buf()),
            Err(_) => abandoned.store(true, Ordering::Relaxed),
        }
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
mod walks;
