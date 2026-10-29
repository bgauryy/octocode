//! CommonJS export shapes: the local bindings `module.exports` and its keys
//! hold once the module body has run. A name is exported only when every
//! write that could change it is an unconditional Program-level statement,
//! so source order alone decides the final value.

use std::collections::{BTreeMap, HashMap};

use oxc_ast::ast::*;
use oxc_semantic::{Scoping, Semantic};
use oxc_span::{GetSpan, Span};

use super::{inner, reference_symbol};
use crate::graph::{COMMONJS_WHOLE_EXPORT, GraphFactCode, GraphFactDiagnostic};
use crate::signatures::js_oxc_shared::SpanPositions;

mod census;

use census::{Target, census, global, target};

/// Skip the export walk when no token can spell `module` or `exports`.
pub(in crate::signatures) fn may_contain_exports(
    tokens: &[oxc_parser::Token],
    source: &str,
) -> bool {
    tokens.iter().any(|token| {
        token.kind() == oxc_parser::Kind::Ident
            && (token.escaped()
                || matches!(
                    &source[token.start() as usize..token.end() as usize],
                    "module" | "exports"
                ))
    })
}

/// A Program-level `=` write, in source order: a key write also keeps the
/// assignment's span and the key's, the anchors of an in-place definition.
enum Write<'a> {
    Whole(&'a Expression<'a>),
    Key(&'a str, bool, &'a Expression<'a>, Span, Span),
}

/// A function, arrow or class an export holds that the module defines in
/// place (`exports.k = function () {}`, `module.exports = { k() {} }`,
/// `module.exports = function name() {}`): the declaration `name` callers
/// reach it by, spanning `span` and selected at `name_span`.
pub(in crate::signatures) struct DefinedExport<'a> {
    pub(in crate::signatures) name: String,
    pub(in crate::signatures) span: Span,
    pub(in crate::signatures) name_span: Span,
    pub(in crate::signatures) value: &'a Expression<'a>,
}

/// What an export holds once the module body has run.
enum Held<'a> {
    /// A module-scope binding.
    Local(&'a IdentifierReference<'a>),
    Defined(DefinedExport<'a>),
    /// Any other value: no declaration.
    Opaque,
}

/// The proven CommonJS exports of a module: `(local, public)` name pairs of
/// module-scope bindings, and the definitions exported in place with their
/// public names.
#[derive(Default)]
pub(in crate::signatures) struct CommonJsExports<'a> {
    pub(in crate::signatures) names: Vec<(String, String)>,
    pub(in crate::signatures) defined: Vec<(String, DefinedExport<'a>)>,
}

/// The module's proven exports, or one linking diagnostic when the writes
/// cannot be ordered (and then no exports at all).
pub(in crate::signatures) fn collect_common_js_exports<'a>(
    semantic: &Semantic<'a>,
    program: &'a Program<'a>,
    li: &SpanPositions,
) -> (CommonJsExports<'a>, Option<GraphFactDiagnostic>) {
    let scoping = semantic.scoping();
    let mut census = census(semantic);
    let mut writes = Vec::new();
    let mut idioms = 0usize;
    for statement in &program.body {
        if let Statement::ExpressionStatement(statement) = statement {
            program_writes(scoping, &statement.expression, &mut writes, &mut idioms);
        }
    }
    let program_wholes = writes
        .iter()
        .filter(|write| matches!(write, Write::Whole(_)))
        .count();
    if census.unprovable
        || census.exports_rebinds != idioms
        || census.whole_writes != program_wholes
        || program_wholes > 1
    {
        let diagnostic = (census.first_write.is_some() || census.exports_rebinds > 0).then(|| {
            GraphFactDiagnostic {
                code: GraphFactCode::UnsupportedCommonJsExports,
                message: "CommonJS export writes are repeated, conditional, deferred or rebound; no CommonJS names are exported".to_owned(),
                line: census.first_write.map(|span| li.range(span).start.line + 1),
            }
        });
        return (CommonJsExports::default(), diagnostic);
    }
    let mut program_keys = HashMap::<&str, usize>::new();
    let mut whole = Held::Opaque;
    let mut seen_whole = false;
    let mut keys = BTreeMap::<String, Held>::new();
    for write in writes {
        match write {
            Write::Whole(value) => {
                seen_whole = true;
                keys.clear();
                whole = match inner(value) {
                    Expression::ObjectExpression(object) => {
                        census.keys_unknown |= object_keys(object, &mut keys);
                        Held::Opaque
                    }
                    value => match own_name(value) {
                        Some((name, name_span)) => {
                            held(value, name.to_owned(), value.span(), name_span)
                        }
                        None => held(value, String::new(), value.span(), value.span()),
                    },
                };
            }
            Write::Key(key, via_exports, value, span, name_span) => {
                *program_keys.entry(key).or_default() += 1;
                // A whole assignment detaches `exports` from `module.exports`,
                // unless it is the idiom that rebinds `exports` to the new
                // value (the census admits rebinds only as that idiom).
                if !(via_exports && seen_whole && idioms == 0) {
                    keys.insert(key.to_owned(), held(value, key.to_owned(), span, name_span));
                }
            }
        }
    }
    let local = |identifier: &IdentifierReference| {
        let symbol = reference_symbol(scoping, identifier)?;
        (scoping.symbol_scope_id(symbol) == scoping.root_scope_id()
            && !scoping.symbol_flags(symbol).is_import()
            && !scoping.symbol_is_mutated(symbol)
            && scoping.symbol_redeclarations(symbol).is_empty())
        .then(|| identifier.name.as_str().to_owned())
    };
    let mut exports = CommonJsExports::default();
    let mut admit = |public: String, held: Held<'a>| match held {
        Held::Local(identifier) => {
            if let Some(name) = local(identifier) {
                exports.names.push((name, public));
            }
        }
        // A definition is named by what callers write; an unnamed whole
        // export has no such name.
        Held::Defined(definition) if !definition.name.is_empty() => {
            exports.defined.push((public, definition));
        }
        _ => {}
    };
    admit(COMMONJS_WHOLE_EXPORT.to_owned(), whole);
    if !census.keys_unknown {
        for (key, held) in keys {
            // A key also written outside Program-level statements has no
            // source-ordered final value.
            if census.key_writes.get(key.as_str()) != program_keys.get(key.as_str()) {
                continue;
            }
            admit(key, held);
        }
    }
    // A definition name the module also binds, or that two definitions
    // share, would name several declarations: neither is declared.
    let mut names = HashMap::<&str, usize>::new();
    for (_, definition) in &exports.defined {
        *names.entry(definition.name.as_str()).or_default() += 1;
    }
    let ambiguous = names
        .into_iter()
        .filter(|(name, count)| *count > 1 || scoping.get_root_binding((*name).into()).is_some())
        .map(|(name, _)| name.to_owned())
        .collect::<Vec<_>>();
    exports
        .defined
        .retain(|(_, definition)| !ambiguous.contains(&definition.name));
    (exports, None)
}

