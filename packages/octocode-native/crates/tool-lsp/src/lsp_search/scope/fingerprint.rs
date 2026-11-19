//! Scope fingerprints, their invalidation proof, and file stamp comparison.
use super::*;

/// A scope fingerprint: the digest, and the entries it covers (relative
/// path → stamp).
pub(super) type Fingerprint = (String, Arc<BTreeMap<String, String>>);

/// What a fingerprint covers: the family and project files of the lexical
/// scope `root` and of the language server's `server_root` that their ignore
/// rules, the dot rule and the default prune admit, as an editor's watcher
/// does, and the build-script output (`target/<profile>/build/<pkg>/out`)
/// rust-analyzer loads with build scripts on. Credential directories remain
/// excluded. A server that loads no file of a Cargo cache directory
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
    /// The global ignore rules of the call, which the honored walks apply.
    pub(super) global_ignore: octocode_engine::search::walk::GlobalIgnore,
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
/// the walk began is not kept ([`Watch::capture`](octocode_tools_shared::page_memo::Watch::capture)).
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
static FINGERPRINTS: octocode_tools_shared::page_memo::PageMemo<
    octocode_tools_shared::page_memo::Proven<SettledFingerprint>,
> = octocode_tools_shared::page_memo::PageMemo::new();

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
    fn enter(
        &mut self,
        dir: &Path,
        walk_root: &Path,
        global: octocode_engine::search::walk::GlobalIgnore,
    ) {
        let inputs = octocode_tools_shared::page_memo::directory_ignore_inputs_with(
            dir,
            walk_root,
            global,
            |_| true,
        );
        self.watched.push(dir.to_path_buf());
        self.watched.extend(inputs.files);
        self.rules.extend(inputs.external);
        self.untracked |= inputs.untracked;
    }

    /// Add what another walker thread read.
    fn absorb(&mut self, other: Self) {
        self.watched.extend(other.watched);
        self.rules.extend(other.rules);
        self.untracked |= other.untracked;
    }

    fn proof(self, root: &Path, started: SystemTime) -> Option<octocode_engine::stamp::Proof> {
        if self.untracked {
            return None;
        }
        octocode_tools_shared::page_memo::Watch {
            root,
            sources: &self.watched,
            rules: &self.rules,
            markers: &octocode_tools_shared::page_memo::repository_markers(root),
        }
        .capture(octocode_engine::stamp::Link::Itself, Some(started))
    }
}

/// The build-script output directories under `server`'s Cargo target
/// directory (`target/<profile>/build/<pkg>/out`): what rust-analyzer loads
/// of it with build scripts on, found by listing three directory levels.
fn build_script_outputs(server: &Path) -> Vec<std::path::PathBuf> {
    let children = |dir: &Path| {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .collect::<Vec<_>>()
    };
    let mut outs = children(&server.join("target"))
        .iter()
        .flat_map(|profile| children(&profile.join("build")))
        .map(|package| package.join("out"))
        .filter(|out| out.is_dir())
        .collect::<Vec<_>>();
    outs.sort_unstable();
    outs
}

/// Test-only count of the fingerprint walks of each scope root.
#[cfg(test)]
pub(super) static WALKS: super::path_counter::PathCounter = super::path_counter::PathCounter::new();

/// The directories a fingerprint walk covers whole: the server's root when
/// the scope sits inside it, else the scope's root and the server's.
pub(in super::super) fn walked_roots(root: &Path, server: Option<&Path>) -> Vec<PathBuf> {
    match server {
        Some(server) if root.starts_with(server) => vec![server.to_path_buf()],
        Some(server) => vec![root.to_path_buf(), server.to_path_buf()],
        None => vec![root.to_path_buf()],
    }
}

