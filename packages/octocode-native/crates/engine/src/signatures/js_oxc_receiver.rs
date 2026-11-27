//! Receiver-type facts for the JS/TS lane.
//!
//! [`ReceiverScopes`] rides along the call walk in `js_oxc_calls` and gives a
//! member call `x.m()` / `this.f.m()` the type its receiver is written with:
//! a TS annotation on the binding or parameter, a `new T(..)` initializer or
//! receiver (`new T().m()`), or the class field `f` (annotated property,
//! `constructor(private f: T)`, or `this.f = new T(..)` / a typed
//! constructor parameter), or the property a destructuring pattern takes
//! from an annotated parameter or from the object the file's own function
//! returns (`const { s } = make()`). A receiver the file types only through
//! a declared type it may not declare itself (`x.f.m()` with `x: T`, a
//! parameter of `const f: Alias = (p) => …`, `p: T["k"]`, a property
//! destructured from such a value) carries that type and its steps for the
//! linker ([`declared`]). Bindings are
//! lexically scoped (functions, arrows, blocks, loops, catch clauses); an
//! unannotated binding reassigned to anything else, any other destructured
//! name, or a field assigned two different ways is unknown and emits nothing.

use std::collections::HashMap;

use oxc_ast::ast::*;

use super::js_oxc_calls::{dotted_expression_name, ts_type_name, unwrap_expr};
use super::js_oxc_shared::property_key_name;

mod aliases;
mod class_fields;
pub(super) mod declared;
mod object_types;
mod paths;
mod return_shapes;

use crate::graph::GraphDeclaredType;
use crate::signatures::receiver_fields::Fields;
use class_fields::class_fields;
use object_types::ObjectTypes;
use return_shapes::ReturnShapes;

struct Binding {
    ty: Option<String>,
    /// A declared type with steps the file cannot name as one type: the
    /// linker follows it.
    path: Option<GraphDeclaredType>,
    /// Written with a type annotation: reassignment cannot change it.
    declared: bool,
    /// `const f = ns.g`: what a call of the name calls.
    alias: Option<String>,
}

pub(super) struct ReceiverScopes {
    scopes: Vec<HashMap<String, Binding>>,
    /// Field types of the instance `this` names; `None` where `this` is not
    /// a class instance (plain functions, static members, module code).
    this_fields: Vec<Option<Fields>>,
    /// Set by a class instance method for its own function.
    method_pending: bool,
    /// The declarators being read belong to a `const`.
    in_const: bool,
    /// The file's object types, for destructured parameters.
    object_types: ObjectTypes,
    /// The objects the file's own functions return, for destructured calls.
    return_shapes: ReturnShapes,
    /// Per enclosing function, the type its annotated declarator gives it
    /// (`const f: A = (p) => …`) and the index of its next parameter.
    contexts: Vec<Option<(GraphDeclaredType, u32)>>,
    /// The functions about to be walked that a declared type gives their
    /// parameters (an annotated declarator, an argument of a call), by span
    /// start.
    pending_context: HashMap<u32, GraphDeclaredType>,
}

impl Default for ReceiverScopes {
    fn default() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            this_fields: vec![None],
            method_pending: false,
            in_const: false,
            object_types: ObjectTypes::default(),
            return_shapes: ReturnShapes::default(),
            contexts: Vec::new(),
            pending_context: HashMap::new(),
        }
    }
}

