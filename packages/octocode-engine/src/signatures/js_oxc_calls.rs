//! Call-graph extraction for the JS/TS graph facts (`extract_graph_facts`).
//!
//! Split out of `js_oxc.rs`: walks the oxc AST attributing every call/construct
//! site to its enclosing owner. Only `collect_program_calls` is public to the
//! parent module; everything else is an internal walker.

use super::{property_key_name, GraphCall, LineIndex};
use oxc_ast::ast::*;
use oxc_span::Span;

pub(super) fn collect_program_calls(program: &Program, li: &LineIndex, calls: &mut Vec<GraphCall>) {
    for stmt in &program.body {
        collect_statement_calls(stmt, li, calls);
    }
}

fn collect_statement_calls(stmt: &Statement, li: &LineIndex, calls: &mut Vec<GraphCall>) {
    match stmt {
        Statement::FunctionDeclaration(function) => {
            if let Some(id) = &function.id {
                collect_function_calls(id.name.as_str(), function, li, calls);
            }
        }
        Statement::ClassDeclaration(class) => collect_class_calls(class, li, calls),
        Statement::VariableDeclaration(variable) => collect_variable_calls(variable, li, calls),
        Statement::ExportNamedDeclaration(decl) => {
            if let Some(inner) = &decl.declaration {
                collect_declaration_calls(inner, li, calls);
            }
        }
        Statement::ExportDefaultDeclaration(decl) => match &decl.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                let owner = function
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .unwrap_or("default");
                collect_function_calls(owner, function, li, calls);
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                collect_class_calls(class, li, calls);
            }
            _ => {}
        },
        _ => {}
    }
}

fn collect_declaration_calls(decl: &Declaration, li: &LineIndex, calls: &mut Vec<GraphCall>) {
    match decl {
        Declaration::FunctionDeclaration(function) => {
            if let Some(id) = &function.id {
                collect_function_calls(id.name.as_str(), function, li, calls);
            }
        }
        Declaration::ClassDeclaration(class) => collect_class_calls(class, li, calls),
        Declaration::VariableDeclaration(variable) => collect_variable_calls(variable, li, calls),
        _ => {}
    }
}

fn collect_class_calls(class: &Class, li: &LineIndex, calls: &mut Vec<GraphCall>) {
    for element in &class.body.body {
        if let ClassElement::MethodDefinition(method) = element {
            if let Some((name, _span)) = property_key_name(&method.key) {
                collect_function_calls(&name, &method.value, li, calls);
            }
        }
    }
}

fn collect_variable_calls(
    variable: &VariableDeclaration,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    for declarator in &variable.declarations {
        let BindingPattern::BindingIdentifier(id) = &declarator.id else {
            continue;
        };
        match &declarator.init {
            Some(Expression::ArrowFunctionExpression(arrow)) => {
                collect_arrow_calls(id.name.as_str(), arrow, li, calls);
            }
            Some(Expression::FunctionExpression(function)) => {
                collect_function_calls(id.name.as_str(), function, li, calls);
            }
            Some(expression) => collect_expression_calls(id.name.as_str(), expression, li, calls),
            None => {}
        }
    }
}

fn collect_params_calls(
    owner: &str,
    params: &FormalParameters,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    for param in &params.items {
        if let Some(init) = &param.initializer {
            collect_expression_calls(owner, init, li, calls);
        }
        collect_binding_pattern_calls(owner, &param.pattern, li, calls);
    }
}

fn collect_binding_pattern_calls(
    owner: &str,
    pattern: &BindingPattern,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    match pattern {
        BindingPattern::AssignmentPattern(assign) => {
            collect_expression_calls(owner, &assign.right, li, calls);
            collect_binding_pattern_calls(owner, &assign.left, li, calls);
        }
        BindingPattern::ObjectPattern(object) => {
            collect_object_pattern_calls(owner, object, li, calls);
        }
        BindingPattern::ArrayPattern(array) => {
            collect_array_pattern_calls(owner, array, li, calls);
        }
        BindingPattern::BindingIdentifier(_) => {}
    }
}

fn collect_object_pattern_calls(
    owner: &str,
    object: &ObjectPattern,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    for property in &object.properties {
        if let Some(key) = property.key.as_expression() {
            collect_expression_calls(owner, key, li, calls);
        }
        collect_binding_pattern_calls(owner, &property.value, li, calls);
    }
}

fn collect_array_pattern_calls(
    owner: &str,
    array: &ArrayPattern,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    for pattern in array.elements.iter().flatten() {
        collect_binding_pattern_calls(owner, pattern, li, calls);
    }
}

