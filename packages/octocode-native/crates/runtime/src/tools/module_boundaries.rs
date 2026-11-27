//! Module-boundary guards the crate graph cannot express, one crate at a time.
//!
//! [`tool_edges`] keeps the tool crates apart inside each crate: in
//! `octocode-tools-shared` a module reaches only modules of a lower layer, so
//! the shared modules form no cycle; in a tool family crate a tool reaches no
//! other tool. The crate graph (`tests/architecture_guards.rs` `CRATE_EDGES`)
//! keeps one family from another, tools-shared from every tool, and every tool
//! from the runtime.
//!
//! The guard resolves every path the production code of each module names
//! (test files and `#[cfg(test)]` items are skipped: tests may drive several
//! modules together): `use` trees with groups, `self`, `super` and aliases,
//! and inline `crate::`/`super::`/bound paths.
//!
//! The same scanner keeps each crate's top-level modules layered
//! ([`LAYERS`] for this crate, [`BASE_LAYERS`] for `octocode-runtime-base`,
//! [`ENGINE_LAYERS`] for `octocode-engine`): production code of a module never
//! names a module of a higher layer, so the lower layers stay separable from
//! the ones that drive them.

use std::path::{Path, PathBuf};

mod scanner;
mod tool_edges;

use scanner::named_paths;

/// The crate's top-level production modules, lowest layer first. A module
/// imports only its own layer's lower neighbours and below; a new top-level
/// module is placed here. The tools are crates of their own below this one.
const LAYERS: &[&[&str]] = &[&["build_identity"], &["runtime"]];

/// `octocode-runtime-base`'s top-level modules, lowest layer first: the
/// identifiers and numbers, the contracts, policy, configuration and security,
/// the request kernel (`execution`), response shaping, retained work and the
/// providers.
const BASE_LAYERS: &[&[&str]] = &[
    &[
        "civil_date",
        "digest",
        "id",
        "json_pointer",
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
];

/// `octocode-engine`'s top-level modules, lowest layer first: shared
/// vocabulary and text below the DTOs (`types`), the DTOs below the parse and
/// graph-fact engines, the parses below the compact view (`minify`), those below redaction, redaction below the text search
/// that judges a long line while it holds it, and all of them below the
/// language-server pool, structural matching and fact scans that compose them.
const ENGINE_LAYERS: &[&[&str]] = &[
    &[
        "contain",
        "cpu",
        "deep_stack",
        "host",
        "json_size",
        "memory",
        "num",
        "process_env",
        "scan_budget",
        "stamp",
        "symbol_kind",
    ],
    &["digest"],
    &["text"],
    &["error", "graph"],
    &["process", "types"],
    &["signatures"],
    // Compact views read comments and strings from the grammars' parses.
    &["minify"],
    &["security"],
    &["search"],
    &["fact_scan", "lsp", "structural"],
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

fn layer_of(layers: &[&[&str]], module: &str) -> Option<usize> {
    layers.iter().position(|layer| layer.contains(&module))
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

/// Every production `.rs` file under `path`, sorted.
pub(super) fn rust_files(path: &Path, out: &mut Vec<PathBuf>) {
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

/// Every import in `src`'s production code from a top-level module into a
/// module of a higher layer of `layers`.
fn upward_imports(src: &Path, layers: &[&[&str]]) -> Vec<String> {
    let mut violations = Vec::new();
    let mut tops: Vec<String> = std::fs::read_dir(src)
        .expect("read src/")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_stem()?.to_str()?.to_owned();
            (name != "lib" && !mounted_for_tests(src, &path)).then_some(name)
        })
        .collect();
    tops.sort();
    tops.dedup();
    for top in &tops {
        let layer = layer_of(layers, top)
            .unwrap_or_else(|| panic!("{}/{top} is in no layer: place it there", src.display()));
        let mut files = Vec::new();
        module_files(src, top, &mut files);
        for path in files {
            if mounted_for_tests(src, &path) {
                continue;
            }
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let code = octocode_runtime_base::test_support::code_only(
                &octocode_runtime_base::test_support::production_text(&source),
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
                if let Some(above) = layer_of(layers, imported).filter(|above| *above > layer) {
                    let shown = path
                        .strip_prefix(src)
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
    violations
}

const UPWARD: &str = "a module imports a higher layer; move the shared piece down or pass it in through a trait the lower layer defines";

/// Production code never imports a higher layer: `id` and `num` below the
/// contracts, the request kernel (`execution`) below response shaping, and
/// response below providers (`octocode-runtime-base`); the build identity
/// below the runtime engine that drives the tools.
#[test]
fn top_level_modules_import_only_lower_layers() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = upward_imports(&crate_dir.join("src"), LAYERS);
    violations.extend(upward_imports(
        &crate_dir.join("../runtime-base/src"),
        BASE_LAYERS,
    ));
    assert!(
        violations.is_empty(),
        "{UPWARD}:\n  {}",
        violations.join("\n  ")
    );
}

/// The engine's modules form no cycle: each imports only its own and lower
/// layers of [`ENGINE_LAYERS`].
#[test]
fn engine_modules_import_only_lower_layers() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../engine/src");
    let violations = upward_imports(&src, ENGINE_LAYERS);
    assert!(
        violations.is_empty(),
        "{UPWARD}:\n  {}",
        violations.join("\n  ")
    );
}

/// A tool's own file walk starts from `octocode_tools_shared::pruned_walk*`
/// (the default prune, credential stores and ignore policy every walk shares),
/// never a raw `ignore::WalkBuilder`: a prune rule changed there reaches every
/// walk. Only `octocode-tools-shared`'s `lib.rs` and `policy/` (in
/// `octocode-runtime-base`) build one. (The engine's own walks live in its
/// crate.)
#[test]
fn tool_walks_start_from_the_shared_pruned_walk() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates directory");
    let builders = [
        crates.join("tools-shared/src/lib.rs"),
        crates.join("runtime-base/src/policy"),
    ];
    let raw = ["WalkBuilder", "::new("].concat();
    let mut violations = Vec::new();
    for src in octocode_runtime_base::test_support::runtime_crate_sources() {
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        violations.extend(
            files
                .iter()
                .filter(|file| !builders.iter().any(|builder| file.starts_with(builder)))
                .filter(|file| !mounted_for_tests(&src, file))
                .filter(|file| std::fs::read_to_string(file).is_ok_and(|text| text.contains(&raw)))
                .map(|file| file.display().to_string()),
        );
    }
    assert!(
        violations.is_empty(),
        "start these walks from octocode_tools_shared::pruned_walk*:\n  {}",
        violations.join("\n  ")
    );
}
