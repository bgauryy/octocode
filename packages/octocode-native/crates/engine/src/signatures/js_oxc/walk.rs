//! The statement walk that collects document symbols: top-level and IIFE
//! statements, member-assigned functions and declarations nested in functions.

use oxc_ast::ast::{
    ArrowFunctionBody, BindingPattern, Class, Declaration, ExportDefaultDeclarationKind,
    Expression, Function, MemberExpression, Program, Statement,
};
use oxc_ast_visit::{VisitJs, walk_js};
use oxc_semantic::ScopeFlags;
use oxc_span::GetSpan;

use crate::deep_stack::job_cancelled;

use crate::signatures::js_oxc_shared::SpanPositions;

use super::DocumentSymbol;
use super::symbols::{
    class_symbol, collect_variable, container, enum_symbol, external_module_symbol, global_symbol,
    interface_symbol, namespace_symbol, type_alias_symbol,
};
use crate::lsp::symbol_kind;

pub(super) fn collect_program(
    program: &Program,
    li: &SpanPositions,
    out: &mut Vec<DocumentSymbol>,
) {
    for stmt in &program.body {
        collect_statement(stmt, li, out);
    }
}

pub(super) fn collect_statement(
    stmt: &Statement,
    li: &SpanPositions,
    out: &mut Vec<DocumentSymbol>,
) {
    let first = out.len();
    match stmt {
        Statement::FunctionDeclaration(f) => push_opt(out, function_symbol(f, li)),
        Statement::ClassDeclaration(c) => push_opt(out, class_symbol(c, li)),
        Statement::VariableDeclaration(v) => collect_variable(v, li, out),
        Statement::TSInterfaceDeclaration(i) => push_opt(out, interface_symbol(i, li)),
        Statement::TSEnumDeclaration(e) => push_opt(out, enum_symbol(e, li)),
        Statement::TSNamespaceDeclaration(m) => push_opt(out, namespace_symbol(m, li)),
        Statement::TSExternalModuleDeclaration(m) => push_opt(out, external_module_symbol(m, li)),
        Statement::TSGlobalDeclaration(m) => push_opt(out, global_symbol(m, li)),
        Statement::TSTypeAliasDeclaration(t) => push_opt(out, type_alias_symbol(t, li)),
        Statement::ExportDeclaration(e) => {
            collect_declaration(&e.declaration, li, out);
        }
        Statement::ExportDefaultDeclaration(e) => match &e.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                push_opt(out, function_symbol(f, li));
            }
            ExportDefaultDeclarationKind::ClassDeclaration(c) => push_opt(out, class_symbol(c, li)),
            _ => {}
        },
        // A top-level IIFE (UMD wrappers, legacy bundles) is the module's real
        // scope: outline its body instead of reporting an empty file.
        Statement::ExpressionStatement(e) => {
            if let Some(statements) = iife_statements(&e.expression) {
                for stmt in statements {
                    collect_statement(stmt, li, out);
                }
            }
        }
        _ => {}
    }
    // Export syntax belongs to the declaration, while selection ranges keep
    // pointing at symbol names. For multiple bindings, attach only the outer
    // edges so sibling ranges do not swallow each other.
    if matches!(
        stmt,
        Statement::ExportDeclaration(_) | Statement::ExportDefaultDeclaration(_)
    ) {
        let span = li.range(stmt.span());
        // A decorator list written before `export` belongs to the class, so
        // the declaration starts at whichever of the two comes first.
        if let Some(symbol) = out.get_mut(first)
            && span.start < symbol.range.start
        {
            symbol.range.start = span.start;
        }
        if out.len() > first
            && let Some(symbol) = out.last_mut()
        {
            symbol.range.end = span.end;
        }
    }
}

/// Functions and classes assigned to a member at module scope
/// (`res.redirect = function () {}`, `exports.handler = () => {}`,
/// `Widget.prototype.render = function () {}`): CommonJS and prototype-style
/// modules declare their API this way. Each spans its whole statement and is
/// named, like a method, by the assigned property, whose span is its
/// selection (the anchor a language server resolves). Only the declaration
/// outline collects them; the dependency graph keeps its binding-based
/// declarations.
pub(super) fn collect_member_functions(
    statements: &[Statement],
    li: &SpanPositions,
    out: &mut Vec<DocumentSymbol>,
) {
    for stmt in statements {
        let Statement::ExpressionStatement(statement) = stmt else {
            continue;
        };
        if let Some(body) = iife_statements(&statement.expression) {
            collect_member_functions(body, li, out);
            continue;
        }
        let Expression::AssignmentExpression(assignment) =
            statement.expression.without_parentheses()
        else {
            continue;
        };
        let Some(MemberExpression::StaticMemberExpression(target)) =
            assignment.left.as_member_expression()
        else {
            continue;
        };
        if member_path(&target.object).is_none() {
            continue;
        }
        let Some((symbol_kind, value)) = assigned_definition(&assignment.right) else {
            continue;
        };
        out.push(container(
            target.property.name.as_str(),
            symbol_kind,
            statement.span,
            target.property.span,
            function_value_children(value, li).unwrap_or_default(),
            li,
        ));
    }
}

/// Whether `expression` is `a`, `this`, or a static member chain `a.b.c`
/// (not a call or computed member).
fn member_path(expression: &Expression) -> Option<()> {
    match expression.without_parentheses() {
        Expression::Identifier(_) | Expression::ThisExpression(_) => Some(()),
        Expression::StaticMemberExpression(member) => member_path(&member.object),
        _ => None,
    }
}

