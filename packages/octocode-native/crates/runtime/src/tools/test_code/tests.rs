use super::rust_source::{first_gated_byte, module_name};
use super::*;
use crate::tools::test_support::{Fixture, workspace_policy};
use octocode_engine::types::TestCodeExcluded;

mod mounts;

/// A named crate shape: its files (path, text) and the file it asks about.
type Shape = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static str,
);

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

/// A `test_*` Rust module and the files it mounts are production code: the
/// `test_` stem is a Python convention, so it must not cascade through the
/// mount index.
#[test]
fn a_test_named_rust_module_mounted_from_production_is_production() {
    let root = Fixture::new();
    let index = index(&root);
    root.write("src/lib.rs", "mod test_code;\n");
    let parent = root.write("src/test_code.rs", "mod child;\nfn a() {}\n");
    let child = root.write("src/test_code/child.rs", "fn b() {}\n");
    assert!(index.spans(&parent).is_empty());
    assert!(index.spans(&child).is_empty());
    let python = root.write("py/test_foo.py", "x\n");
    assert_eq!(*index.spans(&python), vec![WHOLE_FILE]);
}

/// A Rust module a production root mounts without `#[cfg(test)]` is
/// production whatever its file or directory is called: the mount decides
/// for a file the index can place, the spelling only for one it cannot.
#[test]
fn a_mounted_production_module_is_production_whatever_it_is_called() {
    let root = Fixture::new();
    let index = index(&root);
    root.write("src/lib.rs", "mod foo_test;\nmod tests;\nmod spec;\n");
    for module in ["src/foo_test.rs", "src/tests.rs", "src/spec/mod.rs"] {
        let path = root.write(module, "fn run() {}\n");
        assert!(index.spans(&path).is_empty(), "{module}");
        assert!(!index.test_path_unread(&path), "{module} by name");
    }
}

/// The inverse: spelling still marks a file nothing mounts, a gated mount,
/// a `tests/` directory and Cargo's test targets.
#[test]
fn spelling_mounts_and_cargo_targets_still_mark_test_code() {
    let root = Fixture::new();
    let index = index(&root);
    root.write(
        "Cargo.toml",
        "[package]\nname = \"c\"\nedition = \"2021\"\n",
    );
    root.write("src/lib.rs", "#[cfg(test)]\nmod tests;\nmod plain;\n");
    root.write("src/plain.rs", "fn p() {}\n");
    for test in [
        "src/tests.rs",
        "src/orphan_test.rs",
        "tests/it.rs",
        "tests/common/mod.rs",
    ] {
        let path = root.write(test, "fn t() {}\n");
        assert_eq!(*index.spans(&path), vec![WHOLE_FILE], "{test}");
    }
}

/// A file a plain build compiles through any parent is production: test code
/// only when every mount of it is test. Both orders of the parents, a gated and
/// an ungated mount in one parent, and a spelled name all hold.
#[test]
fn a_file_mounted_in_production_by_any_parent_is_production() {
    let root = Fixture::new();
    let index = index(&root);
    // A test parent found first (the module root), a production one after.
    root.write("src/other.rs", "#[path = \"first.rs\"]\nmod f;\n");
    // A production parent found first (the module root), a test one after.
    root.write(
        "src/tp.rs",
        "#[cfg(test)]\n#[path = \"prod_first.rs\"]\nmod t;\n",
    );
    root.write("src/prod_first.rs", "fn f() {}\n");
    root.write(
        "src/lib.rs",
        "mod other;\n#[cfg(test)]\nmod first;\n#[cfg(test)]\n#[path = \"twice_test.rs\"]\nmod a;\n#[path = \"twice_test.rs\"]\nmod b;\nmod plain_only;\nmod prod_first;\n",
    );
    for (file, test) in [
        ("src/first.rs", false),
        ("src/twice_test.rs", false),
        ("src/plain_only.rs", false),
        ("src/prod_first.rs", false),
    ] {
        let path = root.write(file, "fn f() {}\n");
        assert_eq!(index.spans(&path).is_empty(), !test, "{file}");
    }
    let only = root.write("src/test_only.rs", "fn f() {}\n");
    root.write("src/lib.rs", "#[cfg(test)]\nmod test_only;\n");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    assert_eq!(*index.spans(&only), vec![WHOLE_FILE], "every mount is test");
}

