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
    crate::tools::source::read_bounded(&dir.join(MANIFEST_FILE), 1024 * 1024)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|manifest| manifest["kind"] == MANIFEST_KIND)
}

pub(super) fn is_snapshot(dir: &Path) -> bool {
    read_manifest(dir).is_some()
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

/// Removes older snapshots of `root` beyond `keep`, snapshots of any root
/// in an older format (no query of this build can read them; a newer
/// format belongs to a newer build and stays), and abandoned temp dirs.
/// Snapshots are grouped by the root they scanned, never by their scope
/// name: two roots that share a name keep their own snapshots.
pub(super) fn prune(home: &Path, root: &Path, keep: usize) -> Vec<String> {
    let mut removed = Vec::new();
    let root = root.to_string_lossy();
    let mut same_scope = Vec::new();
    let mut outdated = Vec::new();
    for id in list_snapshots(home) {
        let Some(manifest) = read_manifest(&home.join(&id)) else {
            continue;
        };
        if manifest["formatVersion"]
            .as_u64()
            .is_some_and(|version| version < u64::from(format::FORMAT_VERSION))
        {
            outdated.push(id);
        } else if manifest["root"] == root.as_ref() {
            same_scope.push(id);
        }
    }
    let excess = same_scope.len().saturating_sub(keep.max(1));
    for id in outdated
        .into_iter()
        .chain(same_scope.into_iter().take(excess))
    {
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
