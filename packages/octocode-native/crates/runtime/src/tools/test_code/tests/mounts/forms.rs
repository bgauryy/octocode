//! The forms a mount and a test item take past the plain `mod name;` of a small
//! parent: too large to parse, inside an inline module, renamed by `#[path]`.
use super::*;
use octocode_engine::graph::GraphFactRustModule;

/// A test item in a file past the parse limit is test code, and the rest of
/// the file is not.
#[test]
fn a_file_too_large_to_parse_keeps_its_test_items() {
    let root = Fixture::new();
    let padding = "// padding\n".repeat(crate::security::SourceLimits::REAL.whole_source / 11 + 1);
    let big = root.write(
        "src/big.rs",
        format!("pub fn run() {{}}\n{padding}#[cfg(test)]\nmod tests {{\n    fn t() {{}}\n}}\n"),
    );
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    let last = u32::try_from(padding.lines().count()).expect("lines") + 2;
    // The item's span starts at its `#[cfg(test)]` line.
    assert_eq!(*index.spans(&big), vec![[last, last + 3]]);
    assert!(!index.is_test_line(&big, 1));
    assert!(!index.is_test_line(&big, last - 1));
    assert!(index.is_test_line(&big, last));
    assert!(index.is_test_line(&big, last + 2));
    let whole = root.write(
        "src/whole.rs",
        format!("{padding}#![cfg(test)]\nfn t() {{}}\n"),
    );
    assert!(index.test_file(&whole));
}

/// A parent past the parse limit mounts its modules as a small one does.
#[test]
fn a_parent_too_large_to_parse_mounts_under_test() {
    let root = Fixture::new();
    let padding = "// padding\n".repeat(crate::security::SourceLimits::REAL.whole_source / 11 + 1);
    root.write(
        "src/lib.rs",
        format!("{padding}mod open;\n#[cfg(test)]\nmod gated;\n"),
    );
    let open = root.write("src/open.rs", "pub fn f() {}\n");
    let gated = root.write("src/gated.rs", "pub fn f() {}\n");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    assert!(index.test_path(&gated));
    assert!(!index.test_path(&open));
}

/// A file module of a gated inline module lies in that module's directory, as
/// the compiler maps it: below a `mod.rs`-like parent directly, below a named
/// file's own directory otherwise.
#[test]
fn a_file_module_inside_a_gated_inline_module_is_test_code() {
    let root = Fixture::new();
    root.write(
        "src/lib.rs",
        "#[cfg(test)]\nmod a {\n    mod inner;\n    mod deeper {\n        mod leaf;\n    }\n}\nmod open {\n    mod plain;\n    #[cfg(test)]\n    mod gated;\n}\n",
    );
    root.write("src/named.rs", "#[cfg(test)]\nmod b {\n    mod under;\n}\n");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    for test in [
        "src/a/inner.rs",
        "src/a/inner/mod.rs",
        "src/a/deeper/leaf.rs",
        "src/open/gated.rs",
        "src/named/b/under.rs",
    ] {
        let file = root.write(test, "pub fn f() {}\n");
        assert!(index.test_path(&file), "{test}");
    }
    for production in ["src/open/plain.rs", "src/inner.rs", "src/b/under.rs"] {
        let file = root.write(production, "pub fn f() {}\n");
        assert!(!index.test_path(&file), "{production}");
    }
}

/// A higher parent reaches a file by name only through inline modules it opens
/// for the directories between: one that merely mentions them or the file's
/// name is never parsed for it, and trivia inside a head still opens one.
#[test]
fn a_parent_above_is_parsed_only_when_it_opens_the_way_down() {
    let root = Fixture::new();
    root.write(
        "src/lib.rs",
        "// the deep module\n#[cfg(test)]\nuse deep::inner;\nmod deep;\nconst INNER: &str = \"mod inner\";\n",
    );
    root.write("src/deep.rs", "pub mod inner;\n");
    let inner = root.write("src/deep/inner.rs", "pub fn f() {}\n");
    root.write("src/other/mod.rs", "// mod far;\nuse crate::far;\n");
    let far = root.write("src/other/more/far.rs", "pub fn f() {}\n");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    assert!(!index.test_path(&far));
    assert_eq!(index.parents_parsed(), 0, "no parent opens src/other/more");
    assert!(!index.test_path(&inner));
    assert_same_mounts(&root.0);

    root.write(
        "src/lib.rs",
        "#[cfg(test)]\nmod /* c */ deep {\n    mod // d\n    r#inner;\n}\n",
    );
    assert!(TestCodeIndex::new(&workspace_policy(&root.0)).test_path(&inner));
    assert_same_mounts(&root.0);
}

