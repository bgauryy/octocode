//! The graph facts of one parsed tree: the walk, its macro bodies and the
//! document they make.

use super::macro_bodies::visit_macro_bodies;
use super::positions::{NodePositions, syntax_error_lines};
use super::references::count_name_references;
use super::rust::rust_inner_unsupported;
use super::walk::visit_node;
use super::{GraphAccumulator, arguments, canonical_extension, go, js_fallback, languages, note};
use crate::graph::{GraphFactCode, GraphFactsDocument};

/// Graph facts from `tree` (`None`: the parse ran out of time, and the
/// facts say they are incomplete).
pub(super) fn facts_from_tree(
    content: &str,
    file_path: &str,
    extension: &str,
    deadline: std::time::Instant,
    declarations_only: bool,
    tree: Option<&tree_sitter::Tree>,
) -> Option<crate::signatures::GraphFactsExtraction> {
    // One spelling per language for every dispatch below: `pyi` is `py`,
    // `hpp` is `cpp`, as the language registry lists them.
    let ext = canonical_extension(extension).to_owned();
    let entry = languages::find_entry(&ext)?.admitted(if declarations_only {
        crate::text::analysis::AnalysisOperation::Declarations
    } else {
        crate::text::analysis::AnalysisOperation::GraphFacts
    })?;
    let mut acc = GraphAccumulator::new(file_path, &ext);
    acc.declarations_only = declarations_only;
    acc.argument_adapter = entry.argument_adapter;
    acc.call_adapter = entry.call_adapter;
    acc.visibility_adapter = entry.visibility_adapter;
    acc.export_forwarding_node = entry.export_forwarding_node;
    acc.js_fallback = js_fallback::is_fallback_lane(&ext);
    if acc.js_fallback {
        acc.diagnostics
            .push(js_fallback::route_note(content, file_path, extension));
    }
    acc.source_templates = crate::text::analysis::SourceTemplates::new(
        content,
        crate::text::analysis::Producer::new(format!("tree-sitter-facts:{ext}"), "6"),
        if declarations_only {
            crate::text::analysis::AnalysisOperation::Declarations
        } else {
            crate::text::analysis::AnalysisOperation::GraphFacts
        },
    )
    .ok()?;
    acc.source_digest = acc
        .source_templates
        .root_digest()
        .unwrap_or_default()
        .to_owned();
    let mut rust_root_unsupported = (ext == "rs").then_some(true);
    let mut reference_counts = Vec::new();
    let mut error_lines = Vec::new();
    let go_header = tree
        .filter(|_| ext == "go")
        .map(|tree| go::package_header(tree.root_node(), content))
        .unwrap_or_default();
    // An outline still reads such a file; only a graph leaves it out.
    let build_excluded = go_header.build_excluded && !declarations_only;
    if build_excluded {
        acc.diagnostics.push(note(GraphFactCode::BuildExcluded));
    } else if let Some(tree) = tree {
        let root = tree.root_node();
        let line_index = NodePositions::new(content);
        if !declarations_only && let Some(adapter) = entry.argument_adapter {
            acc.argument_bindings = Some(crate::signatures::arguments::binding::Bindings::collect(
                root, adapter, content, deadline,
            ));
        }
        rust_root_unsupported = (ext == "rs").then(|| rust_inner_unsupported(root, content));
        if rust_root_unsupported == Some(true) {
            acc.diagnostics
                .push(note(GraphFactCode::RustCrateAttributes));
        }
        if root.has_error() {
            acc.diagnostics.push(note(GraphFactCode::RecoveredParse));
            error_lines = syntax_error_lines(root);
        }
        acc.name_tokens = (!declarations_only).then(Vec::new);
        let visited = visit_node(root, content, &line_index, &mut acc, deadline, &[], None);
        let name_tokens = acc.name_tokens.take().unwrap_or_default();
        if !visited
            || !visit_macro_bodies(content, &entry.language, &line_index, &mut acc, deadline)
        {
            // Facts gathered before the deadline are positive syntax facts and
            // stay. The diagnostic marks the file incomplete, so consumers must
            // not read a missing import, call or module as absent.
            acc.diagnostics.push(note(GraphFactCode::TraversalDeadline));
            rust_root_unsupported = (ext == "rs").then_some(true);
        } else if !declarations_only {
            let counts = count_name_references(content, &acc, &name_tokens);
            reference_counts = acc
                .declarations
                .iter()
                .map(|declaration| crate::graph::GraphReferenceCount {
                    declaration_id: declaration.id.clone(),
                    count: counts.get(&declaration.name).copied().unwrap_or(0),
                })
                .collect();
        }
    } else {
        acc.diagnostics.push(note(GraphFactCode::ParseDeadline));
    }

    if acc.js_fallback {
        js_fallback::apply_local_exports(&mut acc);
    }
    let lines = content.lines().collect::<Vec<_>>();
    arguments::join_bindings(&mut acc);
    for declaration in &mut acc.declarations {
        declaration.doc_line = crate::signatures::leading_doc_line(
            &lines,
            declaration.range.start.line as usize,
            &ext,
        );
    }
    let facts = GraphFactsDocument {
        declarations: acc.declarations,
        imports: acc.imports,
        exports: acc.exports,
        calls: acc.calls,
        argument_sites: acc.argument_sites,
        edges: acc.edges,
        diagnostics: acc.diagnostics,
        error_lines,
        modules: acc.modules,
        paths: {
            let mut paths = acc.paths;
            // An item-level macro body is read both as tokens and as items.
            paths.sort_unstable();
            paths.dedup();
            paths
        },
        rust_root_unsupported,
        test_lines: acc.test_lines,
        go_package: go_header.package,
        build_excluded,
        ..crate::signatures::graph_facts_document(
            language_label(&ext, entry.language_id),
            file_path,
            acc.source_templates,
        )
    };
    Some(crate::signatures::GraphFactsExtraction {
        facts,
        reference_counts,
    })
}

fn language_label(ext: &str, language_id: Option<&str>) -> String {
    language_id
        .unwrap_or_else(|| canonical_extension(ext))
        .to_owned()
}
