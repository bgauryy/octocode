//! A package's `include!`s and module declarations are read from every one of
//! its sources for each Rust `references` or `callers` answer: the reads happen
//! once per state of those sources, and any change is seen by the next answer.
use super::*;
use crate::stamp::settle_written_so_far;
use crate::test_support::TmpDir;
use std::fs;

/// The includes of `file`, and how many package sources were read for them.
fn counted(file: &Path) -> (PackageIncludes, usize) {
    let before = PACKAGE_FILES_READ.with(std::cell::Cell::get);
    let found = cargo_package_includes(file, &|| false).expect("package");
    (
        found,
        PACKAGE_FILES_READ.with(std::cell::Cell::get) - before,
    )
}

fn package() -> (TmpDir, PathBuf, PathBuf) {
    let root = TmpDir::new();
    let package = root.join("pkg");
    fs::create_dir_all(package.join("src")).expect("src");
    fs::create_dir_all(root.join(".git")).expect("repository");
    fs::write(package.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
    fs::write(package.join("src/lib.rs"), "pub fn a() {}\n").expect("lib");
    fs::write(package.join("src/other.rs"), "pub fn b() {}\n").expect("other");
    let lib = package.join("src/lib.rs");
    (root, package, lib)
}

const OUTSIDE: &str = "include!(\"../../outside.rs\");\n";

#[test]
fn a_settled_package_is_read_once() {
    let (_root, _package, lib) = package();
    settle_written_so_far();
    assert_eq!(counted(&lib).1, 2);
    let (again, reads) = counted(&lib);
    assert_eq!(reads, 0);
    assert!(again.outside.is_empty() && !again.has_more, "{again:?}");
}

/// A kept scan draws on the one process budget, with no cap of its own.
#[test]
fn a_kept_scan_draws_on_the_process_budget() {
    let (_root, _package, lib) = package();
    settle_written_so_far();
    let before = crate::memory::MEMORY.used();
    assert_eq!(counted(&lib).1, 2);
    assert!(crate::memory::MEMORY.used() > before);
}

#[test]
fn an_unsettled_package_is_read_each_time() {
    let (_root, _package, lib) = package();
    assert_eq!(counted(&lib).1, 2);
    assert_eq!(counted(&lib).1, 2);
}

/// Each anchor gets its own owners from the one reading.
#[test]
fn another_file_of_the_package_shares_the_reading() {
    let (_root, package, lib) = package();
    fs::write(
        package.join("src/a.rs"),
        "#[path = \"shared.rs\"]\nmod one;\n",
    )
    .expect("a");
    fs::write(
        package.join("src/b.rs"),
        "#[path = \"shared.rs\"]\nmod two;\n",
    )
    .expect("b");
    fs::write(package.join("src/shared.rs"), "pub fn s() {}\n").expect("shared");
    settle_written_so_far();
    assert_eq!(counted(&lib).1, 5);
    let (shared, reads) = counted(&package.join("src/shared.rs"));
    assert_eq!(reads, 0);
    let fresh = {
        std::thread::scope(|scope| {
            // A thread with no settled clock reads the sources again.
            scope
                .spawn(|| {
                    fs::write(package.join("src/c.rs"), "").expect("new file");
                    cargo_package_includes(&package.join("src/shared.rs"), &|| false)
                })
                .join()
                .expect("thread")
                .expect("package")
        })
    };
    assert_eq!(shared.declared_by, fresh.declared_by);
    assert_eq!(fresh.declared_by.len(), 2, "{fresh:?}");
}

/// Everything the reading depends on is checked: an edited source, a new or
/// removed one, a new directory, a nested package, an edited ignore file.
#[test]
fn any_change_to_the_package_is_seen_by_the_next_answer() {
    let (root, package, lib) = package();
    let outside = root.join("outside.rs");
    let settled = || {
        settle_written_so_far();
        counted(&lib);
        assert_eq!(counted(&lib).1, 0, "stored");
    };
    settled();
    fs::write(package.join("src/other.rs"), OUTSIDE).expect("edit");
    let (found, reads) = counted(&lib);
    assert!(reads > 0);
    assert_eq!(found.outside, vec![outside.clone()]);

    settled();
    fs::write(package.join("src/other.rs"), "").expect("edit");
    assert!(counted(&lib).0.outside.is_empty());

    settled();
    fs::write(package.join("src/new.rs"), OUTSIDE).expect("new source");
    assert_eq!(counted(&lib).0.outside, vec![outside.clone()]);

    settled();
    fs::remove_file(package.join("src/new.rs")).expect("removed");
    assert!(counted(&lib).0.outside.is_empty());

    settled();
    fs::create_dir_all(package.join("src/deep")).expect("directory");
    fs::write(
        package.join("src/deep/mod.rs"),
        "include!(\"../../../outside.rs\");\n",
    )
    .expect("deep source");
    assert_eq!(counted(&lib).0.outside, vec![outside.clone()]);

    settled();
    fs::write(
        package.join("src/deep/Cargo.toml"),
        "[package]\nname = \"d\"\n",
    )
    .expect("nested");
    assert!(
        counted(&lib).0.outside.is_empty(),
        "another package's source"
    );
    fs::remove_file(package.join("src/deep/Cargo.toml")).expect("not nested");

    settled();
    fs::write(root.join(".gitignore"), "deep/\n").expect("ignore file");
    assert!(counted(&lib).0.outside.is_empty(), "an ignored source");

    settled();
    fs::write(root.join(".gitignore"), "\n").expect("ignore file edited");
    assert_eq!(counted(&lib).0.outside, vec![outside.clone()]);

    settled();
    fs::remove_dir_all(root.join(".git")).expect("no repository");
    fs::write(root.join(".gitignore"), "deep/\n").expect("ignore file");
    settled();
    fs::create_dir_all(root.join(".git")).expect("a repository again");
    assert!(
        counted(&lib).0.outside.is_empty(),
        "with a repository .gitignore applies"
    );
}

/// A build script decides whether generated includes count, and is read from
/// the manifest side each time: adding one needs no new reading of the sources.
#[test]
fn a_build_script_is_answered_from_the_same_reading() {
    let (_root, package, lib) = package();
    fs::write(
        package.join("src/other.rs"),
        "include!(concat!(env!(\"OUT_DIR\"), \"/g.rs\"));\n",
    )
    .expect("generated include");
    settle_written_so_far();
    assert!(counted(&lib).0.generated.is_empty());
    assert!(counted(&lib).0.generated.is_empty());
    fs::write(package.join("build.rs"), "fn main() {}\n").expect("build script");
    assert_eq!(
        counted(&lib).0.generated,
        vec![package.join("src/other.rs")]
    );
}

/// A scan the request stopped is partial: it is not what a later answer gets.
#[test]
fn a_stopped_scan_is_not_kept() {
    let (_root, _package, lib) = package();
    settle_written_so_far();
    let stopped = cargo_package_includes(&lib, &|| true).expect("package");
    assert!(stopped.has_more);
    let (whole, reads) = counted(&lib);
    assert!(!whole.has_more && reads > 0, "{whole:?}");
}
