//! Compiling pattern text into the node a pattern matches, per grammar.

use tree_sitter::Node;

use crate::structural::kinds::named_kind_id;
use crate::structural::language::{AgLanguage, Expando, WRAP_MARKER, wrap_in_function_body};
use crate::structural::octo::error::CompileError;
use crate::structural::octo::matching::{
    CandidatePlan, children, named_children, node_text, parse_tree,
};
use crate::structural::octo::metavars::{
    ambiguous_function_body_captures, key_value_pair_capture, meta_from_node, tag_name_capture,
};

use super::{CompiledPattern, JSX_TAG_KINDS, SpecialPattern};

/// Pattern text with metavariables substituted for the selected grammar.
#[derive(Clone)]
struct PatternText {
    source: String,
}

impl PatternText {
    fn new(lang: &AgLanguage, pattern: &str) -> Self {
        Self {
            source: lang.preprocess_pattern(pattern).into_owned(),
        }
    }

    fn plain(lang: &AgLanguage, pattern: &str) -> Self {
        Self {
            source: lang.preprocess_metavariables(pattern).into_owned(),
        }
    }

    fn is_terminated(&self) -> bool {
        self.source.trim_end().ends_with([';', '}'])
    }

    fn terminated(&self) -> Self {
        self.map(|text| format!("{text};"))
    }

    fn map(&self, edit: impl Fn(&str) -> String) -> Self {
        Self {
            source: edit(&self.source),
        }
    }

    /// Insert `ch` at byte `offset` of the parser input.
    fn insert(&mut self, offset: usize, ch: char) {
        self.source.insert(offset, ch);
    }
}

