use super::*;

/// The one `include` rule, as (glob, path below the base, kept): a glob
/// without a `/` names an entry at any depth, one with a `/` reads the path
/// below the base, `*` never crosses `/`, case and spelling are literal.
/// Every matcher of the rule runs this table: this module's, ghStructure's
/// (the base is the listed `path`), ghGetHistoryItem's (the repository
/// root) and the local walks' own compiler.
pub(crate) const RULE: &[(&str, &str, bool)] = &[
    ("*.rs", "lib.rs", true),
    ("*.rs", "src/deep/lib.rs", true),
    ("*.rs", "src/lib.ts", false),
    ("*.RS", "lib.rs", false),
    ("lib.rs", "src/lib.rs", true),
    ("lib.rs", "src/lib.rs.bak", false),
    ("lib", "src/library", false),
    ("*lib*", "src/library", true),
    ("src/*.rs", "src/lib.rs", true),
    ("src/*.rs", "src/deep/lib.rs", false),
    ("src/*.rs", "other/src/lib.rs", false),
    ("src/**", "src/deep/lib.rs", true),
    ("src/**", "other/lib.rs", false),
    ("**/package.json", "a/b/package.json", true),
    ("/lib.rs", "lib.rs", true),
    ("/lib.rs", "src/lib.rs", false),
];

#[test]
fn the_rule_holds_for_the_shared_matcher() {
    for (glob, path, kept) in RULE {
        let globs = IncludeGlobs::new([*glob]).expect("glob");
        assert_eq!(globs.matches(path, false), *kept, "{glob} on {path}");
    }
}

/// The local walks compile their `include` with the same function against
/// the walked root: the table reads the same there, whatever the root.
#[test]
fn the_rule_is_the_local_walks_own() {
    let root = Path::new("/any/walk/root");
    for (glob, path, kept) in RULE {
        let globs =
            octocode_engine::search::glob::compile(root, &[(*glob).to_owned()], &[]).expect("glob");
        assert_eq!(
            globs.includes(&root.join(path), false),
            *kept,
            "{glob} on {path}"
        );
    }
}

#[test]
fn globs_are_any_of_and_a_broken_glob_is_named() {
    let globs = IncludeGlobs::new(["*.rs", "docs/*.md"]).expect("globs");
    assert!(globs.matches("a/b.rs", false));
    assert!(globs.matches("docs/x.md", false));
    assert!(!globs.matches("x.md", false));
    let error = IncludeGlobs::new(["src/[a"]).err().expect("refused");
    assert!(error.contains("src/[a"), "{error}");
}

/// An exact glob keeps its own path and nothing else, whatever characters
/// the path holds and however deep another entry of the same name sits.
#[test]
fn an_exact_glob_keeps_only_its_path() {
    for path in [
        "README.md",
        "src/lib.rs",
        "app/[id]/page.tsx",
        "!important",
        "#notes",
        "a b/c*.rs",
        "x{y}.rs",
    ] {
        let globs = IncludeGlobs::new([exact(path).as_str()]).expect("glob");
        assert!(globs.matches(path, false), "{path}");
        for other in [
            format!("deep/{path}"),
            format!("{path}.bak"),
            format!("{path}/x"),
        ] {
            assert!(!globs.matches(&other, false), "{path} kept {other}");
        }
    }
}
