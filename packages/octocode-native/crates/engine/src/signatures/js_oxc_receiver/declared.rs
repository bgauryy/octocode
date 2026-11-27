//! Declared TS types as the linker follows them across files
//! ([`GraphDeclaredType`]): the type an annotation writes, and the declared
//! type of each class, interface and type alias the module declares at its
//! top level. Only what the source writes: a member without an annotation is
//! untyped, and a type that names a type parameter in scope (`P` in
//! `type A<P> = { p: P }`) depends on its arguments, so it is not one.

use oxc_ast::ast::*;

use super::super::js_oxc_calls::{ts_type_name, unwrap_expr};
use crate::graph::{GraphDeclaredType, GraphPosition, GraphTypeMember, GraphTypeStep};
use crate::signatures::js_oxc_shared::{SpanPositions, property_key_name};

/// The type `ty` writes, with `generics` the type parameter names in scope.
pub(super) fn declared_type(ty: &TSType, generics: &[&str]) -> Option<GraphDeclaredType> {
    match ty {
        TSType::TSTypeReference(reference) => {
            let name = ts_type_name(&reference.type_name)?;
            if name == "InstanceType" {
                return instance_type(reference);
            }
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

/// `InstanceType<typeof C>`: the instance type of the class `C` names, which
/// is the type `C` declares. Any other argument writes no declared type.
fn instance_type(reference: &TSTypeReference) -> Option<GraphDeclaredType> {
    let arguments = reference.type_arguments.as_deref()?;
    let [TSType::TSTypeQuery(query)] = arguments.params.as_slice() else {
        return None;
    };
    let name = query.expr_name.as_ts_type_name()?;
    Some(GraphDeclaredType::Named {
        name: ts_type_name(name)?,
        steps: Vec::new(),
    })
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

/// The declared type of a function's parameters. A parameter written as a
/// type parameter takes that type parameter's constraint (`<T extends A>(a: T)`
/// types `a` as `A`, as tsserver reads a contextually typed argument); one
/// without a constraint is untyped.
fn function_type(
    parameters: &FormalParameters,
    type_parameters: Option<&TSTypeParameterDeclaration>,
) -> GraphDeclaredType {
    let generics = with_parameters(&[], type_parameters);
    let constraint = |ty: &TSType| -> Option<&TSType> {
        let TSType::TSTypeReference(reference) = ty else {
            return None;
        };
        let TSTypeName::IdentifierReference(name) = &reference.type_name else {
            return None;
        };
        type_parameters?
            .params
            .iter()
            .find(|parameter| parameter.name.name == name.name)
            .map(|parameter| parameter.constraint.as_ref())?
    };
    GraphDeclaredType::Function {
        parameters: parameters
            .items
            .iter()
            .map(|parameter| {
                let annotation = &parameter.type_annotation.as_deref()?.type_annotation;
                match constraint(annotation) {
                    Some(bound) => declared_type(bound, &generics),
                    None => declared_type(annotation, &generics),
                }
            })
            .collect(),
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
    /// Declared in the global scope: inside `declare global { .. }`, or in a
    /// file that is a script (no `import` or `export`). A global interface
    /// merges with every other of its name, in any file.
    pub(in crate::signatures) global: bool,
}

/// The declared types of the module's top-level classes, interfaces and
/// type aliases, exported or not, and those of its `declare global` blocks.
pub(in crate::signatures) fn declared_types(
    program: &Program,
    li: &SpanPositions,
) -> Vec<DeclaredAt> {
    let script = !program.body.iter().any(|statement| {
        matches!(
            statement,
            Statement::ImportDeclaration(_)
                | Statement::ExportDeclaration(_)
                | Statement::ExportDefaultDeclaration(_)
        ) || is_commonjs(statement)
    });
    let mut out = Vec::new();
    collect_declared(&program.body, script, li, &mut out);
    out
}

/// A top-level statement only a CommonJS module writes: a `require(..)`
/// call, or an assignment to `module.exports` or `exports.k`. Such a file is
/// a module, so its declarations are not global.
fn is_commonjs(statement: &Statement) -> bool {
    fn requires(expression: &Expression) -> bool {
        match unwrap_expr(expression) {
            Expression::CallExpression(call) => {
                matches!(
                    unwrap_expr(&call.callee),
                    Expression::Identifier(callee) if callee.name == "require"
                ) || requires(&call.callee)
            }
            Expression::StaticMemberExpression(member) => requires(&member.object),
            _ => false,
        }
    }
    fn exports(target: &AssignmentTarget) -> bool {
        let AssignmentTarget::StaticMemberExpression(member) = target else {
            return false;
        };
        match &member.object {
            Expression::Identifier(object) => {
                object.name == "exports"
                    || (object.name == "module" && member.property.name == "exports")
            }
            Expression::StaticMemberExpression(inner) => {
                matches!(&inner.object, Expression::Identifier(module) if module.name == "module")
                    && inner.property.name == "exports"
            }
            _ => false,
        }
    }
    match statement {
        Statement::VariableDeclaration(variables) => variables
            .declarations
            .iter()
            .any(|declarator| declarator.init.as_ref().is_some_and(requires)),
        Statement::ExpressionStatement(statement) => match unwrap_expr(&statement.expression) {
            Expression::AssignmentExpression(assignment) => {
                exports(&assignment.left) || requires(&assignment.right)
            }
            other => requires(other),
        },
        _ => false,
    }
}

fn collect_declared(
    statements: &[Statement],
    global: bool,
    li: &SpanPositions,
    out: &mut Vec<DeclaredAt>,
) {
    let mut push = |name: &BindingIdentifier, kind, ty: Option<GraphDeclaredType>| {
        if let Some(ty) = ty {
            out.push(DeclaredAt {
                name_start: li.position(name.span.start),
                kind,
                ty,
                global,
            });
        }
    };
    let mut blocks = Vec::new();
    // An overloaded function's implementation signature is not one its
    // callers see: no declared type for a name that also has a bare signature.
    let overloaded: Vec<&str> = statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::ExportDeclaration(export) => Some(&export.declaration),
            other => other.as_declaration(),
        })
        .filter_map(|declaration| match declaration {
            Declaration::FunctionDeclaration(function) if function.body.is_none() => {
                function.id.as_ref().map(|id| id.name.as_str())
            }
            _ => None,
        })
        .collect();
    for statement in statements {
        let declaration = match statement {
            Statement::TSGlobalDeclaration(block) => {
                blocks.push(block);
                continue;
            }
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
            Some(Declaration::FunctionDeclaration(function)) => {
                if let Some(id) = &function.id
                    && function.body.is_some()
                    && !overloaded.contains(&id.name.as_str())
                {
                    push(
                        id,
                        "function",
                        Some(function_type(
                            &function.params,
                            function.type_parameters.as_deref(),
                        )),
                    );
                }
            }
            Some(Declaration::VariableDeclaration(variables))
                if variables.kind == VariableDeclarationKind::Const =>
            {
                for declarator in &variables.declarations {
                    let (BindingPattern::BindingIdentifier(id), None, Some(init)) = (
                        &declarator.id,
                        &declarator.type_annotation,
                        &declarator.init,
                    ) else {
                        continue;
                    };
                    let ty = match unwrap_expr(init) {
                        Expression::ArrowFunctionExpression(arrow) => {
                            function_type(&arrow.params, arrow.type_parameters.as_deref())
                        }
                        Expression::FunctionExpression(function) => {
                            function_type(&function.params, function.type_parameters.as_deref())
                        }
                        _ => continue,
                    };
                    push(id, "function", Some(ty));
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
    for block in blocks {
        collect_declared(&block.body.body, true, li, out);
    }
}
