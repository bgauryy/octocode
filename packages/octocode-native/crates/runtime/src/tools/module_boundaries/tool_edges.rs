//! The tool crates' guard. In `octocode-tools-shared` a module imports only
//! modules of a lower layer of [`SHARED_LAYERS`], so no two shared modules
//! import each other. In a tool family crate (`crates/tool-*`) a tool imports
//! no other tool of its crate, only the crate's modules listed in
//! [`NON_TOOLS`]. The crate graph keeps the families from each other and
//! tools-shared from every tool, so this guard reads one crate at a time.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::scanner::{imported_modules, module_of, root_items};
use super::{module_files, mounted_for_tests};

/// The crate root (`lib.rs`) as a node: in `octocode-tools-shared` its walk,
/// path and outline helpers (`pruned_walk`, `display_name`) form the lowest
/// layer, so they import no module of the crate.
const ROOT: &str = "lib";

/// One layer: its modules, each with the reason it sits there.
type Layer = &'static [(&'static str, &'static str)];

/// The modules of `octocode-tools-shared`: primitives any tool may import,
/// lowest layer first. A module imports only modules of a lower layer, never
/// one of its own layer or above, so they form no cycle. A new module of the
/// crate is placed here, with its reason.
const SHARED_LAYERS: &[Layer] = &[
    &[(
        ROOT,
        "lib.rs: walk, path and outline helpers every layer may use",
    )],
    // Import no other module, so every shared module may size its fan-out
    // and keep by the store.
    &[
        (
            "workers",
            "the one per-query fan-out width: the batch's core share, a cap, the items",
        ),
        ("page_memo", "results kept for continuation pages"),
    ],
    &[
        ("bounded_process", "bounded subprocess runner"),
        (
            "declarations",
            "single-file declaration facts, kept by text digest in the one process store",
        ),
        (
            "editor_lock",
            "the editor extension's lock files: trust, liveness, and whether an editor answers for a file",
        ),
        (
            "empty_walk",
            "why an empty local walk is empty: the skip causes that held matches, the rules that named them, the flags that lift them",
        ),
        ("gh_shared", "GitHub failure, paging and history helpers"),
        (
            "judgment",
            "the one classification evaluator: provider client, request-owned single flight, accounting",
        ),
        ("line_spans", "line-span arithmetic"),
        (
            "same_line",
            "which rows state their column: a line and text repeated within the whole result",
        ),
        ("match_keys", "JSON/TOML key matching"),
        (
            "match_list",
            "the regex alternation a matchString list means",
        ),
        (
            "root_read",
            "the file a directory listing leads to first, ranked once for remote and local listings",
        ),
        ("source", "bounded source reads"),
        (
            "variant_field",
            "shared-field accessors over generated query enums",
        ),
    ],
    &[
        // Reads empty_walk's walk and globs.
        ("ast_rule", "AST rule parsing shared by the ast tools"),
        // Runs git through bounded_process.
        (
            "gh_git",
            "system git for GitHub transfers (runner, one limiter-admitted step, endpoint git URLs, blob-only fetches): ghCloneRepo checks out through it and ghStructure materialize reads blobs through it",
        ),
        // Reads source's bounded reads for the configuration beside the sources.
        (
            "import_resolution",
            "how a project's settings place an import (path folding, JS module probing, tsconfig, workspaces, Python roots, C includes): the graph links by it and lspSearch places importer candidates by it",
        ),
        // Reads source's bounded reads.
        (
            "judged_rows",
            "the whole-file redaction judgment a row's text is cut from, read once a request",
        ),
        ("leads", "follow-up queries one tool offers into another"),
        (
            "selection",
            "bounded relevance ordering for local search files and fetch windows, judged through the judgment evaluator",
        ),
        // Plans over declarations's outlines.
        (
            "source_sections",
            "source-coordinate section planning shared by localFetch and clasify",
        ),
        // Reads source's bounded reads and declarations's parse.
        (
            "test_code",
            "the one test-code rule (paths, Rust mounts, cfg(test) lines) every local search scopes by",
        ),
    ],
    &[
        // Discloses a glob's skips through leads's skipped-path record.
        (
            "source_capture",
            "one complete source selection captured through the authorized readers as a classification state",
        ),
        (
            "source_view",
            "the one view engine over fetched source text: localFetch and ghGetFileContent render through it, and local_search sizes its read leads by its limits",
        ),
    ],
    &[
        // Renders its windows through source_view and keeps a pass in page_memo.
        (
            "large_source",
            "the one streamed judgment of a source past the whole-file limit: localFetch serves its windows and localSearch cuts its rows from the lines it serves",
        ),
    ],
];

