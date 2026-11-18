//! Recovery for a file a caller named that is not where it said: the
//! structureSearch listing that finds it by stem, and the sibling a
//! mistyped name was likely meant for.
use super::files_listing::files_listing;
use octocode_contract_types::{StructureSearchQuery, StructureSearchQueryFiles};
use octocode_engine::search::glob;
use octocode_runtime_base::response::result::Continuation;
use serde_json::Value;
use std::path::Path;

/// A listing of files named like `requested` from `root`: its stem (the name
/// without its extension) under any extension, at any depth, so a moved file
/// or a changed extension shows. Never every name merely containing the
/// stem. A name without an extension is matched as written. A name with a
/// wildcard (`*`, `?`) is a glob, which no file is literally named after, so
/// it offers none; a class or alternation (`[id].tsx`, `{a,b}`) is a real
/// route-file spelling and is looked for as written.
pub fn find_file(root: &str, requested: &str) -> Option<Value> {
    let name = Path::new(requested).file_name()?.to_string_lossy();
    if name.contains(['*', '?']) {
        return None;
    }
    let include = match name.split_once('.') {
        Some((stem, _)) if !stem.is_empty() => format!("{}.*", glob::name(stem)),
        _ if name.is_empty() => return None,
        _ => glob::name(&name),
    };
    let listing = StructureSearchQueryFiles {
        include: vec![include],
        ..files_listing(root)?
    };
    Some(Continuation::related_query(StructureSearchQuery::from(listing)).build())
}

/// Names [`closest_name`] reads at most: past them it cannot prove a name
/// the unique closest, and names none.
const MAX_SIBLINGS: usize = 4096;

/// The one file in `dir` whose name is closest to `name` (a mistyped file
/// name); see [`closest_name`]. A directory is no file to read, so it is
/// never a candidate.
pub fn close_name(dir: &Path, name: &str) -> Option<String> {
    let files = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| std::fs::metadata(entry.path()).is_ok_and(|meta| meta.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned());
    closest_name(files, name)
}

/// The one of `candidates` whose name is closest to `name` (a mistyped
/// name): letter case aside, within an edit distance that grows with the
/// name (1 below 8 characters, else 2), and strictly closer than every
/// other candidate. `None` when none is that close, two are equally close,
/// or there are more than [`MAX_SIBLINGS`] candidates. A candidate equal to
/// `name` is the name itself, not a near miss.
pub fn closest_name(candidates: impl IntoIterator<Item = String>, name: &str) -> Option<String> {
    let wanted = name.to_lowercase();
    let max = (wanted.chars().count() / 4).clamp(1, 2);
    let mut best: Option<(usize, String)> = None;
    let mut tied = false;
    for (index, candidate) in candidates.into_iter().enumerate() {
        if index == MAX_SIBLINGS {
            return None;
        }
        if candidate == name {
            continue;
        }
        let distance =
            octocode_runtime_base::contracts::levenshtein(&wanted, &candidate.to_lowercase());
        match &best {
            _ if distance > max => {}
            Some((closest, _)) if distance > *closest => {}
            Some((closest, _)) if distance == *closest => tied = true,
            _ => {
                best = Some((distance, candidate));
                tied = false;
            }
        }
    }
    best.filter(|_| !tied).map(|(_, name)| name)
}
