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
        _ => escape_glob(&name),
    };
    (!include.is_empty()).then(|| {
        Continuation::related(
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
    fn related_file_searches_preserve_scope_and_literal_names() {
        use crate::execution::cancel::NeverCancel;
        use crate::tools::test_support::{Fixture, simple_policy};
        let root = Fixture::new();
        std::fs::create_dir_all(root.0.join("pkg/nested")).unwrap();
        for name in [
            "blok.rs",
            "nested/blok.ts",
            "blok_extra.rs",
            "Makefile",
            "Makefile.txt",
            "[id].tsx",
            "id.tsx",
            "Make[ab]",
            "Makea",
        ] {
            std::fs::write(root.0.join("pkg").join(name), "").unwrap();
        }
        std::fs::write(root.0.join("blok.go"), "").unwrap();
        let (paths, security) = simple_policy(&root.0);
        for (requested, expected) in [
            ("src/blok.rs", vec!["pkg/blok.rs", "pkg/nested/blok.ts"]),
            ("Makefile", vec!["pkg/Makefile"]),
            ("[id].tsx", vec!["pkg/[id].tsx"]),
            ("Make[ab]", vec!["pkg/Make[ab]"]),
        ] {
            let lead = find_file("pkg", requested).expect("related request");
            assert_eq!(lead["kind"], "related");
            crate::contracts::validate("structureSearch", lead["input"].clone())
                .expect("public input");
            let query = serde_json::from_value(lead["input"]["queries"][0].clone()).unwrap();
            let out = crate::tools::structure_search::execute_structure(
                &query,
                &paths,
                &security,
                &NeverCancel,
                None,
            )
            .unwrap();
            let mut actual = out["files"]
                .as_array()
                .expect("files")
                .iter()
                .flat_map(|group| {
                    let entries = if let Some(entry) = group.as_str() {
                        vec![("pkg", entry)]
                    } else {
                        let dir = group["dir"].as_str().expect("directory");
                        group["files"]
                            .as_array()
                            .expect("entries")
                            .iter()
                            .map(|entry| (dir, entry.as_str().expect("entry")))
                            .collect()
                    };
                    entries
                        .into_iter()
                        .map(|(dir, entry)| {
                            let name = entry.rsplit_once(" (").map_or(entry, |(name, _)| name);
                            format!("{dir}/{name}")
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            actual.sort();
            assert_eq!(actual, expected, "{requested}: {out}");
        }
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
