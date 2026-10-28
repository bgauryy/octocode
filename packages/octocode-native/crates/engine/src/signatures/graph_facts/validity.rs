//! Recovery context for extracted occurrences, without guessing repaired names.

use tree_sitter::Node;

use crate::graph::{GraphParseAssessment, GraphParseScope, ObservedSpan};
use crate::signatures::nodes::declaration_extent;

use super::positions::NodePositions;

pub(super) fn byte_range(node: Node<'_>) -> ObservedSpan {
    ObservedSpan::root(node.start_byte() as u32, node.end_byte() as u32)
}

/// A clean child can belong to a recovered header. Inspect the carried
/// ancestor path up to a body boundary, excluding the document root so an
/// unrelated malformed sibling cannot taint a healthy declaration.
pub(super) fn assess(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    positions: &NodePositions<'_>,
) -> GraphParseAssessment {
    let extent = declaration_extent(node);
    if ancestors.first().is_some_and(|root| !root.has_error()) {
        return GraphParseAssessment::parsed(positions.range(extent));
    }
    if extent.has_error() || extent.is_error() || extent.is_missing() {
        return GraphParseAssessment::recovered(
            GraphParseScope::Declaration,
            positions.range(extent),
        );
    }
    let mut child = node;
    for &parent in ancestors.iter().rev() {
        if parent.parent().is_none() {
            break;
        }
        if parent.is_error() || parent.is_missing() {
            return GraphParseAssessment::recovered(
                GraphParseScope::EnclosingDeclaration,
                positions.range(parent),
            );
        }
        if parent.child_by_field_name("body").is_some_and(|body| {
            body.start_byte() <= child.start_byte() && child.end_byte() <= body.end_byte()
        }) {
            break;
        }
        if parent.has_error() {
            return GraphParseAssessment::recovered(
                GraphParseScope::EnclosingDeclaration,
                positions.range(parent),
            );
        }
        child = parent;
    }
    GraphParseAssessment::parsed(positions.range(extent))
}
