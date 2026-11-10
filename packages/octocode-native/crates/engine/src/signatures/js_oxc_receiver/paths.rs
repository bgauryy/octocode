//! Receivers typed through a declared type the linker follows
//! ([`GraphDeclaredType`] with steps): `x.f.m()` where `x: T`, the parameters
//! of a function its declarator annotates (`const f: A = ({ s }) => s.m()`,
//! as tsserver types them contextually), and properties destructured from a
//! typed value (`const { s } = x`).

use oxc_ast::ast::*;

use super::declared::{declared_type, has_steps, stepped};
use super::{Binding, ReceiverScopes};
use crate::graph::{GraphDeclaredType, GraphTypeStep};
use crate::signatures::js_oxc_calls::{dotted_expression_name, unwrap_expr};
use crate::signatures::js_oxc_shared::property_key_name;

/// The span start of `expression` when it is a function value.
fn function_start(expression: &Expression) -> Option<u32> {
    match unwrap_expr(expression) {
        Expression::ArrowFunctionExpression(arrow) => Some(arrow.span.start),
        Expression::FunctionExpression(function) => Some(function.span.start),
        _ => None,
    }
}

/// A type name as a declared type without steps.
fn named(name: String) -> GraphDeclaredType {
    GraphDeclaredType::Named {
        name,
        steps: Vec::new(),
    }
}

impl ReceiverScopes {
    /// `const f: A = <function>`: the function walked next takes its
    /// parameters' types from `A`.
    pub(in crate::signatures) fn annotate_init(&mut self, it: &VariableDeclarator) {
        let (Some(annotation), Some(init)) = (&it.type_annotation, &it.init) else {
            return;
        };
        let Some(start) = function_start(init) else {
            return;
        };
        if let Some(ty) = declared_type(&annotation.type_annotation, &[])
            .filter(|ty| matches!(ty, GraphDeclaredType::Named { .. }))
        {
            self.pending_context.insert(start, ty);
        }
    }

    /// `f(<function>, { k: <function> })` with `f` a name no inner scope
    /// binds: each function takes its parameters' types from the declared
    /// type of `f`'s parameter at its position, then the property `k`.
    pub(in crate::signatures) fn annotate_arguments(&mut self, call: &CallExpression) {
        let Expression::Identifier(callee) = unwrap_expr(&call.callee) else {
            return;
        };
        let name = callee.name.as_str();
        if self
            .scopes
            .iter()
            .skip(1)
            .any(|scope| scope.contains_key(name))
        {
            return;
        }
        let at = |index: usize, key: Option<String>| {
            let steps =
                std::iter::once(GraphTypeStep::Parameter(crate::num::saturating_u32(index)))
                    .chain(key.map(GraphTypeStep::Member))
                    .collect();
            GraphDeclaredType::Named {
                name: name.to_owned(),
                steps,
            }
        };
        for (index, argument) in call.arguments.iter().enumerate() {
            let Some(argument) = argument.as_expression() else {
                continue;
            };
            match unwrap_expr(argument) {
                Expression::ObjectExpression(object) => {
                    for property in &object.properties {
                        let ObjectPropertyKind::ObjectProperty(property) = property else {
                            continue;
                        };
                        let key = (!property.computed)
                            .then(|| property_key_name(&property.key))
                            .flatten();
                        if let (Some((key, _)), Some(start)) =
                            (key, function_start(&property.value))
                        {
                            self.pending_context.insert(start, at(index, Some(key)));
                        }
                    }
                }
                other => {
                    if let Some(start) = function_start(other) {
                        self.pending_context.insert(start, at(index, None));
                    }
                }
            }
        }
    }

    /// Enter the parameters of the function starting at `start`.
    pub(in crate::signatures) fn enter_parameters(&mut self, start: u32) {
        let context = self.pending_context.remove(&start).map(|ty| (ty, 0));
        self.contexts.push(context);
    }

    pub(in crate::signatures) fn exit_parameters(&mut self) {
        self.contexts.pop();
    }

