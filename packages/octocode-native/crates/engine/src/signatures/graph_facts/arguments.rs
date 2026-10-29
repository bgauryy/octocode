use tree_sitter::Node;

use crate::graph::{
    GraphArgumentBinding, GraphArgumentGroup, GraphArgumentGroupObservation,
    GraphArgumentObservation, GraphArgumentSlot, GraphFactArgumentSite, GraphParseAssessment,
    GraphParseScope, argument_occurrence_id,
};
use crate::signatures::arguments::{ArgumentAdapter, syntax};
use crate::signatures::nodes::{call_callee, node_text};

use super::{GraphAccumulator, positions::NodePositions, validity};

pub(super) fn collect(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    content: &str,
    positions: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    caller_id: Option<&str>,
    caller: Option<&str>,
    deadline: std::time::Instant,
) {
    let Some(adapter) = acc.argument_adapter else {
        return;
    };
    if !adapter.is_recipient(node.kind()) {
        return;
    }
    if adapter == ArgumentAdapter::Scala
        && node.parent().is_some_and(|parent| {
            parent.kind() == "call_expression"
                && parent.child_by_field_name("function") == Some(node)
        })
    {
        return;
    }
    let (groups, recipient) = groups(node, adapter);
    let source_range = validity::byte_range(node);
    let id = argument_occurrence_id(
        &acc.file_path,
        &acc.source_digest,
        &source_range,
        "recipient",
    );
    let mut observations = Vec::new();
    for (group_index, group) in groups.into_iter().enumerate() {
        let mut group_cursor = group.walk();
        let attributed = group
            .named_children(&mut group_cursor)
            .any(|child| child.kind() == "attribute_item");
        let observation = if !attributed
            && matches!(group.kind(), "arguments" | "argument_list")
            && node.child_by_field_name("placement") != Some(group)
        {
            let mut slots = Vec::new();
            let mut cursor = group.walk();
            for slot in group.named_children(&mut cursor).filter(|n| !n.is_extra()) {
                if std::time::Instant::now() >= deadline || crate::deep_stack::job_cancelled() {
                    break;
                }
                let (label, value) = syntax::slot_value(adapter, slot, content);
                let observation = match value.map(|value| syntax::observe(adapter, value, content))
                {
                    Some(syntax::Observation::Reference(reference)) => {
                        GraphArgumentObservation::Reference {
                            target_name: reference.name,
                            form: reference.form,
                            selection_range: positions.range(reference.selection),
                            selection_byte_range: validity::byte_range(reference.selection),
                            binding: acc.argument_bindings.as_ref().map_or(
                                GraphArgumentBinding::NotAssessed,
                                |bindings| {
                                    bindings.assess(
                                        node_text(reference.root, content).unwrap_or_default(),
                                        slot.start_byte(),
                                    )
                                },
                            ),
                        }
                    }
                    Some(syntax::Observation::Value(value_kind)) => {
                        GraphArgumentObservation::NonReference { value_kind }
                    }
                    _ => GraphArgumentObservation::Unsupported {
                        construct: value.unwrap_or(slot).kind().to_owned(),
                    },
                };
                let source_range = validity::byte_range(slot);
                let index = slots.len() as u32;
                slots.push(GraphArgumentSlot {
                    id: argument_occurrence_id(
                        &acc.file_path,
                        &acc.source_digest,
                        &source_range,
                        &format!("argument:{group_index}:{index}"),
                    ),
                    index,
                    label,
                    range: positions.range(slot),
                    source_range,
                    observation,
                });
            }
            GraphArgumentGroupObservation::Slots { slots }
        } else {
            GraphArgumentGroupObservation::Unsupported {
                construct: group.kind().to_owned(),
            }
        };
        observations.push(GraphArgumentGroup {
            index: group_index as u32,
            range: positions.range(group),
            source_range: validity::byte_range(group),
            observation,
        });
    }
    let mut parse_assessment = validity::assess(node, ancestors, positions);
    if let Some(declaration) = caller_id.and_then(|id| {
        acc.declarations
            .iter()
            .rev()
            .find(|declaration| declaration.id == id)
    }) && matches!(
        declaration.parse_assessment,
        GraphParseAssessment::Recovered { .. }
    ) {
        parse_assessment = GraphParseAssessment::recovered(
            GraphParseScope::EnclosingDeclaration,
            declaration.range.clone(),
        );
    }
    match &mut parse_assessment {
        GraphParseAssessment::Parsed { scope, .. }
        | GraphParseAssessment::Recovered { scope, .. }
            if *scope == GraphParseScope::Declaration =>
        {
            *scope = GraphParseScope::Occurrence;
        }
        _ => {}
    }
    acc.argument_sites.push(GraphFactArgumentSite {
        id,
        caller: caller.unwrap_or("module").to_owned(),
        caller_id: caller_id.map(str::to_owned),
        call_id: acc
            .calls
            .last()
            .filter(|call| call.source_range.as_ref() == Some(&source_range))
            .map(|call| call.id.clone()),
        recipient_name: recipient
            .child_by_field_name("function")
            .and_then(
                |function| match syntax::observe(adapter, function, content) {
                    syntax::Observation::Reference(reference) => Some(reference.name),
                    _ => None,
                },
            )
            .or_else(|| {
                (recipient.kind() == "method_invocation")
                    .then(|| call_callee(recipient, content).map(|(name, _)| name))
                    .flatten()
            }),
        range: positions.range(node),
        source_range,
        parse_assessment,
        groups: observations,
    });
}

