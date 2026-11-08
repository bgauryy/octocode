//! Declared TS types as the linker follows them across files
//! ([`GraphDeclaredType`]): the type an annotation writes, and the declared
//! type of each class, interface and type alias the module declares at its
//! top level. Only what the source writes: a member without an annotation is
//! untyped, and a type that names a type parameter in scope (`P` in
//! `type A<P> = { p: P }`) depends on its arguments, so it is not one.

use oxc_ast::ast::*;

use super::super::js_oxc_calls::ts_type_name;
use crate::graph::{GraphDeclaredType, GraphPosition, GraphTypeMember, GraphTypeStep};
use crate::signatures::js_oxc_shared::{SpanPositions, property_key_name};

/// The type `ty` writes, with `generics` the type parameter names in scope.
pub(super) fn declared_type(ty: &TSType, generics: &[&str]) -> Option<GraphDeclaredType> {
    match ty {
        TSType::TSTypeReference(reference) => {
            let name = ts_type_name(&reference.type_name)?;
            let head = name.split('.').next().unwrap_or(&name);
            (!generics.contains(&head)).then_some(GraphDeclaredType::Named {
                name,
                steps: Vec::new(),
            })
        }
        TSType::TSParenthesizedType(inner) => declared_type(&inner.type_annotation, generics),
        TSType::TSUnionType(union) => {
            let mut rest = union.types.iter().filter(|ty| {
                !matches!(ty, TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_))
            });
            match (rest.next(), rest.next()) {
                (Some(only), None) => declared_type(only, generics),
                _ => None,
            }
        }
        TSType::TSIndexedAccessType(indexed) => {
            let TSType::TSLiteralType(literal) = &indexed.index_type else {
                return None;
            };
            let TSLiteral::StringLiteral(key) = &literal.literal else {
                return None;
            };
            let object = declared_type(&indexed.object_type, generics)?;
            stepped(object, GraphTypeStep::Member(key.value.as_str().to_owned()))
        }
        TSType::TSTypeLiteral(literal) => Some(object_type(&literal.members, generics)),
        TSType::TSFunctionType(function) => {
            let generics = with_parameters(generics, function.type_parameters.as_deref());
            Some(GraphDeclaredType::Function {
                parameters: function
                    .params
                    .items
                    .iter()
                    .map(|parameter| annotated(parameter.type_annotation.as_deref(), &generics))
                    .collect(),
            })
        }
        _ => None,
    }
}

/// `ty` followed one more step; only a named type has steps.
pub(super) fn stepped(ty: GraphDeclaredType, step: GraphTypeStep) -> Option<GraphDeclaredType> {
    match ty {
        GraphDeclaredType::Named { name, mut steps } => {
            steps.push(step);
            Some(GraphDeclaredType::Named { name, steps })
        }
        _ => None,
    }
}

/// A named type with steps: a receiver only the linker can type.
pub(super) fn has_steps(ty: &GraphDeclaredType) -> bool {
    matches!(ty, GraphDeclaredType::Named { steps, .. } if !steps.is_empty())
}

fn annotated(
    annotation: Option<&TSTypeAnnotation>,
    generics: &[&str],
) -> Option<GraphDeclaredType> {
    declared_type(&annotation?.type_annotation, generics)
}

fn with_parameters<'g>(
    generics: &[&'g str],
    parameters: Option<&'g TSTypeParameterDeclaration>,
) -> Vec<&'g str> {
    let mut names = generics.to_vec();
    names.extend(
        parameters
            .into_iter()
            .flat_map(|declaration| &declaration.params)
            .map(|parameter| parameter.name.name.as_str()),
    );
    names
}