/// A file spelled as test code that a production parent mounts is production,
/// whether that parent is a crate root or a module itself: the parents are
/// decided as exactly as the file is, so the rule holds at any depth, for a
/// walk reading every file and for one asking module roots first (a `#[path]`
/// mount that only the full verdict finds included).
#[test]
fn a_spelled_test_file_mounted_by_a_production_module_is_production() {
    let shapes: [Shape; 5] = [
        (
            "a module mounts tests.rs",
            &[
                ("src/lib.rs", "mod foo;\n"),
                ("src/foo.rs", "mod tests;\n"),
                ("src/foo/tests.rs", "fn f() {}\n"),
            ],
            "src/foo/tests.rs",
        ),
        (
            "a module mounts x_test.rs",
            &[
                ("src/lib.rs", "mod foo;\n"),
                ("src/foo.rs", "mod x_test;\n"),
                ("src/foo/x_test.rs", "fn f() {}\n"),
            ],
            "src/foo/x_test.rs",
        ),
        (
            "two modules deep",
            &[
                ("src/lib.rs", "mod foo;\n"),
                ("src/foo/mod.rs", "mod bar;\n"),
                ("src/foo/bar.rs", "mod x_test;\n"),
                ("src/foo/bar/x_test.rs", "fn f() {}\n"),
            ],
            "src/foo/bar/x_test.rs",
        ),
        (
            "a file in a spelled directory a module root mounts",
            &[
                ("src/lib.rs", "mod foo;\n"),
                ("src/foo.rs", "mod test;\n"),
                ("src/foo/test/mod.rs", "mod a;\n"),
                ("src/foo/test/a.rs", "fn f() {}\n"),
            ],
            "src/foo/test/a.rs",
        ),
        (
            "a diamond whose production side is a module",
            &[
                ("src/lib.rs", "mod a;\n#[cfg(test)]\nmod b;\n"),
                ("src/a.rs", "#[path = \"shared/tests.rs\"]\nmod c;\n"),
                ("src/b.rs", "#[path = \"shared/tests.rs\"]\nmod c;\n"),
                ("src/shared/tests.rs", "fn f() {}\n"),
            ],
            "src/shared/tests.rs",
        ),
    ];
    for (shape, files, target) in shapes {
        let root = Fixture::new();
        for (file, text) in files {
            root.write(file, text);
        }
        let path = root.0.join(target);
        let index = index(&root);
        assert!(!index.test_path(&path), "{shape}: by every file");
        assert!(!index.spans(&path).contains(&WHOLE_FILE), "{shape}: spans");
        let index = TestCodeIndex::new(&workspace_policy(&root.0));
        assert!(!index.test_path_unread(&path), "{shape}: by module roots");
    }
}

/// The same files, mounted only under test, stay test code.
#[test]
fn a_spelled_test_file_mounted_by_a_test_module_stays_test() {
    let root = Fixture::new();
    root.write("src/lib.rs", "#[cfg(test)]\nmod foo;\n");
    root.write("src/foo.rs", "mod tests;\n");
    let path = root.write("src/foo/tests.rs", "fn f() {}\n");
    let index = index(&root);
    assert!(index.test_path(&path));
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    assert!(index.test_path_unread(&path));
}

/// A `#[path]` cycle of spelled files is placed by the production root that
/// mounts it: a parent still being decided is no evidence, and never falls
/// back to its spelling.
#[test]
fn a_cycle_of_spelled_files_is_placed_by_its_root() {
    let root = Fixture::new();
    root.write("src/lib.rs", "mod a_test;\n");
    let a = root.write("src/a_test.rs", "#[path = \"b_test.rs\"]\nmod b;\n");
    let b = root.write("src/b_test.rs", "#[path = \"a_test.rs\"]\nmod a;\n");
    let index = index(&root);
    assert!(index.spans(&a).is_empty() && index.spans(&b).is_empty());
    // The root's own `mod a_test;` places `a_test.rs` in production by name,
    // and the full verdict, which also reads the `#[path]` cycle, agrees.
    let by_name = TestCodeIndex::new(&workspace_policy(&root.0));
    assert_eq!(by_name.placement_by_name(&a), Placement::Production);
    assert!(!index.test_path(&a) && !by_name.test_path_unread(&a));
}

