//! The snapshot home: where snapshots live, how they are named, listed,
//! read and pruned.
use super::*;
use std::collections::HashMap;

/// A marker inside a snapshot directory: its modification time is the last
/// time a reader used the snapshot. A snapshot that retention would remove
/// stays queryable for as long as the continuations of a walk over it do
/// ([`octocode_runtime_base::cache::CONTINUATION_IDLE`] after that read): a rebuild by
/// another agent must not end that walk. Past it, the snapshot is idle and
/// the next build prunes it; its ID then names its successor.
pub(super) const READ_LEASE_FILE: &str = "last-read";

/// The directory Octocode owns inside a configured `local.graphHome`. The
/// configured directory is the user's (it may be `~` or `/tmp`): snapshots,
/// `latest`, the build lock, `.facts` and every temp entry go in here, so
/// retention and the temp sweep never see what the user keeps beside it.
const CONFIGURED_HOME_DIR: &str = "octocode-graph";

/// The snapshot home of a configured `local.graphHome`
/// ([`CONFIGURED_HOME_DIR`] inside it).
pub fn configured_graph_home(configured: &Path) -> PathBuf {
    configured.join(CONFIGURED_HOME_DIR)
}

/// The snapshot home: [`configured_graph_home`] of `configured`
/// (`local.graphHome`) when set, else `<workspace>/.octocode/graph`. The
/// workspace is the caller's: the runtime names one ([`default_workspace`]
/// of its own working directory) when the call does not, so the process
/// directory never decides it.
pub fn graph_home(workspace: Option<&Path>, configured: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(configured) = configured {
        return Ok(configured_graph_home(configured));
    }
    let workspace =
        workspace.ok_or("no workspace: name the directory that owns .octocode/graph")?;
    Ok(workspace.join(".octocode").join("graph"))
}

/// The nearest repository root at or above `cwd`
/// ([`octocode_engine::search::repository`]) that `paths` allows (else `cwd`),
/// so a run from a repository subdirectory stays inside the allowed roots.
pub fn default_workspace(cwd: PathBuf, paths: &PathPolicy) -> PathBuf {
    cwd.ancestors()
        .find(|dir| {
            octocode_engine::search::repository::is_root(dir) && paths.validate(dir).is_ok()
        })
        .map(Path::to_path_buf)
        .unwrap_or(cwd)
}

/// Filesystem-safe scope name: the scanned root relative to the workspace,
/// or its last two components when it lies outside.
pub(super) fn scope_slug(root: &Path, workspace: &Path) -> String {
    let relative = root
        .strip_prefix(workspace)
        .ok()
        .map(|rel| rel.to_string_lossy().into_owned())
        .filter(|rel| !rel.is_empty());
    let raw = relative.unwrap_or_else(|| {
        let parts = root
            .components()
            .rev()
            .take(2)
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        parts.into_iter().rev().collect::<Vec<_>>().join("-")
    });
    let mut slug = String::new();
    for ch in raw.chars() {
        let ch = if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_') {
            ch
        } else {
            '-'
        };
        if ch != '-' || !slug.ends_with('-') {
            slug.push(ch);
        }
    }
    let slug = slug
        .trim_matches(['-', '.'])
        .chars()
        .take(64)
        .collect::<String>();
    if slug.is_empty() { "root".into() } else { slug }
}

/// The six hex digits that tell apart roots whose names map to one slug.
pub(super) fn root_tag(root: &Path) -> String {
    octocode_engine::digest::sha256(root.to_string_lossy().as_bytes())[..6].to_owned()
}

/// Records that a reader is using the snapshot in `dir`, renewing its lease.
pub(super) fn renew_lease(dir: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(dir.join(READ_LEASE_FILE))?
        .set_modified(std::time::SystemTime::now())
}

/// Whether a reader used the snapshot in `dir` within
/// [`octocode_runtime_base::cache::CONTINUATION_IDLE`].
fn leased(dir: &Path) -> bool {
    std::fs::metadata(dir.join(READ_LEASE_FILE))
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.elapsed().ok())
        .is_some_and(|idle| idle < octocode_runtime_base::cache::CONTINUATION_IDLE)
}

/// The name [`remove_unless_leased`] claims a snapshot under. Its claims
/// are the only entries the temp sweep of [`prune_with`] removes; staging
/// directories go through [`super::ingest`](mod@super::ingest)'s marker-checked cleanup.
const CLAIM_PREFIX: &str = ".tmp-removing-";

