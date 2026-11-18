//! Walked listings kept for continuation pages, keyed by snapshot and the
//! path policy that produced them, so page 2 onward does not walk the tree
//! again. A stored walk is served only while every directory it entered and
//! every listed entry whose metadata its rows show keeps its stamp (an entry
//! created, removed or renamed in a directory changes the directory's); the
//! executor then recomputes the snapshot from the submitted query and the
//! stored rows before serving a page. Those stamps are the ones the walk
//! read, taken before what they prove, so keeping a walk stats none of them.
//! A metadata filter's verdicts are watched the same way: every entry it read,
//! listed or not, since a file's size or time moves no directory's stamp.
//! The ignore rules the walk applied are watched as astSearch's scan memo
//! watches them ([`Watch`]): each `.gitignore`/`.ignore` of a directory it
//! entered, the rule files outside the walk and the repository markers above
//! it, so an edited rule drops the walk even when no listed entry changed.
use octocode_engine::stamp::{Link, Proof};
use octocode_tools_shared::page_memo::{
    PageMemo, Proven, Watch, directory_ignore_inputs_with, repository_markers,
};
use std::{sync::Arc, time::SystemTime};

/// A listed entry's size and modification time as `symlink_metadata` reads
/// them now (a directory's size is 0).
fn observed(path: &std::path::Path) -> Option<(u64, Option<SystemTime>)> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    Some((
        if meta.is_dir() { 0 } else { meta.len() },
        meta.modified().ok(),
    ))
}

/// A listed entry with the size and time its walk read (a directory's size
/// is 0, as [`observed`] reads it).
pub(super) struct Seen<'a> {
    pub path: &'a std::path::Path,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// The stamps of `rows` a proof watches. A walk that read metadata stamped every
/// row, so one without cannot be watched; a walk of names has none to watch,
/// unless a metadata filter read them to decide.
pub(super) fn row_stamps<'a>(
    names_only: bool,
    rows: impl Iterator<
        Item = (
            &'a std::path::Path,
            Option<octocode_engine::stamp::SourceStamp>,
        ),
    >,
) -> Option<Vec<(std::path::PathBuf, octocode_engine::stamp::SourceStamp)>> {
    let stamped = |(path, stamp): (&std::path::Path, Option<_>)| Some((path.to_path_buf(), stamp?));
    if names_only {
        Some(rows.filter_map(stamped).collect())
    } else {
        rows.map(stamped).collect()
    }
}

/// The proof of `listed` entries, every directory holding one, the walked
/// `root` and the ignore rules that shaped the walk (each directory's
/// [`directory_ignore_inputs_with`] under `paths`, and the rule files outside
/// the walk); `None` when a listed entry no longer has the size and time
/// its walk read (it changed before it could be stamped), or an ignore file
/// is one no stamp can watch. `inside` are directories the walk holds open
/// (a cursor's): their own ignore files are watched too, as a directory
/// holding a listed entry's are.
pub(super) fn proof(
    root: &std::path::Path,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    listed: Vec<Seen<'_>>,
    inside: &[&std::path::Path],
) -> Option<Proof> {
    let sources = listed
        .iter()
        .map(|seen| seen.path.to_path_buf())
        .collect::<Vec<_>>();
    let mut dirs = std::collections::BTreeSet::from([root.to_path_buf()]);
    dirs.extend(
        sources
            .iter()
            .filter_map(|source| source.parent())
            .filter(|parent| parent.starts_with(root))
            .map(std::path::Path::to_path_buf),
    );
    dirs.extend(inside.iter().map(|dir| dir.to_path_buf()));
    let mut rules =
        octocode_engine::search::walk::external_ignore_inputs_with(root, paths.global_ignore());
    for dir in &dirs {
        let inputs = directory_ignore_inputs_with(dir, root, paths.global_ignore(), |ignore| {
            paths.validate_read(ignore).is_ok()
        });
        if inputs.untracked {
            return None;
        }
        rules.extend(inputs.files);
        rules.extend(inputs.external);
    }
    let proof = Watch {
        root,
        sources: &sources,
        rules: &rules,
        markers: &repository_markers(root),
    }
    .capture(Link::Itself, None)?;
    listed
        .iter()
        .all(|seen| observed(seen.path) == Some((seen.size, seen.modified)))
        .then_some(proof)
}

/// The proof of a walk of `root`: the stamps it read of the directories it
/// `walked` and of its `listed` rows, the ignore files those directories'
/// listings showed and the rule files outside the walk. `None` when an ignore
/// file is one no stamp can watch: a link or other non-file, or one the
/// policy withholds. A directory the walk took from a resume cursor has no
/// stamp of this walk's and is stamped now, like a root that is a file.
pub(super) fn walk_proof(
    root: &std::path::Path,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    walked: &[octocode_engine::types::WalkedDir],
    listed: Vec<(std::path::PathBuf, octocode_engine::stamp::SourceStamp)>,
) -> Option<Proof> {
    let mut rules =
        octocode_engine::search::walk::external_ignore_inputs_with(root, paths.global_ignore());
    let mut unstamped = Vec::new();
    let mut taken = listed;
    if !walked.iter().any(|dir| dir.path == root) {
        unstamped.push(root.to_path_buf());
    }
    for dir in walked {
        match dir.stamp {
            Some(stamp) => taken.push((dir.path.clone(), stamp)),
            None => unstamped.push(dir.path.clone()),
        }
        for (file, regular) in &dir.ignore_files {
            if !regular || paths.validate_read(file).is_err() {
                return None;
            }
            rules.push(file.clone());
        }
        if dir.holds_git && dir.path != root {
            rules.extend(octocode_engine::search::walk::external_ignore_inputs_with(
                &dir.path,
                paths.global_ignore(),
            ));
        }
    }
    Some(
        Watch {
            root,
            sources: &unstamped,
            rules: &rules,
            markers: &repository_markers(root),
        }
        .capture(Link::Itself, None)?
        .with_taken(Link::Itself, taken),
    )
}

/// Stored walks, weighed by the heap they and their proofs hold against the
/// one process budget: a walk too large for all of it is not kept.
static STORE: PageMemo<Proven<super::walked::Walked>> = PageMemo::new();

/// The stored walk for `snapshot` when its entries are unchanged; a changed
/// one is evicted.
pub(super) fn get(snapshot: &str, policy: &str) -> Option<Arc<super::walked::Walked>> {
    STORE.current(snapshot, policy)
}

/// Keep `walk` of `root` for the pages after the first.
pub(super) fn put(
    snapshot: String,
    policy: String,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    root: &std::path::Path,
    walk: &Arc<super::walked::Walked>,
) {
    let Some(proof) = walk.proof(root, paths) else {
        return;
    };
    let bytes = walk.weight()
        + if walk.shares_proof() {
            0
        } else {
            proof.weight()
        };
    STORE.put_proven(snapshot, policy, bytes, proof, Arc::clone(walk));
}

pub(super) fn evict(snapshot: &str) {
    STORE.evict(snapshot);
}

#[cfg(test)]
mod tests;
