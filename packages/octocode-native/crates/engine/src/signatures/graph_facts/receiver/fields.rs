//! Fields of the enclosing type: declared or constructed member types, and
//! the file's uniquely named type declarations.

use std::collections::HashMap;

use tree_sitter::Node;

use super::*;

impl<'t> ReceiverTypes<'t> {
    /// The type of member `name` on the class/struct enclosing `at`. A bare
    /// name (`unqualified`) also searches outer classes.
    /// Field `name` of the type that encloses `path` (root first, as in
    /// [`Self::receiver_type`]).
    pub(super) fn enclosing_field(
        &mut self,
        path: &[Node<'t>],
        name: &str,
        content: &str,
        unqualified: bool,
    ) -> Option<String> {
        let owners: &[&str] = match self.lang {
            Lang::Rust => {
                let implementation = path[nearest(path, &["impl_item"])?];
                let ty = clean_type_text(
                    node_text(implementation.child_by_field_name("type")?, content)?,
                    Lang::Rust,
                )?;
                let declaration = self.unique_type(last_segment(&ty), content)?;
                return self.fields_of(declaration, content).get(name).cloned()?;
            }
            Lang::Go => return None,
            Lang::Python => &["class_definition"],
            Lang::Java => &[
                "class_declaration",
                "enum_declaration",
                "record_declaration",
                "interface_declaration",
            ],
            Lang::CSharp => &[
                "class_declaration",
                "struct_declaration",
                "record_declaration",
                "record_struct_declaration",
                "interface_declaration",
            ],
            Lang::Cpp => &["class_specifier", "struct_specifier"],
        };
        // Index in `path` of the current owner; `None` once the owner came
        // from elsewhere in the file (an out-of-line C++ member's class).
        let mut index = nearest(path, owners);
        let mut current = index.map(|i| path[i]);
        if current.is_none() && self.lang == Lang::Cpp {
            // Out-of-line member `void Svc::run() { .. }`.
            let function = path[nearest(path, &["function_definition"])?];
            let declarator = function.child_by_field_name("declarator")?;
            let qualified = declarator.child_by_field_name("declarator")?;
            let owner = node_text(qualified.child_by_field_name("scope")?, content)?;
            current = self.unique_type(owner, content);
        }
        while let Some(owner) = current {
            if let Some(ty) = self.fields_of(owner, content).get(name) {
                return ty.clone();
            }
            if !unqualified {
                return None;
            }
            current = match index {
                Some(i) => {
                    index = nearest(&path[..i], owners);
                    index.map(|j| path[j])
                }
                None => ancestor(self.root, owner, owners),
            };
        }
        None
    }

    /// The only declaration of the named struct/class in this file.
    pub(super) fn unique_type(&mut self, name: &str, content: &str) -> Option<Node<'t>> {
        let lang = self.lang;
        let root = self.root;
        let types = self.types.get_or_insert_with(|| {
            let mut types: HashMap<String, Vec<Node<'t>>> = HashMap::new();
            let mut pending = vec![root];
            let mut cursor = root.walk();
            while let Some(node) = pending.pop() {
                let declaration = match (lang, node.kind()) {
                    (Lang::Rust, "struct_item")
                    | (Lang::Cpp, "class_specifier" | "struct_specifier") => {
                        node.child_by_field_name("body").map(|_| node)
                    }
                    (Lang::Go, "type_spec") => node
                        .child_by_field_name("type")
                        .filter(|ty| ty.kind() == "struct_type")
                        .map(|_| node),
                    _ => None,
                };
                if let Some(declaration) = declaration
                    && let Some(name) = declaration
                        .child_by_field_name("name")
                        .and_then(|name| node_text(name, content))
                {
                    types.entry(name.to_owned()).or_default().push(declaration);
                }
                pending.extend(node.named_children(&mut cursor));
            }
            types
        });
        match types.get(name).map(Vec::as_slice) {
            Some([only]) => Some(*only),
            _ => None,
        }
    }

    pub(super) fn fields_of(&mut self, owner: Node<'t>, content: &str) -> &Fields {
        if !self.fields.contains_key(&owner.id()) {
            let fields = match self.lang {
                Lang::Python => self.python_fields(owner, content),
                _ => self.declared_fields(owner, content),
            };
            self.fields.insert(owner.id(), fields);
        }
        &self.fields[&owner.id()]
    }