/// Removes snapshot `id` unless a reader holds its lease, deciding at the
/// removal itself: the directory is first claimed by an atomic rename, and the
/// lease is read again on the claimed directory. A renewal that landed before
/// the claim keeps the snapshot (the claim is undone); one that comes after
/// finds it gone, as after any removal. A snapshot another prune already
/// took is not this call's to report. `between` runs after the first idle
/// verdict and before the claim.
fn remove_unless_leased(home: &Path, id: &str, between: &dyn Fn(&Path)) -> bool {
    let dir = home.join(id);
    if leased(&dir) {
        return false;
    }
    between(&dir);
    let claimed = home.join(format!("{CLAIM_PREFIX}{id}"));
    if std::fs::rename(&dir, &claimed).is_err() {
        return false;
    }
    if leased(&claimed) {
        // A reader renewed first: give the snapshot back. If that fails the
        // claimed copy is left for the stale temp sweep, never half-removed.
        let _ = std::fs::rename(&claimed, &dir);
        return false;
    }
    std::fs::remove_dir_all(&claimed).is_ok()
}

/// The newest snapshot of `root` when `id` is an ID this build allocates for
/// that scope ([`next_snapshot_id`]) and no longer resolves: the snapshot
/// that replaced it.
pub(super) fn successor(home: &Path, workspace: &Path, root: &Path, id: &str) -> Option<String> {
    let scope = format!("{}-{}", scope_slug(root, workspace), root_tag(root));
    let sequence = id.split_once('-')?.1.strip_prefix(&scope)?;
    let sequenced = |digits: &str| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit());
    if !(sequence.is_empty() || sequence.strip_prefix('-').is_some_and(sequenced)) {
        return None;
    }
    let root = root.to_string_lossy();
    // IDs order by build time: only a later build of the scope replaced `id`.
    list_snapshots(home).into_iter().rev().find(|candidate| {
        snapshot_order(candidate) > snapshot_order(id)
            && read_snapshot_metadata(&home.join(candidate))
                .is_some_and(|manifest| manifest.root.as_deref() == Some(root.as_ref()))
    })
}

/// `(compact, iso)` UTC timestamps: `20260928T101500Z`, `2026-09-28T10:15:00Z`.
pub(super) fn utc_now() -> (String, String) {
    let iso = octocode_runtime_base::civil_date::iso8601_secs(
        octocode_runtime_base::civil_date::unix_secs(),
    );
    (iso.replace(['-', ':'], ""), iso)
}

fn read_snapshot<T: serde::de::DeserializeOwned>(dir: &Path) -> Option<T> {
    std::fs::File::open(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|file| serde_json::from_reader(std::io::BufReader::new(file)).ok())
}

/// The complete manifest, for reuse and freshness consumers that need its
/// evidence. Discovery reads only [`SnapshotMetadata`].
pub(super) fn read_manifest(dir: &Path) -> Option<Value> {
    let manifest = read_snapshot::<Value>(dir)?;
    (manifest["kind"] == MANIFEST_KIND).then_some(())?;
    evidence_manifest(manifest)
}

/// Stored roots identify snapshots. Analysis resolves file keys and locations from a directory.
pub(super) fn evidence_manifest(mut manifest: Value) -> Option<Value> {
    if manifest["singleFile"] == true {
        let file = manifest["root"].take();
        let parent = Path::new(file.as_str()?).parent()?;
        manifest["root"] = json!(parent);
        manifest["fileScope"] = file;
    }
    Some(manifest)
}

/// Discovery and retention need only these fields. Serde skips every other
/// value directly from the reader, without allocating its scan evidence.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SnapshotMetadata {
    kind: String,
    pub(super) root: Option<String>,
    file_scope: Option<String>,
    pub(super) scope: Option<String>,
    pub(super) format_version: Option<u64>,
}

/// When the newest snapshot of each root in `home` was published, from one
/// read of each snapshot's metadata. Job retention asks it for every job in
/// the home, so a per-root scan would read every manifest once per job.
pub(super) fn published_roots(home: &Path) -> HashMap<String, std::time::SystemTime> {
    let mut newest = HashMap::new();
    for entry in std::fs::read_dir(home).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with('.')
            || !entry.file_type().is_ok_and(|kind| kind.is_dir())
        {
            continue;
        }
        let dir = entry.path();
        let Some(root) = read_snapshot_metadata(&dir).and_then(|manifest| manifest.root) else {
            continue;
        };
        let Ok(published) =
            std::fs::metadata(dir.join(MANIFEST_FILE)).and_then(|meta| meta.modified())
        else {
            continue;
        };
        newest
            .entry(root)
            .and_modify(|at: &mut std::time::SystemTime| *at = (*at).max(published))
            .or_insert(published);
    }
    newest
}

pub(super) fn read_snapshot_metadata(dir: &Path) -> Option<SnapshotMetadata> {
    let mut metadata =
        read_snapshot::<SnapshotMetadata>(dir).filter(|manifest| manifest.kind == MANIFEST_KIND)?;
    if let Some(file) = metadata.file_scope.take() {
        metadata.root = Some(file);
    }
    Some(metadata)
}

