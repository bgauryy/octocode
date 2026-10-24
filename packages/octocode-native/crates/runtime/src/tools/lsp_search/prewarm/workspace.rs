//! Workspace prewarm: the servers a session's first `lspSearch` is likely to
//! need, started before any call names a file.
//!
//! A bounded walk of the workspace (ignore rules honored, default prunes)
//! counts source files per server: one representative file per directory
//! and extension is planned as [`schedule`] plans a lead's file, and the
//! files of that directory count toward its server. Per language family,
//! the server with the most files is warmed, families ranked by file count,
//! at most [`MAX_PREWARM_KEYS`]; a workspace with no supported file starts
//! nothing.
use super::{LspExecutionConfig, MAX_PREWARM_KEYS, Planned, plan, start};
use crate::policy::path::PathPolicy;
use octocode_engine::lsp::config::detect_language_id;
use octocode_engine::lsp::family::family_of;
use octocode_engine::lsp::pool::LspClientPool;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Entries and wall time of the workspace walk.
const WALK_ENTRIES: usize = 20_000;
const WALK_BUDGET: Duration = Duration::from_millis(500);
/// Directory and extension groups planned (each plan resolves a workspace
/// root and a server).
const PLANNED_GROUPS: usize = 256;

/// Start the language servers of the workspace at `root` in the background
/// (see the module docs). Nothing waits: the walk runs on a blocking
/// worker, and each start is a [`schedule`]-style warm.
pub fn schedule_workspace(
    handle: &tokio::runtime::Handle,
    pool: &Arc<LspClientPool>,
    paths: &PathPolicy,
    execution: &Arc<LspExecutionConfig>,
    root: &Path,
) {
    let (runner, pool, paths, execution, root) = (
        handle.clone(),
        Arc::clone(pool),
        paths.clone(),
        Arc::clone(execution),
        root.to_path_buf(),
    );
    drop(handle.spawn_blocking(move || {
        for planned in workspace_plans(&root, &paths, &execution) {
            start(&runner, &pool, &execution, planned);
        }
    }));
}

/// The language family of a file's server: the family's first id, or the
/// language id itself.
fn family(file: &Path) -> Option<String> {
    let language = detect_language_id(&file.to_string_lossy())?;
    Some(family_of(&language).map_or(language, |family| family[0].to_owned()))
}

/// The starts [`schedule_workspace`] makes for `root`, most files first.
pub(super) fn workspace_plans(
    root: &Path,
    paths: &PathPolicy,
    execution: &LspExecutionConfig,
) -> Vec<Planned> {
    // (directory, extension) → (first file, file count), in walk order.
    let mut groups: BTreeMap<(PathBuf, String), (PathBuf, usize)> = BTreeMap::new();
    let started = Instant::now();
    let walk = crate::tools::pruned_walk(root, [])
        .sort_by_file_name(std::ffi::OsStr::cmp)
        .build();
    for (entries, entry) in walk.enumerate() {
        if entries >= WALK_ENTRIES || started.elapsed() > WALK_BUDGET {
            break;
        }
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let (Some(parent), Some(extension)) = (path.parent(), path.extension()) else {
            continue;
        };
        groups
            .entry((
                parent.to_path_buf(),
                extension.to_string_lossy().into_owned(),
            ))
            .or_insert_with(|| (path.to_path_buf(), 0))
            .1 += 1;
    }
    // family → server key → (files, representative plan).
    let mut servers: BTreeMap<String, BTreeMap<String, (usize, Planned)>> = BTreeMap::new();
    let planned = groups
        .into_values()
        .filter_map(|(file, count)| Some((family(&file)?, file, count)))
        .take(PLANNED_GROUPS);
    for (family, file, count) in planned {
        let Some(planned) = plan(paths, execution, &file) else {
            continue;
        };
        servers
            .entry(family)
            .or_default()
            .entry(planned.key.clone())
            .or_insert((0, planned))
            .0 += count;
    }
    let mut best = servers
        .into_values()
        .filter_map(|keys| {
            let total = keys.values().map(|(count, _)| count).sum::<usize>();
            let (_, (_, planned)) = keys.into_iter().max_by_key(|(_, (count, _))| *count)?;
            Some((total, planned))
        })
        .collect::<Vec<_>>();
    best.sort_by(|(a, _), (b, _)| b.cmp(a));
    best.into_iter()
        .take(MAX_PREWARM_KEYS)
        .map(|(_, planned)| planned)
        .collect()
}

#[cfg(test)]
mod tests;
