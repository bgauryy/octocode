//! Name resolution for uses, calls and heritage: the export resolver, each
//! file's bindings, and the call linker.
use super::*;
use octocode_engine::graph::{GraphCallBinding, GraphCallBoundary};

mod c_family;
mod choice;
mod classes;
mod constructors;
mod es_members;
mod exports;
mod go;
mod inherited;
mod lexical;
mod linker;
mod rust;
mod scala;
mod scope;
pub(super) use exports::{ExportResolver, FileBindings, confidence_for};
pub(super) use inherited::{BaseIndex, base_index};
pub(super) use linker::{Linker, Site};
pub(super) use scala::ScalaBase;
pub(super) use scope::LinkScope;

/// Files each file star-re-exports.
pub(super) fn star_targets(built: &BuiltGraph) -> HashMap<&str, Vec<&str>> {
    built
        .nodes
        .iter()
        .map(|(file, node)| {
            let targets = node
                .edges
                .iter()
                .filter(|(_, kinds)| kinds.keys().any(|kind| kind.ends_with("star-reexport")))
                .map(|(target, _)| target.as_str())
                .collect::<Vec<_>>();
            (file.as_str(), targets)
        })
        .collect()
}

/// Unique-name candidates that are members, by `(container name, name)`,
/// in [`Symbols::global_names`] order: a member call looks up its type's
/// members instead of filtering every declaration of the name.
pub(super) fn member_index<'a>(
    global_names: &HashMap<&'a str, Vec<u32>>,
    container_names: &HashMap<u32, &'a str>,
) -> MemberIndex<'a> {
    let mut members = MemberIndex::new();
    for (name, ids) in global_names {
        for id in ids {
            if let Some(container) = container_names.get(id) {
                members.entry((*container, *name)).or_default().push(*id);
            }
        }
    }
    members
}

/// See [`member_index`].
pub(super) type MemberIndex<'a> = HashMap<(&'a str, &'a str), Vec<u32>>;

/// Language-family, Rust module and workspace-crate lookups for graph-wide
/// and qualified call paths; `families` is [`link_family`] by file node id.
pub(super) fn link_scope<'a>(
    file_paths: &'a [&'a str],
    families: Vec<&'a str>,
    workspace: &Workspace,
) -> LinkScope<'a> {
    let mut modules = HashMap::<&str, Vec<&str>>::new();
    let mut rust_files = HashSet::new();
    for path in file_paths.iter().filter(|path| !path.is_empty()) {
        let Some(stem) = path.strip_suffix(".rs") else {
            continue;
        };
        rust_files.insert(*path);
        let module = match stem.rsplit_once('/') {
            Some((dir, "mod")) => dir.rsplit('/').next().unwrap_or(dir),
            Some((_, name)) => name,
            None => stem,
        };
        modules.entry(module).or_default().push(*path);
    }
    let crate_dirs = workspace
        .components
        .values()
        .flatten()
        .filter(|c| c.ecosystem == "cargo")
        .map(|c| (c.name.replace('-', "_"), c.dir.clone()))
        .collect::<HashMap<_, _>>();
    LinkScope {
        file_paths,
        families,
        modules,
        rust_files,
        crate_dirs,
    }
}

/// An edge found while the node list is borrowed, pushed afterwards.
pub(super) type PendingEdge = (u32, u32, EdgeKind, Confidence, String, u32);

#[derive(Default)]
pub(super) struct CallOutput {
    pub edges: Vec<LinkedCall>,
    pub unresolved: Vec<UnlinkedCall>,
}

pub(super) struct LinkedCall {
    pub edge: PendingEdge,
    pub occurrence: String,
    pub source_range: Option<octocode_engine::text::analysis::ObservedSpan>,
}

pub(super) struct UnlinkedCall {
    pub src: u32,
    pub occurrence: String,
    pub callee: String,
    pub line: u32,
    pub reason: &'static str,
    pub source_range: Option<octocode_engine::text::analysis::ObservedSpan>,
}

/// One file's call-linking context.
pub(super) struct FileLinks<'f> {
    pub(super) bindings: FileBindings<'f>,
    pub(super) index: &'f FileIndex<'f>,
    pub(super) file_id: u32,
    pub(super) language: &'f str,
}

