//! The text check before a parent's parse changes no decision: every file is
//! test code by mount exactly as when every parent is parsed.
use super::*;
use crate::tools::test_support::{Fixture, workspace_policy};
use std::cell::RefCell;

mod texts;

/// The mount rule with no shortcut: every candidate parent read and parsed.
struct EveryParentParsed<'a> {
    index: &'a TestCodeIndex,
    mounted: RefCell<HashMap<PathBuf, bool>>,
    names: RefCell<HashMap<PathBuf, Arc<Vec<String>>>>,
}

impl<'a> EveryParentParsed<'a> {
    fn new(index: &'a TestCodeIndex) -> Self {
        Self {
            index,
            mounted: RefCell::default(),
            names: RefCell::default(),
        }
    }

    fn test_path(&self, path: &Path) -> bool {
        self.index.by_spelling(path) || self.mounted_under_test(path)
    }

    fn mounted_under_test(&self, path: &Path) -> bool {
        if !TestCodeIndex::may_hold_test_code(path) {
            return false;
        }
        if let Some(known) = self.mounted.borrow().get(path) {
            return *known;
        }
        let mounted = module_name(path).is_some_and(|name| {
            parents_of(path)
                .iter()
                .any(|parent| self.names(parent).contains(&name))
        });
        self.mounted
            .borrow_mut()
            .insert(path.to_path_buf(), mounted);
        mounted
    }

