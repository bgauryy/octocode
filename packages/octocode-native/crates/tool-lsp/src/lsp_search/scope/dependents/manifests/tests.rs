//! Build files naming the anchor's project, on the shapes the large repositories use.
use super::*;

const GUAVA: &str = "<project><parent><groupId>com.google.guava</groupId><artifactId>guava-parent</artifactId><version>1-jre</version></parent><artifactId>guava</artifactId><dependencies><dependency><groupId>com.google.guava</groupId><artifactId>failureaccess</artifactId></dependency></dependencies></project>";

#[test]
fn a_pom_names_its_own_coordinates_apart_from_its_sections() {
    let (own, parent) = project_tags(GUAVA);
    assert_eq!(tag(&own, "artifactId").as_deref(), Some("guava"));
    assert_eq!(tag(&own, "groupId"), None);
    assert_eq!(tag(&parent, "groupId").as_deref(), Some("com.google.guava"));
}

/// guava's `android/` flavour depends on `guava` at its own version, not the
/// anchor's.
#[test]
fn a_dependency_at_another_version_of_the_same_artifact_is_another_project() {
    let android = "<project><parent><groupId>com.google.guava</groupId><artifactId>guava-parent</artifactId><version>1-android</version></parent><artifactId>guava-tests</artifactId><dependencies><dependency><groupId>com.google.guava</groupId><artifactId>guava</artifactId><version>${project.version}</version></dependency></dependencies></project>";
    assert!(!maven_requires(
        android,
        "com.google.guava",
        "guava",
        Some("1-jre")
    ));
    assert!(maven_requires(
        android,
        "com.google.guava",
        "guava",
        Some("1-android")
    ));
    // Left to dependency management: may be the anchor's.
    let managed = android.replace("<version>${project.version}</version>", "");
    assert!(maven_requires(
        &managed,
        "com.google.guava",
        "guava",
        Some("1-jre")
    ));
    assert!(!maven_requires(
        GUAVA,
        "com.google.guava",
        "guava",
        Some("1-jre")
    ));
}

#[test]
fn go_modules_name_a_dependency_in_require_and_replace() {
    let single = "module example.com/b\n\nrequire example.com/a v0.0.0\n";
    let blocked = "module example.com/b\nrequire (\n\texample.com/a v1.2.3 // indirect\n)\n";
    let replaced = "module example.com/b\nreplace example.com/a => ../a\n";
    for text in [single, blocked, replaced] {
        assert!(go_requires(text, "example.com/a"), "{text}");
    }
    assert!(!go_requires(
        "module example.com/b\nrequire example.com/c v1\n",
        "example.com/a"
    ));
}

/// A Cargo build file names the anchor's package by key or `package`, in
/// any dependency table, target-specific ones included, under Cargo's
/// `-`/`_` normalization, when the dependency's source is that package; a
/// registry dependency of that name may be it.
#[test]
fn cargo_names_a_dependency_by_key_or_package() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    let write = |relative: &str, text: &str| {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(&path, text).expect("write");
        path
    };
    write(
        "my-crate/Cargo.toml",
        "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\n",
    );
    let anchor = write("my-crate/src/lib.rs", "pub fn f() {}\n");
    let (identity, _) = Identity::of(&anchor, "rs").expect("identity");
    let user = |name: &str, deps: &str| {
        write(
            &format!("{name}/Cargo.toml"),
            &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n{deps}"),
        )
    };
    for (name, deps) in [
        (
            "by-key",
            "[dependencies]\nmy_crate = { path = \"../my-crate\" }\n",
        ),
        (
            "renamed",
            "[dev-dependencies]\nalias = { package = \"my-crate\", path = \"../my-crate\" }\n",
        ),
        (
            "targeted",
            "[target.'cfg(unix)'.dependencies]\nmy-crate = { path = \"../my-crate\" }\n",
        ),
        ("registry", "[dependencies]\nmy-crate = \"1\"\n"),
    ] {
        assert!(depends_on(&user(name, deps), &identity), "{name}");
    }
    for (name, deps) in [
        ("other", "[dependencies]\nb = \"1\"\n"),
        (
            "elsewhere",
            "[dependencies]\nmy-crate = { path = \"../missing\" }\n",
        ),
    ] {
        assert!(!depends_on(&user(name, deps), &identity), "{name}");
    }
}