/// A Rust file is read from disk once per request, whichever verdict asks:
/// the directory scan for `#[path]`, a parent's mounts, the spans of a file.
#[test]
fn a_rust_file_is_read_once_per_request() {
    let root = Fixture::new();
    root.write("src/lib.rs", "mod a;\n#[path = \"x/b.rs\"]\nmod b;\n");
    let a = root.write("src/a.rs", "#[cfg(test)]\nmod t {}\nfn f() {}\n");
    let b = root.write("src/x/b.rs", "mod c;\n");
    let c = root.write("src/x/b/c.rs", "fn g() {}\n");
    let tests = root.write("src/tests.rs", "fn h() {}\n");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    for file in [&a, &b, &c, &tests] {
        index.test_path_unread(file);
        index.test_path(file);
        index.spans(file);
        index.test_file(file);
    }
    assert_eq!(index.read_twice(), Vec::<PathBuf>::new());
}

/// Spelled children cost no more parses than unspelled ones: a parent is
/// parsed once, however many files of it are asked about.
#[test]
fn a_parent_is_parsed_once_for_spelled_children() {
    let root = Fixture::new();
    let mut gated = String::new();
    let mut files = Vec::new();
    for module in 0..48 {
        gated.push_str(&format!("#[cfg(test)]\nmod m{module}_test;\n"));
        files.push(root.write(&format!("gated/m{module}_test.rs"), "pub fn f() {}\n"));
    }
    root.write("gated/lib.rs", gated);
    root.write("open/lib.rs", "mod a_test;\n");
    let open = root.write("open/a_test.rs", "pub fn f() {}\n");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    for file in &files {
        assert!(index.test_path(file), "{}", file.display());
    }
    assert_eq!(index.parents_parsed(), 1, "one gated parent");
    assert!(!index.test_path(&open));
    assert_eq!(index.parents_parsed(), 2, "one parse per parent");
}

/// A walk's verdicts place every file it lists by what its parents mount, so
/// they read each parent through the linear scan: no declarations parse runs
/// for a parent however many parents and children the tree has. Only a file
/// whose own spans are asked is parsed.
#[test]
fn placing_files_by_their_parents_parses_no_parent() {
    for parents in [4, 16, 64] {
        let root = Fixture::new();
        let mut files = Vec::new();
        for parent in 0..parents {
            root.write(
                &format!("c{parent}/lib.rs"),
                "#[cfg(test)]\nmod a_test;\nmod b_test;\nmod plain;\n",
            );
            for module in ["a_test", "b_test", "plain"] {
                files.push(root.write(&format!("c{parent}/{module}.rs"), "pub fn f() {}\n"));
            }
        }
        let index = TestCodeIndex::new(&workspace_policy(&root.0));
        let before = parses();
        let placed = files
            .iter()
            .map(|file| (index.test_path_unread(file), index.test_path(file)))
            .collect::<Vec<_>>();
        assert_eq!(parses(), before, "{parents} parents: no parent is parsed");
        for (at, verdicts) in placed.chunks(3).enumerate() {
            assert_eq!(
                verdicts,
                [(true, true), (false, false), (false, false)],
                "parent {at}"
            );
        }
    }
}

/// Each parent's text is scanned for the modules it names once, however many
/// test-spelled children of its directory ask: the lookups share one pass.
#[test]
fn a_parent_text_is_scanned_once_for_many_children() {
    let root = Fixture::new();
    let mut declared = String::new();
    let mut files = Vec::new();
    for module in 0..48 {
        declared.push_str(&format!("mod m{module}_test;\n"));
        files.push(root.write(&format!("src/m{module}_test.rs"), "pub fn f() {}\n"));
    }
    root.write("src/lib.rs", declared);
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    for file in &files {
        // Mounted by a plain `mod` of the root: production, whatever the name.
        assert!(!index.test_path_unread(file), "{}", file.display());
    }
    let parents = index.heads_scanned();
    assert!(parents > 0, "the children asked their parents");
    for file in &files {
        index.test_path_unread(file);
        index.test_path(file);
    }
    assert_eq!(index.heads_scanned(), parents, "no later ask rescans");
    assert!(
        parents < files.len(),
        "fewer scans than children: {parents} for {}",
        files.len()
    );
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

/// A well-formed file's test spans come from the linear scan alone: no
/// declarations parse runs for them, and they are the spans the parse reports.
/// Only a text the scan finds malformed asks the parse.
#[test]
fn test_spans_of_a_well_formed_file_need_no_declarations_parse() {
    let root = Fixture::new();
    let index = index(&root);
    for (at, (text, malformed)) in [
        (
            "pub fn spanned_by_the_scan() {}\n\n#[cfg(test)]\nfn helper() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n",
            false,
        ),
        ("#![cfg(test)]\nfn whole_by_the_scan() {}\n", false),
        ("pub fn f() {}\n#[cfg(test)]\nmod tests {\n", true),
    ]
    .into_iter()
    .enumerate()
    {
        let path = root.write(&format!("src/spanned{at}.rs"), text);
        assert_eq!(rust_scan::scan(text).malformed, malformed, "{text:?}");
        let before = declaration_parses();
        let spans = index.spans(&path);
        assert_eq!(
            declaration_parses() - before,
            usize::from(malformed),
            "{text:?}: declarations parses"
        );
        let parsed = TestCodeIndex::parse(&path, text).expect("the parse accepts the text");
        assert_eq!(*spans, whole_or(parsed.test_lines.clone(), text), "{text:?}");
        assert!(!spans.is_empty(), "{text:?}");
    }
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
        TestCodeExcluded::default(),
        "a listing counts itself"
    );
}

