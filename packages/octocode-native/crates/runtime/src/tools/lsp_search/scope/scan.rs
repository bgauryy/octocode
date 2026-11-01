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
    let options = TextSearchOptions {
        path: root.to_owned(),
        pattern: word_pattern(symbol),
        output: octocode_engine::types::SearchOutput::FilesWithMatch,
        include: (!include.is_empty()).then(|| include.to_vec()),
        ..TextSearchOptions::default()
    };
    let filter: Arc<dyn octocode_engine::search::text_search::TextSearchPathFilter> =
        Arc::new(crate::policy::path::DiscoveryWalk::owned(policy.clone()));
    let base = root.to_owned();
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
                        canonical(&file.path)
                    } else {
                        canonical(&Path::new(&base).join(path).to_string_lossy())
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

/// The flight key of a scope.
type FlightKey = (PathBuf, Option<PathBuf>, Vec<String>);

impl FingerprintScope<'_> {
    fn key(&self) -> FlightKey {
        (
            self.root.to_path_buf(),
            self.server_root.map(Path::to_path_buf),
            self.include.to_vec(),
        )
    }
}

/// One fingerprint walk in flight per scope: the walks a batch's queries
/// start at once over one scope share it.
type Flight = Arc<std::sync::OnceLock<Option<Fingerprint>>>;
/// The walks in flight, by scope.
type Flights = Mutex<HashMap<FlightKey, Flight>>;

fn flights() -> &'static Flights {
    static FLIGHTS: std::sync::LazyLock<Flights> = std::sync::LazyLock::new(Mutex::default);
    &FLIGHTS
}

/// [`fingerprint`] of `scope`, shared by the requests that ask while one
/// walk of that scope is running: a batch's queries over one scope (they
/// start together) walk it once, and each takes the digest of the files as
/// that walk found them. A request asking after the walk ends walks again,
/// so no edit made since is missed. A request whose shared walk was stopped
/// by the request that ran it walks for itself.
pub(super) fn shared_fingerprint(
    scope: &FingerprintScope<'_>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<Fingerprint> {
    let key = scope.key();
    let flight = Arc::clone(
        flights()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(key.clone())
            .or_default(),
    );
    let mut ran = false;
    let found = flight
        .get_or_init(|| {
            ran = true;
            let found = fingerprint(scope, stopped);
            let mut flights = flights()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if flights
                .get(&key)
                .is_some_and(|held| Arc::ptr_eq(held, &flight))
            {
                flights.remove(&key);
            }
            found
        })
        .clone();
    match found {
        None if !ran && !stopped() => fingerprint(scope, stopped),
        found => found,
    }
}

/// The requests holding the walk of `scope` now (the flight table's own
/// reference included); 0 when none is in flight.
#[cfg(test)]
pub(super) fn flight_holders(scope: &FingerprintScope<'_>) -> usize {
    flights()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&scope.key())
        .map_or(0, Arc::strong_count)
}

/// The digest of every file `scope` covers ([`FingerprintScope`]), with
/// the entries it covers (path relative to the scope root → stamp). The
/// walks cover the whole scope, however long they take: only `stopped`
/// (the request's cancellation or deadline) ends them early, and then, or
/// when a file cannot be stamped, there is no fingerprint (responses are
/// not reused across requests: correct, only slower).
pub(super) fn fingerprint(
    scope: &FingerprintScope<'_>,
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
        Some(server) => vec![(scope.root, honored), (server, lifted)],
        None => vec![(scope.root, honored)],
    };
    for (dir, policy) in walks {
        if !walk_lines(scope, dir, policy, stopped, &lines) {
            return None;
        }
    }
    let mut entries = lines.into_inner().ok()?;
    entries.sort_unstable();
    // A file both walks reach is one entry.
    entries.dedup();
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

/// Add one digest line per family or project file under `dir` (walked
/// under `policy`) to `lines`: path relative to the scope root, length,
/// mtime, and the content hash of a freshly modified file. False when the
/// walk was stopped or a file could not be stamped.
fn walk_lines(
    scope: &FingerprintScope<'_>,
    dir: &Path,
    policy: octocode_engine::types::IgnorePolicy,
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
    let now = SystemTime::now();
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
