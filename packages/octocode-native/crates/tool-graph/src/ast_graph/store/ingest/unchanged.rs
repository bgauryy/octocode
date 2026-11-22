//! An unchanged scope answers with its current snapshot instead of a build.
//!
//! "Unchanged" is the one freshness rule queries report as `fresh`
//! ([`snapshot_changes`] plus [`analysis_current`]), over a snapshot built
//! with the same options, parser registry, format and Cargo setting. Only
//! then is a rebuild certain to publish the same graph, so answering with
//! the existing snapshot loses nothing; `refresh` always rebuilds.
use super::*;

/// The inputs a build reads besides the tree: equal identities over an
/// unchanged tree build the same snapshot. Recorded in the manifest.
pub(super) fn build_identity(
    options: &IngestOptions,
    cargo: Option<&str>,
    registry_identity: &str,
) -> String {
    identity_of(
        Versions::CURRENT,
        options,
        cargo,
        registry_identity,
        octocode_engine::security::patterns_identity(),
    )
}

/// The representation versions a snapshot depends on: storage format,
/// extracted facts, and binding and projection.
#[derive(Clone, Copy)]
struct Versions {
    format: u32,
    facts: u32,
    analysis: u32,
}

impl Versions {
    const CURRENT: Self = Self {
        format: format::FORMAT_VERSION,
        facts: octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION,
        analysis: super::super::super::ANALYSIS_VERSION,
    };
}

fn identity_of(
    versions: Versions,
    options: &IngestOptions,
    cargo: Option<&str>,
    registry_identity: &str,
    patterns: &str,
) -> String {
    octocode_engine::digest::sha256(
        json!([
            versions.format,
            versions.facts,
            versions.analysis,
            env!("CARGO_PKG_VERSION"),
            registry_identity,
            // The snapshot's redacted spans and secret-bearing marks come
            // from the known-format list; see `graph::cache::namespace`.
            patterns,
            options.exclude,
            options.walk.disclosure(),
            options.language_globs,
            cargo,
        ])
        .to_string()
        .as_bytes(),
    )
}

/// What a build's inputs besides the tree hash to ([`build_identity`]) and
/// the parser registry's own identity, as a manifest records both.
pub(super) struct Identity<'a> {
    pub build: &'a str,
    pub registry: &'a str,
}

/// Whether a scope's newest snapshot answers a build.
pub(super) enum Reuse {
    /// It does: its receipt.
    Current(GraphOutput),
    /// It does not, and `judged` is the snapshot that was found wanting
    /// (`None`: the scope had none, or the build must run regardless).
    Build { judged: Option<String> },
}

/// The newest snapshot of `options.path`, when it was built with
/// `identity` and nothing it covers changed since: its receipt, with
/// `latest` pointed at it and its read lease renewed so retention keeps it.
/// The check runs under the build lock, which every publication and prune
/// takes, so the snapshot cannot be replaced or removed while it runs.
pub(super) fn reuse(
    options: &IngestOptions,
    home: &Path,
    paths: &PathPolicy,
    identity: &Identity<'_>,
    cancel: &dyn CancellationCheck,
    started: Instant,
) -> Reuse {
    if options.refresh || !home.is_dir() {
        return Reuse::Build { judged: None };
    }
    let Ok(root) = paths.validate(&options.path) else {
        return Reuse::Build { judged: None };
    };
    // Unlocked probe first: a scope with no current snapshot never waits
    // for another scope's publication.
    let judged = match newest(home, &root.canonical.to_string_lossy()) {
        Some((_, manifest)) if manifest["buildIdentity"] == identity.build => None,
        probed => {
            return Reuse::Build {
                judged: probed.map(|(id, _)| id),
            };
        }
    };
    let _lock = match publication::lock(home, paths, cancel) {
        Ok(lock) => Some(lock),
        // A home this process cannot write takes no lock, and nothing can
        // replace or prune a snapshot through it either: the snapshot
        // answers an unchanged scope as it is, without the `latest` marker
        // and lease a writable home renews. Any other failure builds.
        Err(refused)
            if octocode_runtime_base::id::error_code_of(&refused.value)
                == Some(ErrorCode::FileAccessFailed)
                && !writable(home) =>
        {
            None
        }
        Err(_) => return Reuse::Build { judged },
    };
    current(options, home, paths, identity, cancel, started)
}

/// Whether this process can create files in `home`.
fn writable(home: &Path) -> bool {
    let probe = home.join(format!(".write-probe-{}", std::process::id()));
    let created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    let _ = std::fs::remove_file(&probe);
    created
}

