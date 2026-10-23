//! Instance field types of a class: annotations and parameter properties,
//! else the one type every `this.f = …` assignment constructs.

use oxc_ast::ast::*;
use oxc_ast_visit::{VisitJs, walk_js};
use oxc_semantic::ScopeFlags;

use crate::signatures::js_oxc_shared::property_key_name;

use super::{Fields, expression_type, is_nullish, ts_type};

/// Record `ty` for `name`; a second, different type makes it unknown.
/// A field's annotated type is declared; otherwise a non-nullish initializer
/// contributes its constructed type to the inferred fields.
fn type_field(
    (declared, inferred): (&mut Fields, &mut Fields),
    key: &PropertyKey,
    annotation: Option<&TSTypeAnnotation>,
    value: Option<&Expression>,
) {
    let Some((name, _)) = property_key_name(key) else {
        return;
    };
    if let Some(annotation) = annotation {
        declared.insert(name, ts_type(&annotation.type_annotation));
    } else if let Some(value) = value
        && !is_nullish(value)
    {
        merge(inferred, name, expression_type(value, None));
    }
}

fn merge(fields: &mut Fields, name: String, ty: Option<String>) {
    match fields.get_mut(&name) {
        Some(existing) if *existing != ty => *existing = None,
        Some(_) => {}
        None => {
            fields.insert(name, ty);
        }
    }
}

/// Instance field types of `class`. Annotations (`f: T`, parameter
/// properties) are declared and win; otherwise every `this.f = …` in the
/// class body and the field initializer must agree on one constructed type.
pub(super) fn class_fields(class: &Class) -> Fields {
    let mut declared = Fields::new();
    let mut inferred = Fields::new();
    let mut constructor_parameters = Fields::new();
    for element in &class.body.body {
        match element {
            ClassElement::PropertyDefinition(property) if !property.r#static => type_field(
                (&mut declared, &mut inferred),
                &property.key,
                property.type_annotation.as_deref(),
                property.value.as_ref(),
            ),
            ClassElement::AccessorProperty(property) if !property.r#static => type_field(
                (&mut declared, &mut inferred),
                &property.key,
                property.type_annotation.as_deref(),
                None,
            ),
            ClassElement::MethodDefinition(method)
                if method.kind == MethodDefinitionKind::Constructor =>
            {
                for parameter in &method.value.params.items {
                    let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern else {
                        continue;
                    };
                    let ty = parameter
                        .type_annotation
                        .as_ref()
                        .and_then(|a| ts_type(&a.type_annotation));
                    let name = identifier.name.as_str().to_owned();
                    if parameter.accessibility.is_some()
                        || parameter.readonly
                        || parameter.r#override
                    {
                        declared.insert(name.clone(), ty.clone());
                    }
                    constructor_parameters.insert(name, ty);
                }
            }
            _ => {}
        }
    }
    let mut assignments = ThisAssignments {
        parameters: None,
        fields: &mut inferred,
    };
    for element in &class.body.body {
        match element {
            ClassElement::MethodDefinition(method) if !method.r#static => {
                let Some(body) = &method.value.body else {
                    continue;
                };
                assignments.parameters = (method.kind == MethodDefinitionKind::Constructor)
                    .then_some(&constructor_parameters);
                assignments.visit_function_body(body);
            }
            ClassElement::PropertyDefinition(property) if !property.r#static => {
                if let Some(Expression::ArrowFunctionExpression(arrow)) = &property.value {
                    assignments.parameters = None;
                    assignments.visit_arrow_function_body(&arrow.body);
                }
            }
            _ => {}
        }
    }
    inferred.extend(declared);
    inferred
}

/// Collects `this.f = value` inside one method body; nested non-arrow
/// functions and classes rebind `this` and are skipped.
struct ThisAssignments<'f, 'p> {
    parameters: Option<&'p Fields>,
    fields: &'f mut Fields,
}

impl<'a> VisitJs<'a> for ThisAssignments<'_, '_> {
    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {}

    fn visit_class(&mut self, _it: &Class<'a>) {}

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        let name = match &it.left {
            AssignmentTarget::StaticMemberExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                Some(member.property.name.as_str().to_owned())
            }
            AssignmentTarget::PrivateFieldExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                Some(format!("#{}", member.field.name.as_str()))
            }
            _ => None,
        };
        if let Some(name) = name
            && !is_nullish(&it.right)
        {
            let ty = if it.operator == AssignmentOperator::Assign {
                expression_type(&it.right, self.parameters)
            } else {
                None
            };
            merge(self.fields, name, ty);
        }
        walk_js::walk_assignment_expression(self, it);
    }
}
