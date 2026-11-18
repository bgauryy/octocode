#[test]
fn conflicting_conditional_aliases_and_registry_names_do_not_fabricate_edges() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let package = |name: &str, dependencies: serde_json::Value| {
        serde_json::json!({
            "name":name, "manifest_path":root.join(name).join("Cargo.toml"),
            "targets":[{"name":name, "kind":["lib"], "src_path":root.join(name).join("src/lib.rs")}],
            "dependencies":dependencies,
        })
    };
    let value = serde_json::json!({"packages":[
        package("app", serde_json::json!([
            {"name":"one", "rename":"shared", "path":root.join("one"), "target":"cfg(unix)"},
            {"name":"two", "rename":"shared", "path":root.join("two"), "target":"cfg(windows)"},
            {"name":"one", "rename":"mixed", "path":root.join("one"), "target":"cfg(unix)"},
            {"name":"two", "rename":"mixed", "source":"registry+https://example.invalid/index", "target":"cfg(windows)"},
            {"name":"one", "source":"registry+https://example.invalid/index"},
        ])),
        package("one", serde_json::json!([])),
        package("two", serde_json::json!([])),
    ]});
    let crates = super::parse_cargo_crates(&root, &value);
    assert_eq!(crates.resolve("app/src/lib.rs", "shared"), None);
    assert_eq!(crates.resolve("app/src/lib.rs", "mixed"), None);
    assert_eq!(crates.resolve("app/src/lib.rs", "one"), None);
    assert_eq!(
        crates.resolve("app/src/lib.rs", "app").map(String::as_str),
        Some("app/src/lib.rs")
    );
}
