//! Recovery for a file a caller named that is not where it said: the
//! sibling a mistyped name was likely meant for.
use std::path::Path;

/// Names [`closest_name`] reads at most: past them it cannot prove a name
/// the unique closest, and names none.
const MAX_SIBLINGS: usize = 4096;

/// The one file in `dir` whose name is closest to `name` (a mistyped file
/// name); see [`closest_name`]. A directory is no file to read, so it is
/// never a candidate. A name with a wildcard (`*`, `?`) is a glob, which no
/// file is a mistyping of: it has none.
pub fn close_name(dir: &Path, name: &str) -> Option<String> {
    if name.contains(['*', '?']) {
        return None;
    }
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
