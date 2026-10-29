//! Scala term scope for call facts.
//!
//! The head of a call (`f` in `f(x)`, `G` in `G.tailRecM(x)`) is looked up
//! in the term namespace, nearest scope first: lambda, `case` and `for`
//! bindings, the parameters of each enclosing method and class (closures and
//! nested defs capture them), `val`/`def`/`object` statements of enclosing
//! blocks (a `val` only after its definition), and the members a template
//! body declares itself. Type parameters live in the type namespace and never
//! shadow a term (`def F: Monad[F]` binds the term `F`).
//!
//! - A receiver bound to a term with a readable declared type gets that type
//!   without its arguments (`Monad[G]` → `Monad`), unless the type's head is
//!   a type parameter in scope (`fa: F[Int]`).
//! - A head bound to a term the graph does not declare (a parameter, a lambda
//!   or pattern variable, an abstract `val`), or a receiver whose term has no
//!   usable type, is a term binding: it never names a type, an object or a
//!   declaration elsewhere.
//!
//! - A self alias (`trait T { self => … }`) names the template that declares
//!   it, wherever the call is nested: `self.m()` inside an inner class or
//!   anonymous class is a member of the outer template. An alias on an
//!   anonymous class that no declaration holds names nothing.
//!
//! Members a template inherits are not visible here; the linker resolves
//! them through the template's bases.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

use super::super::scala::is_declaration_value;
use crate::signatures::nodes::{declaration, node_text, scala_applied_function};

mod types;
use types::{type_parameters, type_text};

#[derive(Clone)]
enum Term {
    /// Bound by a binder the graph does not declare.
    Value(Option<String>),
    /// A `val`/`def` the graph declares, with its declared type if written.
    Declared(Option<String>),
    Object,
    /// A self alias, with the name of the template that declares it.
    SelfAlias(Option<String>),
}

struct Binder {
    name: String,
    term: Term,
    /// First byte where the binding is visible.
    from: usize,
}

/// What the term scope says about one call's head.
#[derive(Default)]
pub(in crate::signatures::graph_facts) struct ScalaCall {
    pub receiver_type: Option<String>,
    pub term_binding: bool,
    pub self_template: Option<String>,
}

/// Per-tree Scala term scopes, cached by scope node.
#[derive(Default)]
pub(in crate::signatures::graph_facts) struct ScalaTerms {
    scopes: HashMap<usize, Vec<Binder>>,
}