/// What a directory's scan for `#[path]` may read is what a single read may: a
/// withheld file or a link out of the workspace declares nothing, and neither
/// does a file that is not text.
#[test]
fn a_path_attribute_counts_only_in_a_file_a_read_may_open() {
    let root = Fixture::new();
    let declares = "#[cfg(test)]\n#[path = \"target_x.rs\"]\nmod target;\n";
    root.write("src/lib.rs", "pub fn f() {}\n");
    let target = root.write("src/target_x.rs", "pub fn f() {}\n");
    root.write("src/.env.rs", declares);
    let outside = Fixture::new();
    let linked = outside.write("elsewhere.rs", declares);
    std::os::unix::fs::symlink(&linked, root.0.join("src/linked.rs")).expect("link");
    let mut broken = declares.as_bytes().to_vec();
    broken.push(0xff);
    std::fs::write(root.0.join("src/broken.rs"), broken).expect("write");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    assert!(!index.test_path(&target));
    assert!(index.unread().is_empty(), "{:?}", index.unread());

    let plain = root.write("src/plain.rs", declares);
    assert!(TestCodeIndex::new(&workspace_policy(&root.0)).test_path(&target));
    std::fs::remove_file(plain).expect("remove");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    assert!(!index.test_path(&target));
}

/// The bytes that rule a file out of declaring a `#[path]` keep every spelling
/// the compiler lexes as one, and a non-ASCII blank beside the word.
#[test]
fn the_path_spelling_check_keeps_every_form_of_the_attribute() {
    for spelled in [
        "#[path = \"x.rs\"]",
        "#[path=\"x.rs\"]",
        "#[ path\t=\n\"x.rs\"]",
        "#[\u{2028}path\u{2029}= \"x.rs\"]",
        "#[\u{0b}path\u{0b}= \"x.rs\"]",
        "#[cfg_attr(test, path = \"x.rs\")]",
    ] {
        assert!(
            super::super::rust_source::spells_path_attribute(spelled.as_bytes()),
            "{spelled}"
        );
    }
    for other in [
        "let path = 1;",
        "#[xpath = \"x\"]",
        "#[path_x = \"x\"]",
        "f(path);",
        "[path, x]",
        "",
    ] {
        assert!(
            !super::super::rust_source::spells_path_attribute(other.as_bytes()),
            "{other}"
        );
    }
}

/// `#[path]` names the file a module mounts, from the declaring file's
/// directory: that file is test code, the module's default file is not. It may
/// be declared by any file, not just a directory's module root.
#[test]
fn a_path_attribute_decides_which_file_a_gate_mounts() {
    let root = Fixture::new();
    root.write(
        "src/lib.rs",
        "#[cfg(test)]\n#[path = \"helpers_x.rs\"]\nmod fixtures;\n#[path = \"open_x.rs\"]\nmod open;\n#[cfg(test)]\n#[path = \"sub/below.rs\"]\nmod below;\n#[cfg(test)]\n#[path = \"dir_x\"]\nmod inline {\n    mod in_dir;\n    #[path = \"renamed.rs\"]\n    mod other;\n}\n",
    );
    root.write(
        "src/util/plain.rs",
        "#[cfg(test)]\n#[path = \"plain_cases.rs\"]\nmod cases;\n#[cfg(test)]\nmod tests {\n    #[path = \"nested_case.rs\"]\n    mod nested;\n}\n",
    );
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    for test in [
        "src/helpers_x.rs",
        "src/sub/below.rs",
        "src/dir_x/in_dir.rs",
        "src/dir_x/renamed.rs",
        "src/util/plain_cases.rs",
        "src/util/plain/tests/nested_case.rs",
    ] {
        let file = root.write(test, "pub fn f() {}\n");
        assert!(index.test_path(&file), "{test}");
    }
    for production in [
        "src/fixtures.rs",
        "src/open_x.rs",
        "src/open.rs",
        "src/inline/in_dir.rs",
        "src/util/nested_case.rs",
        "src/util/cases.rs",
    ] {
        let file = root.write(production, "pub fn f() {}\n");
        assert!(!index.test_path(&file), "{production}");
    }
}