fn collect_function_calls(
    owner: &str,
    function: &Function,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    collect_params_calls(owner, &function.params, li, calls);
    if let Some(body) = &function.body {
        for stmt in &body.statements {
            collect_owned_statement_calls(owner, stmt, li, calls);
        }
    }
}

fn collect_arrow_calls(
    owner: &str,
    arrow: &ArrowFunctionExpression,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    collect_params_calls(owner, &arrow.params, li, calls);
    for stmt in &arrow.body.statements {
        collect_owned_statement_calls(owner, stmt, li, calls);
    }
}

fn collect_owned_statement_calls(
    owner: &str,
    stmt: &Statement,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    match stmt {
        Statement::BlockStatement(block) => {
            for child in &block.body {
                collect_owned_statement_calls(owner, child, li, calls);
            }
        }
        Statement::ExpressionStatement(expression) => {
            collect_expression_calls(owner, &expression.expression, li, calls);
        }
        Statement::ReturnStatement(ret) => {
            if let Some(argument) = &ret.argument {
                collect_expression_calls(owner, argument, li, calls);
            }
        }
        Statement::VariableDeclaration(variable) => {
            for declarator in &variable.declarations {
                if let Some(init) = &declarator.init {
                    collect_expression_calls(owner, init, li, calls);
                }
            }
        }
        Statement::IfStatement(if_stmt) => {
            collect_expression_calls(owner, &if_stmt.test, li, calls);
            collect_owned_statement_calls(owner, &if_stmt.consequent, li, calls);
            if let Some(alternate) = &if_stmt.alternate {
                collect_owned_statement_calls(owner, alternate, li, calls);
            }
        }
        Statement::ForStatement(for_stmt) => {
            match &for_stmt.init {
                Some(ForStatementInit::VariableDeclaration(variable)) => {
                    for declarator in &variable.declarations {
                        if let Some(init) = &declarator.init {
                            collect_expression_calls(owner, init, li, calls);
                        }
                    }
                }
                Some(init) => {
                    if let Some(expr) = init.as_expression() {
                        collect_expression_calls(owner, expr, li, calls);
                    }
                }
                None => {}
            }
            if let Some(test) = &for_stmt.test {
                collect_expression_calls(owner, test, li, calls);
            }
            if let Some(update) = &for_stmt.update {
                collect_expression_calls(owner, update, li, calls);
            }
            collect_owned_statement_calls(owner, &for_stmt.body, li, calls);
        }
        Statement::WhileStatement(while_stmt) => {
            collect_expression_calls(owner, &while_stmt.test, li, calls);
            collect_owned_statement_calls(owner, &while_stmt.body, li, calls);
        }
        Statement::DoWhileStatement(do_while) => {
            collect_owned_statement_calls(owner, &do_while.body, li, calls);
            collect_expression_calls(owner, &do_while.test, li, calls);
        }
        Statement::ThrowStatement(throw_stmt) => {
            collect_expression_calls(owner, &throw_stmt.argument, li, calls);
        }
        Statement::SwitchStatement(switch_stmt) => {
            collect_expression_calls(owner, &switch_stmt.discriminant, li, calls);
            for case in &switch_stmt.cases {
                if let Some(test) = &case.test {
                    collect_expression_calls(owner, test, li, calls);
                }
                for child in &case.consequent {
                    collect_owned_statement_calls(owner, child, li, calls);
                }
            }
        }
        Statement::TryStatement(try_stmt) => {
            for child in &try_stmt.block.body {
                collect_owned_statement_calls(owner, child, li, calls);
            }
            if let Some(handler) = &try_stmt.handler {
                for child in &handler.body.body {
                    collect_owned_statement_calls(owner, child, li, calls);
                }
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                for child in &finalizer.body {
                    collect_owned_statement_calls(owner, child, li, calls);
                }
            }
        }
        Statement::ForInStatement(for_in) => {
            collect_expression_calls(owner, &for_in.right, li, calls);
            collect_owned_statement_calls(owner, &for_in.body, li, calls);
        }
        Statement::ForOfStatement(for_of) => {
            collect_expression_calls(owner, &for_of.right, li, calls);
            collect_owned_statement_calls(owner, &for_of.body, li, calls);
        }
        Statement::LabeledStatement(labeled) => {
            collect_owned_statement_calls(owner, &labeled.body, li, calls);
        }
        Statement::WithStatement(with_stmt) => {
            collect_expression_calls(owner, &with_stmt.object, li, calls);
            collect_owned_statement_calls(owner, &with_stmt.body, li, calls);
        }
        _ => {}
    }
}

