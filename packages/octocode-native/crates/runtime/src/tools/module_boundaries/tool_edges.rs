//! The `tools/` guard: a tool reaches shared code only, a shared module
//! reaches no tool, and shared modules form one declared order
//! ([`SHARED_LAYERS`]) so no two of them import each other.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::module_files;
use super::scanner::{imported_modules, module_of, root_items};

/// The `tools.rs` node: the helpers it defines (`pruned_walk`,
/// `display_name`, `outlines`, `OUTLINE_LEAD_MAX_FILES`) form the lowest
/// shared layer, so they import no `tools/` module.
const ROOT: &str = "tools";

/// Modules under `tools/` that are not tools: primitives any tool may import,
/// lowest layer first. A shared module imports only shared modules of a
/// lower layer, never one of its own layer or above, so the shared modules
/// form no cycle. A new `tools/` entry is a tool unless it is listed here,
/// with its reason.
const SHARED_LAYERS: &[&[(&str, &str)]] = &[
    &[(
        ROOT,
        "tools.rs: walk, path and outline helpers every layer may use",
    )],
    &[
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
        ("line_spans", "line-span arithmetic"),
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
        (
            "workers",
            "the one per-query fan-out width: the batch's core share, a cap, the items",
        ),
    ],
    &[
        // Reads empty_walk's walk and globs.
        ("ast_rule", "AST rule parsing shared by the ast tools"),
        ("leads", "follow-up queries one tool offers into another"),
        ("page_memo", "page memoization"),
        (
            "selection",
            "bounded relevance ordering for local search files and fetch windows, judged through the judgment evaluator",
        ),
        (
            "source_capture",
            "one complete source selection captured through the authorized readers as a classification state",
        ),
        // Plans over declarations_cache's outlines.
        (
            "source_sections",
            "source-coordinate section planning shared by localFetch and clasify",
        ),
    ],
    &[(
        "source_view",
        "the one view engine over fetched source text: localFetch and ghGetFileContent render through it, and local_search sizes its read leads by its limits",
    )],
];

/// Where `module` sits in [`SHARED_LAYERS`], or `None` for a tool.
fn shared_layer(module: &str) -> Option<usize> {
    SHARED_LAYERS
        .iter()
        .position(|layer| layer.iter().any(|(name, _)| *name == module))
}

/// Every `(importer, imported)` pair between distinct `tools/` nodes (each
/// tool, each shared module, and [`ROOT`] for the items `tools.rs`
/// defines), with one example file.
fn module_edges(tools_dir: &Path) -> BTreeMap<(String, String), String> {
    let root_file = tools_dir.with_extension("rs");
    let roots = root_items(&std::fs::read_to_string(&root_file).unwrap_or_default());
    let mut modules: BTreeSet<String> = std::fs::read_dir(tools_dir)
        .expect("read tools/")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_stem()?.to_str()?.to_owned();
            (name != "module_boundaries" && name != "test_support").then_some(name)
        })
        .collect();
    modules.insert(ROOT.to_owned());
    let mut edges = BTreeMap::new();
    for importer in &modules {
        let mut files = Vec::new();
        if importer == ROOT {
            files.push(root_file.clone());
        } else {
            module_files(tools_dir, importer, &mut files);
        }
        for path in files {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            for imported in imported_modules(&source, &module_of(tools_dir, &path)) {
                let imported = if roots.contains(&imported) {
                    ROOT.to_owned()
                } else {
                    imported
                };
                if &imported != importer && modules.contains(&imported) {
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

/// The edges the guard rejects: tool → tool, shared → tool, and shared →
/// shared unless the imported module's layer is lower.
fn violations(edges: &BTreeMap<(String, String), String>) -> Vec<String> {
    edges
        .iter()
        .filter_map(|((from, to), file)| {
            let shown = format!("{from} -> {to} (e.g. tools/{file})");
            match (shared_layer(from), shared_layer(to)) {
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

#[test]
fn tool_modules_import_only_shared_modules_in_layer_order() {
    let tools_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools");
    let mut listed = BTreeSet::new();
    for (name, _) in SHARED_LAYERS.iter().flat_map(|layer| layer.iter()) {
        assert!(listed.insert(*name), "SHARED_LAYERS lists `{name}` twice");
        assert!(
            *name == ROOT
                || tools_dir.join(name).exists()
                || tools_dir.join(format!("{name}.rs")).exists(),
            "SHARED_LAYERS lists `{name}`, which is not under tools/: remove it"
        );
    }
    let violations = violations(&module_edges(&tools_dir));
    assert!(
        violations.is_empty(),
        "a module imports a tool's internals or a shared module at or above its own layer; move the shared piece to a lower shared module both import:\n  {}",
        violations.join("\n  ")
    );
}

/// The judgment ↔ selection ↔ source_capture cycle this guard once missed:
/// on a tree with its three edges, the guard names each shared → shared edge
/// that points up or sideways, and the `tools.rs` helpers are a node.
#[test]
fn the_guard_rejects_a_shared_module_cycle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let tools = dir.path().join("tools");
    let write = |file: &str, text: &str| {
        let path = dir.path().join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    };
    write(
        "tools.rs",
        "pub(crate) mod judgment;\npub(crate) fn pruned_walk() {}\n",
    );
    write(
        "tools/judgment/batch.rs",
        "fn f() { let r = crate::tools::source_capture::assets::referenced; }",
    );
    write(
        "tools/judgment/select.rs",
        "use crate::tools::selection::{Decision, SelectionJudge};",
    );
    write(
        "tools/selection/run.rs",
        "use crate::tools::judgment::stats::ClassificationUsage;",
    );
    write(
        "tools/source_capture.rs",
        "use crate::tools::judgment::Evaluator;\nfn w() { crate::tools::pruned_walk(); }",
    );
    let edges = module_edges(&tools);
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
    let rejected = violations(&edges);
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