/// What a test-only `#[path]` file mounts is test code too.
#[test]
fn a_module_of_a_path_mounted_file_inherits_test_code() {
    let root = Fixture::new();
    root.write("src/lib.rs", "#[cfg(test)]\n#[path = \"h.rs\"]\nmod h;\n");
    root.write(
        "src/h.rs",
        "mod deep;\n#[path = \"beside.rs\"]\nmod near;\n",
    );
    let deep = root.write("src/h/deep.rs", "pub fn f() {}\n");
    let beside = root.write("src/beside.rs", "pub fn f() {}\n");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    assert!(index.test_path(&deep) && index.test_path(&beside));
}

/// Files that name each other by `#[path]` end the walk, on every thread.
#[test]
fn path_attributes_in_a_cycle_end_the_walk() {
    let root = Fixture::new();
    let a = root.write("src/a.rs", "#[path = \"b.rs\"]\nmod b;\n");
    let b = root.write("src/b.rs", "#[path = \"a.rs\"]\nmod a;\n");
    let c = root.write("src/c.rs", "#[path = \"c.rs\"]\nmod c;\n");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|threads| {
        for _ in 0..4 {
            threads.spawn(|| {
                barrier.wait();
                for file in [&a, &b, &c] {
                    assert!(!index.test_path(file), "{}", file.display());
                }
            });
        }
    });
}

/// What a parse and a scan of `content` each say, in one order: spans, then each
/// module's name, line, scope, whether inline, and `#[path]`.
type Facts = (
    Vec<[u32; 2]>,
    Vec<(String, u32, Vec<String>, bool, Option<String>)>,
);

fn facts(mut spans: Vec<[u32; 2]>, modules: &[GraphFactRustModule], content: &str) -> Facts {
    spans = whole_or(spans, content);
    spans.sort_unstable();
    // The parse reports a comment or attribute under a gate as its own span.
    spans.dedup_by(|next, kept| {
        let touches = next[0] <= kept[1].saturating_add(1);
        if touches {
            kept[1] = kept[1].max(next[1]);
        }
        touches
    });
    let mut modules = modules
        .iter()
        .map(|module| {
            (
                module.name.clone(),
                module.line,
                module.scope.clone(),
                module.inline,
                module.path.clone(),
            )
        })
        .collect::<Vec<_>>();
    modules.sort();
    (spans, modules)
}

/// `content` scanned as it parses; `None` when it does.
fn disagreement(content: &str) -> Option<String> {
    let document = crate::tools::declarations::file_document(content, "a.rs")?;
    let parsed = facts(document.test_lines.clone(), &document.modules, content);
    let scanned = rust_scan::scan(content);
    let scanned = facts(scanned.test_lines, &scanned.modules, content);
    (parsed != scanned).then(|| format!("parsed {parsed:?}\nscanned {scanned:?}"))
}

/// The scan reads what the parse reads, on this repository's own source: every
/// test span and every `mod`, with its scope and `#[path]`.
#[test]
fn every_file_of_real_source_scans_as_it_parses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates directory");
    let disagreements = rust_files(root)
        .into_iter()
        .filter_map(|file| {
            let content = std::fs::read_to_string(&file).ok()?;
            let found = disagreement(&content)?;
            Some(format!("{}\n{found}", file.display()))
        })
        .collect::<Vec<_>>();
    assert!(
        disagreements.is_empty(),
        "{} files\n{}",
        disagreements.len(),
        disagreements[..disagreements.len().min(12)].join("\n\n")
    );
}