/// The modules of a tool family crate that are not tools, lowest layer first,
/// with their reasons; every other top-level module of a family is a tool.
const NON_TOOLS: &[(&str, &[Layer])] = &[
    (
        "tool-github",
        &[&[(
            "qualifier_keywords",
            "which field carries a search qualifier a keyword spells: ghSearchCode, ghSearchHistory and ghSearchRepo name it in an empty answer",
        )]],
    ),
    (
        "tool-packages",
        &[&[(
            "artifact",
            "the package-registry providers artifactSearch and securityCheck read",
        )]],
    ),
];

/// Where `module` sits in `layers`, or `None` for a tool.
fn layer_in(layers: &[Layer], module: &str) -> Option<usize> {
    layers
        .iter()
        .position(|layer| layer.iter().any(|(name, _)| *name == module))
}

/// The top-level production modules of the crate at `src`: every entry but
/// `lib.rs` and the modules mounted only for tests.
fn top_modules(src: &Path) -> BTreeSet<String> {
    std::fs::read_dir(src)
        .unwrap_or_else(|error| panic!("read {}: {error}", src.display()))
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_stem()?.to_str()?.to_owned();
            (name != ROOT && !mounted_for_tests(src, &path)).then_some(name)
        })
        .collect()
}

/// Every `(importer, imported)` pair between distinct top-level modules of the
/// crate at `src` (and [`ROOT`] for the items its `lib.rs` defines), with one
/// example file.
fn module_edges(src: &Path) -> BTreeMap<(String, String), String> {
    let root_file = src.join("lib.rs");
    let roots = root_items(&std::fs::read_to_string(&root_file).unwrap_or_default());
    let mut modules = top_modules(src);
    modules.insert(ROOT.to_owned());
    let mut edges = BTreeMap::new();
    for importer in &modules {
        let mut files = Vec::new();
        if importer == ROOT {
            files.push(root_file.clone());
        } else {
            module_files(src, importer, &mut files);
        }
        for path in files {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            for imported in imported_modules(&source, &module_of(src, &path)) {
                let imported = if roots.contains(&imported) {
                    ROOT.to_owned()
                } else {
                    imported
                };
                if &imported != importer && modules.contains(&imported) {
                    let shown = path
                        .strip_prefix(src)
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

/// The edges the guard rejects: tool → tool, non-tool → tool, and non-tool →
/// non-tool unless the imported module's layer is lower.
fn violations(edges: &BTreeMap<(String, String), String>, layers: &[Layer]) -> Vec<String> {
    edges
        .iter()
        .filter_map(|((from, to), file)| {
            let shown = format!("{from} -> {to} (e.g. {file})");
            match (layer_in(layers, from), layer_in(layers, to)) {
                (None, None) => Some(format!("tool {shown}")),
                (Some(_), None) => Some(format!("shared module {shown}")),
                (Some(below), Some(above)) if above >= below => Some(format!(
                    "shared module (layer {below}) {shown}, layer {above}: \
                     import only lower layers"
                )),
                _ => None,
            }
        })
        .collect()
}

/// Each listed module exists under `src`, once.
fn listed_modules_exist(crate_name: &str, src: &Path, layers: &[Layer]) {
    let mut listed = BTreeSet::new();
    for (name, _) in layers.iter().flat_map(|layer| layer.iter()) {
        assert!(listed.insert(*name), "{crate_name} lists `{name}` twice");
        assert!(
            *name == ROOT || src.join(name).exists() || src.join(format!("{name}.rs")).exists(),
            "{crate_name} lists `{name}`, which is not in {}: remove it",
            src.display()
        );
    }
}

#[test]
fn tool_crates_import_only_lower_modules_and_no_other_tool() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates directory");
    let shared = crates.join("tools-shared/src");
    listed_modules_exist("SHARED_LAYERS", &shared, SHARED_LAYERS);
    let unplaced: Vec<_> = top_modules(&shared)
        .into_iter()
        .filter(|module| layer_in(SHARED_LAYERS, module).is_none())
        .collect();
    assert!(
        unplaced.is_empty(),
        "place these octocode-tools-shared modules in a layer of SHARED_LAYERS: {unplaced:?}"
    );
    let mut found: Vec<String> = violations(&module_edges(&shared), SHARED_LAYERS)
        .into_iter()
        .map(|violation| format!("tools-shared: {violation}"))
        .collect();
    let mut families: Vec<String> = std::fs::read_dir(crates)
        .expect("read the crates directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("tool-"))
        .collect();
    families.sort();
    assert!(!families.is_empty(), "no tool family crate under crates/");
    for (family, _) in NON_TOOLS {
        assert!(
            families.iter().any(|name| name == family),
            "NON_TOOLS names `{family}`, which is no crate under crates/: remove it"
        );
    }
    for family in &families {
        let src = crates.join(family).join("src");
        let layers = NON_TOOLS
            .iter()
            .find(|(name, _)| name == family)
            .map_or(&[][..], |(_, layers)| *layers);
        listed_modules_exist(family, &src, layers);
        // A family's `lib.rs` only mounts its tools.
        let edges = module_edges(&src)
            .into_iter()
            .filter(|((from, _), _)| from != ROOT)
            .collect();
        found.extend(
            violations(&edges, layers)
                .into_iter()
                .map(|violation| format!("{family}: {violation}")),
        );
    }
    assert!(
        found.is_empty(),
        "a module imports a tool, or a shared module at or above its own layer; move the shared piece to a lower module both import:\n  {}",
        found.join("\n  ")
    );
}

/// The judgment ↔ selection ↔ source_capture cycle this guard once missed:
/// on a crate with its three edges, the guard names each shared → shared edge
/// that points up or sideways, and the `lib.rs` helpers are a node.
#[test]
fn the_guard_rejects_a_shared_module_cycle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let write = |file: &str, text: &str| {
        let path = src.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    };
    write(
        "lib.rs",
        "pub mod judgment;\npub mod selection;\npub mod source_capture;\npub fn pruned_walk() {}\n",
    );
    write(
        "judgment/batch.rs",
        "fn f() { let r = crate::source_capture::assets::referenced; }",
    );
    write(
        "judgment/select.rs",
        "use crate::selection::{Decision, SelectionJudge};",
    );
    write(
        "selection/run.rs",
        "use crate::judgment::stats::ClassificationUsage;",
    );
    write(
        "source_capture.rs",
        "use crate::judgment::Evaluator;\nfn w() { crate::pruned_walk(); }",
    );
    let edges = module_edges(&src);
    assert_eq!(
        edges.keys().cloned().collect::<Vec<_>>(),
        [
            ("judgment", "selection"),
            ("judgment", "source_capture"),
            ("selection", "judgment"),
            ("source_capture", "judgment"),
            ("source_capture", ROOT),
        ]
        .map(|(from, to)| (from.to_owned(), to.to_owned()))
    );
    let rejected = violations(&edges, SHARED_LAYERS);
    assert_eq!(rejected.len(), 2, "{rejected:#?}");
    assert!(
        rejected[0].contains("judgment -> selection"),
        "{rejected:#?}"
    );
    assert!(
        rejected[1].contains("judgment -> source_capture"),
        "{rejected:#?}"
    );
}

/// In a family crate a tool reaching another tool is rejected, while a tool
/// reaching the crate's provider module is not.
#[test]
fn the_guard_rejects_a_tool_reaching_another_tool_of_its_family() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let write = |file: &str, text: &str| {
        let path = src.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    };
    write(
        "lib.rs",
        "pub mod artifact;\npub mod artifact_search;\npub mod security_check;\n",
    );
    write("artifact.rs", "pub fn lookup() {}");
    write("artifact_search.rs", "use crate::artifact::lookup;");
    write(
        "security_check/osv.rs",
        "fn f() { crate::artifact::lookup(); crate::artifact_search::x(); }",
    );
    let layers: &[Layer] = &[&[("artifact", "the providers")]];
    let rejected = violations(&module_edges(&src), layers);
    assert_eq!(rejected.len(), 1, "{rejected:#?}");
    assert!(
        rejected[0].contains("tool security_check -> artifact_search"),
        "{rejected:#?}"
    );
}
