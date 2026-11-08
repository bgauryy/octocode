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
        declaration.doc_line = crate::signatures::declaration_doc_line(&lines, declaration, &ext);
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

/// Where [`facts_from_tree`] would name `name` in a file: the zero-based lines
/// of its declarations' names, and every line a call whose callee ends in
/// `name` spans (`None` when only the whole walk can tell).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NamedSites {
    pub declared: Vec<usize>,
    pub called: Option<Vec<usize>>,
}

fn declared_in(acc: &GraphAccumulator, name: &str) -> Vec<usize> {
    acc.declarations
        .iter()
        .filter(|declaration| declaration.name == name)
        .map(|declaration| declaration.selection_range.start.line as usize)
        .collect()
}

fn called_in(acc: &GraphAccumulator, name: &str) -> Vec<usize> {
    acc.calls
        .iter()
        .filter(|call| {
            call.callee
                .rsplit_once(['.', ':'])
                .map_or(call.callee.as_str(), |(_, last)| last)
                == name
        })
        .flat_map(|call| call.range.start.line as usize..=call.range.end.line as usize)
        .collect()
}

/// What one walk of only the nodes that spell `name` reads: `read` of its
/// accumulator, and again after the Rust macro bodies it found (`None` when it
/// found none). A declaration's name, a call's callee and a heritage clause's
/// base lie inside their nodes, so only nodes that spell `name` are entered,
/// and the walk does the same per-node work as [`facts_from_tree`]. `None` when
/// only the whole walk can tell: `name` is not one identifier, or what the file
/// declares depends on code elsewhere in it (a GoogleTest include). Which
/// bodies the whole walk reads depends on the file's other macros, imports and
/// body count; it reads no body this one does not, so a body naming nothing
/// here names nothing there.
fn narrow_walk<T>(
    content: &str,
    file_path: &str,
    extension: &str,
    tree: &tree_sitter::Tree,
    name: &str,
    declarations_only: bool,
    read: impl Fn(&GraphAccumulator) -> T,
) -> Option<(T, Option<T>)> {
    if name.is_empty() || !name.chars().all(crate::text::is_ident_char) {
        return None;
    }
    let ext = canonical_extension(extension).to_owned();
    let entry = languages::find_entry(&ext)?
        .admitted(crate::text::analysis::AnalysisOperation::GraphFacts)?;
    if matches!(ext.as_str(), "cpp" | "cu") && content.contains("gtest/gtest.h") {
        return None;
    }
    let mut acc = GraphAccumulator::new(file_path, &ext);
    let root = tree.root_node();
    if ext == "go" && go::package_header(root, content).build_excluded {
        return Some((read(&acc), None));
    }
    acc.declarations_only = declarations_only;
    acc.call_adapter = entry.call_adapter;
    acc.visibility_adapter = entry.visibility_adapter;
    acc.export_forwarding_node = entry.export_forwarding_node;
    acc.js_fallback = js_fallback::is_fallback_lane(&ext);
    acc.only_spelling = Some(crate::signatures::spelled::Spelled::of(content, name));
    let positions = NodePositions::new(content);
    let deadline = crate::signatures::extractor::whole_file_deadline();
    if !visit_node(root, content, &positions, &mut acc, deadline, &[], None) {
        return None;
    }
    let walked = read(&acc);
    if acc.macro_bodies.is_empty() {
        return Some((walked, None));
    }
    acc.source_templates = crate::text::analysis::SourceTemplates::new(
        content,
        crate::text::analysis::Producer::new(format!("tree-sitter-facts:{ext}"), "6"),
        crate::text::analysis::AnalysisOperation::GraphFacts,
    )
    .ok()?;
    if !visit_macro_bodies(content, &entry.language, &positions, &mut acc, deadline) {
        return None;
    }
    Some((walked, Some(read(&acc))))
}

/// The [`NamedSites`] of `name` in `tree` as [`facts_from_tree`] would find
/// them, from a [`narrow_walk`]. `None` when only the whole walk can tell, or a
/// Rust macro body declares the name; the calls alone are unknown when a macro
/// body calls it.
pub(crate) fn named_sites_from_tree(
    content: &str,
    file_path: &str,
    extension: &str,
    tree: &tree_sitter::Tree,
    name: &str,
) -> Option<NamedSites> {
    let read = |acc: &GraphAccumulator| (declared_in(acc, name), called_in(acc, name));
    let ((declared, called), after) =
        narrow_walk(content, file_path, extension, tree, name, false, read)?;
    match after {
        Some((declared_after, _)) if declared_after != declared => None,
        // A body calling the name may be one the whole walk does not read.
        Some((_, called_after)) if called_after != called => Some(NamedSites {
            declared,
            called: None,
        }),
        _ => Some(NamedSites {
            declared,
            called: Some(called),
        }),
    }
}

/// The declarations whose heritage clause names `name`, with those edges, as
/// [`facts_from_tree`] would record them, from a declarations-only
/// [`narrow_walk`]: a document holding just them. `None` when only the whole
/// walk can tell, or a Rust macro body adds one.
pub(crate) fn named_heritage_from_tree(
    content: &str,
    file_path: &str,
    extension: &str,
    tree: &tree_sitter::Tree,
    name: &str,
) -> Option<GraphFactsDocument> {
    let read = |acc: &GraphAccumulator| GraphFactsDocument {
        declarations: acc.declarations.clone(),
        edges: acc.edges.clone(),
        ..GraphFactsDocument::default()
    };
    match narrow_walk(content, file_path, extension, tree, name, true, read)? {
        (walked, Some(after)) if after.edges != walked.edges => None,
        (walked, _) => Some(walked),
    }
}

fn language_label(ext: &str, language_id: Option<&str>) -> String {
    language_id
        .unwrap_or_else(|| canonical_extension(ext))
        .to_owned()
}
