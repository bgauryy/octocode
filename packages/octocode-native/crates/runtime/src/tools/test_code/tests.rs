use super::*;
use crate::tools::test_support::{Fixture, workspace_policy};

fn index(root: &Fixture) -> TestCodeIndex {
    TestCodeIndex::new(&workspace_policy(&root.0))
}

/// Test files and directories by each language's convention are test code
/// whole; production files are not.
#[test]
fn test_files_and_directories_are_test_code_whole() {
    let root = Fixture::new();
    let index = index(&root);
    for test in [
        "tests/integration.rs",
        "src/parser/tests.rs",
        "web/src/a.test.ts",
        "web/src/__tests__/b.ts",
        "py/test_api.py",
    ] {
        let path = root.write(test, "x\n");
        assert_eq!(*index.spans(&path), vec![WHOLE_FILE], "{test}");
    }
    for production in ["web/src/a.ts", "py/api.py"] {
        let path = root.write(production, "x\n");
        assert!(index.spans(&path).is_empty(), "{production}");
    }
}

/// Rust: items under `#[cfg(test)]` are test lines; `#![cfg(test)]` makes
/// the whole file test code; a module mounted only under `#[cfg(test)]` is
/// test code whole, and so is every module below it.
#[test]
fn rust_test_items_files_and_mounted_modules_are_test_code() {
    let root = Fixture::new();
    let index = index(&root);
    let lib = root.write(
        "src/lib.rs",
        "pub mod api;\n#[cfg(test)]\nmod helpers;\npub fn run() {}\n#[cfg(test)]\nmod inline {\n    fn t() {}\n}\n",
    );
    let spans = index.spans(&lib);
    assert!(!in_spans(&spans, 4), "production fn: {spans:?}");
    assert!(in_spans(&spans, 7), "inline test module: {spans:?}");
    let api = root.write("src/api.rs", "pub fn serve() {}\n");
    assert!(index.spans(&api).is_empty());
    let helpers = root.write("src/helpers.rs", "pub mod deep;\npub fn fixture() {}\n");
    assert_eq!(*index.spans(&helpers), vec![WHOLE_FILE]);
    let deep = root.write("src/helpers/deep.rs", "pub fn nested() {}\n");
    assert_eq!(
        *index.spans(&deep),
        vec![WHOLE_FILE],
        "mounted below a test module"
    );
    let gated = root.write("src/gated.rs", "#![cfg(test)]\nfn only_in_tests() {}\n");
    let spans = index.spans(&gated);
    assert!(in_spans(&spans, 1) && in_spans(&spans, 2), "{spans:?}");
}

#[test]
fn a_scope_keeps_its_side() {
    assert_eq!(Keep::of(None), None);
    assert_eq!(Keep::of(Some(TestCode::Include)), None);
    assert!(Keep::Production.keeps(false) && !Keep::Production.keeps(true));
    assert!(Keep::Test.keeps(true) && !Keep::Test.keeps(false));
}

/// A listing keeps whole entries: a test directory, and a file wholly test
/// code (by path, mount or `#![cfg(test)]`), are test code; a Rust file
/// with only some test items is production code.
#[test]
fn a_listing_keeps_whole_entries_on_its_side() {
    let root = Fixture::new();
    let policy = workspace_policy(&root.0);
    let lib = root.write(
        "src/lib.rs",
        "#[cfg(test)]\nmod helpers;\npub fn run() {}\n#[cfg(test)]\nmod inline {}\n",
    );
    let helpers = root.write("src/helpers.rs", "pub fn fixture() {}\n");
    let gated = root.write("src/gated.rs", "//! Gated.\n#![cfg(test)]\nfn t() {}\n");
    let spec = root.write("web/__tests__/a.ts", "x\n");
    let dirs = [root.0.join("web/__tests__"), root.0.join("src")];
    let only = Scoped::of(Some(TestCode::Only), &policy).expect("scoped");
    let exclude = Scoped::of(Some(TestCode::Exclude), &policy).expect("scoped");
    for test in [&helpers, &gated, &spec] {
        assert!(
            only.lists(test, false) && !exclude.lists(test, false),
            "{test:?}"
        );
    }
    assert!(
        !only.lists(&lib, false) && exclude.lists(&lib, false),
        "mixed file"
    );
    assert!(only.lists(&dirs[0], true) && !exclude.lists(&dirs[0], true));
    assert!(!only.lists(&dirs[1], true) && exclude.lists(&dirs[1], true));
    assert_eq!(
        only.excluded(),
        Excluded::default(),
        "a listing counts itself"
    );
}
