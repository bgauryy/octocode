//! Line counts and file metadata: full detail, line sorts, unreadable and
//! binary files, the descendant policy, cancellation and escaped links.

use super::*;

#[test]
fn line_counts_only_read_the_selected_page() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "one\ntwo\n").expect("source");
    }
    let before = super::super::rows::line_count_reads();
    let first = run_budget(
        &root.0,
        json!({"operation":"files","path":root.0,"sort":{"by":"name","order":"asc"},"maxEntries":4}),
        1,
    )
    .expect("listing");
    assert_eq!(listed(&first).len(), 1);
    assert_eq!(super::super::rows::line_count_reads() - before, 1);
}

#[test]
fn file_metadata_is_available_without_a_projection_switch() {
    let root = Fixture::new();
    root.write("source.rs", "source\n");
    let expected = root
        .0
        .join("source.rs")
        .metadata()
        .expect("file")
        .modified()
        .expect("mtime")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("epoch")
        .as_millis() as i64;
    for operation in ["tree", "files"] {
        let out = run(&root.0, json!({"operation":operation,"path":root.0})).expect("listing");
        let rows = listed(&out);
        assert_eq!(rows[0]["size"], 7, "{out}");
        assert_eq!(rows[0]["totalLines"], 1, "{out}");
        assert!(
            (rows[0]["modifiedMs"].as_i64().expect("mtime") - expected).abs() <= 1,
            "{out}"
        );
    }
}

#[test]
fn line_sort_counts_above_two_thousand_entries() {
    let root = Fixture::new();
    for i in 0..2001 {
        std::fs::write(root.0.join(format!("file-{i:04}.rs")), "line\n").expect("file");
    }
    std::fs::write(root.0.join("largest.rs"), "line\n".repeat(7)).expect("largest");
    let out = run(&root.0, json!({"operation":"files","path":root.0,"entryType": "file","sort":{"by":"lineCount","order":"desc"}})).expect("files");
    assert_eq!(out["pagination"]["totalItems"], 2002);
    assert!(
        listed(&out)[0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("/largest.rs")),
        "{out}"
    );
    assert_eq!(listed(&out)[0]["totalLines"], 7, "{out}");
}

/// A list sorted by lines shows the value it is sorted by, with no extras:
/// each file row carries its `totalLines`, in descending order.
#[test]
fn a_line_sort_exposes_each_sort_key() {
    let root = Fixture::new();
    std::fs::write(root.0.join("big.rs"), "line\n".repeat(9)).expect("big");
    std::fs::write(root.0.join("small.rs"), "line\n".repeat(2)).expect("small");
    std::fs::write(root.0.join("empty.rs"), "").expect("empty");
    std::fs::create_dir(root.0.join("dir")).expect("dir");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"sort":{"by":"lineCount","order":"desc"}}),
    )
    .expect("files");
    let rows = listed(&out);
    let lines = rows
        .iter()
        .filter(|row| row["type"] != "directory")
        .map(|row| row["totalLines"].clone())
        .collect::<Vec<_>>();
    assert_eq!(lines, [json!(9), json!(2), json!(0)], "{out}");
    let by_size = run(
        &root.0,
        json!({"operation":"files","path":root.0,"entryType": "file","sort":{"by":"sizeBytes","order":"desc"}}),
    )
    .expect("files");
    assert_eq!(listed(&by_size)[0]["totalLines"], 9, "{by_size}");
}

/// A line sort counts text files only: a binary file has no lines (it
/// says `binary`) and an unreadable one an unknown count (`unreadable`),
/// never a count of 0 that reads like an empty file; both rank after the
/// counted files.
#[test]
fn a_line_sort_names_binary_and_unreadable_files_instead_of_counting_them() {
    let root = Fixture::new();
    std::fs::write(root.0.join("text.rs"), "a\nb\n").expect("text");
    std::fs::write(root.0.join("empty.rs"), "").expect("empty");
    let mut binary = vec![b'\n'; 64];
    binary.extend([0_u8, 1, 2, 3]);
    std::fs::write(root.0.join("blob.bin"), &binary).expect("binary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(root.0.join("locked.rs"), "a\n").expect("locked");
        std::fs::set_permissions(
            root.0.join("locked.rs"),
            std::fs::Permissions::from_mode(0o000),
        )
        .expect("chmod");
    }
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"entryType": "file","sort":{"by":"lineCount","order":"desc"}}),
    )
    .expect("files");
    let lines = listed(&out)
        .iter()
        .map(|row| {
            let path = row["path"].as_str().expect("path");
            (
                path.rsplit('/').next().expect("name").to_owned(),
                row["totalLines"].clone(),
            )
        })
        .collect::<Vec<_>>();
    let expected = vec![
        ("text.rs".to_owned(), json!(2)),
        ("empty.rs".to_owned(), json!(0)),
        ("blob.bin".to_owned(), json!("binary")),
    ];
    // Root reads every file whatever its mode.
    #[cfg(unix)]
    let expected = {
        let mut expected = expected;
        if std::fs::read(root.0.join("locked.rs")).is_err() {
            expected.push(("locked.rs".to_owned(), json!("unreadable")));
        } else {
            expected.insert(1, ("locked.rs".to_owned(), json!(1)));
        }
        expected
    };
    assert_eq!(lines, expected, "{out}");
}

