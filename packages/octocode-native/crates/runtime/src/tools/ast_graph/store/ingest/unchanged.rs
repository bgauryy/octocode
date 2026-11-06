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
    octocode_engine::digest::sha256(
        json!([
            format::FORMAT_VERSION,
            super::super::super::ANALYSIS_VERSION,
            env!("CARGO_PKG_VERSION"),
            registry_identity,
            options.exclude,
            options.language_globs,
            cargo,
        ])
        .to_string()
        .as_bytes(),
    )
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
    identity: &str,
    registry_identity: &str,
    cancel: &dyn CancellationCheck,
    started: Instant,
) -> Option<GraphOutput> {
    if options.refresh || !home.is_dir() {
        return None;
    }
    let root = paths.validate(&options.path).ok()?.canonical;
    let root_text = root.to_string_lossy();
    // Unlocked probe first: a scope with no current snapshot never waits
    // for another scope's publication.
    newest(home, &root_text).filter(|(_, manifest)| manifest["buildIdentity"] == identity)?;
    let _lock = publication::lock(home, paths, cancel).ok()?;
    let (id, manifest) = newest(home, &root_text)?;
    // The digest covers these too; the manifest's own record of them must
    // agree, so an edited manifest is never taken as current.
    if manifest["buildIdentity"] != identity
        || manifest["registryIdentity"] != registry_identity
        || manifest["formatVersion"] != format::FORMAT_VERSION
        || !analysis_current(&manifest)
    {
        return None;
    }
    let dir = home.join(&id);
    let tables = format::SnapshotReader::open(
        &dir.join(GRAPH_FILE),
        manifest["graph"]["sha256"].as_str()?,
        cancel,
    )
    .and_then(|mut reader| reader.topology_tables(cancel))
    .ok()?;
    if !snapshot_changes(paths, &root, &tables, &manifest, true)
        .changes
        .is_empty()
    {
        return None;
    }
    write_atomic(&home.join(LATEST_FILE), id.as_bytes(), true).ok()?;
    // A lease that cannot be renewed only shortens how long retention keeps
    // the snapshot; the answer still holds.
    let _ = renew_lease(&dir);
    progress::emit(|| format!("reused {id}: its sources are unchanged"));
    Some(GraphOutput::ok(snapshot::receipt(
        &id, &dir, &manifest, started,
    )))
}

/// The newest snapshot whose manifest names `root`, with that manifest.
fn newest(home: &Path, root: &str) -> Option<(String, Value)> {
    list_snapshots(home).into_iter().rev().find_map(|id| {
        let manifest = read_manifest(&home.join(&id))?;
        (manifest["root"] == root).then_some((id, manifest))
    })
}
