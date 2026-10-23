//! Listing facts: exact byte sizes, no root dot, summaries that match the
//! pages, bare-word includes and directories without a size.

use super::*;

/// Tree and files print the same exact byte size.
#[test]
fn tree_and_files_print_exact_byte_sizes() {
    let root = Fixture::new();
    std::fs::write(root.0.join("f.txt"), "x".repeat(9216)).expect("file");
    let tree = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("tree");
    let files = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("files");
    assert_eq!(
        tree["files"],
        json!([{"dir":".","files":["f.txt (9216)"]}]),
        "{tree}"
    );
    assert_eq!(
        files["files"],
        json!([{"dir":".","files":["f.txt (9216)"]}]),
        "{files}"
    );
}

/// A directory root is the listing, never one of its rows.
#[test]
fn files_never_lists_root_dot() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "a\n").expect("file");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("files");
    assert!(!has_string(&out["files"], "./"), "{out}");
    assert_eq!(
        out["files"],
        json!([{"dir":".","files":["a.rs (2)"]}]),
        "{out}"
    );
}

/// The tree summary states only the file and directory counts, and
/// totalItems counts the strings the pages show.
#[test]
fn tree_summary_matches_paged_row_counts() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b")).expect("dirs");
    std::fs::write(root.0.join("a/x.rs"), "x\n").expect("x");
    std::fs::write(root.0.join("a/b/y.rs"), "y\n").expect("y");
    std::fs::write(root.0.join("top.rs"), "t\n").expect("top");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":3,"pageSize":1}),
    )
    .expect("tree");
    assert_eq!(
        out["summary"],
        json!({"files":3,"dirs":2,"bytes":6}),
        "{out}"
    );
    let total = out["pagination"]["totalItems"].as_u64().expect("total");
    let query = json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":3,"pageSize":1});
    let mut strings = 0;
    for page in walk_pages(&root.0, query) {
        for entry in page["files"].as_array().expect("files") {
            strings += entry
                .get("entries")
                .and_then(Value::as_array)
                .map_or(1, Vec::len);
        }
    }
    assert_eq!(strings as u64, total, "{out}");
}

/// A bare word include lists the files inside directories named by it.
#[test]
fn bare_word_include_lists_dir_files() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/inner")).expect("dirs");
    std::fs::write(root.0.join("src/inner/a.rs"), "a\n").expect("a");
    std::fs::write(root.0.join("other.rs"), "o\n").expect("other");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"include":["src"]}),
    )
    .expect("files");
    let paths = listed(&out)
        .iter()
        .map(|entry| entry["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert!(
        paths.iter().any(|path| path.ends_with("src/inner/a.rs")),
        "{out}"
    );
    crate::tools::test_support::checks::assert_absent(&out["files"], "other.rs");
}

/// A directory's own length (its entry table) is no content: `sort:size`
/// ranks it after every file and names no size for it, and a size filter
/// lists no directory.
#[test]
fn directories_carry_no_size_to_rank_or_filter_by() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("crowded")).expect("dir");
    for index in 0..64 {
        std::fs::write(
            root.0
                .join(format!("crowded/entry-with-a-long-name-{index:03}")),
            "",
        )
        .expect("entry");
    }
    std::fs::write(root.0.join("a.txt"), "hello").expect("file");
    assert!(
        std::fs::metadata(root.0.join("crowded"))
            .expect("dir")
            .len()
            > 5
    );

    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"maxDepth":1,"sort":"size"}),
    )
    .expect("listing");
    let rows = listed(&out);
    assert_eq!(rows.len(), 2, "{out}");
    assert!(
        rows[0]["path"].as_str().expect("path").ends_with("a.txt"),
        "{out}"
    );
    assert_eq!(rows[0]["size"], 5, "{out}");
    assert_eq!(rows[1]["type"], "directory", "{out}");
    assert!(rows[1].get("size").is_none(), "{out}");

    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"maxDepth":1,"size":{"greater":"1"}}),
    )
    .expect("sized listing");
    let rows = listed(&out);
    assert_eq!(rows.len(), 1, "{out}");
    assert!(
        rows[0]["path"].as_str().expect("path").ends_with("a.txt"),
        "{out}"
    );
}
