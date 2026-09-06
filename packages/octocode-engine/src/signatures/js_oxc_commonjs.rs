//! CommonJS module-load candidates with native in-file binding provenance.
//! This establishes literal module paths and lexical scope, not runtime identity.

use std::collections::{HashMap, HashSet};

use oxc_ast::{ast::*, AstKind};
use oxc_semantic::SemanticBuilder;

use super::{module_export_name, GraphCommonJsLoad, LineIndex};

/// Skip scope construction only when OXC's tokens rule out every supported
/// loader spelling. Comments, regexes, and ordinary prose strings cannot create
/// candidates. Escaped identifiers and sufficiently long escaped strings are
/// conservatively retained for AST decoding and binding checks.
pub(super) fn may_contain_loader(tokens: &[oxc_parser::Token], source: &str) -> bool {
    tokens.iter().any(|token| {
        let kind = token.kind();
        if kind != oxc_parser::Kind::Ident
            && kind != oxc_parser::Kind::Require
            && kind != oxc_parser::Kind::Str
        {
            return false;
        }
        let text = &source[token.start() as usize..token.end() as usize];
        if kind != oxc_parser::Kind::Str {
            token.escaped() || matches!(text, "require" | "createRequire")
        } else {
            // An escaped literal spelling a seven-character loader name needs
            // at least nine source bytes, including its two quote delimiters.
            (token.escaped() && text.len() >= 9)
                || matches!(
                    text,
                    "'require'" | "\"require\"" | "'createRequire'" | "\"createRequire\""
                )
        }
    })
}

