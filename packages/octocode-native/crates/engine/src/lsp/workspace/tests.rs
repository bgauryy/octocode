use super::*;
use crate::test_support::TmpDir;
use std::fs;

mod modules;

/// A source directory resolves to the project above it; a tree with no
/// marker has no project root.
#[test]
fn project_root_of_dir_walks_up_to_the_marker() {
    let root = TmpDir::new();
    fs::write(root.join("tsconfig.json"), b"{}").expect("write marker");
    let src = root.join("src").join("cli");
    fs::create_dir_all(&src).expect("create src");
    let bare = TmpDir::new();
    let found = project_root_of_dir(&src);
    let none = project_root_of_dir(&bare);
    assert_eq!(found.as_deref(), Some(&*root));
    // The temp dir's ancestors may hold a marker on some hosts; only a
    // found root must be an ancestor.
    assert!(none.is_none_or(|dir| bare.starts_with(dir)));
}

#[test]
fn resolves_root_from_nearest_marker() {
    // Layout: root/package.json  root/src/index.ts
    let root = TmpDir::new();
    fs::write(root.join("package.json"), b"{}").expect("write marker");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("create src");
    let file = src.join("index.ts");
    fs::write(&file, b"export {}").expect("write file");

    let result =
        resolve_workspace_root_for_file(file.to_string_lossy().into_owned()).expect("resolve");

    // Normalize both sides in case of symlink-resolved paths (macOS /var → /private/var).
    let resolved = PathBuf::from(&result)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&result));
    let expected = root.canonicalize().unwrap_or_else(|_| root.0.clone());
    assert_eq!(resolved, expected);
}

#[test]
fn resolves_root_skips_deep_nested_marker_in_favour_of_parent() {
    // Layout: root/Cargo.toml  root/a/b/file.rs
    // Nearest marker walking upward from file.rs is root/ (a/ and a/b/ have none).
    let root = TmpDir::new();
    fs::write(root.join("Cargo.toml"), b"[package]").expect("write marker");
    let deep = root.join("a").join("b");
    fs::create_dir_all(&deep).expect("create deep");
    let file = deep.join("file.rs");
    fs::write(&file, b"fn main() {}").expect("write file");

    let result =
        resolve_workspace_root_for_file(file.to_string_lossy().into_owned()).expect("resolve");

    let resolved = PathBuf::from(&result)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&result));
    let expected = root.canonicalize().unwrap_or_else(|_| root.0.clone());
    assert_eq!(resolved, expected);
}

/// A workspace package without its own tsconfig roots a TypeScript file at
/// the repository tsconfig above it (tsserver searches only up to its root);
/// a package with its own tsconfig, and a non-TypeScript file, keep the
/// package marker.
#[test]
fn typescript_files_root_at_their_governing_tsconfig() {
    let root = TmpDir::new();
    fs::write(root.join("tsconfig.json"), b"{}").expect("root tsconfig");
    let bare = root.join("packages").join("bare");
    let owned = root.join("packages").join("owned");
    for package in [&bare, &owned] {
        fs::create_dir_all(package.join("src")).expect("package src");
        fs::write(package.join("package.json"), b"{}").expect("package manifest");
    }
    fs::write(owned.join("tsconfig.json"), b"{}").expect("package tsconfig");
    let resolve = |file: PathBuf| {
        let root =
            resolve_workspace_root_for_file(file.to_string_lossy().into_owned()).expect("resolve");
        PathBuf::from(root)
    };
    assert_eq!(resolve(bare.join("src").join("guards.ts")), *root);
    assert_eq!(resolve(bare.join("src").join("view.jsx")), *root);
    assert_eq!(resolve(owned.join("src").join("index.ts")), owned);
    assert_eq!(resolve(bare.join("src").join("main.py")), bare);
}

