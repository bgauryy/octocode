use octocode_tools_shared::leads::find_file::close_name;
use octocode_tools_shared::leads::find_file::closest_name;
use octocode_tools_shared::leads::find_file::find_file;

#[test]
fn related_file_searches_preserve_scope_and_literal_names() {
    use octocode_runtime_base::execution::cancel::NeverCancel;
    use octocode_runtime_base::test_support::{Fixture, simple_policy};
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
        "!bang.ts",
        "bang.ts",
        "#hash.ts",
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
        // A leading `!` or `#` is part of the name, never a gitignore
        // negation or comment.
        ("!bang.rs", vec!["pkg/!bang.ts"]),
        ("#hash.rs", vec!["pkg/#hash.ts"]),
    ] {
        let lead = find_file("pkg", requested).expect("related request");
        assert_eq!(lead["kind"], "related");
        octocode_runtime_base::contracts::validate("structureSearch", lead["input"].clone())
            .expect("public input");
        let query = serde_json::from_value(lead["input"]["queries"][0].clone()).unwrap();
        let out = octocode_tool_local::structure_search::execute_structure(
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

/// A name with a wildcard (`*`, `?`) is a glob, not a file that moved:
/// no listing of files literally so named is offered.
#[test]
fn a_wildcard_name_offers_no_listing() {
    for requested in ["w/*.md", "src/a?.rs", "*"] {
        assert!(find_file("pkg", requested).is_none(), "{requested}");
    }
}
