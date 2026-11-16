//! Scope fingerprints, their invalidation proof, and file stamp comparison.
use super::*;

/// A scope fingerprint: the digest, and the entries it covers (relative
/// path → stamp).
pub(super) type Fingerprint = (String, Arc<BTreeMap<String, String>>);

/// What a fingerprint covers: the family and project files of the lexical
/// scope `root` that its ignore rules admit (what the text scans read),
/// and every one under the language server's `server_root`, including hidden
/// and ignored build sources the server may compile. Credential directories
/// remain excluded. A server that loads no file of a Cargo cache directory
/// ([`octocode_engine::lsp::workspace::UnloadedCargoCaches`]: rust-analyzer
/// with build scripts off) has none of them covered.
#[derive(Clone, Copy)]
pub(super) struct FingerprintScope<'a> {
    pub(super) root: &'a Path,
    pub(super) server_root: Option<&'a Path>,
    pub(super) include: &'a [String],
    /// The server is rust-analyzer with build scripts off: the Cargo cache
    /// directories it leaves out are not walked.
    pub(super) cargo_caches_unloaded: bool,
}

/// The digest of every file `scope` covers ([`FingerprintScope`]), with
/// the entries it covers (path relative to the scope root → stamp), as of
/// `observed`: a file modified within [`FRESH_MTIME`] before it (or after
/// it) also digests its content. The digest depends on the files and
/// `observed` alone, never on when the walk runs, so a later walk at the
/// same `observed` gives unchanged files the same digest. The walks cover
/// the whole scope, however long they take: only `stopped` (the request's
/// cancellation or deadline) ends them early. Then, or when a file cannot
/// be read or stamped consistently, there is no fingerprint. Watched-file
/// synchronization must report that failure rather than accept a stale index.
///
/// A fingerprint none of whose files was fresh is kept across requests with
/// a proof of what the walks read: each directory they entered (an entry
/// added, removed or renamed changes its stamp), the ignore files there and
/// outside them, the repository markers, and every file they stamped. While
/// it holds, every file is as it was and none is fresh at a later instant,
/// so a fresh walk would give the same lines: the kept fingerprint is
/// served without walking only at or after its captured observation. A pinned
/// earlier instant may still need content hashes. A scope written within the settle window before
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
        if scope.cargo_caches_unloaded {
            "cargo caches unloaded"
        } else {
            ""
        },
    ]
    .join("\u{1}");
    if let Some(kept) = FINGERPRINTS.current(&key, ANY_POLICY)
        && observed >= kept.observed
    {
        return Some((kept.fingerprint.0.clone(), Arc::clone(&kept.fingerprint.1)));
    }
    #[cfg(test)]
    WALKS.note(scope.root);
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
        let bytes = std::mem::size_of::<SystemTime>()
            + found.0.len()
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
            Arc::new(SettledFingerprint {
                observed,
                fingerprint: found.clone(),
            }),
        );
    }
    Some(found)
}

/// A metadata-only fingerprint remains the same at later observations while
/// its filesystem proof holds. An earlier observation may require content
/// hashes, so it must walk again without discarding this useful settled value.
struct SettledFingerprint {
    observed: SystemTime,
    fingerprint: Fingerprint,
}

/// Kept fingerprints, by scope root, server root and family globs, against
/// the one process budget. Observation bounds belong to the value, not the key.
static FINGERPRINTS: crate::tools::page_memo::PageMemo<
    crate::tools::page_memo::Proven<SettledFingerprint>,
> = crate::tools::page_memo::PageMemo::new();

/// Scope policy is already in the key; the value checks observation validity.
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
pub(super) static WALKS: super::path_counter::PathCounter = super::path_counter::PathCounter::new();