    fn declared_fields(&self, owner: Node<'t>, content: &str) -> Fields {
        let lang = self.lang;
        let mut fields = Fields::new();
        let mut add = |name: Node<'_>, ty: Option<String>| {
            if let Some(name) = node_text(name, content) {
                merge(&mut fields, name, ty);
            }
        };
        let body = match lang {
            Lang::Go => owner
                .child_by_field_name("type")
                .and_then(|ty| ty.named_child(0))
                .filter(|list| list.kind() == "field_declaration_list"),
            _ => owner.child_by_field_name("body"),
        };
        let Some(body) = body else { return fields };
        let mut members = named_children(body);
        if let Some(extra) = members
            .iter()
            .find(|member| member.kind() == "enum_body_declarations")
        {
            members.extend(named_children(*extra));
        }
        // Record components and C# primary-constructor parameters.
        if let Some(parameters) = owner.child_by_field_name("parameters").or_else(|| {
            (lang == Lang::CSharp)
                .then(|| named_children(owner))
                .and_then(|children| children.into_iter().find(|c| c.kind() == "parameter_list"))
        }) {
            members.extend(named_children(parameters));
        }
        for member in members {
            match (lang, member.kind()) {
                (Lang::Rust, "field_declaration") => {
                    if let (Some(name), Some(ty)) = (
                        member.child_by_field_name("name"),
                        member.child_by_field_name("type"),
                    ) {
                        add(name, self.rust_type(ty, content));
                    }
                }
                (Lang::Go, "field_declaration") => {
                    let ty = member
                        .child_by_field_name("type")
                        .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Go));
                    let mut cursor = member.walk();
                    for name in member.children_by_field_name("name", &mut cursor) {
                        add(name, ty.clone());
                    }
                }
                (Lang::Java, "field_declaration" | "constant_declaration") => {
                    let ty = member
                        .child_by_field_name("type")
                        .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Java));
                    let mut cursor = member.walk();
                    for declarator in member.children_by_field_name("declarator", &mut cursor) {
                        if let Some(name) = declarator.child_by_field_name("name") {
                            add(name, ty.clone());
                        }
                    }
                }
                (Lang::Java, "formal_parameter")
                | (Lang::CSharp, "parameter" | "property_declaration") => {
                    if let (Some(name), Some(ty)) = (
                        member.child_by_field_name("name"),
                        member.child_by_field_name("type"),
                    ) {
                        add(
                            name,
                            node_text(ty, content).and_then(|ty| clean_type_text(ty, lang)),
                        );
                    }
                }
                (Lang::CSharp, "field_declaration") => {
                    let Some(declaration) = named_children(member)
                        .into_iter()
                        .find(|child| child.kind() == "variable_declaration")
                    else {
                        continue;
                    };
                    let ty = declaration
                        .child_by_field_name("type")
                        .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::CSharp));
                    for declarator in named_children(declaration) {
                        if declarator.kind() == "variable_declarator"
                            && let Some(name) = declarator.child_by_field_name("name")
                        {
                            add(name, ty.clone());
                        }
                    }
                }
                (Lang::Cpp, "field_declaration") => {
                    let ty = member
                        .child_by_field_name("type")
                        .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Cpp));
                    let mut cursor = member.walk();
                    for declarator in member.children_by_field_name("declarator", &mut cursor) {
                        if declarator.kind() != "function_declarator"
                            && let Some(name) = cpp_declarator_name(declarator)
                        {
                            add(name, ty.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        // A Rust tuple struct's positional fields: `self.0`.
        if lang == Lang::Rust && body.kind() == "ordered_field_declaration_list" {
            let mut cursor = body.walk();
            for (index, ty) in body.children_by_field_name("type", &mut cursor).enumerate() {
                merge(&mut fields, &index.to_string(), self.rust_type(ty, content));
            }
        }
        fields
    }
}

/// Record `ty` for `name`; a second, different type makes it unknown.
pub(super) fn merge(fields: &mut Fields, name: &str, ty: Option<String>) {
    match fields.get_mut(name) {
        Some(existing) if *existing != ty => *existing = None,
        Some(_) => {}
        None => {
            fields.insert(name.to_owned(), ty);
        }
    }
}
