//! `testCode` on a files listing: whole entries on the kept side, the
//! entries left out counted, the same on every page.
use super::*;

/// One fixture per convention: Rust by path, mount and `#![cfg(test)]`
/// (and a production file with test items), TypeScript `*.test.ts` and
/// `__tests__`, Python `test_*.py`.
fn mixed() -> Fixture {
    let root = Fixture::new();
    root.write(
        "src/lib.rs",
        "#[cfg(test)]\nmod helpers;\npub fn run() {}\n#[cfg(test)]\nmod tests {}\n",
    );
    root.write("src/helpers.rs", "pub fn fixture() {}\n");
    root.write("src/gated.rs", "#![cfg(test)]\nfn t() {}\n");
    root.write("tests/it.rs", "fn it() {}\n");
    root.write("web/a.ts", "export const a = 1;\n");
    root.write("web/a.test.ts", "test('a', () => {});\n");
    root.write("web/__tests__/b.ts", "test('b', () => {});\n");
    root.write("py/api.py", "def serve(): pass\n");
    root.write("py/test_api.py", "def test_serve(): pass\n");
    root
}

fn names(out: &Value) -> Vec<String> {
    let mut names = paths(out)
        .iter()
        .map(|path| {
            let path = path.trim_start_matches("./");
            path.rsplit_once("/./")
                .map_or(path, |(_, tail)| tail)
                .to_owned()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn exclude_lists_production_entries_and_counts_the_test_ones() {
    let root = mixed();
    let all = run(&root.0, json!({"operation":"files","path":"."})).expect("listing");
    assert!(all.get("testCodeExcluded").is_none(), "{all}");
    let out = run(
        &root.0,
        json!({"operation":"files","path":".","testCode":"exclude"}),
    )
    .expect("listing");
    let listed = names(&out);
    for production in ["src/lib.rs", "web/a.ts", "py/api.py"] {
        assert!(
            listed.iter().any(|name| name.ends_with(production)),
            "{production}: {out}"
        );
    }
    for test in [
        "helpers.rs",
        "gated.rs",
        "it.rs",
        "a.test.ts",
        "b.ts",
        "test_api.py",
        "__tests__",
        "tests",
    ] {
        assert!(
            !listed.iter().any(|name| name.ends_with(test)),
            "{test} listed: {out}"
        );
    }
    assert_eq!(out["testCodeExcluded"], json!({"files": 8}), "{out}");
    assert_eq!(
        listed.len() + 8,
        names(&all).len(),
        "every entry listed or counted: {all}"
    );
    crate::tools::test_support::assert_row_contract(crate::id::ToolId::StructureSearch, &out);
}

#[test]
fn only_lists_whole_test_entries_and_walks_production_dirs_for_them() {
    let root = mixed();
    let out = run(
        &root.0,
        json!({"operation":"files","path":".","testCode":"only"}),
    )
    .expect("listing");
    let listed = names(&out);
    for test in [
        "src/helpers.rs",
        "src/gated.rs",
        "tests/it.rs",
        "web/a.test.ts",
        "web/__tests__/b.ts",
        "py/test_api.py",
    ] {
        assert!(
            listed.iter().any(|name| name.ends_with(test)),
            "{test}: {out}"
        );
    }
    for production in ["lib.rs", "a.ts", "api.py", "src", "web", "py"] {
        assert!(
            !listed
                .iter()
                .any(|name| name.ends_with(&format!("/{production}")) || name == production),
            "{production} listed: {out}"
        );
    }
    assert_eq!(out["testCodeExcluded"], json!({"files": 6}), "{out}");
}

/// Every page of a scoped listing lists only its side, and the pages
/// together list every kept entry once.
#[test]
fn scoped_pages_list_each_kept_entry_once() {
    let root = mixed();
    let pages = walk_pages(
        &root.0,
        json!({"operation":"files","path":".","testCode":"only","limit":2}),
    );
    let listed = pages.iter().flat_map(names).collect::<Vec<_>>();
    assert_eq!(listed.len(), 8, "{pages:?}");
}
