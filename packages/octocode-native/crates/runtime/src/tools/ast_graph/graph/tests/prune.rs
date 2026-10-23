//! The graph scans what every local tool walks: one default prune, so the
//! graph, its staleness scan and the LSP fingerprint (`tools::pruned_walk`)
//! cover the same files.

use super::fixture_graph;
use std::collections::BTreeSet;

/// A tree with source in `src/`, `build/` and `node_modules/`; with `git`, a
/// work tree that ignores `node_modules/` only (so `build/` is git's own).
fn tree(git: bool) -> crate::tools::test_support::Fixture {
    let root = crate::tools::test_support::Fixture::new();
    root.write("src/app.ts", "export const app = 1;\n");
    root.write("build/gen.ts", "export const gen = 1;\n");
    root.write("node_modules/dep/index.ts", "export const dep = 1;\n");
    if git {
        std::fs::create_dir(root.0.join(".git")).expect("repository marker");
        root.write(".gitignore", "node_modules/\n");
    }
    root
}

/// The `.ts` files a tool's own pruned walk of `root` reaches.
fn own_walk(root: &std::path::Path) -> BTreeSet<String> {
    crate::tools::pruned_walk(root, [])
        .build()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "ts"))
        .filter_map(|entry| {
            let relative = entry.path().strip_prefix(root).ok()?;
            Some(octocode_engine::text::slash_path(relative))
        })
        .collect()
}

#[test]
fn a_git_tracked_build_dir_is_scanned_as_every_tool_walks_it() {
    for git in [true, false] {
        let root = tree(git);
        let graph = fixture_graph(&root.0, &[])
            .facts
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let expected = if git {
            ["build/gen.ts", "src/app.ts"].as_slice()
        } else {
            ["src/app.ts"].as_slice()
        };
        let expected = expected
            .iter()
            .map(|path| (*path).to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(graph, expected, "git work tree: {git}");
        assert_eq!(own_walk(&root.0), expected, "git work tree: {git}");
    }
}
