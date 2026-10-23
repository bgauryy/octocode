//! The [`VisitJs`] walk of [`super::CallCollector`]: which owner each call,
//! render and decorator site is attributed to.

use crate::deep_stack::job_cancelled;
use oxc_ast::ast::*;
use oxc_ast_visit::{VisitJs, walk_js};
use oxc_semantic::ScopeFlags;

use super::names::{callee_name, jsx_component_name, ts_type_name, unwrap_expr};
use super::{CallCollector, MODULE_OWNER};

impl<'a> VisitJs<'a> for CallCollector<'_, '_> {
    fn visit_statement(&mut self, it: &Statement<'a>) {
        if job_cancelled() {
            return;
        }
        if self.owner.is_none() && is_plain_statement(it) {
            self.with_owner(Some(MODULE_OWNER.to_string()), |this| {
                walk_js::walk_statement(this, it);
            });
        } else {
            walk_js::walk_statement(self, it);
        }
    }

    fn visit_expression(&mut self, it: &Expression<'a>) {
        if job_cancelled() {
            return;
        }
        walk_js::walk_expression(self, it);
    }

    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        let owner = match &it.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => function.id.as_ref(),
            ExportDefaultDeclarationKind::ClassDeclaration(class) => class.id.as_ref(),
            _ => None,
        }
        .map_or_else(|| "default".to_string(), |id| id.name.as_str().to_string());
        self.with_owner(Some(owner), |this| {
            walk_js::walk_export_default_declaration(this, it);
        });
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        let rebound_this = self.receivers.enter_function();
        self.visit_function_owned(it, flags);
        self.receivers.exit_function(rebound_this);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.scoped(|this| walk_js::walk_arrow_function_expression(this, it));
    }

    fn visit_block_statement(&mut self, it: &BlockStatement<'a>) {
        self.scoped(|this| walk_js::walk_block_statement(this, it));
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        self.scoped(|this| walk_js::walk_for_statement(this, it));
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.scoped(|this| walk_js::walk_for_in_statement(this, it));
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.scoped(|this| walk_js::walk_for_of_statement(this, it));
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        self.scoped(|this| {
            if let Some(param) = &it.param {
                this.receivers.bind_catch(param);
            }
            walk_js::walk_catch_clause(this, it);
        });
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        walk_js::walk_assignment_expression(self, it);
        self.receivers.assign(it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        self.visit_variable_declarator_owned(it);
        self.receivers.bind_declarator(it);
    }

    // Default initializer before the pattern: the order the call list has
    // always used for parameters.
    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        self.visit_decorators(&it.decorators);
        if let Some(initializer) = &it.initializer {
            self.visit_expression(initializer);
        }
        self.visit_binding_pattern(&it.pattern);
        self.receivers.bind_parameter(it);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        // Class-level code (decorators, `extends`, static blocks, non-function
        // field initializers) belongs to the enclosing owner, or to the class
        // itself at the top level. Methods still take their own names.
        let owner = self
            .owner
            .clone()
            .or_else(|| it.id.as_ref().map(|id| id.name.as_str().to_string()))
            .unwrap_or_else(|| MODULE_OWNER.to_string());
        self.collect_class_heritage(it);
        self.receivers.enter_class(it);
        self.with_owner(Some(owner), |this| walk_js::walk_class(this, it));
        self.receivers.exit_class();
    }

    // `VisitJs` does not walk TS interfaces; only their `extends` clauses
    // matter here (interfaces hold no calls).
    fn visit_declaration(&mut self, it: &Declaration<'a>) {
        if let Declaration::TSInterfaceDeclaration(interface) = it {
            for heritage in &interface.extends {
                if let Some(name) = ts_type_name(&heritage.type_name) {
                    self.push_heritage(
                        interface.id.span,
                        "interface",
                        "extends",
                        name,
                        heritage.span,
                    );
                }
            }
        }
        walk_js::walk_declaration(self, it);
    }

    /// A bare decorator (`@Foo`, `@a.b`) applies `Foo` without a call
    /// expression; `@Foo()` is a call expression and stays a `calls` fact.
    fn visit_decorator(&mut self, it: &Decorator<'a>) {
        let expression = unwrap_expr(&it.expression);
        if !matches!(expression, Expression::CallExpression(_))
            && let Some(name) = callee_name(expression)
        {
            self.push(name, it.span, "decorates");
        }
        walk_js::walk_decorator(self, it);
    }

    fn visit_jsx_opening_element(&mut self, it: &JSXOpeningElement<'a>) {
        if let Some(name) = jsx_component_name(&it.name) {
            self.push(name, it.span, "renders");
        }
        walk_js::walk_jsx_opening_element(self, it);
    }

    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        self.visit_decorators(&it.decorators);
        self.visit_property_key(&it.key);
        let owner = self.member_owner(&it.key);
        if !it.r#static {
            self.receivers.mark_method();
        }
        self.with_owner(owner, |this| {
            this.visit_function(&it.value, ScopeFlags::Function);
        });
    }

    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        self.visit_decorators(&it.decorators);
        self.visit_property_key(&it.key);
        if let Some(value) = &it.value {
            self.visit_member_value(&it.key, value);
        }
    }

    fn visit_accessor_property(&mut self, it: &AccessorProperty<'a>) {
        self.visit_decorators(&it.decorators);
        self.visit_property_key(&it.key);
        if let Some(value) = &it.value {
            self.visit_member_value(&it.key, value);
        }
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        self.visit_property_key(&it.key);
        self.visit_member_value(&it.key, &it.value);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some(name) = callee_name(&it.callee) {
            let receiver_type = self.receivers.receiver_type(&it.callee);
            self.push_with_receiver(name, it.span, "calls", receiver_type);
        }
        walk_js::walk_call_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        if let Some(name) = callee_name(&it.callee) {
            self.push(name, it.span, "constructs");
        }
        walk_js::walk_new_expression(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        if let Some(name) = callee_name(&it.tag) {
            self.push(name, it.span, "calls");
        }
        walk_js::walk_tagged_template_expression(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        // A string-literal specifier resolves to a file the same way a
        // static `import`/`export ... from` does, so the dead-code graph
        // can treat its target as reachable — without this, a file
        // reached only via dynamic import is invisible to the graph and
        // reads as a false-positive "dead" file. A computed specifier
        // (`import(expr)`) can't be resolved statically and is
        // deliberately left uncaptured rather than guessed at.
        if let Expression::StringLiteral(literal) = unwrap_expr(&it.source) {
            self.push(
                literal.value.as_str().to_string(),
                it.span,
                "dynamic-import",
            );
        }
        walk_js::walk_import_expression(self, it);
    }
}

/// Statements that are not declarations. At the top level they have no
/// declaration name, so they are owned by the `module` placeholder (e.g. a
/// top-level `describe('...', () => {...})` expression statement).
fn is_plain_statement(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::BlockStatement(_)
            | Statement::BreakStatement(_)
            | Statement::ContinueStatement(_)
            | Statement::DebuggerStatement(_)
            | Statement::DoWhileStatement(_)
            | Statement::EmptyStatement(_)
            | Statement::ExpressionStatement(_)
            | Statement::ForInStatement(_)
            | Statement::ForOfStatement(_)
            | Statement::ForStatement(_)
            | Statement::IfStatement(_)
            | Statement::LabeledStatement(_)
            | Statement::ReturnStatement(_)
            | Statement::SwitchStatement(_)
            | Statement::ThrowStatement(_)
            | Statement::TryStatement(_)
            | Statement::WhileStatement(_)
            | Statement::WithStatement(_)
    )
}

/// First bound identifier name found inside a (possibly nested) binding
/// pattern, e.g. `a` in `{ a, b }` or `x` in `[{ x }]`. Used as a call-graph
/// owner placeholder when a destructured declarator has no single name.
pub(super) fn first_pattern_identifier(pattern: &BindingPattern) -> Option<String> {
    match pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().to_string()),
        BindingPattern::ObjectPattern(object) => object
            .properties
            .iter()
            .find_map(|prop| first_pattern_identifier(&prop.value))
            .or_else(|| {
                object
                    .rest
                    .as_ref()
                    .and_then(|rest| first_pattern_identifier(&rest.argument))
            }),
        BindingPattern::ArrayPattern(array) => array
            .elements
            .iter()
            .flatten()
            .find_map(first_pattern_identifier)
            .or_else(|| {
                array
                    .rest
                    .as_ref()
                    .and_then(|rest| first_pattern_identifier(&rest.argument))
            }),
        BindingPattern::AssignmentPattern(assignment) => first_pattern_identifier(&assignment.left),
    }
}