fn push_call(
    owner: &str,
    callee: String,
    span: Span,
    li: &LineIndex,
    kind: &'static str,
    calls: &mut Vec<GraphCall>,
) {
    let range = li.range(span);
    let line = range.start.line + 1;
    let id_prefix = if kind == "constructs" {
        "construct"
    } else {
        "call"
    };
    calls.push(GraphCall {
        id: format!("{id_prefix}:{owner}:{callee}:{line}"),
        caller: owner.to_string(),
        callee,
        line,
        range,
        kind,
    });
}

fn collect_argument_calls(owner: &str, arg: &Argument, li: &LineIndex, calls: &mut Vec<GraphCall>) {
    if let Some(expr) = arg.as_expression() {
        collect_expression_calls(owner, expr, li, calls);
    } else if let Argument::SpreadElement(spread) = arg {
        collect_expression_calls(owner, &spread.argument, li, calls);
    }
}

fn unwrap_expr<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(paren) => unwrap_expr(&paren.expression),
        Expression::TSAsExpression(ts) => unwrap_expr(&ts.expression),
        Expression::TSSatisfiesExpression(ts) => unwrap_expr(&ts.expression),
        Expression::TSTypeAssertion(ts) => unwrap_expr(&ts.expression),
        Expression::TSNonNullExpression(ts) => unwrap_expr(&ts.expression),
        Expression::TSInstantiationExpression(ts) => unwrap_expr(&ts.expression),
        other => other,
    }
}

fn collect_call_like(
    owner: &str,
    callee: &Expression,
    arguments: &[Argument],
    span: Span,
    li: &LineIndex,
    kind: &'static str,
    calls: &mut Vec<GraphCall>,
) {
    if let Some(name) = callee_name(callee) {
        push_call(owner, name, span, li, kind, calls);
    }
    // IIFE / immediately-invoked arrow: attribute body calls to the outer owner.
    match unwrap_expr(callee) {
        Expression::FunctionExpression(function) => {
            collect_function_calls(owner, function, li, calls);
        }
        Expression::ArrowFunctionExpression(arrow) => {
            collect_arrow_calls(owner, arrow, li, calls);
        }
        other => collect_expression_calls(owner, other, li, calls),
    }
    for arg in arguments {
        collect_argument_calls(owner, arg, li, calls);
    }
}

