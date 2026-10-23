use super::*;

#[test]
fn manifests_yield_components_dependencies_and_entries() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("packages/app/src/pages")).expect("dirs");
    std::fs::create_dir_all(root.join("crates/core/src/bin")).expect("dirs");
    std::fs::write(
        root.join("packages/app/package.json"),
        r#"{"name":"app","main":"dist/index.js","dependencies":{"react":"1"},"devDependencies":{"vitest":"1","@types/node":"1"}}"#,
    )
    .expect("pkg");
    std::fs::write(
        root.join("crates/core/Cargo.toml"),
        "[package]\nname = \"core-lib\"\n[dependencies]\nserde-json = \"1\"\n[dev-dependencies]\ntempfile = \"3\"\n",
    )
    .expect("cargo");
    let files = [
        "packages/app/src/index.ts",
        "packages/app/src/pages/home.tsx",
        "packages/app/src/util.ts",
        "crates/core/src/lib.rs",
        "crates/core/src/bin/tool.rs",
        "crates/core/src/x.rs",
    ];
    let ws = Workspace::discover(
        root,
        &files,
        &BTreeSet::new(),
        &|_| SourceMarks::default(),
        &|path| std::fs::read_to_string(path).ok(),
    );
    assert_eq!(
        ws.entries.get("packages/app/src/index.ts"),
        Some(&"package.json")
    );
    assert!(ws.entries.contains_key("packages/app/src/pages/home.tsx"));
    assert!(ws.entries.contains_key("crates/core/src/lib.rs"));
    assert!(ws.entries.contains_key("crates/core/src/bin/tool.rs"));
    assert!(!ws.entries.contains_key("packages/app/src/util.ts"));

    let file = "packages/app/src/util.ts";
    assert_eq!(ws.declared(file, "react", "npm"), Declared::Runtime);
    assert_eq!(ws.declared(file, "vitest", "npm"), Declared::Dev);
    assert_eq!(ws.declared(file, "node:fs", "npm"), Declared::Builtin);
    assert_eq!(ws.declared(file, "lodash", "npm"), Declared::Undeclared);
    assert_eq!(
        ws.declared("crates/core/src/x.rs", "serde_json", "cargo"),
        Declared::Runtime
    );
    assert_eq!(
        ws.declared("crates/core/src/x.rs", "tempfile", "cargo"),
        Declared::Dev
    );
    assert_eq!(
        ws.declared("crates/core/src/x.rs", "std", "cargo"),
        Declared::Builtin
    );
    assert_eq!(
        ws.component_of(file, "npm").map(|c| c.name.as_str()),
        Some("app")
    );
}
