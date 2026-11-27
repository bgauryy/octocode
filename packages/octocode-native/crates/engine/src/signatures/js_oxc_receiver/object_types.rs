//! Property types of the object types a file declares (`interface P { s:
//! Scene }`, `type P = { s: Scene }`) and of inline type literals: a
//! parameter destructured under such an annotation (`({ s }: P) => …`) binds
//! each name to its property's written type, as tsserver types it.

use std::collections::HashMap;

use oxc_ast::ast::*;

use super::ts_type;
use crate::signatures::js_oxc_shared::property_key_name;
use crate::signatures::receiver_fields::{Fields, merge};

/// Object type name → its property types, for the file's top-level and
/// exported interfaces and type-literal aliases. A name declared twice
/// (declaration merging) merges; a disagreement makes the property unknown.
#[derive(Default)]
pub(super) struct ObjectTypes(HashMap<String, Fields>);

impl ObjectTypes {
    pub(super) fn of_program(program: &Program) -> Self {
        let mut types = Self::default();
        for statement in &program.body {
            let declaration = match statement {
                Statement::ExportDeclaration(export) => Some(&export.declaration),
                other => other.as_declaration(),
            };
            match declaration {
                Some(Declaration::TSInterfaceDeclaration(interface)) => {
                    types.add(interface.id.name.as_str(), &interface.body.body);
                }
                Some(Declaration::TSTypeAliasDeclaration(alias)) => {
                    if let TSType::TSTypeLiteral(literal) = &alias.type_annotation {
                        types.add(alias.id.name.as_str(), &literal.members);
                    }
                }
                _ => {}
            }
        }
        types
    }

    fn add(&mut self, name: &str, members: &[TSSignature]) {
        let fields = self.0.entry(name.to_owned()).or_default();
        for (property, ty) in signature_types(members) {
            merge(fields, &property, ty);
        }
    }

    /// Property types of an annotation: an inline type literal or a
    /// reference to a local object type.
    pub(super) fn fields_of(&self, ty: &TSType) -> Option<Fields> {
        match ty {
            TSType::TSTypeLiteral(literal) => {
                let mut fields = Fields::new();
                for (property, ty) in signature_types(&literal.members) {
                    merge(&mut fields, &property, ty);
                }
                Some(fields)
            }
            TSType::TSTypeReference(reference) => match &reference.type_name {
                TSTypeName::IdentifierReference(name) => self.0.get(name.name.as_str()).cloned(),
                _ => None,
            },
            TSType::TSParenthesizedType(inner) => self.fields_of(&inner.type_annotation),
            _ => None,
        }
    }

    /// The written type of `property` on the local object type `owner`.
    pub(super) fn property(&self, owner: &str, property: &str) -> Option<String> {
        self.0.get(owner)?.get(property)?.clone()
    }
}

fn signature_types<'s>(
    members: &'s [TSSignature<'s>],
) -> impl Iterator<Item = (String, Option<String>)> + 's {
    members.iter().filter_map(|member| match member {
        TSSignature::TSPropertySignature(property) if !property.computed => {
            let (name, _) = property_key_name(&property.key)?;
            let ty = property
                .type_annotation
                .as_ref()
                .and_then(|annotation| ts_type(&annotation.type_annotation));
            Some((name, ty))
        }
        _ => None,
    })
}
