//! The text check before a parent's parse changes no decision: every file is
//! test code by mount exactly as when every parent is parsed.
use super::*;
use crate::tools::test_support::{Fixture, workspace_policy};
use std::cell::RefCell;

mod crate_roots;
mod forms;
mod texts;

/// The mount rule with no shortcut: every Rust file a mount may come from read
/// and parsed.
struct EveryParentParsed<'a> {
    index: &'a TestCodeIndex,
    mounted: RefCell<HashMap<PathBuf, Option<bool>>>,
    targets: RefCell<HashMap<PathBuf, Arc<Targets>>>,
}

/// What one file mounts: under a gate, and at all.
#[derive(Default)]
struct Targets {
    gated: Vec<PathBuf>,
    any: Vec<PathBuf>,
}

impl<'a> EveryParentParsed<'a> {
    fn new(index: &'a TestCodeIndex) -> Self {
        Self {
            index,
            mounted: RefCell::default(),
            targets: RefCell::default(),
        }
    }

    fn test_path(&self, path: &Path) -> bool {
        self.placement(path)
            .unwrap_or_else(|| self.index.by_spelling(path))
    }

    /// Whether the mounts place the file in test code (`Some(true)`) or in
    /// production code; `None` when no parent is found to mount it.
    fn placement(&self, path: &Path) -> Option<bool> {
        if !TestCodeIndex::may_hold_test_code(path) || module_name(path).is_none() {
            return None;
        }
        if let Some(under_test) = self.index.crate_target(path) {
            return Some(under_test);
        }
        if let Some(known) = self.mounted.borrow().get(path) {
            return *known;
        }
        self.mounted.borrow_mut().insert(path.to_path_buf(), None);
        let target = octocode_engine::text::lexical(path);
        let mut placed = None;
        for parent in self.parents_of(path).iter() {
            let targets = self.targets(parent);
            if targets.gated.contains(&target)
                || (targets.any.contains(&target) && self.test_path(parent))
            {
                placed = Some(true);
                break;
            }
            if targets.any.contains(&target) {
                placed = Some(false);
            }
        }
        self.mounted.borrow_mut().insert(path.to_path_buf(), placed);
        placed
    }

    fn targets(&self, parent: &Path) -> Arc<Targets> {
        if let Some(known) = self.targets.borrow().get(parent) {
            return Arc::clone(known);
        }
        let targets = self
            .index
            .source(parent)
            .and_then(|content| TestCodeIndex::parse(parent, &content))
            .map(|document| {
                let root = self.index.crate_target(parent).is_some();
                let modules = mount_targets::file_modules(parent, &document.modules, root);
                Targets {
                    gated: modules
                        .iter()
                        .filter(|mount| {
                            mount.under_test || in_spans(&document.test_lines, mount.line)
                        })
                        .map(|mount| mount.path.clone())
                        .collect(),
                    any: modules.into_iter().map(|mount| mount.path).collect(),
                }
            })
            .unwrap_or_default();
        let targets = Arc::new(targets);
        self.targets
            .borrow_mut()
            .insert(parent.to_path_buf(), Arc::clone(&targets));
        targets
    }

    /// Every Rust file but `path` in its directory and those above it, up to
    /// its crate or the workspace, and the sibling file named after each.
    fn parents_of(&self, path: &Path) -> Vec<PathBuf> {
        let mut parents = Vec::new();
        let mut dir = path.parent();
        while let Some(level) = dir {
            for entry in std::fs::read_dir(level).expect("listing").flatten() {
                let file = entry.path();
                if file.is_file() && TestCodeIndex::may_hold_test_code(&file) && file != path {
                    parents.push(file);
                }
            }
            if let (Some(up), Some(name)) = (level.parent(), level.file_name()) {
                let named = up.join(format!("{}.rs", name.to_string_lossy()));
                if named.is_file() && named != path {
                    parents.push(named);
                }
            }
            let top = level.join("Cargo.toml").is_file()
                || self.index.paths.workspace_relative(level).as_deref() == Some(".");
            dir = if top { None } else { level.parent() };
        }
        parents
    }
}

pub(super) fn rust_files(root: &Path) -> Vec<PathBuf> {
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
    assert_same_mounts_of(root, &rust_files(root));
}

/// [`assert_same_mounts`] over `files` of `root`.
fn assert_same_mounts_of(root: &Path, files: &[PathBuf]) {
    let policy = workspace_policy(root);
    let (reference, index) = (TestCodeIndex::new(&policy), TestCodeIndex::new(&policy));
    let parsed = EveryParentParsed::new(&reference);
    let unread = TestCodeIndex::new(&policy);
    for file in files {
        let verdict = index.test_path(file);
        assert_eq!(
            verdict,
            parsed.test_path(file),
            "{} under:\n{}",
            file.display(),
            std::fs::read_to_string(root.join("src/lib.rs")).unwrap_or_default()
        );
        // Left unread as test code only when the by-name verdict and the full
        // one both say test.
        let by_name = unread.placed_as_test(file, unread.placement_by_name(file));
        assert_eq!(
            unread.test_path_unread(file),
            by_name && verdict,
            "by name {by_name}, in full {verdict}: {}",
            file.display()
        );
    }
}

