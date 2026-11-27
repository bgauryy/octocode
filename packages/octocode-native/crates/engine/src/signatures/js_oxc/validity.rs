//! OXC diagnostics scoped to source occurrences. An unlocated diagnostic
//! conservatively assesses the whole document as recovered.

use crate::graph::{GraphFactCall, GraphFactDeclaration, GraphParseAssessment, GraphParseScope};
use crate::signatures::js_oxc_shared::SpanPositions;

pub(super) fn assess_recovery(
    parsed: &oxc_parser::ParserReturn<'_>,
    positions: &SpanPositions<'_>,
    declarations: &mut [GraphFactDeclaration],
    calls: &mut [GraphFactCall],
) {
    if parsed.diagnostics.is_empty() {
        return;
    }
    let unlocated = parsed
        .diagnostics
        .iter()
        .any(|error| error.labels.is_empty());
    let spans = parsed
        .diagnostics
        .iter()
        .flat_map(|error| {
            error
                .labels
                .iter()
                .map(|label| (label.offset(), label.offset() + label.len()))
        })
        .collect::<Vec<_>>();
    let assess = |range: &crate::graph::GraphRange| {
        let bytes = positions.byte_range(range);
        if unlocated {
            Some(GraphParseAssessment::recovered(
                GraphParseScope::Document,
                positions.range(parsed.program.span),
            ))
        } else if spans
            .iter()
            .any(|&(start, end)| start <= bytes.end_byte && end >= bytes.start_byte)
        {
            Some(GraphParseAssessment::recovered(
                GraphParseScope::Declaration,
                range.clone(),
            ))
        } else {
            None
        }
    };
    for declaration in declarations {
        if let Some(assessment) = assess(&declaration.range) {
            declaration.parse_assessment = assessment;
        }
    }
    for call in calls {
        if let Some(assessment) = assess(&call.range) {
            call.parse_assessment = assessment;
        }
    }
}