#[test]
fn descendant_policy_precedes_discovery_totals_and_line_reads() {
    let root = sensitive_fixture();
    let files = run(
        &root.0,
        json!({"operation":"files","path":root.0,"sort":{"by":"lineCount","order":"desc"},"entryType": "file"}),
    )
    .expect("files");
    assert_eq!(files["pagination"]["totalItems"], 1);
    assert_eq!(listed(&files)[0]["totalLines"], 2);
    crate::tools::test_support::checks::assert_absent(&files, "credentials");
    crate::tools::test_support::checks::assert_absent(&files, "tfstate");
    let tree = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"includeHidden":true}),
    )
    .expect("tree");
    assert_eq!(listed(&tree).len(), 1, "{tree}");
    assert!(paths(&tree)[0].ends_with("/visible.rs"), "{tree}");
}

#[test]
fn cancellation_interrupts_descendant_traversal() {
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source").expect("source");
    let (paths, security) = simple_policy(&root.0);
    for operation in ["files", "tree"] {
        let error = execute_row(
            json!({"operation":operation,"path":root.0}),
            &paths,
            &security,
            &AfterRoot(std::sync::atomic::AtomicUsize::new(0)),
        )
        .expect_err("cancel during walk");
        assert_eq!(error.code, ErrorCode::Cancelled, "{operation}");
    }
}

#[cfg(unix)]
#[test]
fn escaped_links_are_pruned_before_line_counting() {
    let root = Fixture::new();
    let outside = Fixture::new();
    std::fs::write(outside.0.join("outside.rs"), "outside\n").expect("outside");
    std::os::unix::fs::symlink(outside.0.join("outside.rs"), root.0.join("link.rs")).expect("link");
    let (paths, security) = simple_policy(&root.0);
    let files = execute_row(
        json!({"operation":"files","path":root.0,"entryType": "file"}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("files");
    assert_eq!(files["pagination"]["totalItems"], 0);
}

#[test]
fn files_line_count_counts_lines_not_newlines_plus_one() {
    let root = Fixture::new();
    std::fs::write(root.0.join("two.rs"), "a\nb\n").expect("two");
    std::fs::write(root.0.join("partial.rs"), "a\nb").expect("partial");
    std::fs::write(root.0.join("empty.rs"), "").expect("empty");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"entryType": "file"}),
    )
    .expect("files");
    let count = |name: &str| {
        listed(&out)
            .iter()
            .find(|f| f["path"].as_str().is_some_and(|p| p.ends_with(name)))
            .map(|f| f["totalLines"].clone())
            .unwrap_or_else(|| panic!("{name} in {out}"))
    };
    assert_eq!(count("two.rs"), 2);
    assert_eq!(count("partial.rs"), 2);
    // Zero-line files omit totalLines rather than reporting an extra line.
    assert!(count("empty.rs").is_null());
}

#[test]
fn file_rows_omit_the_default_file_type() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("dir")).expect("dir");
    std::fs::write(root.0.join("dir/a.rs"), "fn a() {}\n").expect("a");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"sort":{"by":"path","order":"asc"}}),
    )
    .expect("files");
    let rows = listed(&out);
    let file = rows
        .iter()
        .find(|r| r["path"].as_str().is_some_and(|p| p.ends_with("a.rs")))
        .expect("file row");
    assert!(file.get("type").is_none(), "{file}");
    if let Some(dir) = rows
        .iter()
        .find(|r| r["path"].as_str().is_some_and(|p| p.ends_with("/dir")))
    {
        assert_eq!(dir["type"], "directory", "{dir}");
    }
}

/// A listed link to a directory has no lines: it shows no line count, in
/// details or under a line sort, never `unreadable` (a failed read of a
/// file). A link to a file still counts its target's lines.
#[cfg(unix)]
#[test]
fn a_link_to_a_directory_has_no_line_count_not_unreadable() {
    let root = Fixture::new();
    root.write("src/a.rs", "a\nb\n");
    std::os::unix::fs::symlink("src", root.0.join("to_dir")).expect("dir link");
    std::os::unix::fs::symlink("src/a.rs", root.0.join("to_file")).expect("file link");
    for query in [
        json!({"operation":"tree","path":root.0}),
        json!({"operation":"files","path":root.0,"maxDepth":1,"sort":{"by":"lineCount","order":"desc"}}),
    ] {
        let out = run(&root.0, query).expect("listing");
        let rows = listed(&out);
        let row = |name: &str| {
            rows.iter()
                .find(|row| {
                    row["path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with(name))
                })
                .cloned()
                .unwrap_or_else(|| panic!("{name} listed: {out}"))
        };
        assert_eq!(row("to_dir")["type"], "symlink", "{out}");
        assert!(row("to_dir").get("totalLines").is_none(), "{out}");
        assert_eq!(row("to_file")["totalLines"], 2, "{out}");
    }
}
