//! Cargo linking follows the workspaces that enclose the scanned Rust files,
//! whatever folder the scan starts at.
use super::*;

const WORKSPACE: &str = "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\nresolver = \"2\"\n";

fn package(name: &str, dependency: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\n{dependency}\n"
    )
}

fn deps(dir: &Path, file: &str) -> Vec<String> {
    let out = ask(dir, "deps", Some(file), |_| {});
    let mut found = ids(&out);
    found.sort();
    found
}

/// A scan of the folder that holds the crates, below the workspace manifest,
/// links one crate's import of another to its library file.
#[test]
fn a_folder_of_crates_links_imports_between_them() {
    let dir = git_app(&[
        ("Cargo.toml", WORKSPACE),
        ("crates/a/Cargo.toml", &package("my-a", "")),
        ("crates/a/src/lib.rs", "pub fn f() {}\n"),
        (
            "crates/b/Cargo.toml",
            &package("my-b", "my-a = { path = \"../a\" }"),
        ),
        ("crates/b/src/lib.rs", "use my_a::f;\npub fn g() { f(); }\n"),
    ]);
    let root = dir.0.as_path();
    let built = ingest_root(root, &root.join("app/crates"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    assert_eq!(deps(root, "b/src/lib.rs"), vec!["a/src/lib.rs"]);
}

/// Two independent workspaces under one root, with no manifest at the root:
/// each links its own path dependency.
#[test]
fn independent_workspaces_under_one_root_each_link() {
    let mut files = Vec::new();
    for ws in ["one", "two"] {
        files.push((format!("{ws}/Cargo.toml"), WORKSPACE.to_owned()));
        files.push((
            format!("{ws}/crates/a/Cargo.toml"),
            package(&format!("{ws}-a"), ""),
        ));
        files.push((
            format!("{ws}/crates/a/src/lib.rs"),
            "pub fn f() {}\n".to_owned(),
        ));
        files.push((
            format!("{ws}/crates/b/Cargo.toml"),
            package(
                &format!("{ws}-b"),
                &format!("{ws}-a = {{ path = \"../a\" }}"),
            ),
        ));
        files.push((
            format!("{ws}/crates/b/src/lib.rs"),
            format!("use {ws}_a::f;\npub fn g() {{ f(); }}\n"),
        ));
    }
    let files = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let dir = git_app(&files);
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    for ws in ["one", "two"] {
        assert_eq!(
            deps(dir.0.as_path(), &format!("{ws}/crates/b/src/lib.rs")),
            vec![format!("{ws}/crates/a/src/lib.rs")]
        );
    }
}

/// Rust files no manifest encloses still link their module paths by syntax,
/// and the snapshot says their crate-name imports stay external.
#[test]
fn rust_without_a_manifest_links_modules_and_says_so() {
    let dir = git_app(&[
        (
            "src/lib.rs",
            "mod util;\nuse util::f;\npub fn g() { f(); }\n",
        ),
        ("src/util.rs", "pub fn f() {}\n"),
    ]);
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    assert_eq!(deps(dir.0.as_path(), "src/lib.rs"), vec!["src/util.rs"]);
    let diagnostics = ask(dir.0.as_path(), "diagnostics", None, |_| {});
    assert!(
        diagnostics.value["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| row["message"].as_str().is_some_and(
                |message| message.contains("2 Rust file(s) have no enclosing Cargo.toml")
            )),
        "{}",
        diagnostics.value
    );
}