/// [`fingerprint`](fn@fingerprint) by walking, recording what the walks read in `read`.
fn walked_fingerprint(
    scope: &FingerprintScope<'_>,
    observed: SystemTime,
    stopped: &(dyn Fn() -> bool + Sync),
    read: &Mutex<Read>,
) -> Option<Fingerprint> {
    let lines = Mutex::new(Vec::new());
    // Every walk honors ignore rules, hidden entries and the default prune,
    // as an editor's watcher does (VS Code's `files.watcherExclude` drops
    // `.git/objects` and `node_modules/*/**`, rust-analyzer's own watcher
    // `target/`): what a server reads there it reads itself, and walking a
    // repository's dependency and build trees on every request costs more
    // than the request.
    let honored = octocode_engine::types::IgnorePolicy::default();
    let mut walks = walked_roots(scope.root, scope.server_root)
        .into_iter()
        .map(|dir| (dir, honored))
        .collect::<Vec<_>>();
    if let Some(server) = scope.server_root.filter(|_| !scope.cargo_caches_unloaded) {
        let lifted = octocode_engine::types::IgnorePolicy::ALL;
        walks.extend(
            build_script_outputs(server)
                .into_iter()
                .map(|out| (out, lifted)),
        );
    }
    for (dir, policy) in &walks {
        let (dir, policy) = (dir.as_path(), *policy);
        if !walk_lines(scope, dir, policy, observed, stopped, &lines, read) {
            return None;
        }
        if let Ok(mut read) = read.lock() {
            read.rules
                .extend(octocode_engine::search::walk::external_ignore_inputs_with(
                    dir,
                    scope.global_ignore,
                ));
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
/// the write must then be its [`SourceStamp::settle_for`](octocode_engine::stamp::SourceStamp::settle_for) window older than
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
    let extensions = scope
        .include
        .iter()
        .filter_map(|glob| glob.strip_prefix("*."))
        .collect::<Vec<_>>();
    let abandoned = AtomicBool::new(false);
    let unloaded = (scope.cargo_caches_unloaded && scope.server_root == Some(dir))
        .then(|| octocode_engine::lsp::workspace::UnloadedCargoCaches::of(dir))
        .flatten();
    let stamp = Stamp {
        scope,
        dir,
        extensions: &extensions,
        observed,
        stopped,
    };
    // Both walks keep the default prune and the credential exclusions.
    octocode_tools_shared::pruned_walk_under(dir, true, [], (policy, scope.global_ignore))
        .build_parallel()
        .run(|| {
            // Each walker thread keeps its own lines and reads, merged once
            // when it ends: an entry takes no lock.
            let mut local = Local {
                lines: Vec::new(),
                read: Read::default(),
                into: (lines, read),
                abandoned: &abandoned,
            };
            let (stamp, unloaded, abandoned) = (&stamp, &unloaded, &abandoned);
            Box::new(move |entry| {
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
                    local.read.watched.push(
                        entry
                            .path()
                            .join(octocode_engine::lsp::workspace::CACHEDIR_TAG),
                    );
                    return ignore::WalkState::Skip;
                }
                match stamp.line(&entry, &mut local.read) {
                    Ok(Some(line)) => local.lines.push(line),
                    Ok(None) => {}
                    Err(()) => {
                        abandoned.store(true, Ordering::Relaxed);
                        return ignore::WalkState::Quit;
                    }
                }
                ignore::WalkState::Continue
            })
        });
    !abandoned.load(Ordering::Relaxed)
}

/// One walker thread's lines and reads, added to the walk's shared ones when
/// the thread's visitor is dropped.
struct Local<'a> {
    lines: Vec<String>,
    read: Read,
    into: (&'a Mutex<Vec<String>>, &'a Mutex<Read>),
    abandoned: &'a std::sync::atomic::AtomicBool,
}

impl Drop for Local<'_> {
    fn drop(&mut self) {
        let (lines, read) = self.into;
        match (lines.lock(), read.lock()) {
            (Ok(mut lines), Ok(mut read)) => {
                lines.append(&mut self.lines);
                read.absorb(std::mem::take(&mut self.read));
            }
            _ => self
                .abandoned
                .store(true, std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// How a fingerprint walk of `dir` stamps its entries.
struct Stamp<'a> {
    scope: &'a FingerprintScope<'a>,
    dir: &'a Path,
    extensions: &'a [&'a str],
    observed: SystemTime,
    stopped: &'a (dyn Fn() -> bool + Sync),
}

impl Stamp<'_> {
    /// One digest line per family or project file: relative path, length,
    /// mtime, and the content hash of a file modified near `observed`; an
    /// entered directory is recorded in `read`. `Err` when a file could not
    /// be stamped: the fingerprint is abandoned.
    fn line(&self, entry: &ignore::DirEntry, read: &mut Read) -> Result<Option<String>, ()> {
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            read.enter(entry.path(), self.dir, self.scope.global_ignore);
            return Ok(None);
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            return Ok(None);
        }
        let path = entry.path();
        let name = path.file_name().map(|name| name.to_string_lossy());
        let family = path.extension().is_some_and(|extension| {
            self.extensions.is_empty()
                || self
                    .extensions
                    .contains(&extension.to_string_lossy().as_ref())
        });
        if !family && !name.is_some_and(|name| PROJECT_FILES.contains(&name.as_ref())) {
            return Ok(None);
        }
        let meta = entry.metadata().map_err(|_| ())?;
        let stamp = octocode_engine::stamp::cache_stamp(&meta).ok_or(())?;
        let fresh = meta.modified().ok().is_none_or(|time| {
            !self
                .observed
                .duration_since(time)
                .is_ok_and(|age| age >= FRESH_MTIME)
        });
        let content = if fresh {
            source_digest(path, Some(stamp), self.stopped).ok_or(())?
        } else {
            String::new()
        };
        read.watched.push(path.to_path_buf());
        let relative = path.strip_prefix(self.scope.root).unwrap_or(path);
        Ok(Some(format!(
            "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{content}",
            relative.to_string_lossy(),
            stamp.size,
            stamp.modified_ns,
            stamp.changed_ns,
            stamp.inode,
        )))
    }
}

#[cfg(test)]
mod cargo_caches;
#[cfg(test)]
mod walks;