pub(super) fn is_snapshot(dir: &Path) -> bool {
    read_snapshot_metadata(dir).is_some()
}

/// Allocate after every surviving same-second id, including unpublished entries.
/// Filling a pruned gap would make a new snapshot sort older than its predecessors.
pub(super) fn next_snapshot_id(home: &Path, base: &str) -> Result<String, String> {
    let prefix = format!("{base}-");
    let last = std::fs::read_dir(home)
        .map_err(|error| format!("cannot list graph snapshots: {error}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if name == base {
                Some(1_u64)
            } else {
                name.strip_prefix(&prefix)?.parse::<u64>().ok()
            }
        })
        .max();
    match last {
        None => Ok(base.to_owned()),
        Some(last) => last
            .checked_add(1)
            .map(|next| format!("{base}-{next}"))
            .ok_or_else(|| "graph snapshot sequence exhausted".to_owned()),
    }
}

fn snapshot_order(id: &str) -> (&str, u64) {
    id.rsplit_once('-')
        .and_then(|(base, suffix)| suffix.parse().ok().map(|sequence| (base, sequence)))
        .unwrap_or((id, 0))
}

/// Published snapshot ids, oldest first, with numeric same-second sequences.
pub(crate) fn list_snapshots(home: &Path) -> Vec<String> {
    let mut ids = std::fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.') && is_snapshot(&home.join(name)))
        .collect::<Vec<_>>();
    ids.sort_by(|left, right| snapshot_order(left).cmp(&snapshot_order(right)));
    ids
}

/// Whether the directory a snapshot scanned is gone (not merely unreadable
/// or on an unmounted volume that answers another way).
fn root_is_gone(root: &str) -> bool {
    matches!(
        std::fs::metadata(root),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

/// Removes, among snapshots no reader used within
/// [`octocode_runtime_base::cache::CONTINUATION_IDLE`] (a rebuild never ends a walk in
/// progress, whichever build reads it), older snapshots of `root` beyond
/// `keep`, snapshots of any root in an older format (no query of this
/// build can read them; a newer format belongs to a newer build and stays)
/// and snapshots of any root that no longer exists,
/// plus the claims of removals a dead process abandoned ([`CLAIM_PREFIX`];
/// no other entry, whatever its name or age).
/// Snapshots are grouped by the root they scanned, never by their scope
/// name: two roots that share a name keep their own snapshots.
pub(super) fn prune(home: &Path, root: &Path, keep: usize) -> Vec<Pruned> {
    prune_with(home, root, keep, &|_| {})
}

/// A snapshot [`prune`] removed, and why: an answer names both, since a
/// snapshot another session built may be among them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Pruned {
    pub(super) id: String,
    /// It was in a format this build cannot read; else it was older than
    /// the `keep` newest of its root.
    pub(super) outdated: bool,
}

/// [`prune`] that runs `between` on a snapshot's directory after it is
/// judged idle and before it is removed: the moment a reader's renewal races
/// the removal.
pub(super) fn prune_with(
    home: &Path,
    root: &Path,
    keep: usize,
    between: &dyn Fn(&Path),
) -> Vec<Pruned> {
    let mut removed = Vec::new();
    let root = root.to_string_lossy();
    let mut same_scope = Vec::new();
    let mut outdated = Vec::new();
    let mut orphaned = Vec::new();
    for id in list_snapshots(home) {
        let Some(manifest) = read_snapshot_metadata(&home.join(&id)) else {
            continue;
        };
        if manifest
            .format_version
            .is_some_and(|version| version < u64::from(format::FORMAT_VERSION))
        {
            outdated.push(id);
        } else if manifest.root.as_deref() == Some(root.as_ref()) {
            same_scope.push(id);
        } else if manifest.root.as_deref().is_some_and(root_is_gone) {
            orphaned.push(id);
        }
    }
    // A snapshot of a root that no longer exists answers no query of any
    // scope; it is reaped like any other idle snapshot, and is not among the
    // removals a build reports for its own root.
    for id in orphaned {
        remove_unless_leased(home, &id, between);
    }
    let excess = same_scope.len().saturating_sub(keep.max(1));
    let expired = outdated
        .into_iter()
        .map(|id| (id, true))
        .chain(same_scope.into_iter().take(excess).map(|id| (id, false)));
    for (id, outdated) in expired {
        if remove_unless_leased(home, &id, between) {
            removed.push(Pruned { id, outdated });
        }
    }
    let stale_after = std::time::Duration::from_secs(3600);
    for entry in std::fs::read_dir(home).into_iter().flatten().flatten() {
        let claim = entry
            .file_name()
            .to_string_lossy()
            .starts_with(CLAIM_PREFIX)
            && entry.file_type().is_ok_and(|kind| kind.is_dir());
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age > stale_after);
        if claim && old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    removed
}
