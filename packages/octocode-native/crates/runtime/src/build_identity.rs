//! The one identity every saved cursor, snapshot and replay plan binds to:
//! a digest of this build's runtime sources (`build.rs`). A cursor minted by
//! another build fails its snapshot check and restarts, so no hand-bumped
//! version can be forgotten when a row shape or page cut changes.

/// Differs between builds of different sources.
pub(crate) const BUILD_IDENTITY: &str = env!("OCTOCODE_BUILD_IDENTITY");

/// Every root `build.rs` hashed into [`BUILD_IDENTITY`], relative to the
/// workspace, separated by `;` (cargo env values hold no newline).
#[cfg(test)]
const HASHED_ROOTS: &str = env!("OCTOCODE_BUILD_IDENTITY_ROOTS");

/// The input list `build.rs` shares with the contract embedding.
#[cfg(test)]
#[path = "../build/inputs.rs"]
mod inputs;

#[cfg(test)]
mod tests {
    use super::BUILD_IDENTITY;

    #[test]
    fn the_identity_is_a_digest_every_cursor_kind_binds() {
        assert_eq!(BUILD_IDENTITY.len(), 16, "{BUILD_IDENTITY}");
        assert!(BUILD_IDENTITY.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(crate::response::cursor::rows_snapshot(10, "d").contains(BUILD_IDENTITY));
    }

    /// A row-shape or page-cut version is hand-bumped and can be forgotten: a
    /// cursor-binding constant of this kind must not come back.
    #[test]
    fn no_hand_bumped_cursor_version_remains() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![root];
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
}