    fn names(&self, parent: &Path) -> Arc<Vec<String>> {
        if let Some(known) = self.names.borrow().get(parent) {
            return Arc::clone(known);
        }
        let parent_test = self.test_path(parent);
        let names = self
            .index
            .source(parent)
            .and_then(|content| TestCodeIndex::parse(parent, &content))
            .map(|document| {
                document
                    .modules
                    .iter()
                    .filter(|module| !module.inline && module.scope.is_empty())
                    .filter(|module| parent_test || in_spans(&document.test_lines, module.line))
                    .map(|module| module.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let names = Arc::new(names);
        self.names
            .borrow_mut()
            .insert(parent.to_path_buf(), Arc::clone(&names));
        names
    }
}

/// The files that may declare the module `path` defines.
fn parents_of(path: &Path) -> Vec<PathBuf> {
    let dir = path.parent().expect("a parent");
    let dir = if path.file_stem().is_some_and(|stem| stem == "mod") {
        dir.parent().expect("a directory above")
    } else {
        dir
    };
    let mut parents = ["mod.rs", "lib.rs", "main.rs"]
        .into_iter()
        .map(|name| dir.join(name))
        .collect::<Vec<_>>();
    if let (Some(up), Some(name)) = (dir.parent(), dir.file_name()) {
        parents.push(up.join(format!("{}.rs", name.to_string_lossy())));
    }
    parents.retain(|parent| parent != path && parent.is_file());
    parents
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("listing").flatten() {
            let path = entry.path();
            if entry.file_type().expect("type").is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn assert_same_mounts(root: &Path) {
    let policy = workspace_policy(root);
    let (reference, index) = (TestCodeIndex::new(&policy), TestCodeIndex::new(&policy));
    let parsed = EveryParentParsed::new(&reference);
    for file in rust_files(root) {
        assert_eq!(
            index.test_path(&file),
            parsed.test_path(&file),
            "{} under:\n{}",
            file.display(),
            std::fs::read_to_string(root.join("src/lib.rs")).unwrap_or_default()
        );
    }
}

/// Whether a file is test code whole, decided as when it is always parsed.
fn assert_same_whole_file(root: &Path, path: &Path) {
    let policy = workspace_policy(root);
    let (reference, index) = (TestCodeIndex::new(&policy), TestCodeIndex::new(&policy));
    let parsed = EveryParentParsed::new(&reference);
    let whole = parsed.test_path(path)
        || reference.source(path).is_some_and(|content| {
            TestCodeIndex::parse(path, &content).is_some_and(|document| {
                whole_or(document.test_lines.clone(), &content) == [WHOLE_FILE]
            })
        });
    assert_eq!(
        index.test_file(path),
        whole,
        "{}",
        std::fs::read_to_string(path).unwrap_or_default()
    );
}

/// This repository's own source, every file.
#[test]
fn every_file_of_real_source_is_decided_as_when_every_parent_is_parsed() {
    assert_same_mounts(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the crates directory"),
    );
}

/// `src/<module>.rs` beside a `src/lib.rs` of `parent`, each module named by
/// the parent or not.
fn fixture(parent: &str, modules: &[&str]) -> Fixture {
    let root = Fixture::new();
    root.write("src/lib.rs", parent);
    for module in modules {
        root.write(&format!("src/{module}.rs"), "pub fn f() {}\n");
    }
    root
}

/// The modules of `modules` a parent's text mounts only under test.
fn mounted(parent: &str, modules: &[&str]) -> Vec<String> {
    let root = fixture(parent, modules);
    assert_same_mounts(&root.0);
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    modules
        .iter()
        .filter(|module| index.test_path(&root.0.join(format!("src/{module}.rs"))))
        .map(|module| (*module).to_owned())
        .collect()
}

#[test]
fn a_gating_attribute_decides_each_mount_as_the_parse_does() {
    let all = ["a", "b"];
    for (parent, expected) in [
        ("#[cfg(test)]\nmod a;\nmod b;\n", vec!["a"]),
        ("//! Docs.\n#![cfg(test)]\nmod a;\nmod b;\n", vec!["a", "b"]),
        ("#[cfg(test)] pub(crate) mod a;\n", vec!["a"]),
        ("#[cfg( test )]\nmod a;\n", vec!["a"]),
        ("#[cfg(all(test, unix))]\nmod a;\n", vec!["a"]),
        ("#[cfg(any(test, all(test, unix)))]\nmod a;\n", vec!["a"]),
        ("#[cfg(test)]\n// note; see\n/* a;\n*/\nmod a;\n", vec!["a"]),
        ("#[cfg(test)]\n#[doc = \"a;b\"]\nmod a;\n", vec!["a"]),
        ("#[cfg(test)]\n#[path = \"b.rs\"]\nmod a;\n", vec!["a"]),
        ("#[path = \"b.rs\"]\n#[cfg(test)]\nmod a;\n", vec!["a"]),
        ("#[cfg(unix)]\nmod a;\n", vec![]),
        ("#[cfg(not(test))]\nmod a;\n", vec![]),
        ("#[cfg(any(test, unix))]\nmod a;\n", vec![]),
        ("#[cfg(feature = \"test\")]\nmod a;\n", vec![]),
        ("#[cfg_attr(test, allow(unused))]\nmod a;\n", vec![]),
        ("#[cfg_attr(test, path = \"b.rs\")]\nmod a;\n", vec![]),
        (
            "#![cfg_attr(test, allow(unused))]\nmod a;\nmod b;\n",
            vec![],
        ),
        ("#![cfg(not(test))]\nmod a;\n", vec![]),
        ("#[path = \"a.rs\"]\nmod b;\n", vec![]),
        ("mod a;\nmod b;\n", vec![]),
    ] {
        assert_eq!(mounted(parent, &all), expected, "{parent}");
    }
}

/// An inline module holds no file of the parent's directory: neither itself
/// nor what it declares mounts one, gated or not.
#[test]
fn an_inline_module_mounts_no_file_beside_its_parent() {
    let all = ["a", "b"];
    for parent in [
        "#[cfg(test)]\nmod a {\n    fn t() {}\n}\n",
        "#[cfg(test)]\nmod a {\n    mod b;\n}\n",
        "mod a {\n    #![cfg(test)]\n    mod b;\n}\n",
        "mod a {\n    #[cfg(test)]\n    mod b;\n}\n",
    ] {
        assert_eq!(mounted(parent, &all), Vec::<String>::new(), "{parent}");
    }
}

/// A parent test code itself mounts every module it declares, and a module
/// mounted by one mounts its own in turn.
#[test]
fn mounts_follow_through_a_parent_that_is_test_code() {
    let root = fixture("#[cfg(test)]\nmod a;\n", &["a"]);
    root.write("src/a.rs", "pub mod deep;\n");
    let deep = root.write("src/a/deep.rs", "pub mod under;\n");
    let under = root.write("src/a/deep/under.rs", "pub fn f() {}\n");
    let loose = root.write("src/a/deep/loose.rs", "pub fn f() {}\n");
    assert_same_mounts(&root.0);
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    assert!(index.test_path(&deep) && index.test_path(&under));
    assert!(!index.test_path(&loose), "declared by no parent");
}

/// A parent no text can make mount under test is read and never parsed; one
/// that can is parsed once however many files and threads ask about it.
#[test]
fn a_parent_is_parsed_once_and_only_when_its_text_can_mount() {
    let root = Fixture::new();
    let mut gated = String::new();
    let mut files = Vec::new();
    for module in 0..48 {
        gated.push_str(&format!("#[cfg(test)]\nmod m{module};\n"));
        files.push(root.write(&format!("gated/m{module}.rs"), "pub fn f() {}\n"));
    }
    root.write("gated/lib.rs", gated.repeat(8));
    for (parent, text) in [
        ("open/lib.rs", "#[cfg(unix)]\nmod a;\nmod b;\n"),
        ("quiet/lib.rs", "#[cfg_attr(test, allow(unused))]\nmod a;\n"),
        ("late/lib.rs", "mod a;\n#[cfg(test)]\nmod tests;\n"),
    ] {
        root.write(parent, text);
    }
    let no_parse = ["open/a.rs", "open/b.rs", "quiet/a.rs", "late/a.rs"]
        .map(|module| root.write(module, "pub fn f() {}\n"));
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|threads| {
        for _ in 0..8 {
            threads.spawn(|| {
                barrier.wait();
                for file in &files {
                    assert!(index.test_path(file), "{}", file.display());
                }
            });
        }
    });
    assert_eq!(index.parents_parsed(), 1, "one gated parent");
    for module in &no_parse {
        assert!(!index.test_path(module), "{}", module.display());
    }
    assert_eq!(
        index.parents_parsed(),
        1,
        "no text left the parse anything to find"
    );
}