    /// The contextual type of the next parameter of the current function.
    pub(super) fn next_context_parameter(&mut self) -> Option<GraphDeclaredType> {
        let (ty, next) = self.contexts.last_mut()?.as_mut()?;
        let index = *next;
        *next += 1;
        stepped(ty.clone(), GraphTypeStep::Parameter(index))
    }

    /// The declared type a binding holds: its type name, or its path.
    pub(super) fn lookup_declared(&self, name: &str) -> Option<GraphDeclaredType> {
        let binding = self.scopes.iter().rev().find_map(|scope| scope.get(name))?;
        binding
            .ty
            .clone()
            .map(named)
            .or_else(|| binding.path.clone())
    }

    /// `{ a, b: c = d }` taken from a value of type `base`: each name a
    /// property binds is `base` then that property; anything nested is
    /// unknown.
    pub(super) fn bind_declared_object(
        &mut self,
        object: &ObjectPattern,
        base: &GraphDeclaredType,
    ) {
        for property in &object.properties {
            let path = (!property.computed)
                .then(|| property_key_name(&property.key))
                .flatten()
                .and_then(|(key, _)| stepped(base.clone(), GraphTypeStep::Member(key)));
            let value = match &property.value {
                BindingPattern::AssignmentPattern(assignment) => &assignment.left,
                other => other,
            };
            match (value, path) {
                (BindingPattern::BindingIdentifier(_), Some(path)) => self.bind_path(value, path),
                (other, _) => self.bind_pattern(other, None, true),
            }
        }
        if let Some(rest) = &object.rest {
            self.bind_pattern(&rest.argument, None, true);
        }
    }

    /// Bind `pattern`, a name, to the declared type `path`.
    pub(super) fn bind_path(&mut self, pattern: &BindingPattern, path: GraphDeclaredType) {
        let BindingPattern::BindingIdentifier(identifier) = pattern else {
            self.bind_pattern(pattern, None, true);
            return;
        };
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        scope.insert(
            identifier.name.as_str().to_owned(),
            Binding {
                ty: None,
                path: Some(path),
                declared: true,
                alias: None,
            },
        );
    }

    /// The receiver of a call whose callee is `callee`: the type name the
    /// file gives it, else a declared type with steps for the linker.
    pub(in crate::signatures) fn receiver(
        &self,
        callee: &Expression,
    ) -> (Option<String>, Option<GraphDeclaredType>) {
        if let Some(ty) = self.receiver_type(callee) {
            return (Some(ty), None);
        }
        let object = match unwrap_expr(callee) {
            Expression::StaticMemberExpression(member) => &member.object,
            Expression::PrivateFieldExpression(member) => &member.object,
            _ => return (None, None),
        };
        (None, self.declared_of(object).filter(has_steps))
    }

    /// `window`, when no scope binds the name: the global `Window` interface
    /// (spelled `globalThis.Window`: a `Window` the file declares or imports
    /// is another type), which `declare global` blocks across the repository
    /// extend.
    fn global_object(&self, name: &str) -> Option<GraphDeclaredType> {
        let bound = self.scopes.iter().any(|scope| scope.contains_key(name));
        (name == "window" && !bound).then(|| named("globalThis.Window".to_owned()))
    }

    /// The declared type of `expression`, as far as names and member steps
    /// spell it.
    pub(super) fn declared_of(&self, expression: &Expression) -> Option<GraphDeclaredType> {
        match unwrap_expr(expression) {
            Expression::Identifier(identifier) => self
                .lookup_declared(identifier.name.as_str())
                .or_else(|| self.global_object(identifier.name.as_str())),
            Expression::NewExpression(construct) => {
                dotted_expression_name(&construct.callee).map(named)
            }
            Expression::StaticMemberExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                self.this_field(member.property.name.as_str()).map(named)
            }
            Expression::PrivateFieldExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                self.this_field(&format!("#{}", member.field.name.as_str()))
                    .map(named)
            }
            Expression::StaticMemberExpression(member) => stepped(
                self.declared_of(&member.object)?,
                GraphTypeStep::Member(member.property.name.as_str().to_owned()),
            ),
            _ => None,
        }
    }
}
