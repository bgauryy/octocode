/// Git prints the path relative to the directory it ran in (an older Git,
/// or a plain checkout); an absolute one stands as it is.
#[test]
fn a_relative_git_path_is_read_from_the_directory_git_ran_in() {
    let root = std::path::Path::new("/w/repo");
    assert_eq!(
        super::printed_git_path(root, ".git/shallow\n"),
        root.join(".git/shallow")
    );
    assert_eq!(
        super::printed_git_path(root, "/elsewhere/.git/shallow\n"),
        std::path::Path::new("/elsewhere/.git/shallow")
    );
}

/// Fields of a `-z` log: each commit's hash after an empty field, its
/// files after the header's newline; the target is not its own row.
#[test]
fn a_z_log_counts_commits_and_the_files_beside_the_target() {
    let log = "\0h1\0\nsrc/a.ts\0src/b c.ts\0\0h2\0\nsrc/b c.ts\0src/a.ts\0\0h3\0\nsrc/a.ts\0";
    let none = std::collections::HashSet::new();
    let (commits, skipped, counts) = super::shared_commits(log, "src/a.ts", &none);
    assert_eq!((commits, skipped), (3, 0));
    assert_eq!(counts.len(), 1);
    assert_eq!(counts.get("src/b c.ts"), Some(&2));
    assert_eq!(super::shared_commits("", "src/a.ts", &none).0, 0);
    // A boundary commit counts toward nothing, its files included.
    let boundary = std::collections::HashSet::from(["h2".to_owned()]);
    let (commits, skipped, counts) = super::shared_commits(log, "src/a.ts", &boundary);
    assert_eq!((commits, skipped), (2, 1));
    assert_eq!(counts.get("src/b c.ts"), Some(&1));
}