fn collect_expression_calls(
    owner: &str,
    expr: &Expression,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    match expr {
        Expression::CallExpression(call) => {
            collect_call_like(
                owner,
                &call.callee,
                &call.arguments,
                call.span,
                li,
                "calls",
                calls,
            );
        }
        Expression::NewExpression(new_expr) => {
            collect_call_like(
                owner,
                &new_expr.callee,
                &new_expr.arguments,
                new_expr.span,
                li,
                "constructs",
                calls,
            );
        }
        Expression::BinaryExpression(bin) => {
            collect_expression_calls(owner, &bin.left, li, calls);
            collect_expression_calls(owner, &bin.right, li, calls);
        }
        Expression::LogicalExpression(log) => {
            collect_expression_calls(owner, &log.left, li, calls);
            collect_expression_calls(owner, &log.right, li, calls);
        }
        Expression::ConditionalExpression(cond) => {
            collect_expression_calls(owner, &cond.test, li, calls);
            collect_expression_calls(owner, &cond.consequent, li, calls);
            collect_expression_calls(owner, &cond.alternate, li, calls);
        }
        Expression::AssignmentExpression(assign) => {
            if let Some(member) = assign.left.as_member_expression() {
                collect_expression_calls(owner, member.object(), li, calls);
                if let oxc_ast::ast::MemberExpression::ComputedMemberExpression(computed) = member {
                    collect_expression_calls(owner, &computed.expression, li, calls);
                }
            } else if let Some(left) = assign.left.get_expression() {
                collect_expression_calls(owner, left, li, calls);
            }
            collect_expression_calls(owner, &assign.right, li, calls);
        }
        Expression::SequenceExpression(seq) => {
            for child in &seq.expressions {
                collect_expression_calls(owner, child, li, calls);
            }
        }
        Expression::AwaitExpression(await_expr) => {
            collect_expression_calls(owner, &await_expr.argument, li, calls);
        }
        Expression::ParenthesizedExpression(paren) => {
            collect_expression_calls(owner, &paren.expression, li, calls);
        }
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => {
                collect_call_like(
                    owner,
                    &call.callee,
                    &call.arguments,
                    call.span,
                    li,
                    "calls",
                    calls,
                );
            }
            ChainElement::TSNonNullExpression(non_null) => {
                collect_expression_calls(owner, &non_null.expression, li, calls);
            }
            other => {
                if let Some(member) = other.as_member_expression() {
                    collect_expression_calls(owner, member.object(), li, calls);
                    if let oxc_ast::ast::MemberExpression::ComputedMemberExpression(computed) =
                        member
                    {
                        collect_expression_calls(owner, &computed.expression, li, calls);
                    }
                }
            }
        },
        Expression::UnaryExpression(unary) => {
            collect_expression_calls(owner, &unary.argument, li, calls);
        }
        Expression::YieldExpression(yield_expr) => {
            if let Some(argument) = &yield_expr.argument {
                collect_expression_calls(owner, argument, li, calls);
            }
        }
        Expression::ArrayExpression(array) => {
            for element in &array.elements {
                if let Some(child) = element.as_expression() {
                    collect_expression_calls(owner, child, li, calls);
                } else if let ArrayExpressionElement::SpreadElement(spread) = element {
                    collect_expression_calls(owner, &spread.argument, li, calls);
                }
            }
        }
        Expression::ObjectExpression(object) => {
            for property in &object.properties {
                match property {
                    ObjectPropertyKind::ObjectProperty(prop) => {
                        if let Some(key) = prop.key.as_expression() {
                            collect_expression_calls(owner, key, li, calls);
                        }
                        let method_owner = property_key_name(&prop.key)
                            .map(|(name, _)| name)
                            .unwrap_or_else(|| owner.to_string());
                        match &prop.value {
                            Expression::FunctionExpression(function) => {
                                collect_function_calls(&method_owner, function, li, calls);
                            }
                            Expression::ArrowFunctionExpression(arrow) => {
                                collect_arrow_calls(&method_owner, arrow, li, calls);
                            }
                            other => collect_expression_calls(owner, other, li, calls),
                        }
                    }
                    ObjectPropertyKind::SpreadProperty(spread) => {
                        collect_expression_calls(owner, &spread.argument, li, calls);
                    }
                }
            }
        }
        Expression::TemplateLiteral(template) => {
            for child in &template.expressions {
                collect_expression_calls(owner, child, li, calls);
            }
        }
        Expression::TaggedTemplateExpression(tagged) => {
            if let Some(name) = callee_name(&tagged.tag) {
                push_call(owner, name, tagged.span, li, "calls", calls);
            }
            collect_expression_calls(owner, &tagged.tag, li, calls);
            for child in &tagged.quasi.expressions {
                collect_expression_calls(owner, child, li, calls);
            }
        }
        Expression::ImportExpression(import) => {
            collect_expression_calls(owner, &import.source, li, calls);
            if let Some(options) = &import.options {
                collect_expression_calls(owner, options, li, calls);
            }
        }
        Expression::PrivateInExpression(private_in) => {
            collect_expression_calls(owner, &private_in.right, li, calls);
        }
        Expression::TSAsExpression(ts) => {
            collect_expression_calls(owner, &ts.expression, li, calls);
        }
        Expression::TSSatisfiesExpression(ts) => {
            collect_expression_calls(owner, &ts.expression, li, calls);
        }
        Expression::TSTypeAssertion(ts) => {
            collect_expression_calls(owner, &ts.expression, li, calls);
        }
        Expression::TSNonNullExpression(ts) => {
            collect_expression_calls(owner, &ts.expression, li, calls);
        }
        Expression::TSInstantiationExpression(ts) => {
            collect_expression_calls(owner, &ts.expression, li, calls);
        }
        Expression::StaticMemberExpression(member) => {
            collect_expression_calls(owner, &member.object, li, calls);
        }
        Expression::ComputedMemberExpression(member) => {
            collect_expression_calls(owner, &member.object, li, calls);
            collect_expression_calls(owner, &member.expression, li, calls);
        }
        Expression::PrivateFieldExpression(member) => {
            collect_expression_calls(owner, &member.object, li, calls);
        }
        Expression::JSXElement(element) => {
            collect_jsx_element_calls(owner, element, li, calls);
        }
        Expression::JSXFragment(fragment) => {
            collect_jsx_fragment_calls(owner, fragment, li, calls);
        }
        // Nested function/class bodies have their own owners via declaration/variable/object walkers.
        // IIFE bodies are handled in collect_call_like.
        Expression::ArrowFunctionExpression(_)
        | Expression::FunctionExpression(_)
        | Expression::ClassExpression(_) => {}
        _ => {}
    }
}

