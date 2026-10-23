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
//! (see [`crate::deep_stack::job_cancelled`]).

use super::js_oxc_receiver::ReceiverScopes;
use super::js_oxc_shared::{SpanPositions, property_key_name};
use crate::graph::{GraphFactCall, GraphPosition};
use oxc_ast::ast::*;
use oxc_ast_visit::{VisitJs, walk_js};
use oxc_semantic::ScopeFlags;
use oxc_span::{GetSpan, Span};

mod names;
mod visitor;

use names::push_call;
pub(super) use names::{dotted_expression_name, ts_type_name, unwrap_expr};
use visitor::first_pattern_identifier;

pub(super) fn collect_program_calls(
    program: &Program,
    li: &SpanPositions,
    calls: &mut Vec<GraphFactCall>,
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
    pub(super) name_start: GraphPosition,
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
    li: &'c SpanPositions<'l>,
    calls: &'c mut Vec<GraphFactCall>,
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
