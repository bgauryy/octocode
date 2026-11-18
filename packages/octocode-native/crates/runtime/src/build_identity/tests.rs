use super::BUILD_IDENTITY;

#[test]
fn the_identity_is_a_digest_every_cursor_kind_binds() {
    assert_eq!(BUILD_IDENTITY.len(), 16, "{BUILD_IDENTITY}");
    assert!(BUILD_IDENTITY.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(
        octocode_runtime_base::response::cursor::rows_snapshot(BUILD_IDENTITY, 10, "d")
            .contains(BUILD_IDENTITY)
    );
}

/// A row-shape or page-cut version is hand-bumped and can be forgotten: a
/// cursor-binding constant of this kind must not come back in any crate of
/// the workspace: the tools, the layers below them that cut the pages, or
/// the engine.
#[test]
fn no_hand_bumped_cursor_version_remains() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut stack = vec![crates];
    let mut found = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).expect("src").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && !path.ends_with("build_identity.rs")
            {
                let text = std::fs::read_to_string(&path).expect("source");
                for name in ["MATCH_ROWS_VERSION", "ROW_PLAN_VERSION", "SNAPSHOT_VERSION"] {
                    if text.contains(&format!("const {name}")) {
                        found.push(format!("{name} in {}", path.display()));
                    }
                }
            }
        }
    }
    assert!(found.is_empty(), "{found:?}");
}

/// A test-only module never reaches the binary: its file and child
/// directory, or the file its `#[path]` names, are left out of the
/// identity, so an edited unit test rebuilds its test target only. A file a
/// non-test declaration mounts by `#[path]` stays in.
#[test]
fn a_test_only_module_is_left_out_of_the_identity() {
    let dir = std::path::Path::new("crate").join("src");
    let text = "#[cfg(test)]\nmod tests;\n\
            #[cfg(all(test, unix))]\npub(crate) mod unix_tests;\n\
            #[cfg(any(test, feature = \"hooks\"))]\nmod hooks;\n\
            #[cfg(test)]\n#[path = \"shared.rs\"]\nmod shared;\n\
            #[cfg(test)] mod same_line;\n\
            #[cfg(test)]\nmod inline {\n    mod nested;\n}\n\
            #[cfg(all(\n    test,\n    target_os = \"linux\"\n))]\n/// Documented.\nmod wrapped;\n\
            #[cfg(not(test))]\nmod production;\n\
            #[cfg(test)]\n#[path = \"../up_tests.rs\"]\nmod up;\n\
            #[path = \"other.rs\"]\nmod renamed;\n\
            mod plain;\n";
    let child = |name: &str| dir.join("tool").join(name);
    let declared = super::inputs::declared_modules(&dir.join("tool.rs"), text);
    assert_eq!(
        declared.test_only,
        [
            child("tests.rs"),
            child("tests"),
            child("unix_tests.rs"),
            child("unix_tests"),
            dir.join("shared.rs"),
            child("same_line.rs"),
            child("same_line"),
            child("inline"),
            child("wrapped.rs"),
            child("wrapped"),
            std::path::Path::new("crate").join("up_tests.rs"),
        ]
    );
    assert_eq!(declared.named, [dir.join("other.rs")]);
    // A crate root's modules sit beside it.
    assert_eq!(
        super::inputs::declared_modules(&dir.join("lib.rs"), "#[cfg(test)]\nmod tests;\n")
            .test_only,
        [dir.join("tests.rs"), dir.join("tests")]
    );
}

/// On this crate: production sources and the build inputs a test mounts
/// are hashed, a unit-test file is not.
#[test]
fn the_identity_hashes_production_sources_and_skips_unit_tests() {
    let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("runtime");
    let sources = super::inputs::crate_sources(&runtime).expect("crate sources");
    let under = |root: &std::path::Path, relative: &str| {
        relative
            .split('/')
            .fold(root.to_path_buf(), |path, part| path.join(part))
    };
    let at = |relative: &str| under(&runtime, relative);
    for hashed in [
        "src/lib.rs",
        "src/build_identity.rs",
        "build/inputs.rs",
        "Cargo.toml",
    ] {
        assert!(sources.contains(&at(hashed)), "{hashed} is not hashed");
    }
    let unit_test = at("src/tools/hints_tests.rs");
    assert!(unit_test.is_file(), "{}", unit_test.display());
    assert!(!sources.contains(&unit_test), "a unit test is hashed");
    // A test file mounted by `#[cfg(test)] #[path]` is left out too: one of
    // `octocode-runtime-base`, a path dependency the identity hashes.
    let base = runtime
        .join("../runtime-base")
        .canonicalize()
        .expect("runtime-base");
    let base_sources = super::inputs::crate_sources(&base).expect("base sources");
    assert!(base_sources.contains(&under(&base, "src/lib.rs")));
    let mounted = under(&base, "src/response/workflow_lead_tests.rs");
    assert!(mounted.is_file(), "{}", mounted.display());
    assert!(
        !base_sources.contains(&mounted),
        "a path-mounted unit test is hashed"
    );
}

/// Crate directories reachable from `dir` through `path = ...` entries of
/// the manifests' build and normal dependency tables.
fn path_dependency_dirs(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
    let dir = dir.canonicalize().expect("crate directory");
    if found.contains(&dir) {
        return;
    }
    found.push(dir.clone());
    let manifest: toml::Table =
        toml::from_str(&std::fs::read_to_string(dir.join("Cargo.toml")).expect("manifest"))
            .expect("manifest parses");
    let mut tables = vec![];
    for key in ["dependencies", "build-dependencies"] {
        tables.extend(manifest.get(key).and_then(toml::Value::as_table));
    }
    let targets = manifest.get("target").and_then(toml::Value::as_table);
    for config in targets.into_iter().flat_map(toml::Table::values) {
        for key in ["dependencies", "build-dependencies"] {
            tables.extend(config.get(key).and_then(toml::Value::as_table));
        }
    }
    for dependency in tables.into_iter().flat_map(toml::Table::values) {
        if let Some(path) = dependency.get("path").and_then(toml::Value::as_str) {
            path_dependency_dirs(&dir.join(path), found);
        }
    }
}

/// A page is cut by the engine, the I/O layer and every crate the runtime
/// links, so the identity must cover each path dependency the manifests
/// name (a new crate included), the lockfile, and the contract it embeds.
#[test]
fn the_identity_hashes_every_path_dependency_the_manifests_name() {
    let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = runtime.join("../..").canonicalize().expect("workspace");
    let mut expected = Vec::new();
    path_dependency_dirs(runtime, &mut expected);
    assert!(expected.len() > 1, "no path dependency found: {expected:?}");
    // The build script's own walk finds the same crates.
    let mut found = super::inputs::path_dependency_dirs(runtime).expect("build script walk");
    found.sort();
    expected.sort();
    assert_eq!(found, expected);
    let hashed: Vec<_> = super::HASHED_ROOTS.split(';').collect();
    for dir in &expected {
        let name = dir.strip_prefix(&workspace).expect("inside workspace");
        assert!(
            hashed.contains(&name.to_string_lossy().as_ref()),
            "{name:?} is not hashed: {hashed:?}"
        );
    }
    assert!(hashed.contains(&"Cargo.lock"), "{hashed:?}");
    for (label, _) in super::inputs::contract_inputs(runtime) {
        assert!(hashed.contains(&label.as_str()), "{label} is not hashed");
    }
}