pub(super) fn collect_common_js_loads(program: &Program, li: &LineIndex) -> Vec<GraphCommonJsLoad> {
    let semantic = SemanticBuilder::new()
        .with_build_nodes(true)
        .build(program)
        .semantic;
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut factories = HashSet::new();
    for statement in &program.body {
        let Statement::ImportDeclaration(import) = statement else {
            continue;
        };
        if !matches!(import.source.value.as_str(), "node:module" | "module")
            || import.import_kind == ImportOrExportKind::Type
        {
            continue;
        }
        for specifier in import.specifiers.iter().flatten() {
            let ImportDeclarationSpecifier::ImportSpecifier(specifier) = specifier else {
                continue;
            };
            if module_export_name(&specifier.imported).as_deref() == Some("createRequire")
                && specifier.import_kind != ImportOrExportKind::Type
            {
                if let Some(symbol) = specifier.local.symbol_id.get() {
                    factories.insert(symbol);
                }
            }
        }
    }
    let reference_symbol = |identifier: &IdentifierReference| {
        identifier
            .reference_id
            .get()
            .and_then(|id| scoping.get_reference(id).symbol_id())
    };
    let mut global_writes = HashSet::new();
    let mut symbol_writes = HashSet::new();
    let mut module_member_writes = Vec::new();
    let mut deferred_bodies = Vec::new();
    for node in nodes.iter() {
        match node.kind() {
            AstKind::Function(function) => deferred_bodies.push(function.span),
            AstKind::ArrowFunctionExpression(function) => deferred_bodies.push(function.span),
            AstKind::AssignmentExpression(assignment) => {
                if let Some(member) = assignment.left.as_member_expression() {
                    let is_require = match member {
                        MemberExpression::StaticMemberExpression(member) => {
                            member.property.name == "require"
                        }
                        MemberExpression::ComputedMemberExpression(member) => {
                            matches!(inner(&member.expression), Expression::StringLiteral(literal) if literal.value == "require")
                        }
                        _ => false,
                    };
                    if is_require
                        && matches!(inner(member.object()), Expression::Identifier(identifier)
                            if identifier.name == "module" && reference_symbol(identifier).is_none())
                    {
                        module_member_writes.push(assignment.span);
                    }
                }
            }
            _ => {}
        }
        if let AstKind::IdentifierReference(identifier) = node.kind() {
            if let Some(id) = identifier.reference_id.get() {
                let reference = scoping.get_reference(id);
                if reference.symbol_id().is_none() && reference.is_write() {
                    global_writes.insert(identifier.name.as_str());
                }
                if reference.is_write() {
                    if let Some(symbol) = reference.symbol_id() {
                        symbol_writes.insert(symbol);
                    }
                }
            }
        }
    }
    let is_deferred = |offset| {
        deferred_bodies
            .iter()
            .any(|span| span.start <= offset && offset < span.end)
    };
    let deferred_member_write = module_member_writes
        .iter()
        .any(|span| is_deferred(span.start));
    let first_member_write_end = module_member_writes.iter().map(|span| span.end).min();
    let module_reassigned = |call: &CallExpression| {
        global_writes.contains("module")
            || first_member_write_end.is_some_and(|end| {
                // Assignment RHS calls run before the member write commits. Function
                // bodies can execute later, so source offsets cannot order them.
                end <= call.span.start || deferred_member_write || is_deferred(call.span.start)
            })
    };
    let mut constants = HashMap::new();
    let mut loaders = HashMap::new();
    for node in nodes.iter() {
        let AstKind::VariableDeclaration(declaration) = node.kind() else {
            continue;
        };
        if declaration.kind != VariableDeclarationKind::Const {
            continue;
        }
        for declarator in &declaration.declarations {
            let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
                continue;
            };
            let Some(symbol) = identifier.symbol_id.get() else {
                continue;
            };
            let Some(initializer) = declarator.init.as_ref() else {
                continue;
            };
            if let Expression::StringLiteral(literal) = inner(initializer) {
                if symbol_writes.contains(&symbol) {
                    continue;
                }
                constants.insert(symbol, literal.value.as_str().to_owned());
            }
            let Expression::CallExpression(call) = inner(initializer) else {
                continue;
            };
            let Expression::Identifier(factory) = inner(&call.callee) else {
                continue;
            };
            if reference_symbol(factory).is_some_and(|id| factories.contains(&id)) {
                let local_base = call.arguments.len() == 1
                    && call.arguments[0]
                        .as_expression()
                        .is_some_and(is_import_meta_url);
                let reason = if symbol_writes.contains(&symbol)
                    || reference_symbol(factory).is_some_and(|id| symbol_writes.contains(&id))
                {
                    Some("loader-reassigned")
                } else {
                    (!local_base).then_some("non-local-loader-base")
                };
                loaders.insert(symbol, reason);
            }
        }
    }
    let mut loads = Vec::new();
    for node in nodes.iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let (binding, loader_reason) = match inner(&call.callee) {
            Expression::Identifier(identifier) => match reference_symbol(identifier) {
                None if identifier.name == "require" => (
                    "unshadowed-global",
                    global_writes
                        .contains("require")
                        .then_some("loader-reassigned"),
                ),
                Some(symbol) if loaders.contains_key(&symbol) => {
                    ("create-require", loaders[&symbol])
                }
                _ => continue,
            },
            Expression::StaticMemberExpression(member) if member.property.name == "require" => {
                let Expression::Identifier(identifier) = inner(&member.object) else {
                    continue;
                };
                if identifier.name != "module" || reference_symbol(identifier).is_some() {
                    continue;
                }
                (
                    "unshadowed-global",
                    module_reassigned(call).then_some("loader-reassigned"),
                )
            }
            Expression::ComputedMemberExpression(member) if matches!(inner(&member.expression), Expression::StringLiteral(literal) if literal.value == "require") =>
            {
                let Expression::Identifier(identifier) = inner(&member.object) else {
                    continue;
                };
                if identifier.name != "module" || reference_symbol(identifier).is_some() {
                    continue;
                }
                (
                    "unshadowed-global",
                    module_reassigned(call).then_some("loader-reassigned"),
                )
            }
            _ => continue,
        };
        let specifier = if loader_reason.is_none() && !call.optional && call.arguments.len() == 1 {
            call.arguments[0]
                .as_expression()
                .and_then(|argument| match inner(argument) {
                    Expression::StringLiteral(literal) => Some(literal.value.as_str().to_owned()),
                    Expression::Identifier(identifier) => reference_symbol(identifier)
                        .and_then(|symbol| constants.get(&symbol).cloned()),
                    _ => None,
                })
        } else {
            None
        };
        let reason =
            loader_reason.or_else(|| specifier.is_none().then_some("non-literal-specifier"));
        loads.push(GraphCommonJsLoad {
            specifier,
            line: li.range(call.span).start.line + 1,
            kind: "commonjs-require",
            binding,
            reason,
        });
    }
    loads
}

fn is_import_meta_url(expression: &Expression) -> bool {
    matches!(inner(expression), Expression::StaticMemberExpression(member)
        if member.property.name == "url" && matches!(inner(&member.object), Expression::ImportMeta(_)))
}

fn inner<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
    expression.get_inner_expression()
}
