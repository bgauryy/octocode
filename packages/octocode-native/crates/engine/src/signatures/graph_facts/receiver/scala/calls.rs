//! A Scala call read from its surrounding names: a member call's receiver, and
//! a `def` with no parameter list that a bare name calls.
use super::*;

impl ScalaTerms {
    /// What the term scope says of a member call's identifier `receiver`.
    pub(super) fn observe_receiver(
        &mut self,
        receiver: Node<'_>,
        path: &[Node<'_>],
        content: &str,
    ) -> ScalaCall {
        let mut out = ScalaCall::default();
        let Some(name) = node_text(receiver, content) else {
            return out;
        };
        match self.lookup(path, name, receiver.start_byte(), content) {
            Some(Term::Value(ty) | Term::Declared(ty) | Term::Parameterless(ty)) => {
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
        out
    }

    /// The call fact of the last node of `path`, when naming it calls.
    pub(in crate::signatures::graph_facts) fn collect_parameterless(
        &mut self,
        path: &[Node<'_>],
        content: &str,
        line_index: &crate::signatures::graph_facts::positions::NodePositions<'_>,
        acc: &mut crate::signatures::graph_facts::GraphAccumulator,
        caller: Option<&(String, String)>,
    ) {
        let [ancestors @ .., node] = path else {
            return;
        };
        if let Some(callee) = self.parameterless_call(*node, path, content) {
            crate::signatures::graph_facts::collect::collect_parameterless_call(
                callee, *node, ancestors, content, line_index, acc, caller,
            );
        }
    }

    /// The callee `node`, last of `path` (root first), calls by naming a
    /// `def` with no parameter list that its scope declares: `noArgs`,
    /// `this.noArgs`. A parameter or `val` of that name is read, and a name
    /// found nowhere in scope (an inherited `def`) cannot be told from one.
    fn parameterless_call(
        &mut self,
        node: Node<'_>,
        path: &[Node<'_>],
        content: &str,
    ) -> Option<String> {
        let [.., parent, _] = path else {
            return None;
        };
        if !is_expression_of(node, *parent) {
            return None;
        }
        let (name, qualified) = match node.kind() {
            "identifier" => (node, false),
            "field_expression"
                if node
                    .child_by_field_name("value")
                    .is_some_and(|value| node_text(value, content) == Some("this")) =>
            {
                (node.child_by_field_name("field")?, true)
            }
            _ => return None,
        };
        let text = node_text(name, content)?;
        matches!(
            self.lookup(path, text, name.start_byte(), content),
            Some(Term::Parameterless(_))
        )
        .then(|| {
            if qualified {
                format!("this.{text}")
            } else {
                text.to_owned()
            }
        })
    }
}

/// Whether `node` is evaluated as an expression of `parent`, not a name it
/// declares, a pattern, a named argument or the function of a call.
fn is_expression_of(node: Node<'_>, parent: Node<'_>) -> bool {
    let field = |name: &str| parent.child_by_field_name(name) == Some(node);
    match parent.kind() {
        "block"
        | "indented_block"
        | "arguments"
        | "parenthesized_expression"
        | "tuple_expression"
        | "return_expression"
        | "interpolation" => true,
        "function_definition" => field("body"),
        "val_definition" | "var_definition" => field("value"),
        "infix_expression" => field("left") || field("right"),
        "assignment_expression" => field("right"),
        "match_expression" => field("value"),
        "if_expression" => field("condition") || field("consequence") || field("alternative"),
        _ => false,
    }
}
