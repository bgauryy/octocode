//! Argument observations from OXC nodes, joined to its lexical reference map.

use std::collections::HashMap;

use oxc_ast::{AstKind, ast::*};
use oxc_semantic::Semantic;
use oxc_span::{GetSpan, Span};

use crate::deep_stack::job_cancelled;
use crate::graph::*;

use super::js_oxc_calls::unwrap_expr;
use super::js_oxc_shared::SpanPositions;

pub(super) fn collect(
    semantic: &Semantic<'_>,
    parsed: &oxc_parser::ParserReturn<'_>,
    positions: &SpanPositions<'_>,
    (source, digest): (&str, &str),
    file: &str,
    declarations: &[GraphFactDeclaration],
    imports: &[GraphFactImport],
    calls: &[GraphFactCall],
) -> Vec<GraphFactArgumentSite> {
    let by_name_span = declarations
        .iter()
        .filter_map(|d| {
            d.selection_byte_range
                .as_ref()
                .map(|s| ((s.start_byte, s.end_byte), d))
        })
        .collect::<HashMap<_, _>>();
    let by_call_span = calls
        .iter()
        .filter_map(|c| {
            c.source_range
                .as_ref()
                .map(|s| ((s.start_byte, s.end_byte), c))
        })
        .collect::<HashMap<_, _>>();
    let by_import_span = imports
        .iter()
        .filter_map(|import| {
            import.local_range.as_ref().map(|range| {
                let span = positions.byte_range(range);
                ((span.start_byte, span.end_byte), import)
            })
        })
        .collect::<HashMap<_, _>>();
    let dynamic_scope = semantic.nodes().iter().any(|node| {
        matches!(node.kind(), AstKind::WithStatement(_))
            || semantic
                .scoping()
                .scope_flags(node.scope_id())
                .contains_direct_eval()
    });
    let mut sites = Vec::new();
    for node in semantic.nodes().iter() {
        if job_cancelled() {
            break;
        }
        let (span, callee, arguments, group_start, unsupported_group) = match node.kind() {
            AstKind::CallExpression(call) => (
                call.span,
                &call.callee,
                Some(call.arguments.as_slice()),
                call.type_arguments
                    .as_ref()
                    .map_or(call.callee.span().end, |args| args.span.end),
                None,
            ),
            AstKind::NewExpression(call) => (
                call.span,
                &call.callee,
                Some(call.arguments.as_slice()),
                call.type_arguments
                    .as_ref()
                    .map_or(call.callee.span().end, |args| args.span.end),
                None,
            ),
            AstKind::TaggedTemplateExpression(call) => (
                call.span,
                &call.tag,
                None,
                call.quasi.span.start,
                Some(call.quasi.span),
            ),
            _ => continue,
        };
        let source_range = bytes(span);
        let call = by_call_span.get(&(span.start, span.end)).copied();
        let owner = declarations
            .iter()
            .filter(|d| {
                d.source_range
                    .as_ref()
                    .is_some_and(|s| s.start_byte <= span.start && span.end <= s.end_byte)
            })
            .min_by_key(|d| {
                d.source_range
                    .as_ref()
                    .map_or(u32::MAX, |s| s.end_byte - s.start_byte)
            });
        let parse_assessment = assessment(parsed, positions, span, owner);
        let mut slots = Vec::new();
        if let Some(arguments) = arguments {
            for (index, argument) in arguments.iter().enumerate() {
                if job_cancelled() {
                    break;
                }
                let span = argument.span();
                let observation = argument.as_expression().map_or_else(
                    || GraphArgumentObservation::Unsupported {
                        construct: "spread".to_owned(),
                    },
                    |expression| {
                        observation(
                            expression,
                            semantic,
                            positions,
                            &by_name_span,
                            &by_import_span,
                            dynamic_scope,
                        )
                    },
                );
                let source_range = bytes(span);
                slots.push(GraphArgumentSlot {
                    id: argument_occurrence_id(
                        file,
                        digest,
                        &source_range,
                        &format!("argument:0:{index}"),
                    ),
                    index: crate::num::saturating_u32(index),
                    label: None,
                    range: positions.range(span),
                    source_range,
                    observation,
                });
            }
        }
        let group_span = unsupported_group
            .unwrap_or_else(|| argument_group_span(parsed, source, group_start, span));
        let observation = unsupported_group.map_or_else(
            || GraphArgumentGroupObservation::Slots { slots },
            |_| GraphArgumentGroupObservation::Unsupported {
                construct: "tagged-template".to_owned(),
            },
        );
        sites.push(GraphFactArgumentSite {
            id: argument_occurrence_id(file, digest, &source_range, "recipient"),
            caller: call
                .map(|call| call.caller.clone())
                .or_else(|| owner.map(|d| d.name.clone()))
                .unwrap_or_else(|| "module".to_owned()),
            caller_id: call
                .and_then(|call| call.caller_id.clone())
                .or_else(|| owner.map(|d| d.id.clone())),
            call_id: call.map(|call| call.id.clone()),
            recipient_name: direct_reference(callee).map(|r| r.name),
            range: positions.range(span),
            source_range,
            parse_assessment,
            groups: vec![GraphArgumentGroup {
                index: 0,
                range: positions.range(group_span),
                source_range: bytes(group_span),
                observation,
            }],
        });
    }
    sites
}