/// A member crate's file roots the server at its Cargo workspace, so
/// every member shares one rust-analyzer; a standalone package roots
/// at itself, and a non-Rust file keeps the nearest marker.
#[test]
fn rust_files_root_at_their_cargo_workspace() {
    let root = TmpDir::new();
    fs::write(
        root.join("Cargo.toml"),
        b"[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .expect("workspace manifest");
    let member = root.join("crates").join("engine");
    fs::create_dir_all(member.join("src")).expect("member src");
    fs::write(member.join("Cargo.toml"), b"[package]\nname = \"engine\"\n").expect("member");
    fs::write(member.join("package.json"), b"{}").expect("npm wrapper");
    let rust = member.join("src").join("lib.rs");
    let script = member.join("src").join("gen.js");
    let standalone = TmpDir::new();
    fs::write(standalone.join("Cargo.toml"), b"[package]\nname = \"x\"\n").expect("alone");
    let resolve = |file: &Path| {
        PathBuf::from(
            resolve_workspace_root_for_file(file.to_string_lossy().into_owned()).expect("root"),
        )
    };
    let (member_root, script_root, alone_root) = (
        resolve(&rust),
        resolve(&script),
        resolve(&standalone.join("main.rs")),
    );
    assert_eq!(member_root, root.0);
    assert_eq!(script_root, member);
    assert_eq!(alone_root, standalone.0);
}

#[test]
fn build_script_follows_cargo_defaults() {
    let package = TmpDir::new();
    fs::create_dir_all(package.join("src")).expect("src");
    let manifest = package.join("Cargo.toml");
    fs::write(&manifest, b"[package]\nname = \"p\"\n").expect("manifest");
    let none = cargo_package_build_script(&package);
    fs::write(package.join("build.rs"), b"fn main() {}").expect("build.rs");
    let default = cargo_package_build_script(&package);
    fs::write(&manifest, b"[package]\nname = \"p\"\nbuild = false\n").expect("off");
    let off = cargo_package_build_script(&package);
    fs::write(package.join("gen.rs"), b"fn main() {}").expect("gen.rs");
    fs::write(&manifest, b"[package]\nname = \"p\"\nbuild = \"gen.rs\"\n").expect("named");
    let named = cargo_package_build_script(&package);
    fs::write(&manifest, b"[workspace]\nmembers = []\n").expect("virtual");
    let virtual_manifest = cargo_package_build_script(&package);
    assert_eq!(none, None);
    assert_eq!(default, Some(package.join("build.rs")));
    assert_eq!(off, None);
    assert_eq!(named, Some(package.join("gen.rs")));
    assert_eq!(virtual_manifest, None);
}

/// Build-script includes count only in a package with a build script;
/// a literal include outside the package is never loadable; nested
/// packages, comments and `include_str!` are not includes.
#[test]
fn package_includes_name_what_rust_analyzer_cannot_load() {
    let outer = TmpDir::new();
    let package = outer.join("pkg");
    fs::create_dir_all(package.join("src/inner")).expect("src");
    fs::create_dir_all(package.join("nested/src")).expect("nested");
    fs::create_dir_all(outer.join("contract")).expect("contract");
    fs::write(package.join("Cargo.toml"), b"[package]\nname = \"p\"\n").expect("manifest");
    let lib = package.join("src/lib.rs");
    fs::write(
        &lib,
        concat!(
            "include!(concat!(env!(\"OUT_DIR\"), \"/ids.rs\"));\n",
            "include!(\"inner/local.rs\");\n",
            "// include!(\"../../contract/commented.rs\");\n",
            "const DATA: &str = include_str!(\"../../contract/data.txt\");\n",
        ),
    )
    .expect("lib");
    fs::write(
        package.join("src/inner/types.rs"),
        b"include!(\"../../../contract/types.rs\");\n",
    )
    .expect("types");
    fs::write(
        package.join("nested/Cargo.toml"),
        b"[package]\nname = \"n\"\n",
    )
    .expect("nested manifest");
    fs::write(
        package.join("nested/src/lib.rs"),
        b"include!(\"../../../contract/nested.rs\");\n",
    )
    .expect("nested lib");
    let no_script = cargo_package_includes(&lib, 100).expect("package");
    fs::write(package.join("build.rs"), b"fn main() {}").expect("build.rs");
    let with_script = cargo_package_includes(&lib, 100).expect("package");
    let capped = cargo_package_includes(&lib, 1).expect("package");
    let outside = vec![outer.join("contract/types.rs")];
    assert_eq!(no_script.generated, Vec::<PathBuf>::new());
    assert_eq!(no_script.outside, outside);
    assert!(!no_script.has_more);
    assert_eq!(with_script.generated, vec![lib.clone()]);
    assert_eq!(with_script.outside, outside);
    assert!(capped.has_more);
}

/// A support file two crate roots declare by `#[path]` is declared
/// twice; a file one module tree declares, once; comments and inline
/// modules declare nothing.
#[test]
fn package_includes_name_the_files_declaring_the_anchor() {
    let package = TmpDir::new();
    for dir in ["src/a", "benches/support", "tests"] {
        fs::create_dir_all(package.join(dir)).expect("dir");
    }
    fs::write(package.join("Cargo.toml"), b"[package]\nname = \"p\"\n").expect("manifest");
    let support = package.join("benches/support/frozen.rs");
    fs::write(&support, b"pub fn frozen() {}\n").expect("support");
    let bench = package.join("benches/graph.rs");
    fs::write(
        &bench,
        b"#[path = \"support/frozen.rs\"]\nmod frozen;\nfn main() {}\n",
    )
    .expect("bench");
    let test = package.join("tests/graph.rs");
    fs::write(
        &test,
        b"#[path = \"../benches/support/frozen.rs\"]\n#[allow(dead_code)]\npub(crate) mod frozen;\n",
    )
    .expect("test");
    let lib = package.join("src/lib.rs");
    fs::write(&lib, b"pub mod a;\n// mod b;\nmod inline { }\n").expect("lib");
    let a = package.join("src/a/mod.rs");
    fs::write(&a, b"pub mod b;\n").expect("a");
    let b = package.join("src/a/b.rs");
    fs::write(&b, b"pub fn bee() {}\n").expect("b");
    let declared = |file: &Path| {
        let mut found = cargo_package_includes(file, 100)
            .expect("package")
            .declared_by;
        found.sort();
        found
    };
    assert_eq!(declared(&support), vec![bench, test]);
    assert_eq!(declared(&a), vec![lib.clone()]);
    assert_eq!(declared(&b), vec![a]);
    assert_eq!(declared(&lib), Vec::<PathBuf>::new());
}

#[test]
fn falls_back_to_file_dir_not_cwd() {
    // A temp directory with no marker anywhere above it resolves to the
    // file's own directory, never the process cwd.
    let root = TmpDir::new();
    let file = root.join("orphan.ts");
    fs::write(&file, b"// nothing").expect("write file");
    let has_marker_above = root
        .ancestors()
        .any(|dir| MARKERS.iter().any(|marker| dir.join(marker).exists()));

    let result = resolve_workspace_root_for_file(file.to_string_lossy().into_owned())
        .expect("resolve must not error");
    let cwd = std::env::current_dir().expect("cwd");
    assert_ne!(PathBuf::from(&result), cwd);
    if !has_marker_above {
        assert_eq!(PathBuf::from(&result), root.0);
    }
}

/// rust-analyzer loads a Rust file only from a Cargo package or a
/// `rust-project.json`: a `.git` (or any other marker) above it is no project
/// for that server. Other servers need no project file.
#[test]
fn a_rust_file_needs_a_cargo_package_or_rust_project_json() {
    let repo = TmpDir::new();
    fs::create_dir_all(repo.join(".git")).expect("git dir");
    let src = repo.join("src");
    fs::create_dir_all(&src).expect("src");
    let rust = src.join("lib.rs");
    assert!(outside_required_project(&rust));
    assert!(!outside_required_project(&src.join("app.ts")));
    fs::write(repo.join("rust-project.json"), b"{}").expect("rust-project.json");
    assert!(!outside_required_project(&rust));
    fs::remove_file(repo.join("rust-project.json")).expect("remove");
    fs::write(src.join("Cargo.toml"), b"[package]\nname = \"p\"\n").expect("manifest");
    assert!(!outside_required_project(&rust));
}

/// A module of a multi-module project roots its server at the project's
/// top: the `go.work` above a Go module, the Maven parent listing the
/// module, the pyright configuration above a Python package.
#[test]
fn multi_module_projects_root_at_their_top() {
    let top = TmpDir::new();
    fs::write(top.join("go.work"), b"go 1.22\nuse ./svc\n").expect("go.work");
    let svc = top.join("svc");
    fs::create_dir_all(&svc).expect("svc");
    fs::write(svc.join("go.mod"), b"module example.com/svc\n").expect("go.mod");
    let go_file = svc.join("main.go");
    fs::write(&go_file, b"package main\n").expect("main.go");
    let resolved = resolve_workspace_root_for_file(go_file.to_string_lossy().into_owned())
        .expect("resolve");
    assert_eq!(PathBuf::from(resolved), top.0);

    let maven = TmpDir::new();
    fs::write(
        maven.join("pom.xml"),
        b"<project><modules><module>core</module><module>api</module></modules></project>",
    )
    .expect("parent pom");
    let core = maven.join("core");
    fs::create_dir_all(core.join("src")).expect("core");
    fs::write(core.join("pom.xml"), b"<project/>").expect("module pom");
    let java_file = core.join("src").join("App.java");
    fs::write(&java_file, b"class App {}").expect("App.java");
    let resolved = resolve_workspace_root_for_file(java_file.to_string_lossy().into_owned())
        .expect("resolve");
    assert_eq!(PathBuf::from(resolved), maven.0);
    // A module the parent does not list keeps its own root.
    let other = maven.join("other");
    fs::create_dir_all(&other).expect("other");
    fs::write(other.join("pom.xml"), b"<project/>").expect("other pom");
    let other_file = other.join("Other.java");
    fs::write(&other_file, b"class Other {}").expect("Other.java");
    let resolved = resolve_workspace_root_for_file(other_file.to_string_lossy().into_owned())
        .expect("resolve");
    assert_eq!(PathBuf::from(resolved), other);

    let python = TmpDir::new();
    fs::write(python.join("pyrightconfig.json"), b"{}").expect("pyrightconfig");
    let pkg = python.join("packages").join("lib");
    fs::create_dir_all(&pkg).expect("pkg");
    fs::write(pkg.join("pyproject.toml"), b"[project]\nname = \"lib\"\n").expect("pyproject");
    let py_file = pkg.join("mod.py");
    fs::write(&py_file, b"x = 1\n").expect("mod.py");
    let resolved = resolve_workspace_root_for_file(py_file.to_string_lossy().into_owned())
        .expect("resolve");
    assert_eq!(PathBuf::from(resolved), python.0);
}
