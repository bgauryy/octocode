use super::*;

/// A literal glob matches its own name and no other, as an anchored
/// include line and as a `globset` glob with or without escapes.
#[test]
fn a_literal_glob_matches_only_its_text() {
    let root = Path::new("/r");
    let names = [
        "f\\g.ts",
        "a[1]*.ts",
        "x{y,z}?.ts",
        "has space.ts",
        "end.ts ",
        "plain.ts",
    ];
    for name in names {
        let line = format!("/{}", literal(name));
        let set = set(root, std::iter::once(line.as_str()), "include")
            .expect("compiles")
            .expect("one glob");
        for other in names {
            assert_eq!(
                matches(&set, &root.join(other), false),
                other == name,
                "{line} against {other:?}"
            );
        }
        for escape in [true, false] {
            let glob = globset::GlobBuilder::new(&literal(name))
                .backslash_escape(escape)
                .build()
                .expect("compiles")
                .compile_matcher();
            for other in names {
                assert_eq!(glob.is_match(other), other == name, "{name:?} {escape}");
            }
        }
    }
}

/// A name glob matches that name at any depth and nothing else, even
/// when the name starts with what a gitignore line reads as syntax.
#[test]
fn a_name_glob_matches_its_name_at_any_depth() {
    let root = Path::new("/r");
    let names = ["!bang.ts", "#hash.ts", "bang.ts", "a[1].ts"];
    for name_text in names {
        let line = name(name_text);
        let set = set(root, std::iter::once(line.as_str()), "include")
            .expect("compiles")
            .expect("one glob");
        for other in names {
            for dir in ["", "deep/er/"] {
                let path = root.join(format!("{dir}{other}"));
                assert_eq!(
                    matches(&set, &path, false),
                    other == name_text,
                    "{line} {path:?}"
                );
            }
        }
    }
}

/// A skipped directory is in an include list's scope when a path below it
/// could match a glob; a skipped file when a glob matches it.
#[test]
fn an_include_scope_reaches_only_directories_that_could_hold_a_match() {
    let root = Path::new("/r");
    let scope = |include: &[&str]| {
        IncludeScope::new(
            root,
            &include
                .iter()
                .map(|glob| (*glob).to_owned())
                .collect::<Vec<_>>(),
        )
    };
    let dir = |scope: &IncludeScope, relative: &str| {
        scope.reaches(&root.join(relative), Path::new(relative), true)
    };
    let agents = scope(&[".agents/**"]);
    assert!(dir(&agents, ".agents"));
    assert!(dir(&agents, ".agents/skills/deep"));
    assert!(!dir(&agents, "packages/repos/asm"));
    assert!(!dir(&agents, "releases"));
    let anchored = scope(&["/src/*/lib.rs", "docs/a.md"]);
    assert!(dir(&anchored, "src"));
    assert!(dir(&anchored, "src/x"));
    assert!(!dir(&anchored, "src/x/y"));
    assert!(dir(&anchored, "docs"));
    assert!(!dir(&anchored, "docs/a.md"));
    assert!(!dir(&anchored, "lib"));
    for anywhere in [&["*.rs"][..], &["build/"], &["**/gen/*.ts"], &[]] {
        assert!(dir(&scope(anywhere), "any/deep/dir"), "{anywhere:?}");
    }
    let files = scope(&["*.go"]);
    assert!(files.reaches(&root.join("a/b.go"), Path::new("a/b.go"), false));
    assert!(!files.reaches(&root.join("a/b.ts"), Path::new("a/b.ts"), false));
}

fn globs(include: &[&str], exclude: &[&str]) -> Result<Globs, Error> {
    let owned = |list: &[&str]| {
        list.iter()
            .map(|glob| (*glob).to_owned())
            .collect::<Vec<_>>()
    };
    compile(Path::new("/r"), &owned(include), &owned(exclude))
}

#[test]
fn a_slash_free_glob_matches_a_name_at_any_depth_and_star_never_crosses_a_slash() {
    let set = globs(&["*.ts", "src/*.rs"], &[]).expect("compiles");
    assert!(set.includes(Path::new("/r/a/b/c.ts"), false));
    assert!(set.includes(Path::new("/r/src/lib.rs"), false));
    assert!(!set.includes(Path::new("/r/src/deep/lib.rs"), false));
    assert!(!set.includes(Path::new("/r/lib.rs"), false));
}

#[test]
fn braces_classes_and_double_stars_follow_gitignore() {
    let set = globs(&["packages/{react,react-dom}/src/**", "*.[jt]s"], &[]).expect("compiles");
    assert!(set.includes(Path::new("/r/packages/react/src/a/b.js"), false));
    assert!(set.includes(Path::new("/r/packages/react-dom/src/x.js"), false));
    assert!(!set.includes(Path::new("/r/packages/scheduler/src/x.c"), false));
    assert!(set.includes(Path::new("/r/deep/a.ts"), false));
    assert!(!set.includes(Path::new("/r/deep/a.cs"), false));
}

#[test]
fn excludes_skip_names_paths_and_directories() {
    let set = globs(&[], &["vendor", "!*.min.js", "src/gen/**"]).expect("compiles");
    assert!(set.excludes(Path::new("/r/a/vendor"), true));
    assert!(set.excludes(Path::new("/r/a/b.min.js"), false));
    assert!(set.excludes(Path::new("/r/src/gen/deep/b.ts"), false));
    assert!(!set.excludes(Path::new("/r/src/a.ts"), false));
    assert!(set.includes(Path::new("/r/src/a.ts"), false));
}

#[test]
fn an_invalid_glob_is_an_invalid_glob_error() {
    for (include, exclude) in [
        (&["[.ts"][..], &[][..]),
        (&[], &["{a,b"]),
        (&["a/**/[z-a]"], &[]),
    ] {
        let error = globs(include, exclude).expect_err("invalid");
        assert_eq!(*error.kind(), ErrorKind::InvalidGlob, "{error}");
    }
    let error = set(Path::new("/r"), ["["].into_iter(), "names").expect_err("invalid");
    assert!(
        error.reason.starts_with("invalid names glob '['"),
        "{error}"
    );
}