/// [`fingerprint`] by walking, recording what the walks read in `read`.
fn walked_fingerprint(
    scope: &FingerprintScope<'_>,
    observed: SystemTime,
    stopped: &(dyn Fn() -> bool + Sync),
    read: &Mutex<Read>,
) -> Option<Fingerprint> {
    let lines = Mutex::new(Vec::new());
    let honored = octocode_engine::types::IgnorePolicy::default();
    let lifted = octocode_engine::types::IgnorePolicy::ALL;
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
pub(in super::super) fn same_file(
    path: &Path,
    before: &str,
    after: &str,
    stopped: &(dyn Fn() -> bool + Sync),
) -> bool {
    let ((metadata, hashed), (current, hash)) = (split_stamp(before), split_stamp(after));
    if metadata != current {
        return false;
    }
    match (hashed.is_empty(), hash.is_empty()) {
        (false, false) => hashed == hash,
        (false, true) => source_digest(path, None, stopped).is_some_and(|hash| hash == hashed),
        (true, _) => true,
    }
}

/// Hash a regular source without a size cutoff or a whole-file allocation.
/// The opened file must retain its identity and metadata throughout the read.
fn source_digest(
    path: &Path,
    expected: Option<octocode_engine::stamp::SourceStamp>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<String> {
    use std::io::Read;

    if stopped() {
        return None;
    }
    let (mut file, metadata) = octocode_engine::text::open_regular(path).ok()?;
    let before = octocode_engine::stamp::cache_stamp(&metadata)?;
    if expected.is_some_and(|expected| expected != before) {
        return None;
    }
    let mut hash = octocode_engine::digest::HashWriter::default();
    let mut buffer = vec![0; 64 * 1024].into_boxed_slice();
    loop {
        if stopped() {
            return None;
        }
        let length = match file.read(&mut buffer) {
            Ok(length) => length,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        };
        if length == 0 {
            break;
        }
        hash.update(&buffer[..length]);
    }
    let after = octocode_engine::stamp::cache_stamp(&file.metadata().ok()?)?;
    let current = std::fs::symlink_metadata(path).ok()?;
    (before == after
        && current.is_file()
        && octocode_engine::stamp::cache_stamp(&current) == Some(before))
    .then(|| hex::encode(hash.finalize()))
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
/// mtime, and the content hash of a file modified near `observed`. The
/// server's walk does not enter a Cargo cache directory the server loads
/// nothing of ([`FingerprintScope::cargo_caches_unloaded`]; with no Cargo
/// manifest to say what it loads, it enters every one), and watches its tag
/// instead: removing the tag makes it a directory to walk. False when the
/// walk was stopped or a file could not be stamped.
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
    let unloaded = (scope.cargo_caches_unloaded && scope.server_root == Some(dir))
        .then(|| octocode_engine::lsp::workspace::UnloadedCargoCaches::of(dir))
        .flatten();
    // One digest line per family or project file: relative path, length,
    // mtime, and the content hash of a file modified near `observed`.
    let line = |entry: &ignore::DirEntry| -> Option<String> {
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
        let Ok(meta) = entry.metadata() else {
            abandoned.store(true, Ordering::Relaxed);
            return None;
        };
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
            match source_digest(path, Some(stamp), stopped) {
                Some(hash) => hash,
                None => {
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
    // Search exclusions do not bound what a language server compiles. The
    // server's source inventory includes build directories; lexical scope
    // keeps its usual pruning. Both walks retain credential exclusions.
    let defaults = scope.server_root != Some(dir);
    crate::tools::pruned_walk_under(dir, defaults, [], policy)
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
                let Ok(entry) = entry else {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                };
                if entry.depth() > 0
                    && entry.file_type().is_some_and(|kind| kind.is_dir())
                    && unloaded
                        .as_ref()
                        .is_some_and(|unloaded| unloaded.contains(entry.path()))
                {
                    let tag = entry
                        .path()
                        .join(octocode_engine::lsp::workspace::CACHEDIR_TAG);
                    return match read.lock() {
                        Ok(mut read) => {
                            read.watched.push(tag);
                            ignore::WalkState::Skip
                        }
                        Err(_) => {
                            abandoned.store(true, Ordering::Relaxed);
                            ignore::WalkState::Quit
                        }
                    };
                }
                let Some(line) = line(&entry) else {
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
mod cargo_caches;
#[cfg(test)]
mod walks;