fn groups(node: Node<'_>, adapter: ArgumentAdapter) -> (Vec<Node<'_>>, Node<'_>) {
    let mut groups = Vec::new();
    let mut recipient = node;
    loop {
        if let Some(group) = recipient.child_by_field_name("arguments").or_else(|| {
            let mut cursor = recipient.walk();
            recipient
                .named_children(&mut cursor)
                .find(|n| matches!(n.kind(), "token_tree" | "argument_list"))
        }) {
            groups.push(group);
        }
        if adapter == ArgumentAdapter::Scala
            && let Some(inner) = recipient.child_by_field_name("function")
            && matches!(inner.kind(), "call_expression" | "instance_expression")
        {
            recipient = inner;
        } else {
            break;
        }
    }
    groups.reverse();
    for field in ["placement", "initializer"] {
        if let Some(group) = node.child_by_field_name(field) {
            groups.push(group);
        }
    }
    groups.sort_unstable_by_key(tree_sitter::Node::start_byte);
    (groups, recipient)
}

pub(super) fn join_bindings(acc: &mut GraphAccumulator) {
    let static_module_imports = acc
        .argument_adapter
        .is_some_and(ArgumentAdapter::static_module_imports);
    let declarations = acc
        .declarations
        .iter()
        .filter_map(|declaration| {
            declaration
                .selection_byte_range
                .as_ref()
                .map(|span| ((span.start_byte, span.end_byte), declaration))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut imports = std::collections::HashMap::<&str, Vec<&crate::graph::GraphFactImport>>::new();
    for import in &acc.imports {
        if let Some(name) = &import.local_name {
            imports.entry(name).or_default().push(import);
        }
    }
    for site in &mut acc.argument_sites {
        for group in &mut site.groups {
            let GraphArgumentGroupObservation::Slots { slots } = &mut group.observation else {
                continue;
            };
            for slot in slots {
                if let GraphArgumentObservation::Reference {
                    binding: binding @ GraphArgumentBinding::ScopedLookup { .. },
                    target_name,
                    ..
                } = &mut slot.observation
                {
                    let root = target_name
                        .split(['.', ':'])
                        .find(|part| !part.is_empty())
                        .unwrap_or_default();
                    if let Some(bindings) = imports.get(root) {
                        *binding = if static_module_imports
                            && let [import] = bindings.as_slice()
                            && import.module_scope.as_ref().is_none_or(Vec::is_empty)
                        {
                            GraphArgumentBinding::Import {
                                import_id: import.id.clone(),
                            }
                        } else {
                            GraphArgumentBinding::NotAssessed
                        };
                    }
                }
                if let GraphArgumentObservation::Reference {
                    binding: binding @ GraphArgumentBinding::Unrepresented { .. },
                    target_name,
                    ..
                } = &mut slot.observation
                {
                    let GraphArgumentBinding::Unrepresented { source_range } = binding else {
                        unreachable!()
                    };
                    if let Some(declaration) =
                        declarations.get(&(source_range.start_byte, source_range.end_byte))
                        && declaration.name == *target_name
                    {
                        *binding = GraphArgumentBinding::Declaration {
                            declaration_id: declaration.id.clone(),
                        };
                    }
                }
            }
        }
    }
}
