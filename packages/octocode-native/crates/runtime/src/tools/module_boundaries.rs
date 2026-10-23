//! Module-boundary guard: a tool module (every `tools/` entry not listed in
//! [`SHARED_MODULES`]) reaches shared code, never another tool's internals,
//! and a shared module reaches no tool at all. Two tools that import each
//! other's modules form a cycle and cannot be changed or tested apart; a
//! primitive two tools need lives in a shared module both import.
//!
//! The guard resolves every path the production code of each `tools/` module
//! names (test files and `#[cfg(test)]` items are skipped: tests may drive
//! several tools together): `use` trees with groups, `self`, `super` and
//! aliases, and inline `crate::`/`super::`/bound paths. It fails on any edge
//! from a tool into another tool and on any edge from a shared module into a
//! tool.
//!
//! The same scanner keeps the crate's top-level modules layered
//! ([`LAYERS`]): production code of a module never names a module of a
//! higher layer, so the lower layers stay separable from tools and the
//! runtime engine.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod scanner;

use scanner::{imported_modules, module_of, named_paths};

/// Modules under `tools/` that are not tools: primitives any tool may import.
/// A new `tools/` entry is a tool unless it is listed here, with its reason.
const SHARED_MODULES: &[(&str, &str)] = &[
    ("ast_rule", "AST rule parsing shared by the ast tools"),
    ("bounded_process", "bounded subprocess runner"),
    (
        "declarations_cache",
        "single-file declaration extraction cache",
    ),
    (
        "empty_walk",
        "why an empty local walk is empty: the skip causes that held matches, the rules that named them, the flags that lift them",
    ),
    ("gh_shared", "GitHub failure, paging and history helpers"),
    (
        "judgment",
        "the one classification evaluator: provider client, judgment cache, single flight, accounting",
    ),
    ("leads", "follow-up queries one tool offers into another"),
    ("line_spans", "line-span arithmetic"),
    (
        "selection",
        "shared bounded relevance ordering for local search files and fetch windows",
    ),
    ("match_keys", "JSON/TOML key matching"),
    (
        "match_list",
        "the regex alternation a matchString list means",
    ),
    ("module_boundaries", "this guard"),
    ("page_memo", "page memoization"),
    (
        "variant_field",
        "shared-field accessors over generated query enums",
    ),
    (
        "root_read",
        "the file a directory listing leads to first, ranked once for remote and local listings",
    ),
    ("source", "bounded source reads"),
    (
        "source_view",
        "the one view engine over fetched source text: localFetch and ghGetFileContent render through it, and local_search sizes its read leads by its limits",
    ),
    (
        "source_capture",
        "one complete source selection captured through the authorized readers as a classification state",
    ),
    (
        "source_sections",
        "source-coordinate section planning shared by localFetch and clasify",
    ),
    ("test_support", "test fixtures"),
    (
        "workers",
        "the one per-query fan-out width: the batch's core share, a cap, the items",
    ),
];

/// The crate's top-level modules, lowest layer first. A module imports only
/// its own layer's lower neighbours and below; a new top-level module is
/// placed here.
const LAYERS: &[&[&str]] = &[
    &[
        "civil_date",
        "digest",
        "git_head",
        "id",
        "num",
        "private_file",
        "process_status",
        "regex_repair",
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
    // `a/b.rs` and `a/b/mod.rs` are both module `b`, declared in `a`'s file.
    let (name, dir) = if path.file_stem().is_some_and(|stem| stem == "mod") {
        (
            path.parent().and_then(Path::file_name),
            path.parent().and_then(Path::parent),
        )
    } else {
        (path.file_stem(), path.parent())
    };
    let (Some(name), Some(dir)) = (name.and_then(|name| name.to_str()), dir) else {
        return false;
    };
    let parent = if dir == src {
        src.join("lib.rs")
    } else if dir.join("mod.rs").is_file() {
        dir.join("mod.rs")
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

/// Every `(importer, imported)` pair from a `tools/` module (a tool or a
/// shared module) to a tool, with one example file.
fn module_edges(tools_dir: &Path) -> BTreeMap<(String, String), String> {
    let shared: BTreeSet<&str> = SHARED_MODULES.iter().map(|(name, _)| *name).collect();
    let modules: BTreeSet<String> = std::fs::read_dir(tools_dir)
        .expect("read tools/")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_stem()?.to_str()?.to_owned();
            (name != "mod" && name != "module_boundaries" && name != "test_support").then_some(name)
        })
        .collect();
    for (name, _) in SHARED_MODULES {
        assert!(
            tools_dir.join(name).exists() || tools_dir.join(format!("{name}.rs")).exists(),
            "SHARED_MODULES lists `{name}`, which is not under tools/: remove it"
        );
    }
    let mut edges = BTreeMap::new();
    for importer in &modules {
        let dir = tools_dir.join(importer);
        let file = tools_dir.join(format!("{importer}.rs"));
        let root = if dir.is_dir() { dir } else { file };
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        for path in files {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            for imported in imported_modules(&source, &module_of(tools_dir, &path)) {
                if &imported != importer
                    && modules.contains(&imported)
                    && !shared.contains(imported.as_str())
                {
                    let shown = path
                        .strip_prefix(tools_dir)
                        .unwrap_or(&path)
                        .display()
                        .to_string();
                    edges.entry((importer.clone(), imported)).or_insert(shown);
                }
            }
        }
    }
    edges
}

#[test]
fn tool_modules_import_only_shared_modules() {
    let tools_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools");
    let shared: BTreeSet<&str> = SHARED_MODULES.iter().map(|(name, _)| *name).collect();
    let edges = module_edges(&tools_dir);
    let violations: Vec<String> = edges
        .iter()
        .map(|((from, to), file)| {
            let kind = if shared.contains(from.as_str()) {
                "shared module"
            } else {
                "tool"
            };
            format!("{kind} {from} -> {to} (e.g. tools/{file})")
        })
        .collect();
    assert!(
        violations.is_empty(),
        "a module imports a tool's internals; move the shared piece to a neutral module (gh_shared, leads, result, …) both import:\n  {}",
        violations.join("\n  ")
    );
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
        let dir = src.join(top);
        let root = if dir.is_dir() {
            dir
        } else {
            src.join(format!("{top}.rs"))
        };
        let mut files = Vec::new();
        rust_files(&root, &mut files);
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
            if module.last().is_some_and(|last| last == "mod") {
                module.pop();
            }
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
