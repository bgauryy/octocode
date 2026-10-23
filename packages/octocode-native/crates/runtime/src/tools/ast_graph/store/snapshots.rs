//! The snapshot home: where snapshots live, how they are named, listed,
//! read and pruned.
use super::*;

/// `<workspace>/.octocode/graph`. The workspace is the caller's: the
/// runtime names one ([`default_workspace`] of its own working directory)
/// when the call does not, so the process directory never decides it.
pub(crate) fn graph_home(workspace: Option<&Path>) -> Result<PathBuf, String> {
    let workspace =
        workspace.ok_or("no workspace: name the directory that owns .octocode/graph")?;
    Ok(workspace.join(".octocode").join("graph"))
}

/// The nearest repository root at or above `cwd`
/// ([`octocode_engine::search::repository`]) that `paths` allows (else `cwd`),
/// so a run from a repository subdirectory stays inside the allowed roots.
pub(crate) fn default_workspace(cwd: PathBuf, paths: &PathPolicy) -> PathBuf {
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

/// `(compact, iso)` UTC timestamps: `20260928T101500Z`, `2026-09-28T10:15:00Z`.
pub(super) fn utc_now() -> (String, String) {
    let iso = crate::civil_date::iso8601_secs(crate::civil_date::unix_secs());
    (iso.replace(['-', ':'], ""), iso)
}

/// Whether `dir` is a published snapshot (has a manifest of our kind).
pub(super) fn read_manifest(dir: &Path) -> Option<Value> {
    std::fs::read(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|manifest| manifest["kind"] == MANIFEST_KIND)
}

pub(super) fn is_snapshot(dir: &Path) -> bool {
    read_manifest(dir).is_some()
}

/// Published snapshot ids, oldest first (ids start with a sortable time).
pub(crate) fn list_snapshots(home: &Path) -> Vec<String> {
    let mut ids = std::fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.') && is_snapshot(&home.join(name)))
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

/// Removes older snapshots of `root` beyond `keep` and abandoned temp dirs.
/// Snapshots are grouped by the root they scanned, never by their scope
/// name: two roots that share a name keep their own snapshots.
pub(super) fn prune(home: &Path, root: &Path, keep: usize) -> Vec<String> {
    let mut removed = Vec::new();
    let root = root.to_string_lossy();
    let same_scope = list_snapshots(home)
        .into_iter()
        .filter(|id| read_manifest(&home.join(id)).is_some_and(|m| m["root"] == root.as_ref()))
        .collect::<Vec<_>>();
    let excess = same_scope.len().saturating_sub(keep.max(1));
    for id in same_scope.into_iter().take(excess) {
        if std::fs::remove_dir_all(home.join(&id)).is_ok() {
            removed.push(id);
        }
    }
    let stale_after = std::time::Duration::from_secs(3600);
    for entry in std::fs::read_dir(home).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age > stale_after);
        if name.starts_with(".tmp-") && old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    removed
}
