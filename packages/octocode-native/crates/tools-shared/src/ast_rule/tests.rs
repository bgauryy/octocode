/// A syntax-tree budget stop is terminal: its recovery names no
/// continuation field (syntaxTree pages by `page`/`snapshot`, never an
/// offset) and leads to the tools that reach the unlisted nodes.
#[test]
fn a_tree_budget_recovery_names_no_offset_field() {
    let text = super::diagnostic_recovery("syntaxTree.budget.deadline", None).expect("recovery");
    assert!(!text.contains("nextOffset"), "{text}");
    assert!(text.contains("localFetch"), "{text}");
}

/// The grammar census counts the files the walk's flags admit: an `include`
/// glob only narrows them, so a hidden or ignored file it matches is counted
/// only with the flag that admits it.
#[test]
fn the_grammar_census_counts_no_hidden_or_ignored_file_an_include_glob_matches() {
    use crate::empty_walk::{Globs, Walk};
    let root = tempfile::tempdir().expect("root");
    let write = |path: &str, text: &str| {
        let path = root.path().join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, text).expect("file");
    };
    write("a.ts", "export {};\n");
    write("skip.rb", "x = 1\n");
    write(".hidden/b.py", "x = 1\n");
    write("out/c.rs", "fn f() {}\n");
    write(".ignore", "out/\n");
    let include = ["*.ts", "*.py", "*.rs"].map(str::to_owned);
    let census = |ignored: bool, hidden: bool| {
        let grammars = super::present_grammars(
            root.path(),
            &[],
            Walk {
                ignored,
                hidden,
                default_excludes: false,
                global: octocode_engine::search::walk::GlobalIgnore::NONE,
            },
            usize::MAX,
            Globs {
                include: &include,
                exclude: &[],
            },
            &super::GlobParsers::default(),
            &|_, _| true,
            &|| false,
        )
        .expect("census");
        grammars
            .into_iter()
            .map(|(grammar, _)| grammar)
            .collect::<Vec<_>>()
    };
    let has = |grammars: &[String], grammar: &str| grammars.iter().any(|g| g == grammar);
    let plain = census(false, false);
    assert!(has(&plain, "typescript"), "{plain:?}");
    assert!(!has(&plain, "python") && !has(&plain, "rust"), "{plain:?}");
    // The glob still narrows: a file it does not name is never counted.
    assert!(!has(&plain, "ruby"), "{plain:?}");
    let with_hidden = census(false, true);
    assert!(
        has(&with_hidden, "python") && !has(&with_hidden, "rust"),
        "{with_hidden:?}"
    );
    let with_ignored = census(true, false);
    assert!(
        has(&with_ignored, "rust") && !has(&with_ignored, "python"),
        "{with_ignored:?}"
    );
}
