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

use crate::signatures::node_kinds::kind_of;
use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

use super::super::scala::is_declaration_value;
use crate::signatures::nodes::{declaration, node_text, scala_applied_function};

mod types;
use types::{type_parameters, type_text};

mod calls;

#[derive(Clone)]
enum Term {
    /// Bound by a binder the graph does not declare.
    Value(Option<String>),
    /// A `val`/`def` the graph declares, with its declared type if written.
    Declared(Option<String>),
    /// A declared `def` with no parameter list: naming it calls it. Its
    /// declared type if written.
    Parameterless(Option<String>),
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
        if kind_of(call) == "infix_expression" {
            return call
                .child_by_field_name("left")
                .filter(|left| kind_of(left) == "identifier")
                .map_or(out, |left| self.observe_receiver(left, path, content));
        }
        let Some(function) = scala_applied_function(call) else {
            return out;
        };
        match kind_of(function) {
            "identifier" => {
                let term = node_text(function, content)
                    .and_then(|name| self.lookup(path, name, function.start_byte(), content));
                out.term_binding = matches!(term, Some(Term::Value(_) | Term::SelfAlias(_)));
            }
            "field_expression" => {
                if let Some(receiver) = function
                    .child_by_field_name("value")
                    .filter(|value| kind_of(value) == "identifier")
                {
                    out = self.observe_receiver(receiver, path, content);
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
    match kind_of(scope) {
        "lambda_expression" => {
            let mut cursor = scope.walk();
            for parameters in scope.children_by_field_name("parameters", &mut cursor) {
                match kind_of(parameters) {
                    "identifier" => value(&mut out, parameters, None, content),
                    "bindings" | "binding" => {
                        let mut inner = parameters.walk();
                        let list = if kind_of(parameters) == "binding" {
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
    if kind_of(clause) != kind {
        return;
    }
    let mut cursor = clause.walk();
    for param in clause
        .named_children(&mut cursor)
        .filter(|param| kind_of(param) == parameter)
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
        .find(|child| kind_of(child) == "self_type")
    else {
        return;
    };
    let Some(name) = alias
        .named_child(0)
        .filter(|name| kind_of(name) == "identifier")
        .and_then(|name| node_text(name, content))
        .filter(|name| !matches!(*name, "this" | "_"))
    else {
        return;
    };
    let template = body.parent().and_then(|holder| {
        let named = match kind_of(holder) {
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
        let from = if block && matches!(kind_of(statement), "val_definition" | "var_definition") {
            statement.end_byte()
        } else {
            0
        };
        let (name, term) = match kind_of(statement) {
            "object_definition" => (statement.child_by_field_name("name"), Term::Object),
            "function_definition" | "function_declaration" | "given_definition" => {
                let parameterless = {
                    let mut inner = statement.walk();
                    !statement
                        .named_children(&mut inner)
                        .any(|child| kind_of(child) == "parameters")
                };
                let ty = parameterless
                    .then(|| type_text(statement.child_by_field_name("return_type"), content))
                    .flatten();
                let term = if declared && parameterless && kind_of(statement) != "given_definition"
                {
                    Term::Parameterless(ty)
                } else {
                    term(ty)
                };
                (statement.child_by_field_name("name"), term)
            }
            "val_declaration" | "var_declaration" => {
                let ty = type_text(statement.child_by_field_name("type"), content);
                (statement.child_by_field_name("name"), Term::Value(ty))
            }
            "val_definition" | "var_definition" => {
                let Some(pattern) = statement.child_by_field_name("pattern") else {
                    continue;
                };
                if kind_of(pattern) != "identifier" {
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
        match kind_of(node) {
            "identifier" => {
                if node_text(node, content).is_some_and(|name| name.starts_with(char::is_lowercase))
                {
                    value(out, node, None, content);
                }
            }
            "typed_pattern" => {
                let ty = type_text(node.child_by_field_name("type"), content);
                match node.child_by_field_name("pattern") {
                    Some(name) if kind_of(name) == "identifier" => value(out, name, ty, content),
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