fn collect_jsx_element_calls(
    owner: &str,
    element: &JSXElement,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    for attribute in &element.opening_element.attributes {
        match attribute {
            JSXAttributeItem::Attribute(attr) => {
                if let Some(value) = &attr.value {
                    collect_jsx_attribute_value_calls(owner, value, li, calls);
                }
            }
            JSXAttributeItem::SpreadAttribute(spread) => {
                collect_expression_calls(owner, &spread.argument, li, calls);
            }
        }
    }
    for child in &element.children {
        collect_jsx_child_calls(owner, child, li, calls);
    }
}

fn collect_jsx_fragment_calls(
    owner: &str,
    fragment: &JSXFragment,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    for child in &fragment.children {
        collect_jsx_child_calls(owner, child, li, calls);
    }
}

fn collect_jsx_attribute_value_calls(
    owner: &str,
    value: &JSXAttributeValue,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    match value {
        JSXAttributeValue::ExpressionContainer(container) => {
            if let Some(expr) = container.expression.as_expression() {
                collect_expression_calls(owner, expr, li, calls);
            }
        }
        JSXAttributeValue::Element(element) => {
            collect_jsx_element_calls(owner, element, li, calls);
        }
        JSXAttributeValue::Fragment(fragment) => {
            collect_jsx_fragment_calls(owner, fragment, li, calls);
        }
        JSXAttributeValue::StringLiteral(_) => {}
    }
}

fn collect_jsx_child_calls(
    owner: &str,
    child: &JSXChild,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    match child {
        JSXChild::Element(element) => collect_jsx_element_calls(owner, element, li, calls),
        JSXChild::Fragment(fragment) => collect_jsx_fragment_calls(owner, fragment, li, calls),
        JSXChild::ExpressionContainer(container) => {
            if let Some(expr) = container.expression.as_expression() {
                collect_expression_calls(owner, expr, li, calls);
            }
        }
        JSXChild::Spread(spread) => {
            collect_expression_calls(owner, &spread.expression, li, calls);
        }
        JSXChild::Text(_) => {}
    }
}

fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(identifier) => Some(identifier.name.as_str().to_string()),
        Expression::StaticMemberExpression(member) => {
            let property = member.property.name.as_str();
            Some(
                expression_name(&member.object)
                    .map(|object| format!("{object}.{property}"))
                    .unwrap_or_else(|| property.to_string()),
            )
        }
        Expression::ComputedMemberExpression(member) => {
            let property = match &member.expression {
                Expression::StringLiteral(lit) => Some(lit.value.as_str().to_string()),
                Expression::TemplateLiteral(lit) if lit.expressions.is_empty() => lit
                    .quasis
                    .first()
                    .and_then(|q| q.value.cooked.as_ref())
                    .map(|cooked| cooked.as_str().to_string()),
                _ => None,
            }?;
            Some(
                expression_name(&member.object)
                    .map(|object| format!("{object}.{property}"))
                    .unwrap_or(property),
            )
        }
        Expression::ParenthesizedExpression(paren) => callee_name(&paren.expression),
        Expression::TSAsExpression(ts) => callee_name(&ts.expression),
        Expression::TSSatisfiesExpression(ts) => callee_name(&ts.expression),
        Expression::TSTypeAssertion(ts) => callee_name(&ts.expression),
        Expression::TSNonNullExpression(ts) => callee_name(&ts.expression),
        Expression::TSInstantiationExpression(ts) => callee_name(&ts.expression),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(_) => None,
            ChainElement::TSNonNullExpression(non_null) => callee_name(&non_null.expression),
            other => other
                .as_member_expression()
                .and_then(|member| match member {
                    oxc_ast::ast::MemberExpression::StaticMemberExpression(static_member) => {
                        let property = static_member.property.name.as_str();
                        Some(
                            expression_name(&static_member.object)
                                .map(|object| format!("{object}.{property}"))
                                .unwrap_or_else(|| property.to_string()),
                        )
                    }
                    oxc_ast::ast::MemberExpression::ComputedMemberExpression(computed) => {
                        let property = match &computed.expression {
                            Expression::StringLiteral(lit) => Some(lit.value.as_str().to_string()),
                            _ => None,
                        }?;
                        Some(
                            expression_name(&computed.object)
                                .map(|object| format!("{object}.{property}"))
                                .unwrap_or(property),
                        )
                    }
                    oxc_ast::ast::MemberExpression::PrivateFieldExpression(private) => {
                        let property = format!("#{}", private.field.name.as_str());
                        Some(
                            expression_name(&private.object)
                                .map(|object| format!("{object}.{property}"))
                                .unwrap_or(property),
                        )
                    }
                }),
        },
        _ => None,
    }
}

fn expression_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(identifier) => Some(identifier.name.as_str().to_string()),
        Expression::ThisExpression(_) => Some("this".to_string()),
        _ => None,
    }
}