/// The text check before a parent's parse starts on the line a gate can first
/// touch, and is safe on any text.
#[test]
fn the_text_check_starts_where_a_gate_can_first_reach() {
    let from = |content: &str| first_gated_byte(content);
    assert_eq!(from("mod a;\nmod b;\n"), None);
    assert_eq!(from("#[cfg(unix)]\nmod a;\n"), None);
    assert_eq!(from("#[cfg_attr(test, allow(unused))]\nmod a;\n"), None);
    assert_eq!(from("#[test]\nfn t() {}\n#[cfg(unix)]\nmod a;\n"), None);
    assert_eq!(from("mod a;\n#[cfg(test)]\nmod b;\n"), Some(7));
    assert_eq!(from("mod a; #[cfg(test)]\nmod b;\n"), Some(0));
    assert_eq!(from("mod a;\n#[cfg(\ntest)]\nmod b;\n"), Some(14));
    assert_eq!(from("#[cfg(unix)]\nmod a;\n#[cfg(test)]\n"), Some(20));
    assert_eq!(from("mod a;\n#![cfg(test)]\n"), Some(0));
    assert_eq!(from("mod a;\n#![ cfg (not(test))]\n"), Some(0));
    for broken in [
        "\"unterminated",
        "/* open",
        "'",
        "'\\",
        "r#\"open",
        "é",
        "écfg é test é",
        "#![",
        "#![é",
    ] {
        let _ = from(broken);
    }
}

/// However many verdicts ask about a path, and whether the proof stamps it
/// after, the request stats it at most once; the proof still holds until a
/// watched file changes.
#[test]
fn a_request_stats_each_path_at_most_once() {
    let root = Fixture::new();
    let index = index(&root);
    root.write("Cargo.toml", "[package]\n");
    root.write("src/lib.rs", "mod a;\nmod b;\n#[cfg(test)]\nmod tests;\n");
    root.write("src/tests.rs", "fn t() {}\n");
    root.write("src/a/mod.rs", "mod c;\n");
    let files = [
        root.write("src/a/c.rs", "fn c() {}\n"),
        root.write("src/b.rs", "fn b() {}\n"),
        root.0.join("src/tests.rs"),
    ];
    for _ in 0..2 {
        for file in &files {
            index.test_path_unread(file);
            index.test_path(file);
            index.test_dir(file.parent().unwrap());
        }
    }
    let started = SystemTime::now() + octocode_engine::stamp::SourceStamp::SETTLE;
    let proof = index.proof(started).expect("settled files prove");
    assert!(proof.holds());
    let stats = lock(&index.stats);
    assert!(!stats.is_empty());
    for (path, count) in stats.iter() {
        assert!(*count <= 1, "{} stat {count} times", path.display());
    }
    drop(stats);
    std::fs::write(root.0.join("src/b.rs"), "fn changed_size() {}\n").unwrap();
    assert!(!proof.holds());
}

/// Where a parent's mounts land as `mount_facts` places them: each mounted
/// file, its line, and whether test code holds it.
fn mount_placements(parsed: &Parsed) -> Vec<(PathBuf, u32, bool)> {
    let mut placed = parsed
        .modules
        .iter()
        .map(|mount| {
            let gated = mount.under_test || in_spans(&parsed.test_lines, mount.line);
            (mount.path.clone(), mount.line, gated)
        })
        .collect::<Vec<_>>();
    placed.sort();
    placed
}