/// Forms a scan could misread, each as the parse reads them.
#[test]
fn every_item_form_scans_as_it_parses() {
    for content in [
        "mod a;\n#[cfg(test)]\nmod b;\n",
        // The attribute run belongs to its item, from the first attribute that
        // begins its line; one after other code leaves that line to the code.
        "#[derive(Debug)]\n// note\n#[cfg(test)]\nstruct S;\nfn g() {}\n",
        "fn g() {} #[cfg(test)]\nfn t() {}\n",
        "fn g() {} #[derive(Debug)]\n#[cfg(test)]\nstruct S;\n",
        "struct S {\n    a: u8,\n    #[cfg(test)]\n    b: u8,\n}\n",
        "#[cfg(test)]\npub(crate) mod a;\n",
        "#[cfg(\n  test\n)]\nmod\n  a;\n",
        "#![cfg(test)]\nfn t() {}\n",
        "//! Docs.\n#![cfg(test)]\nmod a;\n",
        "#![ cfg (test)]\nfn t() {}\n",
        "# ! [cfg(test)]\nfn t() {}\n",
        "#![/*x*/cfg(test)]\nfn t() {}\n",
        "#![cfg(not(test))]\nmod a;\n",
        "#[cfg(any(test, unix))]\nmod a;\n",
        "#[cfg(all(unix, test))]\nmod a;\n",
        "#[cfg_attr(test, allow(unused))]\nmod a;\n",
        "#[cfg(test)]\n/// doc; {\n/** more; { */\nmod a;\n",
        "#[cfg(test)]\n#[foo({ x; })]\nmod a;\n",
        "#[path = \"b.rs\"]\n#[cfg(test)]\nmod a;\n",
        "#[cfg(test)]\n#[path = \"dir\"]\nmod a {\n    #[path = \"c.rs\"]\n    mod b;\n}\n",
        "mod a {\n    #![cfg(test)]\n    mod b;\n}\n",
        "mod a {\n    mod b {\n        #[cfg(test)]\n        mod c;\n    }\n}\n",
        "fn f() {\n    #![cfg(test)]\n    mod a;\n}\n",
        "#[cfg(test)]\nfn f() {\n    mod a;\n}\n",
        "#[cfg(test)]\nconst X: T = T {\n    a: 1,\n};\nfn g() {}\n",
        "#[cfg(test)]\nconst fn f() {\n}\nfn g() {}\n",
        "#[cfg(test)]\npub(crate) const X: T = T {\n    a: 1,\n};\nfn g() {}\n",
        "#[cfg(test)]\npub(super) struct S<A, B> {\n    a: A,\n}\nfn g() {}\n",
        "#[cfg(test)]\nstatic X: [u8; 2] = [\n    1, 2,\n];\nfn g() {}\n",
        "#[cfg(test)]\nuse a::{\n    b,\n    c,\n};\nfn g() {}\n",
        "#[cfg(test)]\nfn f<A, B>(a: A, b: B) -> [u8; 2]\nwhere\n    A: X,\n{\n}\nfn g() {}\n",
        "#[cfg(test)]\nstruct S<A, B>(A, B);\nfn g() {}\n",
        "#[cfg(test)]\nimpl<A, B> T<A, B> {\n    fn f() {}\n}\nfn g() {}\n",
        "struct S {\n    #[cfg(test)]\n    a: Vec<u8, A>,\n    b: u8,\n}\n",
        "enum E {\n    #[cfg(test)]\n    A {\n        x: u8,\n    },\n    B,\n}\n",
        "fn f() {\n    #[cfg(test)]\n    let x = if a {\n        1\n    } else {\n        2\n    };\n    g();\n}\n",
        "fn f() {\n    #[cfg(test)]\n    if a {\n        b();\n    } else {\n        c();\n    }\n    g();\n}\n",
        "fn f() {\n    #[cfg(test)]\n    {\n        a();\n    }\n    g();\n}\n",
        "fn f() {\n    #[cfg(test)]\n    a(\n        b,\n    );\n    g();\n}\n",
        "#[cfg(test)]\nmacro_rules! m {\n    () => {};\n}\nfn g() {}\n",
        "macro_rules! m {\n    () => {\n        #[cfg(test)]\n        mod a;\n    };\n}\n",
        "fn f() {\n    q! {\n        #[cfg(test)]\n        mod a;\n    };\n}\n",
        "m! {\n    #[cfg(test)]\n    mod a;\n}\n",
        "const S: &str = \"#[cfg(test)]\\nmod a;\";\n",
        "const S: &str = r#\"#[cfg(test)]\nmod a;\"#;\n",
        "const S: &[u8] = br##\"a\"#;{\"##;\n#[cfg(test)]\nmod a;\n",
        "const C: char = '\"';\n#[cfg(test)]\nmod a;\n",
        "const C: char = '\\'';\n#[cfg(test)]\nmod a;\n",
        "const C: char = '😀';\n#[cfg(test)]\nmod a;\n",
        "fn f<'a>(x: &'a str) { 'l: loop { break 'l; } }\n#[cfg(test)]\nmod a;\n",
        "/* /* #[cfg(test)] */ mod a; */\nmod b;\n",
        "#[cfg(test)]\nmod r#a;\n",
    ] {
        assert_eq!(disagreement(content), None, "{content}");
    }
}

