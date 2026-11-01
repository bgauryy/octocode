//! `testCode` on astSearch: matches and declarations on the kept side of
//! the test-code line, test paths left unread, what was left out counted.
use super::test_rows::run;
use crate::tools::test_support::Fixture;
use serde_json::{Value, json};

/// Rust by path, mount, `#![cfg(test)]` and an inline `#[cfg(test)]`
/// module beside production code; TypeScript and Python by file name.
fn mixed() -> Fixture {
    let root = Fixture::new();
    root.write(
        "src/lib.rs",
        "#[cfg(test)]\nmod helpers;\npub fn run() { a.unwrap(); }\n#[cfg(test)]\nmod tests {\n    fn t() { b.unwrap(); }\n}\n",
    );
    root.write("src/helpers.rs", "pub fn h() { c.unwrap(); }\n");
    root.write("src/gated.rs", "#![cfg(test)]\nfn g() { d.unwrap(); }\n");
    root.write("tests/it.rs", "fn it() { e.unwrap(); }\n");
    // Not parsed by a Rust match, so never counted as left out.
    root.write("tests/README.md", "e.unwrap()\n");
    root
}

/// `path:line` of every match row, sorted, by file name.
fn hits(out: &Value) -> Vec<String> {
    let mut hits = out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| {
            let path = file["path"].as_str().expect("path");
            let name = path.rsplit('/').next().unwrap_or(path).to_owned();
            file["matches"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |row| format!("{name}:{}", row["line"]))
        })
        .collect::<Vec<_>>();
    hits.sort();
    hits
}

fn unwraps(root: &Fixture, path: &std::path::Path, test_code: &str) -> Value {
    run(
        &root.0,
        json!({"operation":"match","path":path,"rule":{"pattern":"$E.unwrap()"},"testCode":test_code}),
    )
    .expect("match")
}

#[test]
fn match_keeps_its_side_of_the_test_code_line() {
    let root = mixed();
    let exclude = unwraps(&root, &root.0, "exclude");
    assert_eq!(hits(&exclude), ["lib.rs:3"], "{exclude}");
    assert_eq!(
        exclude["testCodeExcluded"],
        json!({"files": 3, "lines": 1}),
        "{exclude}"
    );
    let only = unwraps(&root, &root.0, "only");
    assert_eq!(
        hits(&only),
        ["gated.rs:2", "helpers.rs:1", "it.rs:1", "lib.rs:6"],
        "{only}"
    );
    assert_eq!(only["testCodeExcluded"], json!({"lines": 1}), "{only}");
    let all = unwraps(&root, &root.0, "include");
    assert_eq!(hits(&all).len(), 5, "{all}");
    assert!(all.get("testCodeExcluded").is_none(), "{all}");
}

#[test]
fn a_single_file_match_keeps_its_side() {
    let root = mixed();
    let lib = root.0.join("src/lib.rs");
    let exclude = unwraps(&root, &lib, "exclude");
    assert_eq!(hits(&exclude), ["lib.rs:3"], "{exclude}");
    assert_eq!(
        exclude["testCodeExcluded"],
        json!({"lines": 1}),
        "{exclude}"
    );
}

/// Declaration names an outline lists, sorted.
fn declared(out: &Value) -> Vec<String> {
    let mut names = out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| file["symbols"].as_array().into_iter().flatten())
        .filter_map(|symbol| symbol["symbolName"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn symbols_keep_their_side_of_the_test_code_line() {
    let root = mixed();
    root.write("web/a.ts", "export function serve() {}\n");
    root.write("web/a.test.ts", "export function check() {}\n");
    root.write("py/test_api.py", "def test_serve():\n    pass\n");
    let outline = |test_code: &str| {
        run(
            &root.0,
            json!({"operation":"symbols","path":root.0,"testCode":test_code}),
        )
        .expect("symbols")
    };
    let exclude = outline("exclude");
    let names = declared(&exclude);
    for production in ["run", "serve"] {
        assert!(
            names.iter().any(|name| name == production),
            "{production}: {exclude}"
        );
    }
    for test in ["t", "h", "g", "it", "check", "test_serve", "tests"] {
        assert!(
            !names.iter().any(|name| name == test),
            "{test} listed: {exclude}"
        );
    }
    assert!(
        exclude["testCodeExcluded"]["files"].as_u64() >= Some(3),
        "{exclude}"
    );
    let only = outline("only");
    let names = declared(&only);
    for test in ["h", "g", "it", "check", "test_serve"] {
        assert!(names.iter().any(|name| name == test), "{test}: {only}");
    }
    for production in ["run", "serve"] {
        assert!(
            !names.iter().any(|name| name == production),
            "{production} listed: {only}"
        );
    }
}