/// The placements of `content` as its declarations parse reads it; `None`
/// when the parse refuses the text.
fn parsed_placements(
    path: &Path,
    content: &str,
    crate_root: bool,
) -> Option<Vec<(PathBuf, u32, bool)>> {
    let document = TestCodeIndex::parse(path, content)?;
    Some(mount_placements(&Parsed {
        test_lines: document.test_lines.clone(),
        modules: mount_targets::file_modules(path, &document.modules, crate_root),
    }))
}

/// The placements of `content` as the linear scan reads it.
fn scanned_placements(path: &Path, content: &str, crate_root: bool) -> Vec<(PathBuf, u32, bool)> {
    let facts = rust_scan::scan(content);
    mount_placements(&Parsed {
        test_lines: facts.test_lines,
        modules: mount_targets::file_modules(path, &facts.modules, crate_root),
    })
}

/// A parent the scan cannot read well (an unclosed delimiter, a stray closer)
/// places its mounts as the parse does: the fallback asks the parse.
#[test]
fn a_malformed_scan_places_its_parent_as_the_parse_does() {
    let root = Fixture::new();
    let index = index(&root);
    for (at, text) in [
        "mod a;\n#[cfg(test)]\nmod b {\n",
        "mod a;\n}\n#[cfg(test)]\nmod b;\n",
        "#[cfg(test)]\nmod b;\nfn f() { let s = \"unclosed;\n",
    ]
    .into_iter()
    .enumerate()
    {
        let path = root.write(&format!("src/parent{at}.rs"), text);
        assert!(rust_scan::scan(text).malformed, "{text:?}");
        let parsed = parsed_placements(&path, text, index.crate_target(&path).is_some())
            .expect("the parse accepts the text");
        assert_eq!(
            mount_placements(&index.mount_facts(&path, text)),
            parsed,
            "{text:?}"
        );
    }
}

/// Every head text and each of `files` scans to the mounts its parse reports,
/// as a parent of each crate shape.
fn assert_texts_scan_to_their_parse(files: Vec<PathBuf>) {
    let mut texts = module_heads::TEXTS
        .iter()
        .map(|text| ((*text).to_owned(), Path::new("src/parent.rs").to_path_buf()))
        .collect::<Vec<_>>();
    for file in files {
        if let Ok(content) = std::fs::read_to_string(&file) {
            texts.push((content, file));
        }
    }
    let mut differ = Vec::new();
    for (content, path) in &texts {
        for crate_root in [false, true] {
            // A text the parse refuses is read by the scan alone, as `facts`
            // reads it: there is no parse to equal.
            let Some(parsed) = parsed_placements(path, content, crate_root) else {
                continue;
            };
            let scanned = scanned_placements(path, content, crate_root);
            if scanned != parsed {
                differ.push(format!(
                    "{} crate_root {crate_root}\nparsed {parsed:?}\nscanned {scanned:?}",
                    path.display()
                ));
            }
        }
    }
    assert!(differ.is_empty(), "{}", differ.join("\n\n"));
}

/// Every head text and a sample of this repository's Rust files
/// ([`mounts::real_source_sample`]) scan to the mounts their parse reports.
#[test]
fn every_text_and_sampled_file_scans_to_the_mounts_its_parse_reports() {
    assert_texts_scan_to_their_parse(mounts::real_source_sample());
}

/// The same on every Rust file of this repository.
#[test]
#[ignore = "full tree: cargo test -- --ignored full_tree"]
fn every_text_scans_to_the_mounts_its_parse_reports_full_tree() {
    assert_texts_scan_to_their_parse(mounts::rust_files(mounts::real_source_root()));
}

/// A raw `r#mod` is an identifier, not the `mod` keyword: it is no head, and a
/// mount beside it places as the parse does.
#[test]
fn a_raw_mod_identifier_is_covered_as_the_parse_reads_it() {
    let path = Path::new("src/parent.rs");
    for text in [
        "let r#mod = 1;\n#[cfg(test)]\nmod b;\n",
        "mod r#mod;\n#[cfg(test)]\nmod r#if;\n",
        "#[cfg(test)]\nmod r#mod {\n    mod r#a;\n}\n",
        "mod r#type;\nfn r#mod() {}\nmod c;\n",
    ] {
        for crate_root in [false, true] {
            let parsed =
                parsed_placements(path, text, crate_root).expect("the parse accepts the text");
            assert_eq!(
                scanned_placements(path, text, crate_root),
                parsed,
                "{text:?}"
            );
        }
    }
}
