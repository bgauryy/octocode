//! Argument binding premises restrict the existing candidate resolver.
use super::*;

fn lexical_distance(linker: &Linker, caller: u32, parent: u32) -> usize {
    let mut current = caller;
    for distance in 0..linker.nodes.len() {
        if current == NONE {
            break;
        }
        if current == parent {
            return distance;
        }
        current = linker.nodes[current as usize].parent;
    }
    usize::MAX
}

fn visible(
    linker: &Linker,
    caller: u32,
    target: u32,
    qualifier: &str,
    implicit_members: bool,
    lexical_scope: &dyn Fn(u32) -> bool,
) -> bool {
    if !qualifier.is_empty() {
        return true;
    }
    let parent = linker.nodes[target as usize].parent;
    if parent == NONE {
        return true;
    }
    if !lexical_scope(parent) && !implicit_members {
        return false;
    }
    let mut current = caller;
    let mut remaining = linker.nodes.len();
    while current != NONE && remaining > 0 {
        if current == parent {
            return true;
        }
        current = linker.nodes[current as usize].parent;
        remaining -= 1;
    }
    false
}

pub(super) fn imported(
    linker: &Linker,
    facts: &FileFacts,
    import_id: &str,
    target_name: &str,
    accept: &dyn Fn(u32) -> bool,
) -> Result<(Vec<u32>, &'static str, Confidence), &'static str> {
    let imports = facts
        .imports
        .iter()
        .filter(|import| import.id == import_id)
        .collect::<Vec<_>>();
    if imports.is_empty() {
        return Err("scope-not-assessed");
    }
    let mut targets = Vec::new();
    let mut uncertain = false;
    let mut external = false;
    for import in imports {
        external |= import.external;
        let Some(file) = import.target.as_deref() else {
            uncertain = true;
            continue;
        };
        let local = import
            .local_name
            .as_deref()
            .unwrap_or(&import.imported_name);
        let tail = target_name
            .strip_prefix(local)
            .filter(|tail| tail.is_empty() || tail.starts_with('.') || tail.starts_with("::"));
        let Some(tail) = tail else {
            uncertain = true;
            continue;
        };
        let parts = tail
            .trim_start_matches(['.', ':'])
            .split(['.', ':'])
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let namespace = import.imported_name == "*" || import.imported_name.is_empty();
        let (name, rest) = if namespace {
            let Some((first, rest)) = parts.split_first() else {
                uncertain = true;
                continue;
            };
            (*first, rest)
        } else {
            (import.imported_name.as_str(), parts.as_slice())
        };
        let resolved = linker.resolver.resolved(file, name);
        uncertain |= resolved.uncertain;
        let mut bound = resolved.targets;
        for part in rest {
            bound = linker
                .global_names
                .get(part)
                .into_iter()
                .flatten()
                .copied()
                .filter(|id| bound.contains(&linker.nodes[*id as usize].parent))
                .collect();
        }
        targets.extend(bound.into_iter().filter(|id| accept(*id)));
    }
    targets.sort_unstable();
    targets.dedup();
    if targets.is_empty() {
        return Err(if external {
            "external-package"
        } else {
            "not-declared-in-repo"
        });
    }
    let strong = if uncertain {
        Confidence::Medium
    } else {
        Confidence::High
    };
    let confidence = confidence_for(&targets, strong);
    Ok((targets, "import-binding", confidence))
}

pub(super) fn scoped(
    linker: &Linker,
    unit: &FileLinks,
    caller: u32,
    target_name: &str,
    implicit_members: bool,
    accept: &dyn Fn(u32) -> bool,
    lexical_scope: &dyn Fn(u32) -> bool,
) -> Result<(Vec<u32>, &'static str, Confidence), &'static str> {
    let (qualifier, name) = split_callee(target_name);
    let nearest_parent = qualifier
        .is_empty()
        .then(|| {
            unit.index
                .by_name
                .get(name)
                .into_iter()
                .flatten()
                .copied()
                .filter(|id| {
                    accept(*id)
                        && visible(
                            linker,
                            caller,
                            *id,
                            qualifier,
                            implicit_members,
                            lexical_scope,
                        )
                })
                .min_by_key(|id| {
                    lexical_distance(linker, caller, linker.nodes[*id as usize].parent)
                })
                .map(|id| linker.nodes[id as usize].parent)
        })
        .flatten();
    let admitted = |id| {
        accept(id)
            && nearest_parent.is_none_or(|parent| {
                linker.nodes[id as usize].file != unit.file_id
                    || linker.nodes[id as usize].parent == parent
            })
            && visible(
                linker,
                caller,
                id,
                qualifier,
                implicit_members,
                lexical_scope,
            )
    };
    let lookup = Site {
        bindings: &unit.bindings,
        index: unit.index,
        caller,
        qualifier,
        name,
        accept: &admitted,
        receiver_type: None,
    };
    linker.link(&lookup).ok_or_else(|| {
        unlinked_reason(
            &unit.bindings,
            qualifier,
            name,
            linker.global_names,
            &admitted,
        )
    })
}
