//! Name resolution for uses, calls and heritage: the export resolver, each
//! file's bindings, and the call linker.
use super::*;

mod exports;
mod linker;
pub(super) use exports::*;
pub(super) use linker::*;

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

/// Rust module and workspace-crate lookups for qualified call paths.
pub(super) fn link_scope<'a>(file_paths: &'a [&'a str], workspace: &Workspace) -> LinkScope<'a> {
    let mut modules = HashMap::<&str, Vec<&str>>::new();
    for path in file_paths.iter().filter(|path| !path.is_empty()) {
        let Some(stem) = path.strip_suffix(".rs") else {
            continue;
        };
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
        modules,
        crate_dirs,
    }
}

/// An edge found while the node list is borrowed, pushed afterwards.
pub(super) type PendingEdge = (u32, u32, EdgeKind, Confidence, String, u32);

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
    calls: &mut CallStats,
    out: &mut Vec<PendingEdge>,
) {
    for call in &facts.calls {
        calls.sites += 1;
        let caller = call
            .caller_id
            .as_deref()
            .and_then(|id| unit.index.decl_nodes.get(id).copied())
            .unwrap_or(unit.file_id);
        let (qualifier, name) = split_callee(&call.callee);
        let site = Site {
            bindings: &unit.bindings,
            index: unit.index,
            caller,
            qualifier,
            name,
            accept: callable,
            receiver_type: call.receiver_type.as_deref(),
        };
        let Some((targets, resolution, confidence)) = linker.link(&site) else {
            calls.unresolved += 1;
            let reason = unlinked_reason(
                &unit.bindings,
                qualifier,
                name,
                linker.global_names,
                callable,
            );
            *calls.unresolved_by_reason.entry(reason).or_default() += 1;
            if INTERNAL_REASONS.contains(&reason) {
                calls
                    .by_language
                    .entry(unit.language.to_owned())
                    .or_default()[1] += 1;
            }
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
            out.push((
                caller,
                dst,
                EdgeKind::Calls,
                confidence,
                detail.clone(),
                call.line,
            ));
        }
    }
}

/// inherits: `extends` / `implements` base types, linked like calls.
pub(super) fn link_heritage(
    linker: &Linker,
    unit: &FileLinks,
    facts: &FileFacts,
    is_type: &dyn Fn(u32) -> bool,
    out: &mut Vec<PendingEdge>,
) {
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
        };
        let Some((targets, resolution, confidence)) = linker.link(&site) else {
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
}
