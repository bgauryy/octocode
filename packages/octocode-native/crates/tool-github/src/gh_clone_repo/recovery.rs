//! Recovery for a sparse path the commit does not have. The store already
//! holds the commit's trees, so the lead is computed locally: the same clone
//! with a near-miss name corrected, else the listing of the deepest
//! directory that exists.
use super::{GhCloneRepoQuery, GitContext, git};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::Continuation;
use octocode_tools_shared::leads::find_file::closest_name;
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

/// `next` for requested sparse `missing` paths of `commit`, whose trees the
/// repository at `directory` (its checkout or its store) holds: `repair`
/// (the same clone with every missing path corrected) when each has one
/// near-miss sibling, else `viewTree` (ghStructure of the first
/// uncorrectable path's deepest existing directory, at the commit). `None`
/// when the commit's tree cannot be read.
pub(super) fn sparse_next(
    context: &GitContext<'_>,
    directory: &Path,
    commit: &str,
    query: &GhCloneRepoQuery,
    missing: &[&str],
) -> Option<Value> {
    let tree = Tree {
        context,
        directory,
        commit,
    };
    let mut corrections = Vec::new();
    let mut uncorrectable = None;
    for path in missing {
        let path = path.trim_end_matches('/');
        let ancestor = deepest_ancestor(&tree, path)?;
        match corrected_path(&tree, &ancestor) {
            Some(corrected) => corrections.push((path, corrected)),
            None => {
                uncorrectable.get_or_insert(ancestor.dir);
            }
        }
    }
    if let Some(dir) = uncorrectable {
        let lead = octocode_tools_shared::gh_shared::RepoPath {
            owner: query.owner.as_str(),
            repo: query.repo.as_str(),
            path: &dir,
            reference: Some(commit),
        }
        .lead(ToolId::GhStructure)
        .build();
        return Some(json!({"viewTree": lead}));
    }
    let corrected = |requested: &str| {
        let trimmed = requested.trim_end_matches('/');
        corrections
            .iter()
            .find(|(missing, _)| *missing == trimmed)
            .map_or_else(|| requested.to_owned(), |(_, close)| close.clone())
    };
    let paths = query
        .paths
        .iter()
        .map(|path| corrected(path).parse())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let repair = GhCloneRepoQuery {
        paths,
        ref_: Some(commit.parse().ok()?),
        ..query.clone()
    };
    let lead = Continuation::related_query(repair).build();
    Some(json!({"repair": lead}))
}

/// One commit's trees, read in a repository that holds them.
struct Tree<'a> {
    context: &'a GitContext<'a>,
    directory: &'a Path,
    commit: &'a str,
}

impl Tree<'_> {
    /// The entry names directly under `dir` ([`git::tree_names`]).
    fn names(&self, dir: &str) -> Option<Vec<String>> {
        git::tree_names(self.context, self.directory, self.commit, dir).ok()
    }
}

/// The path with its first missing segment replaced by the one near-miss
/// entry of the deepest existing directory, when the result exists.
fn corrected_path(tree: &Tree<'_>, ancestor: &Ancestor) -> Option<String> {
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
    tree.names(parent)?
        .iter()
        .any(|entry| entry == name)
        .then_some(corrected)
}

/// The deepest directory above `path` that exists at HEAD (the root when
/// none does), with its entries.
fn deepest_ancestor(tree: &Tree<'_>, path: &str) -> Option<Ancestor> {
    let mut dir = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    loop {
        let entries = tree.names(dir)?;
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