impl CompiledPattern {
    pub(in crate::structural::octo) fn new(
        lang: &AgLanguage,
        pattern: &str,
    ) -> Result<Self, CompileError> {
        // Special shapes are gated on the grammar actually having the target
        // node kinds; elsewhere the text compiles as an ordinary pattern.
        let language = lang.tree_sitter_language();
        if let Some(capture) = tag_name_capture(pattern)
            && named_kind_id(&language, "jsx_opening_element").is_some()
        {
            return Ok(Self {
                language,
                expando: lang.expando(),
                source: pattern.to_owned(),
                tree: None,
                selected_node: None,
                special: Some(SpecialPattern::JsxTagName {
                    capture: capture.to_owned(),
                }),
                candidate_plan: CandidatePlan::from_kinds(JSX_TAG_KINDS.map(str::to_owned)),
            });
        }

        if let Some((key_capture, value_capture)) = key_value_pair_capture(pattern)
            && named_kind_id(&language, "pair").is_some()
        {
            return Ok(Self {
                language,
                expando: lang.expando(),
                source: pattern.to_owned(),
                tree: None,
                selected_node: None,
                special: Some(SpecialPattern::KeyValuePair {
                    key_capture: key_capture.to_owned(),
                    value_capture: value_capture.to_owned(),
                }),
                candidate_plan: CandidatePlan::from_kind("pair"),
            });
        }

        if pattern.len() > 64_000 {
            return Err(CompileError::grammar(
                "structural pattern exceeds 64000 byte limit",
            ));
        }
        let mut text = PatternText::new(lang, pattern);
        let mut tree = parse_tree(&language, &text.source)?;
        // The C# class wrap gives member patterns (methods, properties) real
        // body context, but an expression or statement (`Helper($A)`,
        // `Log($A);`) is not a class member and fails there. Fall back to the
        // plain snippet, whose top-level `global_statement` wrapper
        // `effective_pattern_root` unwraps.
        if lang.class_wraps() {
            let mut plain = PatternText::plain(lang, pattern);
            let mut plain_tree = parse_tree(&language, &plain.source)?;
            if !plain.is_terminated() {
                // A bare call needs a terminator to be a statement.
                let terminated = plain.terminated();
                let terminated_tree = parse_tree(&language, &terminated.source)?;
                if !effective_pattern_root(terminated_tree.root_node(), &terminated.source)
                    .has_error()
                {
                    plain = terminated;
                    plain_tree = terminated_tree;
                }
            }
            let plain_root = effective_pattern_root(plain_tree.root_node(), &plain.source);
            // `local_function_statement` is the top-level artifact of a
            // member-shaped snippet (`public int $NAME(...) { ... }`): keep
            // the class context for it and whenever the plain parse fails.
            if !plain_root.has_error() && plain_root.kind() != "local_function_statement" {
                text = plain;
                tree = plain_tree;
            }
        }
        // Parse fragments once, at compilation, for both direct patterns and
        // every pattern nested in a rule. A grammar-checked terminator supplies
        // statement/declaration context without depending on source matches:
        // a call takes it whatever the plain parse made of it (Go reads a
        // bare call as a conversion), and any other fragment (`$A`, `$A?`,
        // `$X.await`, `$A as $T`, `&mut $A`) takes it when the plain snippet
        // does not parse. The statement wrapper unwraps, so `$A;` is `$A`.
        // Complete constructs and unrelated shapes retain their original tree.
        if let Some(kind) = lang.terminated_fragment_kind()
            && !text.is_terminated()
        {
            let contextual = text.terminated();
            let contextual_tree = parse_tree(&language, &contextual.source)?;
            let contextual_root =
                effective_pattern_root(contextual_tree.root_node(), &contextual.source);
            let plain_fails = effective_pattern_root(tree.root_node(), &text.source).has_error();
            if !contextual_root.has_error() && (contextual_root.kind() == kind || plain_fails) {
                text = contextual;
                tree = contextual_tree;
            }
        }
        // A statement only parses cleanly inside a function body (see
        // `AgLanguage::c_family`): reparse a snippet that fails
        // alone as the sole statement of a synthetic function, terminated if
        // it needs it. Declarations, which parse at top level, keep their tree.
        if lang.is_c_family() && effective_pattern_root(tree.root_node(), &text.source).has_error()
        {
            let statements = std::iter::once(text.clone())
                .chain((!text.is_terminated()).then(|| text.terminated()));
            for statement in statements {
                let wrapped = statement.map(wrap_in_function_body);
                let wrapped_tree = parse_tree(&language, &wrapped.source)?;
                let wrapped_root =
                    effective_pattern_root(wrapped_tree.root_node(), &wrapped.source);
                if !wrapped_root.has_error()
                    && !is_wrap_marker_function(wrapped_root, &wrapped.source)
                {
                    text = wrapped;
                    tree = wrapped_tree;
                    break;
                }
            }
        }
        let ambiguous_bodies = ambiguous_function_body_captures(
            effective_pattern_root(tree.root_node(), &text.source),
            &text.source,
            lang.expando(),
        );
        if !ambiguous_bodies.is_empty() {
            let mut contextual = text.clone();
            for offset in ambiguous_bodies.iter().rev() {
                contextual.insert(*offset, ';');
            }
            let contextual_tree = parse_tree(&language, &contextual.source)?;
            let contextual_root =
                effective_pattern_root(contextual_tree.root_node(), &contextual.source);
            let resolved = ambiguous_function_body_captures(
                contextual_root,
                &contextual.source,
                lang.expando(),
            )
            .is_empty();
            let no_new_errors = !contextual_root.has_error()
                || effective_pattern_root(tree.root_node(), &text.source).has_error();
            if resolved && no_new_errors {
                text = contextual;
                tree = contextual_tree;
            }
        }
        let PatternText { source } = text;
        let root = effective_pattern_root(tree.root_node(), &source);
        if root.is_error() {
            return Err(CompileError::grammar(unparsed_pattern_message(
                root,
                &source,
                lang.expando(),
            )));
        }
        // Several top-level nodes (`a(); b()`) have no single node to match;
        // compiled as a whole file they would silently match only a file
        // starting with that sequence.
        let top_level = named_children(root)
            .into_iter()
            .filter(|node| !node.is_extra())
            .count();
        if is_pattern_wrapper(root.kind()) && top_level > 1 {
            return Err(CompileError::grammar(format!(
                "invalid structural pattern: it has {top_level} top-level nodes, but a pattern matches one node. Search for one statement, or use a rule with `has`/`inside` to relate them."
            )));
        }
        let is_meta = meta_from_node(root, &source, lang.expando()).is_some();
        let candidate_plan = if is_meta {
            CandidatePlan::Any
        } else {
            CandidatePlan::from_kind(root.kind())
        };
        Ok(Self {
            language,
            expando: lang.expando(),
            source,
            tree: Some(tree),
            selected_node: None,
            special: None,
            candidate_plan,
        })
    }

