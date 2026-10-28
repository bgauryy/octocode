//! Written constructor qualifications constrain type candidates before name lookup.
use super::*;

fn root(linker: &Linker, id: u32) -> bool {
    id == NONE || linker.nodes[id as usize].kind == NodeKind::File
}

fn visible_ancestor(linker: &Linker, caller: u32, parent: u32) -> bool {
    if root(linker, parent) {
        return true;
    }
    let mut current = caller;
    for _ in 0..linker.nodes.len() {
        if current == parent {
            return true;
        }
        if root(linker, current) {
            break;
        }
        current = linker.nodes[current as usize].parent;
    }
    false
}

pub(super) fn admits(linker: &Linker, caller: u32, target: u32, callee: &str) -> bool {
    if !(linker.is_type)(target) {
        return false;
    }
    let mut expected = callee.rsplit([':', '.']).filter(|part| !part.is_empty());
    let mut current = target;
    let mut remaining = linker.nodes.len();
    while let Some(node) = linker.nodes.get(current as usize) {
        if node.kind != NodeKind::Symbol || remaining == 0 {
            break;
        }
        // A grammar may store a namespace's qualified name in one declaration.
        for name in linker.strings[node.name as usize]
            .rsplit([':', '.'])
            .filter(|part| !part.is_empty())
        {
            if expected.next() != Some(name) {
                return false;
            }
        }
        current = node.parent;
        remaining -= 1;
        if expected.clone().next().is_none() {
            return if callee.starts_with("::") {
                root(linker, current)
            } else {
                visible_ancestor(linker, caller, current)
            };
        }
    }
    false
}

/// Exact ancestry admits same-file namespace/type spellings without casing rules.
pub(super) fn local(site: &Site) -> Option<super::linker::Linked> {
    let targets = site
        .index
        .by_name
        .get(site.name)?
        .iter()
        .copied()
        .filter(|id| (site.accept)(*id))
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return None;
    }
    let confidence = confidence_for(&targets, Confidence::High);
    Some((targets, "local", confidence))
}

pub(super) fn scope_unassessed(linker: &Linker, site: &Site) -> bool {
    let mut observed_type = false;
    for id in site
        .index
        .by_name
        .get(site.name)
        .into_iter()
        .flatten()
        .chain(linker.global_names.get(site.name).into_iter().flatten())
    {
        if (linker.is_type)(*id) {
            observed_type = true;
            if (site.accept)(*id) {
                return false;
            }
        }
    }
    observed_type
}
