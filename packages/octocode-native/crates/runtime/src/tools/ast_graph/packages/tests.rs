use super::*;

#[test]
fn go_import_links_every_file_in_a_201_file_package() {
    let dir = tempfile::tempdir().expect("root");
    std::fs::write(dir.path().join("go.mod"), "module example.com/app\n").expect("module");
    let mut known = BTreeSet::from(["main.go".to_owned()]);
    known.extend((0..201).map(|i| format!("pkg/f{i:03}.go")));
    let index = PackageIndex::build(dir.path(), &known, &|path| {
        std::fs::read_to_string(path).ok()
    });
    assert_eq!(
        files(index.resolve(
            super::super::linking::Linker::Go,
            "example.com/app/pkg",
            "main.go"
        ))
        .len(),
        201
    );
}

fn index(root: &Path, files: &[&str]) -> PackageIndex {
    let known = files.iter().map(|file| (*file).to_owned()).collect();
    PackageIndex::build(root, &known, &|path| std::fs::read_to_string(path).ok())
}

fn files(link: PackageLink) -> Vec<String> {
    match link {
        PackageLink::Files(files) => files,
        PackageLink::UnresolvedInternal => vec!["<internal>".into()],
        PackageLink::External => vec![],
    }
}

#[test]
fn go_module_imports_link_package_files_without_tests() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("go.mod"),
        "module example.com/app\n\ngo 1.22\n",
    )
    .expect("go.mod");
    let index = index(
        dir.path(),
        &[
            "main.go",
            "tsdb/head.go",
            "tsdb/db.go",
            "tsdb/db_test.go",
            "empty/x_test.go",
        ],
    );
    assert_eq!(
        files(index.resolve(
            super::super::linking::Linker::Go,
            "example.com/app/tsdb",
            "main.go"
        )),
        vec!["tsdb/db.go", "tsdb/head.go"]
    );
    assert_eq!(
        files(index.resolve(super::super::linking::Linker::Go, "fmt", "main.go")),
        Vec::<String>::new()
    );
    assert_eq!(
        files(index.resolve(
            super::super::linking::Linker::Go,
            "example.com/app/empty",
            "main.go"
        )),
        vec!["<internal>"]
    );
    assert_eq!(
        files(index.resolve(
            super::super::linking::Linker::Go,
            "example.com/application",
            "main.go"
        )),
        Vec::<String>::new()
    );
}

#[test]
fn a_scan_below_its_module_maps_through_the_enclosing_go_mod() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("go.mod"), "module example.com/app\n").expect("go.mod");
    let sub = dir.path().join("tsdb");
    std::fs::create_dir_all(&sub).expect("sub");
    let scanned = index(&sub, &["db.go", "chunkenc/chunk.go"]);
    assert!(!scanned.go_module_missing());
    assert_eq!(
        files(scanned.resolve(
            super::super::linking::Linker::Go,
            "example.com/app/tsdb/chunkenc",
            "db.go"
        )),
        vec!["chunkenc/chunk.go"]
    );
    // A module import outside the scanned subtree stays external.
    assert_eq!(
        files(scanned.resolve(
            super::super::linking::Linker::Go,
            "example.com/app/model",
            "db.go"
        )),
        Vec::<String>::new()
    );
    // No go.mod anywhere: module imports are unlinkable, not external.
    let bare = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(bare.path().join(".git")).expect("repository boundary");
    assert!(index(bare.path(), &["main.go"]).go_module_missing());
}

#[test]
fn nested_go_modules_resolve_by_longest_module_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("go.mod"), "module example.com/app\n").expect("root");
    std::fs::create_dir_all(dir.path().join("sub")).expect("sub");
    std::fs::write(dir.path().join("sub/go.mod"), "module example.com/sub\n").expect("sub mod");
    let index = index(dir.path(), &["main.go", "sub/pkg/a.go"]);
    assert_eq!(
        files(index.resolve(
            super::super::linking::Linker::Go,
            "example.com/sub/pkg",
            "main.go"
        )),
        vec!["sub/pkg/a.go"]
    );
}

#[test]
fn java_imports_link_classes_nested_static_and_wildcards() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = index(
        dir.path(),
        &[
            "guava/src/com/g/collect/ImmutableList.java",
            "guava/src/com/g/collect/Lists.java",
            "android/guava/src/com/g/collect/ImmutableList.java",
            "guava/src/com/g/base/Preconditions.java",
            "guava/src/com/g/app/Main.java",
        ],
    );
    let importer = "guava/src/com/g/app/Main.java";
    assert_eq!(
        files(index.resolve(
            super::super::linking::Linker::Java,
            "com.g.collect.ImmutableList",
            importer
        )),
        vec!["guava/src/com/g/collect/ImmutableList.java"]
    );
    assert_eq!(
        files(index.resolve(
            super::super::linking::Linker::Java,
            "com.g.collect.ImmutableList.Builder",
            importer
        )),
        vec!["guava/src/com/g/collect/ImmutableList.java"]
    );
    assert_eq!(
        files(index.resolve(
            super::super::linking::Linker::Java,
            "static com.g.base.Preconditions.checkNotNull",
            importer
        )),
        vec!["guava/src/com/g/base/Preconditions.java"]
    );
    assert_eq!(
        files(index.resolve(
            super::super::linking::Linker::Java,
            "com.g.collect.*",
            importer
        ))
        .len(),
        2
    );
    assert_eq!(
        files(index.resolve(
            super::super::linking::Linker::Java,
            "java.util.List",
            importer
        )),
        Vec::<String>::new()
    );
}