impl ReceiverScopes {
    pub(super) fn for_program(program: &Program) -> Self {
        Self {
            object_types: ObjectTypes::of_program(program),
            return_shapes: ReturnShapes::of_program(program),
            ..Self::default()
        }
    }

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
        let alias = self.const_alias(it);
        self.bind_typed(it);
        if let (Some(alias), BindingPattern::BindingIdentifier(name), Some(scope)) =
            (alias, &it.id, self.scopes.last_mut())
            && let Some(binding) = scope.get_mut(name.name.as_str())
        {
            binding.alias = Some(alias);
        }
    }

    fn bind_typed(&mut self, it: &VariableDeclarator) {
        if let (BindingPattern::ObjectPattern(object), None) = (&it.id, &it.type_annotation)
            && let Some(fields) = self.returned_shape(it.init.as_ref())
        {
            self.bind_object_pattern(object, &fields);
            return;
        }
        if it.type_annotation.is_none()
            && let Some(init) = &it.init
            && let Some(base) = self.declared_of(init)
        {
            match &it.id {
                BindingPattern::ObjectPattern(object) => {
                    self.bind_declared_object(object, &base);
                    return;
                }
                // `let` may be reassigned: only a constant keeps the type of
                // the value it takes a member of.
                pattern @ BindingPattern::BindingIdentifier(_)
                    if self.in_const && declared::has_steps(&base) =>
                {
                    self.bind_path(pattern, base);
                    return;
                }
                _ => {}
            }
        }
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
        let context = self.next_context_parameter();
        if it.type_annotation.is_none()
            && let Some(context) = context
        {
            match &it.pattern {
                BindingPattern::ObjectPattern(object) => {
                    self.bind_declared_object(object, &context);
                }
                pattern => self.bind_path(pattern, context),
            }
            return;
        }
        if let (BindingPattern::ObjectPattern(object), Some(annotation)) =
            (&it.pattern, &it.type_annotation)
            && let Some(fields) = self.object_types.fields_of(&annotation.type_annotation)
        {
            self.bind_object_pattern(object, &fields);
            return;
        }
        let ty = it
            .type_annotation
            .as_ref()
            .and_then(|a| ts_type(&a.type_annotation));
        if ty.is_none()
            && let Some(path) = it
                .type_annotation
                .as_ref()
                .and_then(|a| declared::declared_type(&a.type_annotation, &[]))
                .filter(declared::has_steps)
        {
            self.bind_path(&it.pattern, path);
            return;
        }
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
            Expression::NewExpression(construct) => dotted_expression_name(&construct.callee),
            Expression::StaticMemberExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                self.this_field(member.property.name.as_str())
            }
            Expression::StaticMemberExpression(member) => match unwrap_expr(&member.object) {
                Expression::Identifier(identifier) => self.object_types.property(
                    &self.lookup(identifier.name.as_str())?,
                    member.property.name.as_str(),
                ),
                _ => None,
            },
            Expression::PrivateFieldExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                self.this_field(&format!("#{}", member.field.name.as_str()))
            }
            _ => None,
        }
    }

    /// The returned object shape of `f(..)` when `f` is the file's own
    /// module function: no inner scope rebinds the name.
    fn returned_shape(&self, init: Option<&Expression>) -> Option<Fields> {
        let Expression::CallExpression(call) = unwrap_expr(init?) else {
            return None;
        };
        let Expression::Identifier(callee) = unwrap_expr(&call.callee) else {
            return None;
        };
        let name = callee.name.as_str();
        if self
            .scopes
            .iter()
            .skip(1)
            .any(|scope| scope.contains_key(name))
        {
            return None;
        }
        self.return_shapes.of(name).cloned()
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

    /// `{ a, b: c = d, ...rest }` under an object type: each name a property
    /// binds takes that property's type; anything nested is unknown.
    fn bind_object_pattern(&mut self, object: &ObjectPattern, fields: &Fields) {
        for property in &object.properties {
            let ty = (!property.computed)
                .then(|| property_key_name(&property.key))
                .flatten()
                .and_then(|(key, _)| fields.get(&key).cloned().flatten());
            let value = match &property.value {
                BindingPattern::AssignmentPattern(assignment) => &assignment.left,
                other => other,
            };
            match value {
                BindingPattern::BindingIdentifier(_) => self.bind_pattern(value, ty, true),
                nested => self.bind_pattern(nested, None, true),
            }
        }
        if let Some(rest) = &object.rest {
            self.bind_pattern(&rest.argument, None, true);
        }
    }

    fn bind_pattern(&mut self, pattern: &BindingPattern, ty: Option<String>, declared: bool) {
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        if let BindingPattern::BindingIdentifier(identifier) = pattern {
            scope.insert(
                identifier.name.as_str().to_owned(),
                Binding {
                    ty,
                    path: None,
                    declared,
                    alias: None,
                },
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
                    path: None,
                    declared: true,
                    alias: None,
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
