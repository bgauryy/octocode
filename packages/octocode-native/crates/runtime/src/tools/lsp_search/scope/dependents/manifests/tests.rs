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

#[test]
fn cargo_names_a_dependency_by_key_or_package() {
    let parse = |text: &str| text.parse::<toml::Table>().expect("toml");
    assert!(cargo_requires(
        &parse("[dependencies]\nmy-crate = { path = \"../a\" }\n"),
        "my_crate"
    ));
    assert!(cargo_requires(
        &parse("[dev-dependencies]\nalias = { package = \"a\" }\n"),
        "a"
    ));
    assert!(cargo_requires(
        &parse("[target.'cfg(unix)'.dependencies]\na = \"1\"\n"),
        "a"
    ));
    assert!(!cargo_requires(&parse("[dependencies]\nb = \"1\"\n"), "a"));
}