/// calls: each call site linked to its target declarations.
pub(super) fn link_calls(
    linker: &Linker,
    unit: &FileLinks,
    facts: &FileFacts,
    callable: &dyn Fn(u32) -> bool,
    method: &dyn Fn(u32) -> bool,
    calls: &mut CallStats,
    out: &mut CallOutput,
) {
    for call in &facts.calls {
        calls.sites += 1;
        let caller = call
            .caller_id
            .as_deref()
            .and_then(|id| unit.index.decl_nodes.get(id).copied())
            .unwrap_or(unit.file_id);
        let (qualifier, name) = split_callee(&call.callee);
        // Rust needs a receiver or type path for an associated function, and
        // a JS/TS class or object member is never in lexical scope. A bare
        // name can still name a nested free function; Java/C++ may also call
        // methods through an implicit receiver; a Go method never is a bare name.
        let bare_lexical = (matches!(unit.language, "rust" | "go")
            || ecosystem(unit.language) == "npm")
            && qualifier.is_empty()
            && call.receiver_type.is_none();
        let accept = |id| match call.binding {
            GraphCallBinding::ConstructedType => {
                constructors::admits(linker, caller, id, &call.callee)
            }
            GraphCallBinding::SyntaxLookup => callable(id) && (!bare_lexical || !method(id)),
            GraphCallBinding::Unassessed { .. } => false,
        };
        let site = Site {
            bindings: &unit.bindings,
            index: unit.index,
            caller,
            qualifier,
            name,
            accept: &accept,
            receiver_type: call.receiver_type.as_deref(),
            term_binding: call.term_binding,
            self_template: call.self_template.as_deref(),
            value_qualifier: matches!(unit.language, "rust" | "c" | "cpp" | "cuda")
                && call.callee.as_bytes().get(qualifier.len()) == Some(&b'.'),
        };
        let recovered = matches!(
            call.parse_assessment,
            octocode_engine::graph::GraphParseAssessment::Recovered { .. }
        );
        let boundary = match call.binding {
            GraphCallBinding::Unassessed { reason } => Some(match reason {
                GraphCallBoundary::TargetTypeUnavailable => "constructor-target-unavailable",
                GraphCallBoundary::ConstructorDelegation => "constructor-binding-not-assessed",
                GraphCallBoundary::UnsupportedConstructorForm => "unsupported-constructor-form",
            }),
            _ => None,
        };
        // A bare std macro name the repo also declares as a `macro_rules!`
        // may expand that macro instead: its arguments are no proven calls.
        let shadowed = call
            .macros
            .iter()
            .any(|path| !path.contains("::") && linker.rust_macros.contains(path.as_str()));
        let resolved = (!recovered && !shadowed && boundary.is_none())
            .then(|| {
                if unit.language == "scala" {
                    linker.scala_call(&site)
                } else if let Some(lang) = classes::ClassLang::of(unit.language) {
                    let constructed = call.binding == GraphCallBinding::ConstructedType;
                    linker.class_call(&site, lang, constructed)
                } else if call.binding == GraphCallBinding::ConstructedType {
                    constructors::local(&site).or_else(|| linker.link(&site))
                } else {
                    linker.link(&site)
                }
            })
            .flatten();
        let Some((targets, resolution, confidence)) = resolved else {
            calls.unresolved += 1;
            let reason = if recovered {
                "recovered-declaration"
            } else if let Some(reason) = boundary {
                reason
            } else if shadowed {
                "ambiguous"
            } else if call.binding == GraphCallBinding::ConstructedType
                && constructors::scope_unassessed(linker, &site)
            {
                "constructor-binding-not-assessed"
            } else {
                unlinked_reason(
                    &unit.bindings,
                    qualifier,
                    name,
                    linker.named(caller, name).filter(|id| accept(*id)).count(),
                )
            };
            *calls.unresolved_by_reason.entry(reason).or_default() += 1;
            out.unresolved.push(UnlinkedCall {
                src: caller,
                occurrence: call.id.clone(),
                callee: call.callee.clone(),
                line: call.line,
                reason,
                source_range: call.source_range,
            });
            let bucket = if INTERNAL_REASONS.contains(&reason) {
                1
            } else {
                2
            };
            calls
                .by_language
                .entry(unit.language.to_owned())
                .or_default()[bucket] += 1;
            continue;
        };
        calls.linked += 1;
        calls
            .by_language
            .entry(unit.language.to_owned())
            .or_default()[0] += 1;
        *calls
            .by_resolution
            .entry(resolution.to_owned())
            .or_default() += 1;
        // JSX renders, bare decorators and `new` keep their kind visible:
        // `renders:import`, `constructs:local`.
        let detail = match call.kind.as_str() {
            "" | "calls" | "call" => resolution.to_owned(),
            kind => format!("{kind}:{resolution}"),
        };
        for dst in targets {
            out.edges.push(LinkedCall {
                edge: (
                    caller,
                    dst,
                    EdgeKind::Calls,
                    confidence,
                    detail.clone(),
                    call.line,
                ),
                occurrence: call.id.clone(),
                source_range: call.source_range,
            });
        }
    }
}

/// inherits: `extends` / `implements` base types, linked like calls. A
/// Scala declaration also records its bases in written order in `scala`,
/// each either the one type it resolved to or opaque.
pub(super) fn link_heritage(
    linker: &Linker,
    unit: &FileLinks,
    facts: &FileFacts,
    is_type: &dyn Fn(u32) -> bool,
    out: &mut Vec<PendingEdge>,
    scala: &mut HashMap<u32, Vec<ScalaBase>>,
) -> HashSet<u32> {
    let mut unresolved = HashSet::new();
    for heritage in &facts.heritage {
        let Some(&src) = unit.index.decl_nodes.get(heritage.decl_id.as_str()) else {
            continue;
        };
        let (qualifier, name) = split_callee(&heritage.target);
        let site = Site {
            bindings: &unit.bindings,
            index: unit.index,
            caller: src,
            qualifier,
            name,
            accept: is_type,
            receiver_type: None,
            term_binding: false,
            self_template: None,
            value_qualifier: false,
        };
        let linked = if unit.language == "scala" {
            let linked = linker.scala_base(&site, &heritage.target);
            let base = match linked
                .as_ref()
                .map(|(targets, _, c)| (targets.as_slice(), c))
            {
                Some(([only], confidence)) if *only != src => ScalaBase::Type(*only, *confidence),
                _ => ScalaBase::Opaque(heritage.target.clone()),
            };
            scala.entry(src).or_default().push(base);
            linked
        } else if let Some(lang) = classes::ClassLang::of(unit.language) {
            linker.class_base(&site, lang, &heritage.target)
        } else {
            linker.link(&site)
        };
        let Some((targets, resolution, confidence)) = linked else {
            if matches!(heritage.relation.as_str(), "extends" | "embeds") {
                unresolved.insert(src);
            }
            continue;
        };
        let detail = format!("{}:{resolution}", heritage.relation);
        for dst in targets.into_iter().filter(|dst| *dst != src) {
            out.push((
                src,
                dst,
                EdgeKind::Inherits,
                confidence,
                detail.clone(),
                heritage.line,
            ));
        }
    }
    unresolved
}
