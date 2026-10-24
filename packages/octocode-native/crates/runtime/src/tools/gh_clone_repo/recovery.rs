//! Recovery for a sparse path the commit does not have. The checkout already
//! holds the commit's trees, so the lead is computed locally: the same clone
//! with a near-miss name corrected, else the listing of the deepest
//! directory that exists.
use super::{CloneContext, GhCloneRepoQuery, git};
use crate::id::ToolId;
use crate::response::result::{Continuation, query_row};
use crate::tools::leads::find_file::closest_name;
use serde_json::{Value, json};
use std::path::Path;

/// Where one missing path stops existing: its deepest existing directory,
/// that directory's entries, and the path below it (its first segment is the
/// first that does not exist).
struct Ancestor {
    dir: String,
    entries: Vec<String>,
    below: String,
}

/// `next` for requested sparse `missing` paths of the checkout in `stage`:
/// `repair` (the same clone with every missing path corrected) when each has
/// one near-miss sibling, else `viewTree` (ghStructure of the first
/// uncorrectable path's deepest existing directory, at the commit). `None`
/// when the commit's tree cannot be read.
pub(super) fn sparse_next(
    context: &CloneContext<'_>,
    stage: &Path,
    query: &GhCloneRepoQuery,
    missing: &[&str],
) -> Option<Value> {
    let commit = git::read_head(context, stage).ok()?;
    let mut corrections = Vec::new();
    let mut uncorrectable = None;
    for path in missing {
        let path = path.trim_end_matches('/');
        let ancestor = deepest_ancestor(context, stage, path)?;
        match corrected_path(context, stage, &ancestor) {
            Some(corrected) => corrections.push((path, corrected)),
            None => {
                uncorrectable.get_or_insert(ancestor.dir);
            }
        }
    }
    if let Some(dir) = uncorrectable {
        let lead = crate::tools::gh_shared::RepoPath {
            owner: query.owner.as_str(),
            repo: query.repo.as_str(),
            path: &dir,
            reference: Some(&commit),
        }
        .lead(ToolId::GhStructure)
        .build();
        return Some(json!({"viewTree": lead}));
    }
    let mut row = query_row(query, &[]);
    row["ref"] = json!(commit);
    let corrected = |requested: &str| {
        let trimmed = requested.trim_end_matches('/');
        corrections
            .iter()
            .find(|(missing, _)| *missing == trimmed)
            .map_or_else(|| requested.to_owned(), |(_, close)| close.clone())
    };
    row["paths"] = query.paths.iter().map(|path| corrected(path)).collect();
    let lead = Continuation::related(ToolId::GhCloneRepo, row).build();
    Some(json!({"repair": lead}))
}

/// The path with its first missing segment replaced by the one near-miss
/// entry of the deepest existing directory, when the result exists.
fn corrected_path(context: &CloneContext<'_>, stage: &Path, ancestor: &Ancestor) -> Option<String> {
    let (segment, rest) = match ancestor.below.split_once('/') {
        Some((segment, rest)) => (segment, Some(rest)),
        None => (ancestor.below.as_str(), None),
    };
    let close = closest_name(ancestor.entries.iter().cloned(), segment)?;
    let head = join(&ancestor.dir, &close);
    let Some(rest) = rest else {
        return Some(head);
    };
    let corrected = format!("{head}/{rest}");
    let (parent, name) = corrected.rsplit_once('/')?;
    git::tree_names(context, stage, parent)
        .ok()?
        .iter()
        .any(|entry| entry == name)
        .then_some(corrected)
}

/// The deepest directory above `path` that exists at HEAD (the root when
/// none does), with its entries.
fn deepest_ancestor(context: &CloneContext<'_>, stage: &Path, path: &str) -> Option<Ancestor> {
    let mut dir = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    loop {
        let entries = git::tree_names(context, stage, dir).ok()?;
        if !entries.is_empty() || dir.is_empty() {
            let below = path
                .strip_prefix(dir)
                .map_or(path, |below| below.trim_start_matches('/'));
            return Some(Ancestor {
                dir: dir.to_owned(),
                entries,
                below: below.to_owned(),
            });
        }
        dir = dir.rsplit_once('/').map_or("", |(parent, _)| parent);
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}
