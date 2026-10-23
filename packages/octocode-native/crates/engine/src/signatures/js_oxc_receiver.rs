//! Receiver-type facts for the JS/TS lane.
//!
//! [`ReceiverScopes`] rides along the call walk in `js_oxc_calls` and gives a
//! member call `x.m()` / `this.f.m()` the type its receiver is written with:
//! a TS annotation on the binding or parameter, a `new T(..)` initializer, or
//! the class field `f` (annotated property, `constructor(private f: T)`, or
//! `this.f = new T(..)` / a typed constructor parameter). Bindings are
//! lexically scoped (functions, arrows, blocks, loops, catch clauses); an
//! unannotated binding reassigned to anything else, a destructured name, or a
//! field assigned two different ways is unknown and emits nothing.

use std::collections::HashMap;

use oxc_ast::ast::*;

use super::js_oxc_calls::{dotted_expression_name, ts_type_name, unwrap_expr};

mod class_fields;

use class_fields::class_fields;

struct Binding {
    ty: Option<String>,
    /// Written with a type annotation: reassignment cannot change it.
    declared: bool,
}

type Fields = HashMap<String, Option<String>>;

pub(super) struct ReceiverScopes {
    scopes: Vec<HashMap<String, Binding>>,
    /// Field types of the instance `this` names; `None` where `this` is not
    /// a class instance (plain functions, static members, module code).
    this_fields: Vec<Option<Fields>>,
    /// Set by a class instance method for its own function.
    method_pending: bool,
}

impl Default for ReceiverScopes {
    fn default() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            this_fields: vec![None],
            method_pending: false,
        }
    }
}

impl ReceiverScopes {
    pub(super) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(super) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// The next function is an instance method: `this` stays the class.
    pub(super) fn mark_method(&mut self) {
        self.method_pending = true;
    }

    /// Enter a non-arrow function; returns whether it rebinds `this`.
    pub(super) fn enter_function(&mut self) -> bool {
        let method = std::mem::take(&mut self.method_pending);
        self.push_scope();
        if !method {
            self.this_fields.push(None);
        }
        !method
    }

    pub(super) fn exit_function(&mut self, rebound_this: bool) {
        self.pop_scope();
        if rebound_this {
            self.this_fields.pop();
        }
    }

    pub(super) fn enter_class(&mut self, class: &Class) {
        self.this_fields.push(Some(class_fields(class)));
    }

    pub(super) fn exit_class(&mut self) {
        self.this_fields.pop();
    }

    pub(super) fn bind_declarator(&mut self, it: &VariableDeclarator) {
        let declared = it
            .type_annotation
            .as_ref()
            .map(|a| ts_type(&a.type_annotation));
        let ty = match &declared {
            Some(ty) => ty.clone(),
            None => it
                .init
                .as_ref()
                .and_then(|init| expression_type(init, None)),
        };
        self.bind_pattern(&it.id, ty, declared.is_some());
    }

    pub(super) fn bind_parameter(&mut self, it: &FormalParameter) {
        let ty = it
            .type_annotation
            .as_ref()
            .and_then(|a| ts_type(&a.type_annotation));
        self.bind_pattern(&it.pattern, ty, true);
    }

    pub(super) fn bind_catch(&mut self, it: &CatchParameter) {
        self.bind_pattern(&it.pattern, None, true);
    }

    /// `x = value`: an unannotated binding keeps its type only when the new
    /// value has the same one.
    pub(super) fn assign(&mut self, it: &AssignmentExpression) {
        if it.operator != AssignmentOperator::Assign {
            return;
        }
        let AssignmentTarget::AssignmentTargetIdentifier(target) = &it.left else {
            return;
        };
        let ty = expression_type(&it.right, None);
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(target.name.as_str()) {
                if !binding.declared && binding.ty != ty {
                    binding.ty = None;
                }
                return;
            }
        }
    }

    /// Receiver type of a call whose callee is `callee`.
    pub(super) fn receiver_type(&self, callee: &Expression) -> Option<String> {
        let object = match unwrap_expr(callee) {
            Expression::StaticMemberExpression(member) => &member.object,
            Expression::PrivateFieldExpression(member) => &member.object,
            _ => return None,
        };
        match unwrap_expr(object) {
            Expression::Identifier(identifier) => self.lookup(identifier.name.as_str()),
            Expression::StaticMemberExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                self.this_field(member.property.name.as_str())
            }
            Expression::PrivateFieldExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                self.this_field(&format!("#{}", member.field.name.as_str()))
            }
            _ => None,
        }
    }

    fn lookup(&self, name: &str) -> Option<String> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .and_then(|binding| binding.ty.clone())
    }

    fn this_field(&self, name: &str) -> Option<String> {
        self.this_fields.last()?.as_ref()?.get(name)?.clone()
    }

    fn bind_pattern(&mut self, pattern: &BindingPattern, ty: Option<String>, declared: bool) {
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        if let BindingPattern::BindingIdentifier(identifier) = pattern {
            scope.insert(
                identifier.name.as_str().to_owned(),
                Binding { ty, declared },
            );
            return;
        }
        let mut names = Vec::new();
        pattern_names(pattern, &mut names);
        for name in names {
            scope.insert(
                name,
                Binding {
                    ty: None,
                    declared: true,
                },
            );
        }
    }
}

fn pattern_names(pattern: &BindingPattern, names: &mut Vec<String>) {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => {
            names.push(identifier.name.as_str().to_owned());
        }
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                pattern_names(&property.value, names);
            }
            if let Some(rest) = &object.rest {
                pattern_names(&rest.argument, names);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                pattern_names(element, names);
            }
            if let Some(rest) = &array.rest {
                pattern_names(&rest.argument, names);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => pattern_names(&assignment.left, names),
    }
}

/// A TS type as a receiver type: a (qualified) type reference without its
/// type arguments; `T | null | undefined` reads as `T`.
fn ts_type(ty: &TSType) -> Option<String> {
    match ty {
        TSType::TSTypeReference(reference) => ts_type_name(&reference.type_name),
        TSType::TSParenthesizedType(inner) => ts_type(&inner.type_annotation),
        TSType::TSUnionType(union) => {
            let mut rest = union.types.iter().filter(|ty| {
                !matches!(ty, TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_))
            });
            match (rest.next(), rest.next()) {
                (Some(only), None) => ts_type(only),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `new T(..)`, `value as T`, or an identifier typed in `parameters`.
fn expression_type(expression: &Expression, parameters: Option<&Fields>) -> Option<String> {
    if let Expression::TSAsExpression(cast) = expression {
        return ts_type(&cast.type_annotation);
    }
    match unwrap_expr(expression) {
        Expression::NewExpression(construct) => dotted_expression_name(&construct.callee),
        Expression::Identifier(identifier) => parameters?.get(identifier.name.as_str())?.clone(),
        _ => None,
    }
}

fn is_nullish(expression: &Expression) -> bool {
    match unwrap_expr(expression) {
        Expression::NullLiteral(_) => true,
        Expression::Identifier(identifier) => identifier.name == "undefined",
        _ => false,
    }
}

#[cfg(test)]
mod tests;
