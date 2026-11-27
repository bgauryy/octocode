//! The files Cargo compiles as a crate's roots, from its manifest and its layout:
//! `[lib]`, `[[bin]]`, `[[example]]`, `[[test]]`, `[[bench]]` and `build` paths, and
//! what target discovery finds unless the manifest turns it off. A root is what
//! nothing mounts: its modules lie beside it, and whether it is test code is what
//! kind of target it is. A file both a root and a module of another root is
//! compiled as a root anyway.
use super::{TestCodeIndex, lock};
use octocode_engine::text::lexical;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use toml::Value;

/// A crate's target roots by whether a plain build compiles them.
#[derive(Default)]
pub(super) struct CrateTargets {
    production: HashSet<PathBuf>,
    tests: HashSet<PathBuf>,
}

impl CrateTargets {
    fn of(dir: &Path, manifest: &Value, listing: &dyn Fn(&Path) -> Vec<(PathBuf, bool)>) -> Self {
        let mut targets = Self::default();
        let Some(package) = manifest.get("package") else {
            return targets;
        };
        let at = |path: &str| lexical(&dir.join(path));
        let lib = manifest.get("lib").and_then(|lib| lib.get("path"));
        let lib = lib.and_then(Value::as_str).unwrap_or("src/lib.rs");
        targets.production.insert(at(lib));
        targets.production.insert(at("src/main.rs"));
        match package.get("build") {
            Some(Value::String(build)) => {
                targets.production.insert(at(build));
            }
            Some(Value::Boolean(false)) => {}
            _ => {
                targets.production.insert(at("build.rs"));
            }
        }
        let modern = package
            .get("edition")
            .and_then(Value::as_str)
            .is_some_and(|edition| edition != "2015");
        for (table, auto, folder, under_test) in [
            ("bin", "autobins", "src/bin", false),
            ("example", "autoexamples", "examples", false),
            ("bench", "autobenches", "benches", false),
            ("test", "autotests", "tests", true),
        ] {
            let explicit = manifest
                .get(table)
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            let side = if under_test {
                &mut targets.tests
            } else {
                &mut targets.production
            };
            for target in explicit {
                match target.get("path").and_then(Value::as_str) {
                    Some(path) => {
                        side.insert(at(path));
                    }
                    None => {
                        let Some(name) = target.get("name").and_then(Value::as_str) else {
                            continue;
                        };
                        side.insert(at(&format!("{folder}/{name}.rs")));
                        side.insert(at(&format!("{folder}/{name}/main.rs")));
                    }
                }
            }
            // Edition 2015 turns discovery off once a target of the kind is listed.
            let discovered = package
                .get(auto)
                .and_then(Value::as_bool)
                .unwrap_or(modern || explicit.is_empty());
            if discovered {
                for (entry, is_dir) in listing(&dir.join(folder)) {
                    if is_dir {
                        side.insert(lexical(&entry.join("main.rs")));
                    } else if entry.extension().is_some_and(|extension| extension == "rs") {
                        side.insert(lexical(&entry));
                    }
                }
            }
        }
        targets
    }
}

impl TestCodeIndex {
    /// The crate directory holding `dir`, up to the workspace root.
    pub(super) fn crate_dir(&self, dir: &Path) -> Option<PathBuf> {
        self.crate_dirs.of(dir, || match self.level_above(dir) {
            Some(up) => self.crate_dir(up),
            None => self
                .is_file(&dir.join("Cargo.toml"))
                .then(|| dir.to_path_buf()),
        })
    }

    fn targets_of(&self, top: &Path) -> Arc<CrateTargets> {
        self.crate_targets.of(top, || {
            let manifest = self
                .source(&top.join("Cargo.toml"))
                .and_then(|text| toml::from_str::<Value>(&text).ok());
            let listing = |folder: &Path| {
                lock(&self.read).insert(folder.to_path_buf());
                std::fs::read_dir(folder)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|entry| {
                        let kind = entry.file_type().ok()?;
                        Some((entry.path(), kind.is_dir()))
                    })
                    .collect::<Vec<_>>()
            };
            Arc::new(manifest.map_or_else(CrateTargets::default, |manifest| {
                CrateTargets::of(top, &manifest, &listing)
            }))
        })
    }

    /// Whether `path` is a crate root Cargo compiles, and if so whether only
    /// under test: a plain build's root wins over a test target's.
    pub(super) fn crate_target(&self, path: &Path) -> Option<bool> {
        let targets = self.targets_of(&self.crate_dir(path.parent()?)?);
        let path = lexical(path);
        if targets.production.contains(&path) {
            return Some(false);
        }
        targets.tests.contains(&path).then_some(true)
    }

    /// The existing crate roots directly in `dir`.
    pub(super) fn crate_roots_in(&self, dir: &Path) -> Vec<PathBuf> {
        let Some(top) = self.crate_dir(dir) else {
            return Vec::new();
        };
        let targets = self.targets_of(&top);
        let dir = lexical(dir);
        targets
            .production
            .iter()
            .chain(&targets.tests)
            .filter(|root| root.parent() == Some(dir.as_path()) && self.is_file(root))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests;