/// This repository's crates directory: the tree the real-source checks read.
pub(super) fn real_source_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates directory")
}

/// Files per mount form in [`real_source_sample`].
const FILES_PER_FORM: usize = 20;

/// A fixed sample of this repository's own Rust source: this module whole,
/// and for each form a mount or a test item takes (a `#[path]`, a gate on a
/// module, an inner gate, a `cfg_attr`, an inline module, an out-of-line
/// module, a raw `mod` identifier, and each file name a crate or module
/// root takes) [`FILES_PER_FORM`] files spread evenly over the sorted files
/// that hold it. The sample is the same at every run of one tree, and every
/// form stays covered however the tree grows; the full tree is read by the
/// ignored `*_full_tree` tests.
pub(super) fn real_source_sample() -> Vec<PathBuf> {
    let files = rust_files(real_source_root());
    let module = real_source_root().join("runtime/src/tools/test_code");
    let texts = files
        .iter()
        .map(|file| std::fs::read_to_string(file).unwrap_or_default())
        .collect::<Vec<_>>();
    let name = |file: &Path| {
        file.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let forms: [(&str, &dyn Fn(usize) -> bool); 10] = [
        ("path", &|at| texts[at].contains("#[path")),
        ("gated mod", &|at| texts[at].contains("#[cfg(test)]\nmod ")),
        ("inner gate", &|at| texts[at].contains("#![cfg")),
        ("cfg_attr", &|at| texts[at].contains("cfg_attr")),
        ("inline mod", &|at| {
            texts[at]
                .lines()
                .any(|line| line.trim_start().starts_with("mod ") && line.ends_with('{'))
        }),
        ("out-of-line mod", &|at| {
            texts[at]
                .lines()
                .any(|line| line.contains("mod ") && line.ends_with(';'))
        }),
        ("raw mod", &|at| texts[at].contains("r#mod")),
        ("lib or main", &|at| {
            matches!(name(&files[at]).as_str(), "lib.rs" | "main.rs")
        }),
        ("mod.rs", &|at| name(&files[at]) == "mod.rs"),
        ("tests file", &|at| {
            files[at]
                .components()
                .any(|part| part.as_os_str() == "tests" || part.as_os_str() == "tests.rs")
        }),
    ];
    let mut chosen = std::collections::BTreeSet::new();
    chosen.extend((0..files.len()).filter(|&at| files[at].starts_with(&module)));
    for (_, holds) in forms {
        let held = (0..files.len()).filter(|&at| holds(at)).collect::<Vec<_>>();
        let step = held.len().div_ceil(FILES_PER_FORM).max(1);
        chosen.extend(held.into_iter().step_by(step));
    }
    chosen.into_iter().map(|at| files[at].clone()).collect()
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

/// A sample of this repository's own source ([`real_source_sample`]).
#[test]
fn sampled_files_of_real_source_are_decided_as_when_every_parent_is_parsed() {
    let sample = real_source_sample();
    assert!(sample.len() > FILES_PER_FORM, "{} files", sample.len());
    assert_same_mounts_of(real_source_root(), &sample);
}

/// This repository's own source, every file.
#[test]
#[ignore = "full tree: cargo test -- --ignored full_tree"]
fn every_file_of_real_source_is_decided_as_when_every_parent_is_parsed_full_tree() {
    assert_same_mounts(real_source_root());
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
        ("#[cfg(test)]\n#[path = \"b.rs\"]\nmod a;\n", vec!["b"]),
        ("#[path = \"b.rs\"]\n#[cfg(test)]\nmod a;\n", vec!["b"]),
        ("#[cfg(unix)]\nmod a;\n", vec![]),
        ("#[cfg(not(test))]\nmod a;\n", vec![]),
        ("#[cfg(any(test, unix))]\nmod a;\n", vec![]),
        ("#[cfg(feature = \"test\")]\nmod a;\n", vec![]),
        ("#[cfg_attr(test, allow(unused))]\nmod a;\n", vec![]),
        ("#[cfg_attr(test, path = \"b.rs\")]\nmod a;\n", vec!["b"]),
        ("#[cfg_attr(unix, path = \"b.rs\")]\nmod a;\n", vec![]),
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

/// A walk's verdict on a file it has yet to read opens no sibling of it: a
/// `#[path]` mount waits for the fuller verdict, which finds it and watches what
/// it read.
#[test]
fn a_path_mount_waits_for_the_verdict_that_needs_it() {
    let root = Fixture::new();
    root.write("src/lib.rs", "pub fn f() {}\n");
    root.write(
        "src/fixtures.rs",
        "#[cfg(test)]\n#[path = \"target_x.rs\"]\nmod target;\n",
    );
    let target = root.write("src/target_x.rs", "pub fn f() {}\n");
    let index = TestCodeIndex::new(&workspace_policy(&root.0));
    let fixtures = |index: &TestCodeIndex| {
        index
            .watched()
            .iter()
            .any(|path| path.ends_with("fixtures.rs"))
    };
    assert!(!index.test_path_unread(&target));
    assert!(!fixtures(&index));
    assert!(index.test_path(&target));
    assert!(fixtures(&index));
}