/// The property signatures of an object type literal or interface body.
fn object_type(signatures: &[TSSignature], generics: &[&str]) -> GraphDeclaredType {
    let members = signatures
        .iter()
        .filter_map(|signature| match signature {
            TSSignature::TSPropertySignature(property) if !property.computed => {
                Some(GraphTypeMember {
                    name: property_key_name(&property.key)?.0,
                    ty: annotated(property.type_annotation.as_deref(), generics),
                })
            }
            _ => None,
        })
        .collect();
    GraphDeclaredType::Object { members }
}

/// The declared instance fields of `class`: annotated properties and
/// accessors, parameter properties, and getters by their return type.
fn class_type(class: &Class) -> GraphDeclaredType {
    let generics = with_parameters(&[], class.type_parameters.as_deref());
    let mut members = Vec::new();
    let mut push = |name: String, annotation: Option<&TSTypeAnnotation>| {
        members.push(GraphTypeMember {
            name,
            ty: annotated(annotation, &generics),
        });
    };
    let key = |key: &PropertyKey| property_key_name(key).map(|(name, _)| name);
    for element in &class.body.body {
        match element {
            ClassElement::PropertyDefinition(property) if !property.r#static => {
                if let Some(name) = key(&property.key) {
                    push(name, property.type_annotation.as_deref());
                }
            }
            ClassElement::AccessorProperty(property) if !property.r#static => {
                if let Some(name) = key(&property.key) {
                    push(name, property.type_annotation.as_deref());
                }
            }
            ClassElement::MethodDefinition(method)
                if !method.r#static && method.kind == MethodDefinitionKind::Get =>
            {
                if let Some(name) = key(&method.key) {
                    push(name, method.value.return_type.as_deref());
                }
            }
            ClassElement::MethodDefinition(method)
                if method.kind == MethodDefinitionKind::Constructor =>
            {
                for parameter in method.value.params.items.iter().filter(|parameter| {
                    parameter.accessibility.is_some() || parameter.readonly || parameter.r#override
                }) {
                    if let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern {
                        push(
                            identifier.name.as_str().to_owned(),
                            parameter.type_annotation.as_deref(),
                        );
                    }
                }
            }
            _ => {}
        }
    }
    GraphDeclaredType::Object { members }
}

/// One declared type of the module, at the start of its declaration's name
/// token with the kind that declaration has.
pub(in crate::signatures) struct DeclaredAt {
    pub(in crate::signatures) name_start: GraphPosition,
    pub(in crate::signatures) kind: &'static str,
    pub(in crate::signatures) ty: GraphDeclaredType,
}

/// The declared types of the module's top-level classes, interfaces and
/// type aliases, exported or not.
pub(in crate::signatures) fn declared_types(
    program: &Program,
    li: &SpanPositions,
) -> Vec<DeclaredAt> {
    let mut out = Vec::new();
    let mut push = |name: &BindingIdentifier, kind, ty: Option<GraphDeclaredType>| {
        if let Some(ty) = ty {
            out.push(DeclaredAt {
                name_start: li.position(name.span.start),
                kind,
                ty,
            });
        }
    };
    for statement in &program.body {
        let declaration = match statement {
            Statement::ExportDeclaration(export) => Some(&export.declaration),
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration
                    && let Some(id) = &class.id
                {
                    push(id, "class", Some(class_type(class)));
                }
                continue;
            }
            other => other.as_declaration(),
        };
        match declaration {
            Some(Declaration::ClassDeclaration(class)) => {
                if let Some(id) = &class.id {
                    push(id, "class", Some(class_type(class)));
                }
            }
            Some(Declaration::TSInterfaceDeclaration(interface)) => {
                let generics = with_parameters(&[], interface.type_parameters.as_deref());
                push(
                    &interface.id,
                    "interface",
                    Some(object_type(&interface.body.body, &generics)),
                );
            }
            Some(Declaration::TSTypeAliasDeclaration(alias)) => {
                let generics = with_parameters(&[], alias.type_parameters.as_deref());
                push(
                    &alias.id,
                    "type",
                    declared_type(&alias.type_annotation, &generics),
                );
            }
            _ => {}
        }
    }
    out
}