impl ScalaTerms {
    /// `path` holds the ancestors of `call` and `call` itself, root first.
    pub(in crate::signatures::graph_facts) fn observe(
        &mut self,
        call: Node<'_>,
        path: &[Node<'_>],
        content: &str,
    ) -> ScalaCall {
        let mut out = ScalaCall::default();
        let Some(function) = scala_applied_function(call) else {
            return out;
        };
        match function.kind() {
            "identifier" => {
                let term = node_text(function, content)
                    .and_then(|name| self.lookup(path, name, function.start_byte(), content));
                out.term_binding = matches!(term, Some(Term::Value(_) | Term::SelfAlias(_)));
            }
            "field_expression" => {
                let Some(receiver) = function
                    .child_by_field_name("value")
                    .filter(|value| value.kind() == "identifier")
                else {
                    return out;
                };
                let Some(name) = node_text(receiver, content) else {
                    return out;
                };
                match self.lookup(path, name, receiver.start_byte(), content) {
                    Some(Term::Value(ty) | Term::Declared(ty)) => {
                        let params = type_parameters(path, content);
                        out.receiver_type = ty.filter(|ty| {
                            let head = ty.split('.').next().unwrap_or(ty);
                            !params.contains(head)
                        });
                        out.term_binding = out.receiver_type.is_none();
                    }
                    Some(Term::SelfAlias(template)) => {
                        out.term_binding = template.is_none();
                        out.self_template = template;
                    }
                    Some(Term::Object) | None => {}
                }
            }
            _ => {}
        }
        out
    }

    fn lookup(&mut self, path: &[Node<'_>], name: &str, at: usize, content: &str) -> Option<Term> {
        for scope in path.iter().rev() {
            let binders = self
                .scopes
                .entry(scope.id())
                .or_insert_with(|| binders(*scope, content));
            if let Some(binder) = binders
                .iter()
                .find(|binder| binder.name == name && binder.from <= at)
            {
                return Some(binder.term.clone());
            }
        }
        None
    }
}

/// The terms `scope` binds for code inside it.
fn binders(scope: Node<'_>, content: &str) -> Vec<Binder> {
    let mut out = Vec::new();
    match scope.kind() {
        "lambda_expression" => {
            let mut cursor = scope.walk();
            for parameters in scope.children_by_field_name("parameters", &mut cursor) {
                match parameters.kind() {
                    "identifier" => value(&mut out, parameters, None, content),
                    "bindings" | "binding" => {
                        let mut inner = parameters.walk();
                        let list = if parameters.kind() == "binding" {
                            vec![parameters]
                        } else {
                            parameters.named_children(&mut inner).collect()
                        };
                        for binding in list {
                            if let Some(name) = binding.child_by_field_name("name") {
                                let ty = binding.child_by_field_name("type");
                                value(&mut out, name, type_text(ty, content), content);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        "case_clause" => {
            if let Some(pattern) = scope.child_by_field_name("pattern") {
                pattern_binders(pattern, content, &mut out);
            }
        }
        "for_expression" => {
            if let Some(enumerators) = scope.child_by_field_name("enumerators") {
                let mut cursor = enumerators.walk();
                for enumerator in enumerators.named_children(&mut cursor) {
                    if let Some(pattern) = enumerator.named_child(0) {
                        pattern_binders(pattern, content, &mut out);
                    }
                }
            }
        }
        "function_definition" | "function_declaration" | "given_definition" => {
            let mut cursor = scope.walk();
            for clause in scope.named_children(&mut cursor) {
                parameter_clause(clause, "parameters", "parameter", content, &mut out);
            }
        }
        "class_definition" | "trait_definition" | "enum_definition" => {
            let mut cursor = scope.walk();
            for clause in scope.named_children(&mut cursor) {
                parameter_clause(
                    clause,
                    "class_parameters",
                    "class_parameter",
                    content,
                    &mut out,
                );
            }
        }
        "template_body" | "with_template_body" => {
            self_alias(scope, content, &mut out);
            statements(scope, false, content, &mut out);
        }
        "compilation_unit" => statements(scope, false, content, &mut out),
        "block" | "indented_block" => statements(scope, true, content, &mut out),
        _ => {}
    }
    out
}

fn parameter_clause(
    clause: Node<'_>,
    kind: &str,
    parameter: &str,
    content: &str,
    out: &mut Vec<Binder>,
) {
    if clause.kind() != kind {
        return;
    }
    let mut cursor = clause.walk();
    for param in clause
        .named_children(&mut cursor)
        .filter(|param| param.kind() == parameter)
    {
        if let Some(name) = param.child_by_field_name("name") {
            let ty = type_text(param.child_by_field_name("type"), content);
            value(out, name, ty, content);
        }
    }
}

/// The alias a template body declares (`self =>`, `self: T =>`), bound to
/// the template's name. `this: T =>` declares no alias.
fn self_alias(body: Node<'_>, content: &str, out: &mut Vec<Binder>) {
    let mut cursor = body.walk();
    let Some(alias) = body
        .named_children(&mut cursor)
        .find(|child| child.kind() == "self_type")
    else {
        return;
    };
    let Some(name) = alias
        .named_child(0)
        .filter(|name| name.kind() == "identifier")
        .and_then(|name| node_text(name, content))
        .filter(|name| !matches!(*name, "this" | "_"))
    else {
        return;
    };
    let template = body.parent().and_then(|holder| {
        let named = match holder.kind() {
            "instance_expression" => {
                let parent = holder.parent();
                is_declaration_value(holder, parent, content).then_some(parent?)?
            }
            _ => holder,
        };
        let (_, name) = declaration(named, content)?;
        node_text(name, content).map(str::to_owned)
    });
    out.push(Binder {
        name: name.to_owned(),
        term: Term::SelfAlias(template),
        from: 0,
    });
}

/// Members of a template body or statements of a block. In a block a `val`
/// is visible after its definition; every other binder from the start.
fn statements(scope: Node<'_>, block: bool, content: &str, out: &mut Vec<Binder>) {
    let mut cursor = scope.walk();
    for statement in scope.named_children(&mut cursor) {
        let declared = declaration(statement, content).is_some();
        let term = |ty: Option<String>| {
            if declared {
                Term::Declared(ty)
            } else {
                Term::Value(ty)
            }
        };
        let from = if block && matches!(statement.kind(), "val_definition" | "var_definition") {
            statement.end_byte()
        } else {
            0
        };
        let (name, term) = match statement.kind() {
            "object_definition" => (statement.child_by_field_name("name"), Term::Object),
            "function_definition" | "function_declaration" | "given_definition" => {
                let parameterless = {
                    let mut inner = statement.walk();
                    !statement
                        .named_children(&mut inner)
                        .any(|child| child.kind() == "parameters")
                };
                let ty = parameterless
                    .then(|| type_text(statement.child_by_field_name("return_type"), content))
                    .flatten();
                (statement.child_by_field_name("name"), term(ty))
            }
            "val_declaration" | "var_declaration" => {
                let ty = type_text(statement.child_by_field_name("type"), content);
                (statement.child_by_field_name("name"), Term::Value(ty))
            }
            "val_definition" | "var_definition" => {
                let Some(pattern) = statement.child_by_field_name("pattern") else {
                    continue;
                };
                if pattern.kind() != "identifier" {
                    let start = out.len();
                    pattern_binders(pattern, content, out);
                    for binder in &mut out[start..] {
                        binder.from = from;
                    }
                    continue;
                }
                let ty = type_text(statement.child_by_field_name("type"), content);
                (Some(pattern), term(ty))
            }
            _ => continue,
        };
        if let Some(name) = name.and_then(|name| node_text(name, content)) {
            out.push(Binder {
                name: name.to_owned(),
                term,
                from,
            });
        }
    }
}

/// Variables a pattern binds: lower-case identifiers (an upper-case one is a
/// stable identifier), typed with `x: T`.
fn pattern_binders(pattern: Node<'_>, content: &str, out: &mut Vec<Binder>) {
    let mut pending = vec![pattern];
    while let Some(node) = pending.pop() {
        match node.kind() {
            "identifier" => {
                if node_text(node, content).is_some_and(|name| name.starts_with(char::is_lowercase))
                {
                    value(out, node, None, content);
                }
            }
            "typed_pattern" => {
                let ty = type_text(node.child_by_field_name("type"), content);
                match node.child_by_field_name("pattern") {
                    Some(name) if name.kind() == "identifier" => value(out, name, ty, content),
                    Some(inner) => pending.push(inner),
                    None => {}
                }
            }
            _ => {
                let mut cursor = node.walk();
                let types = node
                    .children_by_field_name("type", &mut cursor)
                    .map(|child| child.id())
                    .collect::<HashSet<_>>();
                let mut cursor = node.walk();
                pending.extend(
                    node.named_children(&mut cursor)
                        .filter(|child| !types.contains(&child.id())),
                );
            }
        }
    }
}

fn value(out: &mut Vec<Binder>, name: Node<'_>, ty: Option<String>, content: &str) {
    if let Some(name) = node_text(name, content) {
        out.push(Binder {
            name: name.to_owned(),
            term: Term::Value(ty),
            from: 0,
        });
    }
}
