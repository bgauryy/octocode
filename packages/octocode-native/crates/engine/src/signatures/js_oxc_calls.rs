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
//! Beyond call/construct sites the walk records three syntax-only facts with
//! the same owner rules: JSX component usage (`<Foo/>`, `<Foo.Bar>`, kind
//! `renders`; intrinsic lowercase tags and fragments are skipped), bare
//! decorators (`@Foo`, `@a.b`, kind `decorates`; `@Foo()` stays a `calls`
//! fact), and the `extends`/`implements` clauses of named classes and
//! interfaces (collected as [`GraphHeritage`]).
//!
//! The walk stops descending once the running deep-stack job is cancelled
//! (see [`super::deep_stack::job_cancelled`]).

use super::deep_stack::job_cancelled;
use super::js_oxc_receiver::ReceiverScopes;
use super::js_oxc_shared::{GraphCall, LineIndex, Position, property_key_name};
use oxc_ast::ast::*;
use oxc_ast_visit::{VisitJs, walk_js};
use oxc_semantic::ScopeFlags;
use oxc_span::{GetSpan, Span};

pub(super) fn collect_program_calls(
    program: &Program,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
    heritage: &mut Vec<GraphHeritage>,
) {
    let mut collector = CallCollector {
        owner: None,
        li,
        calls,
        heritage,
        receivers: ReceiverScopes::default(),
    };
    collector.visit_program(program);
}

/// One base type named by a class `extends`/`implements` clause or an
/// interface `extends` clause, keyed by the declaring name token so the
/// caller can attach it to that declaration's id.
pub(super) struct GraphHeritage {
    /// Start of the declaring class/interface name token: equal to the
    /// declaration's `selectionRange.start`.
    pub(super) name_start: Position,
    /// `"class"` or `"interface"`: the declaration kind it attaches to.
    pub(super) declaration_kind: &'static str,
    /// `"extends"` or `"implements"`.
    pub(super) relation: &'static str,
    /// The base type as written, without type arguments (`Base`, `ns.Base`).
    pub(super) to: String,
    /// 1-based line of the base type reference.
    pub(super) line: u32,
}

/// Placeholder owner for calls outside any named declaration.
const MODULE_OWNER: &str = "module";

struct CallCollector<'c, 'l> {
    /// `None` only in declaration context: the program top level, a TS
    /// namespace body, or directly under an `export` declaration.
    owner: Option<String>,
    li: &'c LineIndex<'l>,
    calls: &'c mut Vec<GraphCall>,
    heritage: &'c mut Vec<GraphHeritage>,
    /// Lexical bindings and `this` fields for receiver types.
    receivers: ReceiverScopes,
}

impl CallCollector<'_, '_> {
    fn with_owner(&mut self, owner: Option<String>, walk: impl FnOnce(&mut Self)) {
        let previous = std::mem::replace(&mut self.owner, owner);
        walk(self);
        self.owner = previous;
    }

    fn push(&mut self, callee: String, span: Span, kind: &'static str) {
        self.push_with_receiver(callee, span, kind, None);
    }

    fn push_with_receiver(
        &mut self,
        callee: String,
        span: Span,
        kind: &'static str,
        receiver_type: Option<String>,
    ) {
        let owner = self.owner.as_deref().unwrap_or(MODULE_OWNER);
        push_call(
            owner,
            callee,
            span,
            self.li,
            kind,
            receiver_type,
            self.calls,
        );
    }

    fn scoped(&mut self, walk: impl FnOnce(&mut Self)) {
        self.receivers.push_scope();
        walk(self);
        self.receivers.pop_scope();
    }

    fn push_heritage(
        &mut self,
        name_span: Span,
        declaration_kind: &'static str,
        relation: &'static str,
        to: String,
        span: Span,
    ) {
        self.heritage.push(GraphHeritage {
            name_start: self.li.position(name_span.start),
            declaration_kind,
            relation,
            to,
            line: self.li.position(span.start).line + 1,
        });
    }

