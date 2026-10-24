//! Module-boundary guard: a tool module (every `tools/` entry not listed in
//! [`tool_edges`]' `SHARED_LAYERS`) reaches shared code, never another
//! tool's internals; a shared module reaches no tool, and only shared
//! modules of a lower layer, so the shared modules form no cycle. Two
//! modules that import each other cannot be changed or tested apart; a
//! primitive both need lives in a lower shared module both import.
//!
//! The guard resolves every path the production code of each `tools/` module
//! (and of `tools.rs` itself) names (test files and `#[cfg(test)]` items
//! are skipped: tests may drive several tools together): `use` trees with
//! groups, `self`, `super` and aliases, and inline `crate::`/`super::`/bound
//! paths.
//!
//! The same scanner keeps the crate's top-level modules layered
//! ([`LAYERS`]): production code of a module never names a module of a
//! higher layer, so the lower layers stay separable from tools and the
//! runtime engine.

use std::path::{Path, PathBuf};

mod scanner;
mod tool_edges;

use scanner::named_paths;

/// The crate's top-level modules, lowest layer first. A module imports only
/// its own layer's lower neighbours and below; a new top-level module is
/// placed here.
const LAYERS: &[&[&str]] = &[
    &[
        "civil_date",
        "digest",
        "id",
        "num",
        "private_file",
        "process_status",
    ],
    &["content", "contracts"],
    &["policy"],
    &["config", "security"],
    &["execution"],
    &["response"],
    &["cache"],
    &["providers"],
    &["tools"],
    &["runtime"],
];

/// Whether `path` is mounted only for tests: it, or a module above it, is
/// declared `mod <name>;` under `#[cfg(test)]`.
fn mounted_for_tests(src: &Path, path: &Path) -> bool {
    // `a/b.rs` is module `b`, declared in `a.rs` (in `lib.rs` at the root).
    let (Some(name), Some(dir)) = (
        path.file_stem().and_then(|name| name.to_str()),
        path.parent(),
    ) else {
        return false;
    };
    let parent = if dir == src {
        src.join("lib.rs")
    } else {
        dir.with_extension("rs")
    };
    let Ok(text) = std::fs::read_to_string(&parent) else {
        return false;
    };
    let declaration = format!("mod {name};");
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let gated = lines.iter().enumerate().any(|(at, line)| {
        line.ends_with(&declaration)
            && lines[..at]
                .iter()
                .rev()
                .find(|line| !line.is_empty())
                .is_some_and(|line| line.starts_with("#[cfg(") && line.contains("test"))
    });
    gated || (dir != src && mounted_for_tests(src, &parent))
}

fn layer_of(module: &str) -> Option<usize> {
    LAYERS.iter().position(|layer| layer.contains(&module))
}

fn is_test_file(path: &Path) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    stem == "tests"
        || stem == "test_support"
        || stem.ends_with("_tests")
        || path.components().any(|part| {
            let part = part.as_os_str().to_str().unwrap_or("");
            part == "tests" || part == "test_support" || part.ends_with("_tests")
        })
}

fn rust_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().is_some_and(|ext| ext == "rs") && !is_test_file(path) {
            out.push(path.to_owned());
        }
        return;
    }
    let mut entries: Vec<_> = std::fs::read_dir(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for entry in entries {
        rust_files(&entry, out);
    }
}

/// The production files of module `name` under `dir`: its `name.rs` and
/// everything under `name/`.
fn module_files(dir: &Path, name: &str, out: &mut Vec<PathBuf>) {
    for root in [dir.join(format!("{name}.rs")), dir.join(name)] {
        if root.exists() {
            rust_files(&root, out);
        }
    }
}

/// Production code never imports a higher layer: `id` and `num` below the
/// contracts, the request kernel (`execution`) below response shaping, and
/// response below providers, tools and the runtime engine that drives them.
#[test]
fn top_level_modules_import_only_lower_layers() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    let mut tops: Vec<String> = std::fs::read_dir(&src)
        .expect("read src/")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_stem()?.to_str()?.to_owned();
            (name != "lib" && !mounted_for_tests(&src, &path)).then_some(name)
        })
        .collect();
    tops.sort();
    tops.dedup();
    for top in &tops {
        let layer = layer_of(top)
            .unwrap_or_else(|| panic!("src/{top} is in no LAYERS entry: place it there"));
        let mut files = Vec::new();
        module_files(&src, top, &mut files);
        for path in files {
            if mounted_for_tests(&src, &path) {
                continue;
            }
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let code = crate::tools::test_support::code_only(
                &crate::tools::test_support::production_text(&source),
            );
            let mut module = vec![top.clone()];
            let relative = path.strip_prefix(src.join(top)).unwrap_or(Path::new(""));
            module.extend(
                relative
                    .with_extension("")
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned()),
            );
            for named in named_paths(&code, &module) {
                let Some(imported) = named.first() else {
                    continue;
                };
                if let Some(above) = layer_of(imported).filter(|above| *above > layer) {
                    let shown = path
                        .strip_prefix(&src)
                        .unwrap_or(&path)
                        .display()
                        .to_string();
                    violations.push(format!(
                        "{shown} ({top}, layer {layer}) -> {imported} (layer {above})"
                    ));
                }
            }
        }
    }
    violations.sort();
    violations.dedup();
    assert!(
        violations.is_empty(),
        "a module imports a higher layer; move the shared piece down or pass it in through a trait the lower layer defines:\n  {}",
        violations.join("\n  ")
    );
}
