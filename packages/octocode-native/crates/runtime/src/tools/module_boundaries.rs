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
//! into a tool not listed in [`ALLOWED_EDGES`], on any edge from a shared
//! module into a tool, and on a listed edge that no longer exists, so the
//! list only shrinks.
//!
//! The same scanner keeps the crate's top-level modules layered
//! ([`LAYERS`]): production code of a module never names a module of a
//! higher layer, so the lower layers stay separable from tools and the
//! runtime engine.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

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

/// Tool-to-tool imports that stay, `(importer, imported, why)`. Each is one-way
/// (the imported tool never imports back). Remove the entry when the edge is
/// removed (the guard fails on a stale entry).
const ALLOWED_EDGES: &[(&str, &str, &str)] = &[
    (
        "gh_get_file_content",
        "local_fetch",
        "one view engine: ghGetFileContent renders fetched text through localFetch's view pipeline and result type",
    ),
    (
        "local_search",
        "local_fetch",
        "local_search leads size the localFetch reads they offer by its block and page limits",
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
        || path
            .components()
            .any(|part| part.as_os_str() == "tests" || part.as_os_str() == "test_support")
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

/// One code token the import scanner reads: an identifier, `::`, or one
/// punctuation character.
fn tokens(code: &str) -> Vec<&str> {
    let bytes = code.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_whitespace() {
            at += 1;
        } else if byte.is_ascii_alphanumeric() || byte == b'_' {
            let end = bytes[at..]
                .iter()
                .position(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
                .map_or(bytes.len(), |len| at + len);
            out.push(&code[at..end]);
            at = end;
        } else if bytes[at..].starts_with(b"::") {
            out.push("::");
            at += 2;
        } else {
            let width = code[at..].chars().next().map_or(1, char::len_utf8);
            out.push(&code[at..at + width]);
            at += width;
        }
    }
    out
}

fn is_ident(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
}

/// `path` as an absolute module path seen from `module`: `crate::…`,
/// `self::…`, `super::…` (any depth), or a name a `use` bound. `None` for
/// a path rooted elsewhere (an extern crate, a local item).
fn resolve(
    module: &[String],
    path: &[String],
    bindings: &BTreeMap<String, Vec<String>>,
) -> Option<Vec<String>> {
    let (first, rest) = path.split_first()?;
    let mut base = match first.as_str() {
        "crate" => Vec::new(),
        "self" => module.to_vec(),
        "super" => {
            let up = 1 + rest
                .iter()
                .take_while(|segment| *segment == "super")
                .count();
            let base = module[..module.len().checked_sub(up)?].to_vec();
            let mut resolved = base;
            resolved.extend(rest.iter().skip(up - 1).cloned());
            return Some(resolved);
        }
        name => bindings.get(name)?.clone(),
    };
    base.extend(rest.iter().cloned());
    Some(base)
}

/// One `use` tree from `tokens[*at]` under `prefix`: every leaf as
/// `(bound name, path)`; a glob binds nothing.
fn use_tree(
    tokens: &[&str],
    at: &mut usize,
    prefix: Vec<String>,
    leaves: &mut Vec<(Option<String>, Vec<String>)>,
) {
    let mut path = prefix;
    let grouped = !path.is_empty();
    while let Some(token) = tokens.get(*at).copied() {
        match token {
            "{" => {
                *at += 1;
                while tokens.get(*at).is_some_and(|token| *token != "}") {
                    use_tree(tokens, at, path.clone(), leaves);
                    if tokens.get(*at) == Some(&",") {
                        *at += 1;
                    }
                }
                *at += 1;
                return;
            }
            "*" => {
                *at += 1;
                leaves.push((None, path));
                return;
            }
            "self" if grouped && tokens.get(*at + 1) != Some(&"::") => {
                *at += 1;
            }
            token if is_ident(token) => {
                path.push(token.to_owned());
                *at += 1;
                if tokens.get(*at) == Some(&"::") {
                    *at += 1;
                    continue;
                }
            }
            _ => return,
        }
        let bound = if tokens.get(*at) == Some(&"as") {
            *at += 2;
            tokens.get(*at - 1).map(|alias| (*alias).to_owned())
        } else {
            path.last().cloned()
        };
        leaves.push((bound, path));
        return;
    }
}

/// Every absolute module path `code` (production code, no comments or
/// literals) names from `module`: each `use` leaf (groups, `self`, `super`,
/// aliases and globs included) and each inline path rooted at `crate`,
/// `self`, `super` or a `use`-bound name.
fn named_paths(code: &str, module: &[String]) -> Vec<Vec<String>> {
    let tokens = tokens(code);
    let mut bindings = BTreeMap::new();
    let mut found = Vec::new();
    let mut in_use = vec![false; tokens.len()];
    let mut at = 0;
    while at < tokens.len() {
        if tokens[at] == "use" && (at == 0 || tokens[at - 1] != "::") {
            let start = at;
            at += 1;
            if tokens.get(at) == Some(&"::") {
                at += 1;
            }
            let mut leaves = Vec::new();
            use_tree(&tokens, &mut at, Vec::new(), &mut leaves);
            for (bound, path) in leaves {
                let resolved = resolve(module, &path, &bindings).unwrap_or(path);
                if let Some(bound) = bound.filter(|bound| bound != "_") {
                    bindings.insert(bound, resolved.clone());
                }
                found.push(resolved);
            }
            in_use[start..at.min(tokens.len())].fill(true);
        } else {
            at += 1;
        }
    }
    let mut at = 0;
    while at < tokens.len() {
        let starts = !in_use[at]
            && is_ident(tokens[at])
            && tokens.get(at + 1) == Some(&"::")
            && (at == 0 || tokens[at - 1] != "::");
        if !starts {
            at += 1;
            continue;
        }
        let mut path = vec![tokens[at].to_owned()];
        at += 1;
        while tokens.get(at) == Some(&"::") && tokens.get(at + 1).is_some_and(|t| is_ident(t)) {
            path.push(tokens[at + 1].to_owned());
            at += 2;
        }
        found.extend(resolve(module, &path, &bindings));
    }
    found
}

/// The `tools/` modules a production source of `module` reaches.
fn imported_modules(source: &str, module: &[String]) -> BTreeSet<String> {
    let code =
        crate::tools::test_support::code_only(&crate::tools::test_support::production_text(source));
    named_paths(&code, module)
        .into_iter()
        .filter(|path| path.first().is_some_and(|root| root == "tools"))
        .filter_map(|path| path.get(1).cloned())
        .collect()
}

/// The module path of a source file under `tools_dir`: `x/y.rs` is
/// `tools::x::y`, `x/mod.rs` is `tools::x`.
fn module_of(tools_dir: &Path, path: &Path) -> Vec<String> {
    let relative = path.strip_prefix(tools_dir).unwrap_or(path);
    let mut module = vec!["tools".to_owned()];
    module.extend(
        relative
            .with_extension("")
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned()),
    );
    if module.last().is_some_and(|last| last == "mod") {
        module.pop();
    }
    module
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
    let allowed: BTreeSet<(String, String)> = ALLOWED_EDGES
        .iter()
        .map(|(from, to, _)| ((*from).to_owned(), (*to).to_owned()))
        .collect();
    let new: Vec<String> = edges
        .iter()
        .filter(|(edge, _)| !allowed.contains(*edge))
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
        new.is_empty(),
        "a module imports a tool's internals; move the shared piece to a neutral module (gh_shared, leads, result, …) both import:\n  {}",
        new.join("\n  ")
    );
    let stale: Vec<String> = allowed
        .iter()
        .filter(|edge| !edges.contains_key(*edge))
        .map(|(from, to)| format!("{from} -> {to}"))
        .collect();
    assert!(
        stale.is_empty(),
        "ALLOWED_EDGES lists an edge that no longer exists; remove it:\n  {}",
        stale.join("\n  ")
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

/// Every spelling Rust accepts for a path into another module resolves to
/// it: absolute and grouped `crate` paths, `super` and `self` at any depth,
/// a bound module and its alias, and inline paths. Comments, literals and
/// test-only items name nothing.
#[test]
fn the_scanner_resolves_every_import_form() {
    let tools = Path::new("/src/tools");
    let module = |file: &str| module_of(tools, &tools.join(file));
    let reached = |file: &str, source: &str| {
        imported_modules(source, &module(file))
            .into_iter()
            .collect::<Vec<_>>()
    };
    assert_eq!(module("local_search/mod.rs"), ["tools", "local_search"]);
    assert_eq!(
        module("local_search/a/b.rs"),
        ["tools", "local_search", "a", "b"]
    );
    for (file, source) in [
        ("local_search/mod.rs", "use super::local_fetch::A;"),
        ("local_search/mod.rs", "use crate::{tools::local_fetch::A};"),
        (
            "local_search/mod.rs",
            "use crate::tools::{local_fetch::{A, B}};",
        ),
        ("local_search/mod.rs", "fn f() { super::local_fetch::x(); }"),
        (
            "local_search/mod.rs",
            "fn f() { crate::tools::local_fetch::x(); }",
        ),
        ("local_search/a.rs", "use super::super::local_fetch::A;"),
        (
            "local_search/a/b.rs",
            "use super::super::super::local_fetch::{self};",
        ),
        (
            "local_search/mod.rs",
            "use crate::tools;\nfn f() { tools::local_fetch::x(); }",
        ),
        (
            "local_search/mod.rs",
            "use crate::tools as t;\nfn f() { t::local_fetch::x(); }",
        ),
        (
            "local_search/mod.rs",
            "use crate::{tools::{self as t}};\nfn f() { t::local_fetch::x(); }",
        ),
        (
            "local_search/mod.rs",
            "pub(crate) use crate::tools::local_fetch::*;",
        ),
        (
            "local_search/a.rs",
            "use super::super::{local_fetch as fetch};\nfn f() { fetch::x(); }",
        ),
    ] {
        assert_eq!(reached(file, source), ["local_fetch"], "{file}: {source}");
    }
    assert_eq!(
        reached(
            "local_search/a.rs",
            "use self::inner::A;\nuse super::helper;"
        ),
        ["local_search"]
    );
    assert_eq!(
        reached(
            "local_search/mod.rs",
            "use crate::tools::{id::ToolId, local_fetch::{A, B}, result};\n\
             fn f() { crate::tools::gh_shared::x(); }"
        ),
        ["gh_shared", "id", "local_fetch", "result"]
    );
    let nothing = "// crate::tools::lsp_search::ignored\n\
                   /* use super::clasify::x; */\n\
                   const S: &str = \"crate::tools::clasify::y\";\n\
                   #[cfg(test)]\nmod tests { use crate::tools::clasify::t; }\n\
                   #[cfg(test)]\nfn helper() { super::clasify::z(); }\n";
    assert!(reached("local_search/mod.rs", nothing).is_empty());
}