/// The function or class an assignment's right side defines, through chained
/// assignments (`a.x = b.x = function () {}`).
fn assigned_definition<'a>(expression: &'a Expression<'a>) -> Option<(u8, &'a Expression<'a>)> {
    match expression.without_parentheses() {
        value @ (Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)) => {
            Some((symbol_kind::FUNCTION, value))
        }
        value @ Expression::ClassExpression(_) => Some((symbol_kind::CLASS, value)),
        Expression::AssignmentExpression(inner) => assigned_definition(&inner.right),
        _ => None,
    }
}

/// Body statements of an immediately invoked function expression:
/// `(function(){…})()`, `(() => {…})()`, `!function(){…}()`, and
/// `(function(){…}).call(this, …)`.
fn iife_statements<'a>(expression: &'a Expression<'a>) -> Option<&'a [Statement<'a>]> {
    match expression.without_parentheses() {
        Expression::UnaryExpression(unary) => iife_statements(&unary.argument),
        Expression::CallExpression(call) => {
            let callee = match call.callee.without_parentheses() {
                Expression::StaticMemberExpression(member)
                    if matches!(member.property.name.as_str(), "call" | "apply") =>
                {
                    member.object.without_parentheses()
                }
                callee => callee,
            };
            match callee {
                Expression::FunctionExpression(function) => function
                    .body
                    .as_ref()
                    .map(|body| body.statements.as_slice()),
                Expression::ArrowFunctionExpression(arrow) => match &arrow.body {
                    ArrowFunctionBody::FunctionBody(body) => Some(body.statements.as_slice()),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    }
}

fn collect_declaration(decl: &Declaration, li: &SpanPositions, out: &mut Vec<DocumentSymbol>) {
    match decl {
        Declaration::FunctionDeclaration(f) => push_opt(out, function_symbol(f, li)),
        Declaration::ClassDeclaration(c) => push_opt(out, class_symbol(c, li)),
        Declaration::VariableDeclaration(v) => collect_variable(v, li, out),
        Declaration::TSInterfaceDeclaration(i) => push_opt(out, interface_symbol(i, li)),
        Declaration::TSEnumDeclaration(e) => push_opt(out, enum_symbol(e, li)),
        Declaration::TSNamespaceDeclaration(m) => push_opt(out, namespace_symbol(m, li)),
        Declaration::TSExternalModuleDeclaration(m) => push_opt(out, external_module_symbol(m, li)),
        Declaration::TSGlobalDeclaration(m) => push_opt(out, global_symbol(m, li)),
        Declaration::TSTypeAliasDeclaration(t) => push_opt(out, type_alias_symbol(t, li)),
        _ => {}
    }
}

pub(super) fn push_opt(out: &mut Vec<DocumentSymbol>, symbol: Option<DocumentSymbol>) {
    if let Some(symbol) = symbol {
        out.push(symbol);
    }
}

fn function_symbol(f: &Function, li: &SpanPositions) -> Option<DocumentSymbol> {
    let id = f.id.as_ref()?;
    Some(container(
        id.name.as_str(),
        symbol_kind::FUNCTION,
        f.span,
        id.span,
        nested_in_function(f, li),
        li,
    ))
}

/// Named declarations inside a function body: function declarations,
/// function-valued variables, and classes. The tree-sitter extractors report
/// nested Python and Rust functions the same way, each under its parent.
/// Ordinary locals stay out of the outline.
struct NestedDeclarations<'l, 's> {
    li: &'l SpanPositions<'s>,
    out: Vec<DocumentSymbol>,
}

impl<'a> VisitJs<'a> for NestedDeclarations<'_, '_> {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if job_cancelled() {
            return;
        }
        // A declaration is its own container and collects its own body; a
        // function expression is transparent, so its declarations belong to
        // the enclosing named declaration.
        match it
            .is_declaration()
            .then(|| function_symbol(it, self.li))
            .flatten()
        {
            Some(symbol) => self.out.push(symbol),
            None => walk_js::walk_function(self, it, flags),
        }
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        match it
            .is_declaration()
            .then(|| class_symbol(it, self.li))
            .flatten()
        {
            Some(symbol) => self.out.push(symbol),
            None => walk_js::walk_class(self, it),
        }
    }

    fn visit_variable_declarator(&mut self, it: &oxc_ast::ast::VariableDeclarator<'a>) {
        if let BindingPattern::BindingIdentifier(id) = &it.id
            && let Some(init) = &it.init
            && let Some(children) = function_value_children(init, self.li)
        {
            self.out.push(container(
                id.name.as_str(),
                symbol_kind::FUNCTION,
                it.span,
                id.span,
                children,
                self.li,
            ));
            return;
        }
        walk_js::walk_variable_declarator(self, it);
    }
}

pub(super) fn nested_in_function(f: &Function, li: &SpanPositions) -> Vec<DocumentSymbol> {
    let mut nested = NestedDeclarations {
        li,
        out: Vec::new(),
    };
    if let Some(body) = &f.body {
        nested.visit_function_body(body);
    }
    nested.out
}

/// The nested declarations of a function-valued initializer (`() => {…}`,
/// `function () {…}`), or `None` when the value is not a function.
pub(super) fn function_value_children(
    init: &Expression,
    li: &SpanPositions,
) -> Option<Vec<DocumentSymbol>> {
    let mut nested = NestedDeclarations {
        li,
        out: Vec::new(),
    };
    match init.without_parentheses() {
        Expression::ArrowFunctionExpression(arrow) => nested.visit_arrow_function_body(&arrow.body),
        Expression::FunctionExpression(function) => {
            if let Some(body) = &function.body {
                nested.visit_function_body(body);
            }
        }
        _ => return None,
    }
    Some(nested.out)
}
