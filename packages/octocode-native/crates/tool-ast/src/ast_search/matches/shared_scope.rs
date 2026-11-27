//! What directory scans of one scope share (perf P14), such as the rows of a
//! batch asking different rules of one directory.
//!
//! - The walk: the first scan of a scope records its whole walk and keeps
//!   it, watched like a kept scan (the root, every directory it read with
//!   its ignore files, the ignore rules outside the walk, the repository
//!   markers). A scan of the same walk inputs while that proof holds replays
//!   it through its own admission instead of walking
//!   ([`octocode_engine::structural::Walk`]), so its candidates and
//!   everything its walk log records are what a walk of its own gives.
//!   Scans that ask while the first walks wait for its record.
//! - The parse: scans of the scope running at once parse each source once
//!   ([`octocode_engine::structural::TreeShare`]); a scan running alone keeps
//!   no tree.
use octocode_engine::structural::{ScanShare, TreeShare, Walk, WalkRecord};
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_tools_shared::page_memo::{
    PageMemo, Proven, Turn, Watch, directory_ignore_inputs_with, repository_markers,
};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError, Weak};

/// Whole walks of a scope, by [`walk_key`], while what they read holds.
static WALKS: PageMemo<Proven<WalkRecord>> = PageMemo::new();

/// The tree share of each scope that scans run over now, by [`walk_key`].
static TREES: Mutex<std::collections::BTreeMap<String, Weak<TreeShare>>> =
    Mutex::new(std::collections::BTreeMap::new());

/// Every input of a directory walk: the path policy (what an ignore file
/// may be read as), the root, its globs, default excludes, ignore policy,
/// global ignore rules and depth.
pub(super) fn walk_key(
    paths: &PathPolicy,
    root: &Path,
    options: &octocode_engine::structural::StructuralSearchFilesOptions,
) -> String {
    octocode_runtime_base::digest::json_sha256(&serde_json::json!([
        paths.identity(),
        root.to_string_lossy(),
        options.include,
        options.exclude,
        options.default_exclude_dir,
        options.ignore.include_ignored,
        options.ignore.include_hidden,
        options.max_depth,
        format!("{:?}", options.global_ignore),
    ]))
}

/// `run` the scan of `root` under `key` with what it shares (see the
/// module); `stop` ends a wait for another scan's walk, and the scan walks
/// for itself.
pub(super) fn scan<T>(
    key: &str,
    (root, paths): (&Path, &PathPolicy),
    stop: &dyn Fn() -> bool,
    run: impl FnOnce(ScanShare<'_>) -> T,
) -> T {
    let trees = live_trees(key);
    let _part = trees.join();
    // Before the walk: no entry written after it is trusted as walked.
    let started = octocode_engine::stamp::settle_clock();
    loop {
        if let Some(record) = WALKS.current(key, "") {
            #[cfg(test)]
            note(root, "replay");
            return run(ScanShare {
                walk: Walk::Replay(&record),
                trees: Some(&trees),
            });
        }
        match WALKS.turn(key, stop) {
            Turn::Landed => {}
            Turn::Stopped => {
                #[cfg(test)]
                note(root, "walk");
                return run(ScanShare {
                    walk: Walk::Fresh,
                    trees: Some(&trees),
                });
            }
            Turn::Lead(leading) => {
                #[cfg(test)]
                note(root, "walk");
                // Kept, and its flight landed, once the walk is done: the
                // scans waiting on it replay it while this one reads files.
                let leading = Mutex::new(Some(leading));
                let walked = |record: Option<WalkRecord>| {
                    if let Some(record) = record {
                        keep(key, (root, paths), started, record);
                    }
                    drop(
                        leading
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .take(),
                    );
                };
                return run(ScanShare {
                    walk: Walk::Record(&walked),
                    trees: Some(&trees),
                });
            }
        }
    }
}

/// Keep `record`, the whole walk of `root` that started at `started`,
/// watched by every directory it read with its ignore files, the ignore
/// rules outside it and the repository markers above it. Not kept when one
/// of them was written within the settle window before `started`, or an
/// ignore file cannot be watched.
fn keep(
    key: &str,
    (root, paths): (&Path, &PathPolicy),
    started: std::time::SystemTime,
    record: WalkRecord,
) {
    let global = paths.global_ignore();
    let discovery = paths.discovery_walk();
    let mut sources = record.dirs().to_vec();
    let mut rules = octocode_engine::search::walk::external_ignore_inputs_with(root, global);
    for dir in record.dirs() {
        let inputs = directory_ignore_inputs_with(dir, root, global, |ignore| {
            discovery.permits_entry(ignore, None)
        });
        if inputs.untracked {
            return;
        }
        sources.extend(inputs.files);
        rules.extend(inputs.external);
    }
    let Some(proof) = (Watch {
        root,
        sources: &sources,
        rules: &rules,
        markers: &repository_markers(root),
    })
    .capture(octocode_engine::stamp::Link::Follow, Some(started)) else {
        return;
    };
    let bytes = record.heap_bytes() + proof.weight();
    WALKS.put_proven(
        key.to_owned(),
        String::new(),
        bytes,
        proof,
        Arc::new(record),
    );
}

/// The tree share of the scans of `key` running now, or a new one.
fn live_trees(key: &str) -> Arc<TreeShare> {
    let mut live = TREES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(trees) = live.get(key).and_then(Weak::upgrade) {
        return trees;
    }
    live.retain(|_, trees| trees.strong_count() > 0);
    let trees = Arc::new(TreeShare::new());
    live.insert(key.to_owned(), Arc::downgrade(&trees));
    trees
}

/// How each directory scan walked, by root, for the tests that count walks.
#[cfg(test)]
pub(super) static WALKED: Mutex<Vec<(std::path::PathBuf, &'static str)>> = Mutex::new(Vec::new());

#[cfg(test)]
fn note(root: &Path, how: &'static str) {
    WALKED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((root.to_path_buf(), how));
}

#[cfg(test)]
mod tests;