    pub(in crate::structural::octo) fn contextual(
        lang: &AgLanguage,
        context: &str,
        selector: &str,
    ) -> Result<Self, CompileError> {
        if context.len() > 64_000 {
            return Err(CompileError::rule(
                "structural pattern exceeds 64000 byte limit",
            ));
        }
        let language = lang.tree_sitter_language();
        let kind = named_kind_id(&language, selector).ok_or_else(|| {
            CompileError::grammar(format!(
                "unknown selector kind '{selector}' for this language grammar"
            ))
        })?;
        let source = lang.preprocess_metavariables(context).into_owned();
        let tree = parse_tree(&language, &source)?;
        if tree.root_node().has_error() {
            return Err(CompileError::grammar(
                "contextual pattern must parse without syntax errors",
            ));
        }
        let mut pending = vec![tree.root_node()];
        let mut selected = None;
        while let Some(node) = pending.pop() {
            if node.kind_id() == kind {
                selected = Some((node.start_byte(), node.end_byte(), node.kind_id()));
                break;
            }
            pending.extend(children(node).into_iter().rev());
        }
        let selected_node = selected.ok_or_else(|| {
            CompileError::grammar(format!("context contains no '{selector}' node"))
        })?;
        Ok(Self {
            language,
            expando: lang.expando(),
            source,
            tree: Some(tree),
            special: None,
            candidate_plan: CandidatePlan::from_kind(selector),
            selected_node: Some(selected_node),
        })
    }
}

/// Whether `node` is the synthetic C-family function a statement pattern is
/// parsed inside (see `AgLanguage::c_family`), named [`WRAP_MARKER`].
fn is_wrap_marker_function(node: Node<'_>, source: &str) -> bool {
    node.kind() == "function_definition"
        && node
            .child_by_field_name("declarator")
            .and_then(|declarator| declarator.child_by_field_name("declarator"))
            .is_some_and(|name| node_text(name, source) == WRAP_MARKER)
}

/// The node a pattern matches: its parse with every synthetic or
/// single-child wrapper removed. Wrappers we add are recognised by
/// [`WRAP_MARKER`] (rather than by kind, like the grammar's own wrappers), so
/// a real `class $NAME { ... }` or function pattern keeps its outer node.
pub(super) fn effective_pattern_root<'a>(mut node: Node<'a>, source: &str) -> Node<'a> {
    loop {
        let named = named_children(node);
        if named.len() == 1 && is_pattern_wrapper(node.kind()) {
            node = named[0];
            continue;
        }
        // C-family synthetic function: unwrap `void __OctoWrap(void) {
        // <statement> }` down to its single statement.
        if is_wrap_marker_function(node, source)
            && let Some(body) = node.child_by_field_name("body")
        {
            let body_named = named_children(body);
            if body_named.len() == 1 {
                node = body_named[0];
                continue;
            }
        }
        // C#'s synthetic wrapper class: unwrap `class __OctoWrap { <member> }`
        // down to the single real member, giving it real class-body context
        // (a bare `public int Foo(...) { ... }` parsed standalone isn't valid
        // C# at all — no top-level member/method syntax exists outside a
        // type body).
        if node.kind() == "class_declaration" {
            let is_wrapper = node
                .child_by_field_name("name")
                .is_some_and(|n| node_text(n, source) == WRAP_MARKER);
            if is_wrapper && let Some(body) = node.child_by_field_name("body") {
                let body_named = named_children(body);
                if body_named.len() == 1 {
                    node = body_named[0];
                    continue;
                }
            }
        }
        break node;
    }
}

fn is_pattern_wrapper(kind: &str) -> bool {
    matches!(
        kind,
        "program"
            | "source_file"
            | "module"
            | "compilation_unit"
            | "translation_unit"
            | "stylesheet"
            | "fragment"
            | "document"
            | "expression_statement"
            // C# wraps every top-level statement of a snippet in one.
            | "global_statement"
            | "config_file"
            | "body"
    )
}

/// The rejection for a pattern whose root did not parse. It names the last
/// token the parser could not place (`$$$BODY` in `fn $N() $$$BODY`, where a
/// function needs its body), so the caller sees what to complete.
fn unparsed_pattern_message(root: Node<'_>, source: &str, expando: Expando) -> String {
    const BASE: &str = "invalid structural pattern: pattern parsed with syntax errors";
    let near = children(root)
        .into_iter()
        .rev()
        .find(|node| !node.is_extra())
        .map(|node| expando.restore(node_text(node, source).trim()))
        .filter(|text| !text.is_empty());
    match near {
        Some(text) => format!("{BASE} near `{text}`"),
        None => BASE.to_owned(),
    }
}