fn argument_group_span(
    parsed: &oxc_parser::ParserReturn<'_>,
    source: &str,
    group_start: u32,
    span: Span,
) -> Span {
    let index = parsed
        .tokens
        .partition_point(|token| token.start() < group_start);
    let start = parsed.tokens[index..]
        .iter()
        .take_while(|token| token.start() < span.end)
        .find(|token| &source[token.start() as usize..token.end() as usize] == "(")
        .map_or(span.end, oxc_parser::Token::start);
    Span::new(start, span.end)
}

fn assessment(
    parsed: &oxc_parser::ParserReturn<'_>,
    positions: &SpanPositions<'_>,
    span: Span,
    owner: Option<&GraphFactDeclaration>,
) -> GraphParseAssessment {
    let mut parse_assessment = GraphParseAssessment::Parsed {
        scope: GraphParseScope::Occurrence,
        range: positions.range(span),
    };
    if let Some(owner) =
        owner.filter(|d| !matches!(d.parse_assessment, GraphParseAssessment::Parsed { .. }))
    {
        parse_assessment = GraphParseAssessment::recovered(
            GraphParseScope::EnclosingDeclaration,
            owner.range.clone(),
        );
    }
    for diagnostic in &parsed.diagnostics {
        if diagnostic.labels.is_empty() {
            parse_assessment = GraphParseAssessment::recovered(
                GraphParseScope::Document,
                positions.range(parsed.program.span),
            );
            break;
        }
        if diagnostic
            .labels
            .iter()
            .any(|label| label.offset() <= span.end && label.offset() + label.len() >= span.start)
        {
            parse_assessment =
                GraphParseAssessment::recovered(GraphParseScope::Occurrence, positions.range(span));
        }
    }
    parse_assessment
}

struct Reference<'a> {
    name: String,
    form: GraphArgumentForm,
    selected: Span,
    root: &'a IdentifierReference<'a>,
}

fn direct_reference<'a>(expression: &'a Expression<'a>) -> Option<Reference<'a>> {
    match unwrap_expr(expression) {
        Expression::Identifier(identifier) => Some(Reference {
            name: identifier.name.to_string(),
            form: GraphArgumentForm::Identifier,
            selected: identifier.span,
            root: identifier,
        }),
        Expression::StaticMemberExpression(member) if !member.optional => {
            let prefix = direct_reference(&member.object)?;
            Some(Reference {
                name: format!("{}.{}", prefix.name, member.property.name),
                form: GraphArgumentForm::Member,
                selected: member.property.span,
                root: prefix.root,
            })
        }
        _ => None,
    }
}

fn observation(
    expression: &Expression<'_>,
    semantic: &Semantic<'_>,
    positions: &SpanPositions<'_>,
    declarations: &HashMap<(u32, u32), &GraphFactDeclaration>,
    imports: &HashMap<(u32, u32), &GraphFactImport>,
    dynamic_scope: bool,
) -> GraphArgumentObservation {
    if let Some(reference) = direct_reference(expression) {
        let scoping = semantic.scoping();
        let binding = if dynamic_scope {
            GraphArgumentBinding::NotAssessed
        } else if let Some(id) = reference.root.reference_id.get() {
            match scoping.get_reference(id).symbol_id() {
                Some(symbol) => {
                    let span = scoping.symbol_span(symbol);
                    if reference.form == GraphArgumentForm::Identifier
                        && let Some(declaration) = declarations.get(&(span.start, span.end))
                    {
                        GraphArgumentBinding::Declaration {
                            declaration_id: declaration.id.clone(),
                        }
                    } else if scoping.symbol_flags(symbol).is_import()
                        && let Some(import) = imports.get(&(span.start, span.end))
                    {
                        GraphArgumentBinding::Import {
                            import_id: import.id.clone(),
                        }
                    } else {
                        GraphArgumentBinding::Unrepresented {
                            source_range: bytes(span),
                        }
                    }
                }
                None => GraphArgumentBinding::ScopedLookup {
                    implicit_members: false,
                },
            }
        } else {
            GraphArgumentBinding::NotAssessed
        };
        return GraphArgumentObservation::Reference {
            target_name: reference.name,
            form: reference.form,
            selection_range: positions.range(reference.selected),
            selection_byte_range: bytes(reference.selected),
            binding,
        };
    }
    let value_kind = match unwrap_expr(expression) {
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::TemplateLiteral(_) => GraphArgumentValueKind::Literal,
        Expression::CallExpression(_) | Expression::NewExpression(_) => {
            GraphArgumentValueKind::CallResult
        }
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
            GraphArgumentValueKind::Closure
        }
        Expression::BinaryExpression(_)
        | Expression::LogicalExpression(_)
        | Expression::ConditionalExpression(_)
        | Expression::ObjectExpression(_)
        | Expression::ArrayExpression(_)
        | Expression::SequenceExpression(_)
        | Expression::UnaryExpression(_)
        | Expression::UpdateExpression(_) => GraphArgumentValueKind::ValueExpression,
        _ => {
            return GraphArgumentObservation::Unsupported {
                construct: "unsupported-expression".to_owned(),
            };
        }
    };
    GraphArgumentObservation::NonReference { value_kind }
}

fn bytes(span: Span) -> crate::text::analysis::ObservedSpan {
    crate::text::analysis::ObservedSpan::root(span.start, span.end)
}
