//! Call-graph extraction for the JS/TS graph facts (`extract_graph_facts`).
//!
//! Split out of `js_oxc.rs`: walks the oxc AST attributing every call/construct
//! site to its enclosing owner. Only `collect_program_calls` is public to the
//! parent module.
//!
//! The walk is an [`oxc_ast_visit::VisitJs`] implementation, so every nested
//! statement/expression shape is reached by the generated `walk_js::*`
//! functions (switch cases, labeled statements, class static blocks and field
//! initializers, decorators, catch parameters, nested declarations, TS enum
//! initializers and namespace bodies, ...). The overrides below only decide
//! *which owner* a call is attributed to; they never decide whether a subtree
//! is walked. The previous hand-rolled `match` walker silently dropped any
//! shape it had no arm for.
//!
//! Owner rules (unchanged from the hand-rolled walker for every shape it
//! supported):
//! - top-level `function f` / `class` methods / object & class members whose
//!   value is a function or arrow own their calls under their own name;
//! - a top-level `const x = ...` owns every call in its declarator as `x`
//!   (destructured: first bound identifier, else `destructured`);
//! - `export default` function/class use their name, else `default`;
//! - any other top-level statement is owned by the `module` placeholder;
//! - everything nested (callbacks, IIFEs, nested declarations, static blocks,
//!   non-function member values) inherits the enclosing owner.
//!
//! The walk stops descending once the running deep-stack job is cancelled
//! (see [`super::deep_stack::job_cancelled`]).

use super::deep_stack::job_cancelled;
use super::js_oxc_shared::{GraphCall, LineIndex, property_key_name};
use oxc_ast::ast::*;
use oxc_ast_visit::{VisitJs, walk_js};
use oxc_semantic::ScopeFlags;
use oxc_span::Span;

pub(super) fn collect_program_calls(program: &Program, li: &LineIndex, calls: &mut Vec<GraphCall>) {
    let mut collector = CallCollector {
        owner: None,
        li,
        calls,
    };
    collector.visit_program(program);
}

/// Placeholder owner for calls outside any named declaration.
const MODULE_OWNER: &str = "module";

struct CallCollector<'c, 'l> {
    /// `None` only in declaration context: the program top level, a TS
    /// namespace body, or directly under an `export` declaration.
    owner: Option<String>,
    li: &'c LineIndex<'l>,
    calls: &'c mut Vec<GraphCall>,
}

impl CallCollector<'_, '_> {
    fn with_owner(&mut self, owner: Option<String>, walk: impl FnOnce(&mut Self)) {
        let previous = std::mem::replace(&mut self.owner, owner);
        walk(self);
        self.owner = previous;
    }

    fn push(&mut self, callee: String, span: Span, kind: &'static str) {
        let owner = self.owner.as_deref().unwrap_or(MODULE_OWNER);
        push_call(owner, callee, span, self.li, kind, self.calls);
    }

    /// Owner for a member (`key: value`, class field, method): its own key
    /// name when the value is a function/arrow, otherwise the enclosing owner.
    fn member_owner(&self, key: &PropertyKey) -> Option<String> {
        property_key_name(key)
            .map(|(name, _)| name)
            .or_else(|| self.owner.clone())
    }

    /// Visit a member value: functions/arrows are owned by the member key,
    /// any other value by the enclosing owner.
    fn visit_member_value<'a>(&mut self, key: &PropertyKey<'a>, value: &Expression<'a>) {
        if matches!(
            value,
            Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)
        ) {
            let owner = self.member_owner(key);
            self.with_owner(owner, |this| this.visit_expression(value));
        } else {
            self.visit_expression(value);
        }
    }
}

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
        if self.owner.is_none() && it.is_declaration() {
            let owner = it
                .id
                .as_ref()
                .map_or_else(|| "default".to_string(), |id| id.name.as_str().to_string());
            self.with_owner(Some(owner), |this| walk_js::walk_function(this, it, flags));
        } else {
            walk_js::walk_function(self, it, flags);
        }
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if self.owner.is_none() {
            let owner = match &it.id {
                BindingPattern::BindingIdentifier(id) => id.name.as_str().to_string(),
                other => {
                    first_pattern_identifier(other).unwrap_or_else(|| "destructured".to_string())
                }
            };
            self.with_owner(Some(owner), |this| {
                walk_js::walk_variable_declarator(this, it);
            });
        } else {
            walk_js::walk_variable_declarator(self, it);
        }
    }

    // Default initializer before the pattern: the order the call list has
    // always used for parameters.
    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        self.visit_decorators(&it.decorators);
        if let Some(initializer) = &it.initializer {
            self.visit_expression(initializer);
        }
        self.visit_binding_pattern(&it.pattern);
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
        self.with_owner(Some(owner), |this| walk_js::walk_class(this, it));
    }

    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        self.visit_decorators(&it.decorators);
        self.visit_property_key(&it.key);
        let owner = self.member_owner(&it.key);
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
            self.push(name, it.span, "calls");
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
fn first_pattern_identifier(pattern: &BindingPattern) -> Option<String> {
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
        caller_id: None,
        callee,
        line,
        range,
        kind,
    });
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