    /// `extends`/`implements` of a named class (declaration or expression).
    /// A computed base (`extends mixin(A)`) is not a type name: its call is
    /// already a `calls` fact.
    fn collect_class_heritage(&mut self, class: &Class) {
        let Some(id) = &class.id else {
            return;
        };
        if let Some(heritage) = &class.heritage
            && let Some(name) = dotted_expression_name(&heritage.expression)
        {
            self.push_heritage(
                id.span,
                "class",
                "extends",
                name,
                heritage.expression.span(),
            );
        }
        for implemented in &class.implements {
            if let Some(name) = ts_type_name(&implemented.expression) {
                self.push_heritage(id.span, "class", "implements", name, implemented.span);
            }
        }
    }

    /// Owner for a member (`key: value`, class field, method): its own key
    /// name when the value is a function/arrow, otherwise the enclosing owner.
    fn member_owner(&self, key: &PropertyKey) -> Option<String> {
        property_key_name(key)
            .map(|(name, _)| name)
            .or_else(|| self.owner.clone())
    }

    /// Top-level function declarations own their calls under their name.
    fn visit_function_owned<'a>(&mut self, it: &Function<'a>, flags: ScopeFlags) {
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

    /// A top-level `const x = ...` owns every call in its declarator as `x`.
    fn visit_variable_declarator_owned<'a>(&mut self, it: &VariableDeclarator<'a>) {
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
    receiver_type: Option<String>,
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
        receiver_type,
    });
}

pub(super) fn unwrap_expr<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
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

/// A JSX tag that names a component: `Foo`, `Foo.Bar`, `this.Foo`. Intrinsic
/// tags (`div`, `my-element`), namespaced XML names (`svg:rect`) and `<this>`
/// render no user component.
fn jsx_component_name(name: &JSXElementName) -> Option<String> {
    match name {
        JSXElementName::IdentifierReference(reference) => {
            let name = reference.name.as_str();
            (!name.starts_with(|c: char| c.is_ascii_lowercase())).then(|| name.to_string())
        }
        JSXElementName::MemberExpression(member) => Some(jsx_member_name(member)),
        JSXElementName::Identifier(_)
        | JSXElementName::NamespacedName(_)
        | JSXElementName::ThisExpression(_) => None,
    }
}

fn jsx_member_name(member: &JSXMemberExpression) -> String {
    let object = match &member.object {
        JSXMemberExpressionObject::IdentifierReference(reference) => {
            reference.name.as_str().to_string()
        }
        JSXMemberExpressionObject::MemberExpression(inner) => jsx_member_name(inner),
        JSXMemberExpressionObject::ThisExpression(_) => "this".to_string(),
    };
    format!("{object}.{}", member.property.name.as_str())
}

/// A base class written as a (possibly dotted) name: `Base`, `ns.sub.Base`.
pub(super) fn dotted_expression_name(expr: &Expression) -> Option<String> {
    match unwrap_expr(expr) {
        Expression::Identifier(identifier) => Some(identifier.name.as_str().to_string()),
        Expression::StaticMemberExpression(member) => Some(format!(
            "{}.{}",
            dotted_expression_name(&member.object)?,
            member.property.name.as_str()
        )),
        _ => None,
    }
}

/// A TS type reference name as written: `C`, `ns.C` (type arguments are a
/// separate node and never included).
pub(super) fn ts_type_name(name: &TSTypeName) -> Option<String> {
    match name {
        TSTypeName::IdentifierReference(reference) => Some(reference.name.as_str().to_string()),
        TSTypeName::QualifiedName(qualified) => Some(format!(
            "{}.{}",
            ts_type_name(&qualified.left)?,
            qualified.right.name.as_str()
        )),
        TSTypeName::ThisExpression(_) => None,
    }
}

fn expression_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(identifier) => Some(identifier.name.as_str().to_string()),
        Expression::ThisExpression(_) => Some("this".to_string()),
        _ => None,
    }
}