/// A `#[cfg(test)]` field, struct-literal field or statement written over
/// several lines is test code to its last line, in the parse and in the scan
/// alike: a value continued by an operator or a method call on the next line
/// belongs to the field.
#[test]
fn a_multi_line_test_field_or_statement_spans_to_its_last_line() {
    for (content, span) in [
        // Struct-literal fields: a binary operator, a method chain, a nested
        // literal, a comparison, after code on the attribute's line.
        (
            "fn f() -> P {\n    P {\n        a: 1,\n        #[cfg(test)]\n        b: x\n            - y,\n        c: 2,\n    }\n}\n",
            [4, 6],
        ),
        (
            "fn f() -> P {\n    P {\n        #[cfg(test)]\n        b: x\n            .f()\n            .g(),\n        c: 2,\n    }\n}\n",
            [3, 6],
        ),
        (
            "fn f() -> P {\n    P {\n        #[cfg(test)]\n        b: Q {\n            x: 1,\n        }\n        .f(),\n        c: 2,\n    }\n}\n",
            [3, 7],
        ),
        (
            "fn f() -> P {\n    P {\n        #[cfg(test)]\n        b: x\n            < y,\n        c: 2,\n    }\n}\n",
            [3, 5],
        ),
        (
            "fn f() -> P {\n    P {\n        a: 1, #[cfg(test)]\n        b: x\n            + y,\n        c: 2,\n    }\n}\n",
            [4, 5],
        ),
        (
            "fn f() -> P {\n    P {\n        #[cfg(test)]\n        // note\n        b,\n        c: 2,\n    }\n}\n",
            [3, 5],
        ),
        // Struct and tuple-struct fields of a type over several lines.
        (
            "struct S {\n    #[cfg(test)]\n    a: HashMap<\n        u8,\n        u8,\n    >,\n    b: u8,\n}\n",
            [2, 6],
        ),
        (
            "struct S(\n    #[cfg(test)]\n    pub Vec<\n        u8,\n    >,\n    u8,\n);\n",
            [2, 5],
        ),
        // Statements and a call argument.
        (
            "fn f() {\n    #[cfg(test)]\n    let x = a\n        + b;\n    g();\n}\n",
            [2, 4],
        ),
        (
            "fn f() {\n    #[cfg(test)]\n    a\n        .f()\n        .g();\n    g();\n}\n",
            [2, 5],
        ),
        (
            "fn f() {\n    #[cfg(test)]\n    x = a\n        - b;\n    g();\n}\n",
            [2, 4],
        ),
        (
            "fn f() {\n    g(\n        #[cfg(test)]\n        a\n            + b,\n        c,\n    );\n}\n",
            [3, 5],
        ),
    ] {
        let document =
            crate::tools::declarations::file_document(content, "a.rs").expect("a Rust parse");
        let parsed = facts(document.test_lines.clone(), &document.modules, content).0;
        let scanned = rust_scan::scan(content);
        let scanned = facts(scanned.test_lines, &scanned.modules, content).0;
        assert_eq!((&parsed, &scanned), (&vec![span], &vec![span]), "{content}");
    }
}

/// Text that is no Rust scans to an end.
#[test]
fn broken_text_scans_to_an_end() {
    for broken in [
        "\"open",
        "/* open",
        "'",
        "'\\",
        "r#\"open",
        "#",
        "#!",
        "#[",
        "#![cfg(test)",
        "}",
        ")(",
        "mod",
        "mod a",
        "#[cfg(test)]",
        "#[cfg(test)] fn f( {",
        "m!",
        "macro_rules!",
        "é",
        "b'",
    ] {
        let _ = rust_scan::scan(broken);
    }
}
