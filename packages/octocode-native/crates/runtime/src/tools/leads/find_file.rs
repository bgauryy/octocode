//! Recovery for a file a caller named that is not where it said: the
//! structureSearch listing that finds it by stem, and the sibling a
//! mistyped name was likely meant for.
use crate::id::ToolId;
use crate::response::result::Continuation;
use serde_json::{Value, json};
use std::path::Path;

/// A listing of files named like `requested` from `root`: its stem (the name
/// without its extension) under any extension, at any depth, so a moved file
/// or a changed extension shows. Never every name merely containing the
/// stem. A name without an extension is matched as written.
pub(crate) fn find_file(root: &str, requested: &str) -> Option<Value> {
    let name = Path::new(requested).file_name()?.to_string_lossy();
    let include = match name.split_once('.') {
        Some((stem, _)) if !stem.is_empty() => format!("{}.*", escape_glob(stem)),
        _ => name.to_string(),
    };
    (!include.is_empty()).then(|| {
        Continuation::new(
            ToolId::StructureSearch,
            json!({"operation": "files", "path": root, "include": [include]}),
        )
        .build()
    })
}

/// Names [`closest_name`] reads at most: past them it cannot prove a name
/// the unique closest, and names none.
const MAX_SIBLINGS: usize = 4096;

/// The one file in `dir` whose name is closest to `name` (a mistyped file
/// name); see [`closest_name`]. A directory is no file to read, so it is
/// never a candidate.
pub(crate) fn close_name(dir: &Path, name: &str) -> Option<String> {
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
pub(crate) fn closest_name(
    candidates: impl IntoIterator<Item = String>,
    name: &str,
) -> Option<String> {
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
        let distance = crate::contracts::levenshtein(&wanted, &candidate.to_lowercase());
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

/// `text` as a glob that matches only itself.
fn escape_glob(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '*' | '?' | '[' | ']' | '{' | '}' => vec!['[', c, ']'],
            _ => vec![c],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stem_matches_under_any_extension() {
        let lead = find_file("pkg", "src/a/blok.rs").expect("lead");
        let row = &lead["query"]["queries"][0];
        assert_eq!(lead["tool"], "structureSearch", "{lead}");
        assert_eq!(row["operation"], "files", "{lead}");
        assert_eq!(row["path"], "pkg", "{lead}");
        assert_eq!(row["include"], json!(["blok.*"]), "{lead}");
    }

    #[test]
    fn a_name_without_an_extension_and_glob_characters_match_only_themselves() {
        let row = &find_file(".", "Makefile").expect("lead")["query"]["queries"][0];
        assert_eq!(row["include"], json!(["Makefile"]));
        let row = &find_file(".", "[id].tsx").expect("lead")["query"]["queries"][0];
        assert_eq!(row["include"], json!(["[[]id[]].*"]));
    }

    #[test]
    fn a_mistyped_name_finds_its_one_close_sibling_file() {
        let dir = tempfile::tempdir().expect("dir");
        for name in ["executor.rs", "Readme.md", "a.rs", "b.rs"] {
            std::fs::write(dir.path().join(name), "").expect("file");
        }
        std::fs::create_dir(dir.path().join("executer")).expect("subdir");
        let close = |name| close_name(dir.path(), name);
        assert_eq!(close("exector.rs").as_deref(), Some("executor.rs"));
        assert_eq!(close("exectuor.rs").as_deref(), Some("executor.rs"));
        assert_eq!(close("README.md").as_deref(), Some("Readme.md"));
        // Two files are equally close: no guess.
        assert_eq!(close("c.rs"), None);
        // Too far, and a directory is no file to read.
        assert_eq!(close("scheduler.rs"), None);
        assert_eq!(close("executer"), None);
    }

    #[test]
    fn any_named_candidates_find_their_one_close_name() {
        let names = || ["src", "docs", "tests"].map(String::from);
        assert_eq!(closest_name(names(), "sr").as_deref(), Some("src"));
        assert_eq!(closest_name(names(), "src"), None);
        assert_eq!(closest_name(names(), "bench"), None);
    }
}
