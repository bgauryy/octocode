//! Returned object shapes of the file's own functions (R-TS3): `const { a }
//! = f()` binds `a` to the type of property `a` in the object literal every
//! `return` of `f` builds, as tsserver infers the return type. Only module
//! `function f` and `const f = () => …` count, never async or generator
//! functions (their value is a promise or an iterator); a property is typed
//! by `new T(..)`, `x as T`, or a local or parameter written with one.

use std::collections::HashMap;

use oxc_ast::ast::*;
use oxc_ast_visit::{VisitJs, walk_js};
use oxc_semantic::ScopeFlags;

use super::{expression_type, ts_type};
use crate::signatures::js_oxc_calls::unwrap_expr;
use crate::signatures::js_oxc_shared::property_key_name;
use crate::signatures::receiver_fields::{Fields, merge};

/// Function name → its returned object's property types; `None` when the
/// name is declared twice or a `return` is not an object literal.
#[derive(Default)]
pub(super) struct ReturnShapes(HashMap<String, Option<Fields>>);

impl ReturnShapes {
    pub(super) fn of_program(program: &Program) -> Self {
        let mut shapes = Self::default();
        for statement in &program.body {
            let declaration = match statement {
                Statement::ExportDeclaration(export) => Some(&export.declaration),
                other => other.as_declaration(),
            };
            match declaration {
                Some(Declaration::FunctionDeclaration(function)) => {
                    if let Some(id) = &function.id {
                        shapes.add(id.name.as_str(), function_shape(function));
                    }
                }
                Some(Declaration::VariableDeclaration(variables))
                    if variables.kind == VariableDeclarationKind::Const =>
                {
                    for declarator in &variables.declarations {
                        let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                            continue;
                        };
                        let shape = match declarator.init.as_ref().map(unwrap_expr) {
                            Some(Expression::ArrowFunctionExpression(arrow)) => arrow_shape(arrow),
                            Some(Expression::FunctionExpression(function)) => {
                                function_shape(function)
                            }
                            _ => continue,
                        };
                        shapes.add(id.name.as_str(), shape);
                    }
                }
                _ => {}
            }
        }
        shapes
    }

    fn add(&mut self, name: &str, shape: Option<Fields>) {
        self.0
            .entry(name.to_owned())
            .and_modify(|seen| *seen = None)
            .or_insert(shape);
    }

    pub(super) fn of(&self, name: &str) -> Option<&Fields> {
        self.0.get(name)?.as_ref()
    }
}

fn function_shape(function: &Function) -> Option<Fields> {
    if function.r#async || function.generator {
        return None;
    }
    let mut returns = Returns::new(&function.params);
    returns.visit_function_body(function.body.as_ref()?);
    returns.shape()
}

fn arrow_shape(arrow: &ArrowFunctionExpression) -> Option<Fields> {
    if arrow.r#async {
        return None;
    }
    let mut returns = Returns::new(&arrow.params);
    match arrow.body.as_function_body() {
        Some(body) => returns.visit_function_body(body),
        None => returns.returned(arrow.body.as_expression()),
    }
    returns.shape()
}

/// The `return`s of one function body and the types of the names it
/// declares before them; nested functions and classes return for themselves.
#[derive(Default)]
struct Returns {
    locals: Fields,
    /// Property types of each returned object literal, in source order.
    objects: Vec<Fields>,
    /// A `return` of anything else, or of an object with a spread or a
    /// computed key: the shape is unknown.
    other: bool,
}

impl Returns {
    fn new(params: &FormalParameters) -> Self {
        let mut returns = Self::default();
        for parameter in &params.items {
            if let BindingPattern::BindingIdentifier(id) = &parameter.pattern {
                let ty = parameter
                    .type_annotation
                    .as_ref()
                    .and_then(|a| ts_type(&a.type_annotation));
                merge(&mut returns.locals, id.name.as_str(), ty);
            }
        }
        returns
    }

    fn returned(&mut self, value: Option<&Expression>) {
        let fields = match value.map(unwrap_expr) {
            Some(Expression::ObjectExpression(object)) => self.object_fields(object),
            _ => None,
        };
        match fields {
            Some(fields) => self.objects.push(fields),
            None => self.other = true,
        }
    }

    fn object_fields(&self, object: &ObjectExpression) -> Option<Fields> {
        let mut fields = Fields::new();
        for property in &object.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return None;
            };
            let (name, _) = (!property.computed)
                .then(|| property_key_name(&property.key))
                .flatten()?;
            let ty = (property.kind == PropertyKind::Init && !property.method)
                .then(|| expression_type(&property.value, Some(&self.locals)))
                .flatten();
            merge(&mut fields, &name, ty);
        }
        Some(fields)
    }

    /// Each property's type merged over every returned object; a property
    /// some object lacks is unknown.
    fn shape(self) -> Option<Fields> {
        if self.other {
            return None;
        }
        let mut objects = self.objects.into_iter();
        let mut shape = objects.next()?;
        for object in objects {
            for (name, ty) in shape.iter_mut() {
                if object.get(name) != Some(ty) {
                    *ty = None;
                }
            }
            for name in object.keys() {
                shape.entry(name.clone()).or_insert(None);
            }
        }
        Some(shape)
    }
}

impl<'a> VisitJs<'a> for Returns {
    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {}

    fn visit_arrow_function_expression(&mut self, _it: &ArrowFunctionExpression<'a>) {}

    fn visit_class(&mut self, _it: &Class<'a>) {}

    fn visit_return_statement(&mut self, it: &ReturnStatement<'a>) {
        self.returned(it.argument.as_ref());
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        walk_js::walk_variable_declarator(self, it);
        if let BindingPattern::BindingIdentifier(id) = &it.id {
            let ty = match &it.type_annotation {
                Some(annotation) => ts_type(&annotation.type_annotation),
                None => it
                    .init
                    .as_ref()
                    .and_then(|init| expression_type(init, Some(&self.locals))),
            };
            merge(&mut self.locals, id.name.as_str(), ty);
        }
    }
}