/// [`reuse`] for a build that has reached publication and holds the build
/// lock: another build of the scope may have published meanwhile, and the
/// same inputs over an unchanged tree built the same graph, so this one
/// answers with that snapshot instead of publishing a second copy. The
/// snapshot `judged` before the build is not checked twice.
pub(super) fn published_meanwhile(
    options: &IngestOptions,
    home: &Path,
    paths: &PathPolicy,
    identity: &Identity<'_>,
    judged: Option<&str>,
    cancel: &dyn CancellationCheck,
    started: Instant,
) -> Option<(String, GraphOutput)> {
    let root = paths.validate(&options.path).ok()?.canonical;
    let (id, _) = newest(home, &root.to_string_lossy())?;
    if options.refresh || judged == Some(id.as_str()) {
        return None;
    }
    match current(options, home, paths, identity, cancel, started) {
        Reuse::Current(receipt) => Some((id, receipt)),
        Reuse::Build { .. } => None,
    }
}

/// The locked half of [`reuse`]: the caller holds the build lock.
fn current(
    options: &IngestOptions,
    home: &Path,
    paths: &PathPolicy,
    identity: &Identity<'_>,
    cancel: &dyn CancellationCheck,
    started: Instant,
) -> Reuse {
    let newest = paths.validate(&options.path).ok().and_then(|root| {
        Some((
            newest(home, &root.canonical.to_string_lossy())?,
            root.canonical,
        ))
    });
    let Some(((id, manifest), root)) = newest else {
        return Reuse::Build { judged: None };
    };
    match unchanged_receipt(
        home,
        paths,
        &root,
        (&id, &manifest),
        identity,
        cancel,
        started,
    ) {
        Some(receipt) => Reuse::Current(receipt),
        None => Reuse::Build { judged: Some(id) },
    }
}

/// The receipt of snapshot `id`, when it is what a build of `root` with
/// `identity` would publish now.
fn unchanged_receipt(
    home: &Path,
    paths: &PathPolicy,
    root: &Path,
    (id, manifest): (&str, &Value),
    identity: &Identity<'_>,
    cancel: &dyn CancellationCheck,
    started: Instant,
) -> Option<GraphOutput> {
    // The digest covers these too; the manifest's own record of them must
    // agree, so an edited manifest is never taken as current.
    if manifest["buildIdentity"] != identity.build
        || manifest["registryIdentity"] != identity.registry
        || manifest["formatVersion"] != format::FORMAT_VERSION
        || !analysis_current(manifest)
    {
        return None;
    }
    let dir = home.join(id);
    let tables = format::SnapshotReader::open(
        &dir.join(GRAPH_FILE),
        manifest["graph"]["sha256"].as_str()?,
        cancel,
    )
    .and_then(|mut reader| {
        // Reuse serves the snapshot to every later answer, point answers
        // included: its lookup must agree with its topology.
        let tables = reader.topology_tables(cancel)?;
        reader.check_lookup(&tables, cancel)?;
        Ok(tables)
    })
    .ok()?;
    let source_root = if manifest["fileScope"].is_string() {
        root.parent()?
    } else {
        root
    };
    if !snapshot_changes(paths, source_root, &tables, manifest, true)
        .changes
        .is_empty()
    {
        return None;
    }
    // On a home that cannot be written, the marker and lease stay as they
    // are: the receipt names its snapshot by id.
    if write_atomic(&home.join(LATEST_FILE), id.as_bytes(), true).is_err() && writable(home) {
        return None;
    }
    // A lease that cannot be renewed only shortens how long retention keeps
    // the snapshot; the answer still holds.
    let _ = renew_lease(&dir);
    progress::emit(|| format!("reused {id}: its sources are unchanged"));
    let mut receipt = snapshot::receipt(id, &dir, manifest, started);
    // Nothing was rebuilt: the receipt says so beside the snapshot's id.
    receipt["reused"] = json!(true);
    Some(snapshot::answer(receipt))
}

/// The newest snapshot whose manifest names `root`, with that manifest.
pub(super) fn newest(home: &Path, root: &str) -> Option<(String, Value)> {
    list_snapshots(home).into_iter().rev().find_map(|id| {
        let directory = home.join(&id);
        let metadata = read_snapshot_metadata(&directory)?;
        if metadata.root.as_deref() != Some(root) {
            return None;
        }
        Some((id, read_manifest(&directory)?))
    })
}

#[cfg(test)]
mod identity_tests;
