//! A search scoped to production code or to test code.
use super::*;
use crate::types::TestCodeScope;

fn files(result: &TextSearchResult) -> Vec<(String, u32)> {
    let mut files = result
        .files
        .iter()
        .map(|file| {
            (
                Path::new(&file.path)
                    .file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned(),
                file.match_count,
            )
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// `exclude` keeps production hits only, leaving test files unread and
/// test lines out; `only` keeps the rest; both say what they left out.
#[test]
fn a_scope_keeps_one_side_of_the_test_code_line() {
    let t = TmpDir::new();
    t.write(
        "lib.rs",
        "hit();\n\n#[cfg(test)]\nmod tests { fn t() { hit(); } }\n\n",
    );
    t.write("tests/it.rs", "hit();\n");
    t.write("app.ts", "hit();\n");
    t.write("app.test.ts", "hit();\nhit();\n");
    let root = t.path();
    let mut production = opts(root.clone(), "hit");
    production.output = SearchOutput::CountMatches;
    let excluded = {
        let mut options = production.clone();
        options.test_code = scoped_scope(&root, false);
        search(options).expect("exclude")
    };
    assert_eq!(
        files(&excluded),
        [("app.ts".into(), 1), ("lib.rs".into(), 1)]
    );
    assert_eq!(
        excluded.stats.test_code_excluded,
        Some(crate::types::TestCodeExcluded { files: 2, lines: 1 })
    );
    let only = {
        let mut options = production.clone();
        options.test_code = scoped_scope(&root, true);
        search(options).expect("only")
    };
    assert_eq!(
        files(&only),
        [
            ("app.test.ts".into(), 2),
            ("it.rs".into(), 1),
            ("lib.rs".into(), 1)
        ]
    );
    assert_eq!(
        only.stats.test_code_excluded,
        Some(crate::types::TestCodeExcluded { files: 1, lines: 1 })
    );
    let unscoped = search(production).expect("unscoped");
    assert_eq!(unscoped.stats.test_code_excluded, None);
}

/// A searched file whose spans cover it whole (`#![cfg(test)]`) is test
/// code whole: left out as a file, never listed by a files view or a
/// files-without view, and its hit lines are not counted.
#[test]
fn a_file_wholly_test_by_its_spans_is_left_out_as_a_file() {
    let t = TmpDir::new();
    t.write("gated.rs", "hit();\n");
    t.write("quiet_gated.rs", "nothing();\n");
    t.write("app.rs", "hit();\n");
    t.write("plain.rs", "nothing();\n");
    let root = t.path();
    let gated = |path: &Path| {
        path.file_name()
            .is_some_and(|name| name.to_string_lossy().contains("gated"))
    };
    let scope = TestCodeScope {
        keep_test: false,
        test_path: Arc::new(|_| false),
        may_hold: Arc::new(|path| path.extension().is_some_and(|ext| ext == "rs")),
        spans: Arc::new(move |path| {
            Arc::new(if gated(path) {
                vec![crate::types::WHOLE_FILE]
            } else {
                Vec::new()
            })
        }),
    };
    for (output, listed) in [
        (SearchOutput::FilesWithMatch, "app.rs"),
        (SearchOutput::FilesWithoutMatch, "plain.rs"),
    ] {
        let mut options = opts(root.clone(), "hit");
        options.output = output;
        options.test_code = Some(scope.clone());
        let result = search(options).expect("scoped");
        let shown = files(&result)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        assert_eq!(shown, [listed], "{output:?}");
        let excluded = result.stats.test_code_excluded.expect("disclosed");
        assert_eq!(excluded.lines, 0, "{output:?}: {excluded:?}");
        assert!(excluded.files >= 1, "{output:?}: {excluded:?}");
    }
}

/// A scope over `root`: test paths by convention, and `lib.rs` lines 3-5
/// a `#[cfg(test)]` module.
fn scoped_scope(root: &str, keep_test: bool) -> Option<TestCodeScope> {
    let root = root.to_owned();
    let relative = move |path: &Path| {
        path.strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    };
    let relative_too = relative.clone();
    Some(TestCodeScope {
        keep_test,
        test_path: Arc::new(move |path| crate::text::test_paths::is_test_path(&relative(path))),
        may_hold: Arc::new(|path| path.extension().is_some_and(|ext| ext == "rs")),
        spans: Arc::new(move |path| {
            Arc::new(if relative_too(path) == "lib.rs" {
                vec![[3, 5]]
            } else {
                Vec::new()
            })
        }),
    })
}