/// What `value` holds when written to an export named `name`.
fn held<'a>(value: &'a Expression<'a>, name: String, span: Span, name_span: Span) -> Held<'a> {
    match inner(value) {
        Expression::Identifier(identifier) => Held::Local(identifier),
        value @ (Expression::FunctionExpression(_)
        | Expression::ArrowFunctionExpression(_)
        | Expression::ClassExpression(_)) => Held::Defined(DefinedExport {
            name,
            span,
            name_span,
            value,
        }),
        _ => Held::Opaque,
    }
}

/// The name a function or class expression gives itself.
fn own_name<'a>(value: &'a Expression<'a>) -> Option<(&'a str, Span)> {
    let id = match value {
        Expression::FunctionExpression(function) => function.id.as_ref(),
        Expression::ClassExpression(class) => class.id.as_ref(),
        _ => None,
    }?;
    Some((id.name.as_str(), id.span))
}

/// Program-level `=` writes of `expression`, through sequences and chains
/// (`exports.b = exports.a = void 0`), and the `exports = module.exports = X`
/// idioms among them.
fn program_writes<'a>(
    scoping: &Scoping,
    expression: &'a Expression<'a>,
    writes: &mut Vec<Write<'a>>,
    idioms: &mut usize,
) {
    match expression.without_parentheses() {
        Expression::SequenceExpression(sequence) => {
            for expression in &sequence.expressions {
                program_writes(scoping, expression, writes, idioms);
            }
        }
        Expression::AssignmentExpression(assignment)
            if assignment.operator == AssignmentOperator::Assign =>
        {
            // A chain assigns right to left.
            program_writes(scoping, &assignment.right, writes, idioms);
            let value = value_of(&assignment.right);
            // The left side of the next link in the chain, if any.
            let next = match assignment.right.without_parentheses() {
                Expression::AssignmentExpression(next) => Some(&next.left),
                _ => None,
            };
            if rebinds_exports(scoping, &assignment.left) {
                // `exports = module.exports = X`
                *idioms += usize::from(next.is_some_and(|left| writes_whole(scoping, left)));
                return;
            }
            let Some(member) = assignment.left.as_member_expression() else {
                return;
            };
            match target(scoping, member.object(), member.static_property_name()) {
                Some(Target::Whole) => {
                    // `module.exports = exports = X`
                    *idioms += usize::from(next.is_some_and(|left| rebinds_exports(scoping, left)));
                    writes.push(Write::Whole(value));
                }
                Some(Target::Key(Some(key), via_exports)) => {
                    let name_span = match member {
                        MemberExpression::StaticMemberExpression(member) => member.property.span,
                        member => member.span(),
                    };
                    writes.push(Write::Key(
                        key,
                        via_exports,
                        value,
                        assignment.span,
                        name_span,
                    ));
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn rebinds_exports(scoping: &Scoping, left: &AssignmentTarget) -> bool {
    matches!(left, AssignmentTarget::AssignmentTargetIdentifier(identifier)
        if global(scoping, identifier) == Some(false))
}

fn writes_whole(scoping: &Scoping, left: &AssignmentTarget) -> bool {
    left.as_member_expression().is_some_and(|member| {
        matches!(
            target(scoping, member.object(), member.static_property_name()),
            Some(Target::Whole)
        )
    })
}

/// The value an assignment chain stores.
fn value_of<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
    match expression.without_parentheses() {
        Expression::AssignmentExpression(assignment)
            if assignment.operator == AssignmentOperator::Assign =>
        {
            value_of(&assignment.right)
        }
        other => other,
    }
}

/// The keys of a `module.exports = { … }` literal, last one winning;
/// `true` when a spread or computed key may set any key.
fn object_keys<'a>(
    object: &'a ObjectExpression<'a>,
    keys: &mut BTreeMap<String, Held<'a>>,
) -> bool {
    let mut unknown = false;
    for property in &object.properties {
        match property {
            ObjectPropertyKind::SpreadProperty(_) => unknown = true,
            ObjectPropertyKind::ObjectProperty(property) => {
                let Some(key) = property.key.static_name() else {
                    unknown = true;
                    continue;
                };
                let held = if property.kind == PropertyKind::Init {
                    held(
                        &property.value,
                        key.clone().into_owned(),
                        property.span,
                        property.key.span(),
                    )
                } else {
                    Held::Opaque
                };
                keys.insert(key.into_owned(), held);
            }
        }
    }
    unknown
}
