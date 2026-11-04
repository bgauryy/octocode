//! Compiling pattern text into the node a pattern matches, per grammar.

use tree_sitter::{Language, Tree};

use crate::structural::kinds::{named_kind_id, near_kinds};
use crate::structural::language::{
    AgLanguage, WRAP_MARKER, wrap_in_class_body, wrap_in_function_body,
};
use crate::structural::octo::error::CompileError;
use crate::structural::octo::matching::{CandidatePlan, children, node_text, parse_tree};
use crate::structural::octo::metavars::{
    ambiguous_function_body_captures, key_value_pair_capture, meta_from_node, tag_name_capture,
    terminate_block_captures,
};

use super::unsaid::declares;
use super::{CompiledPattern, JSX_TAG_KINDS, SpecialPattern};

mod root;

pub(super) use root::effective_pattern_root;
use root::{
    errors_beyond_captures, is_pattern_wrapper, is_wrap_marker_function, longer_forms,
    parse_errors, stray_error, top_level_nodes, unparsed_pattern_message,
};

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
        if let Some(special) = Self::special(lang, pattern) {
            return Ok(special);
        }
        let language = lang.tree_sitter_language();
        if pattern.len() > 64_000 {
            return Err(CompileError::grammar(
                "structural pattern exceeds 64000 byte limit",
            ));
        }
        let mut text = PatternText::new(lang, pattern);
        let mut tree = parse_tree(&language, &text.source)?;
        if lang.class_wraps() {
            (text, tree) = prefer_plain_parse(lang, &language, pattern, text, tree)?;
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
        (text, tree) = retry_in_context(lang, &language, text, tree)?;
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
        // A recovered parse with text the grammar could not place matches
        // nothing in real source: an error, never an empty answer. A capture in
        // an error is the pattern's own (`$$$` where the grammar wants a node),
        // and the matcher reads it.
        if let Some(stray) = stray_error(root, &source, lang.expando()) {
            return Err(CompileError::grammar(unparsed_pattern_message(
                stray,
                &source,
                lang.expando(),
            )));
        }
        // Several top-level nodes (`a(); b()`) have no single node to match;
        // compiled as a whole file they would silently match only a file
        // starting with that sequence.
        let top_level = top_level_nodes(root);
        if is_pattern_wrapper(root.kind()) && top_level > 1 {
            return Err(CompileError::grammar(format!(
                "invalid structural pattern: it has {top_level} top-level nodes, but a pattern matches one node. Search for one statement, or use a rule with `has`/`inside` to relate them."
            )));
        }
        let is_meta = meta_from_node(root, &source, lang.expando()).is_some();
        let candidate_plan = if is_meta {
            CandidatePlan::Any
        } else {
            CandidatePlan::from_kinds(
                std::iter::once(root.kind().to_owned()).chain(longer_forms(root)),
            )
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

    /// A pattern of a special shape, gated on the grammar actually having the
    /// target node kinds; elsewhere the text compiles as an ordinary pattern.
    fn special(lang: &AgLanguage, pattern: &str) -> Option<Self> {
        let language = lang.tree_sitter_language();
        if let Some(capture) = tag_name_capture(pattern)
            && named_kind_id(&language, "jsx_opening_element").is_some()
        {
            return Some(Self {
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
            return Some(Self {
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
        None
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
                "unknown selector kind '{selector}' for this language grammar{}",
                near_kinds(&language, selector)
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

/// The plain parse of `pattern` over its class-wrapped `text` and `tree`, when
/// the plain one reads it.
///
/// The C# class wrap gives member patterns (methods, properties) real
/// body context, but an expression or statement (`Helper($A)`,
/// `Log($A);`) is not a class member and fails there. Fall back to the
/// plain snippet, whose top-level `global_statement` wrapper
/// `effective_pattern_root` unwraps.
fn prefer_plain_parse(
    lang: &AgLanguage,
    language: &Language,
    pattern: &str,
    mut text: PatternText,
    mut tree: Tree,
) -> Result<(PatternText, Tree), CompileError> {
    let mut plain = PatternText::plain(lang, pattern);
    let mut plain_tree = parse_tree(language, &plain.source)?;
    if !plain.is_terminated() {
        // A bare call needs a terminator to be a statement.
        let terminated = plain.terminated();
        let terminated_tree = parse_tree(language, &terminated.source)?;
        if !effective_pattern_root(terminated_tree.root_node(), &terminated.source).has_error() {
            plain = terminated;
            plain_tree = terminated_tree;
        }
    }
    // A `$$$` block is no block to this grammar until it is terminated.
    if errors_beyond_captures(
        effective_pattern_root(plain_tree.root_node(), &plain.source),
        &plain.source,
        lang.expando(),
    ) && let Some(source) = terminate_block_captures(&plain.source, lang.expando())
    {
        plain = PatternText { source };
        plain_tree = parse_tree(language, &plain.source)?;
    }
    let plain_root = effective_pattern_root(plain_tree.root_node(), &plain.source);
    // `local_function_statement` is the top-level artifact of a
    // member-shaped snippet (`public int $NAME(...) { ... }`): keep
    // the class context for it and whenever the plain parse fails. A
    // lone `$$$` in a block is no statement to the grammar, and the
    // matcher reads it as a capture, so its error is no failure.
    if !errors_beyond_captures(plain_root, &plain.source, lang.expando())
        && !is_pattern_wrapper(plain_root.kind())
        && plain_root.kind() != "local_function_statement"
    {
        text = plain;
        tree = plain_tree;
    }
    Ok((text, tree))
}

/// `text` in the context that reads it, when it does not parse alone.
fn retry_in_context(
    lang: &AgLanguage,
    language: &Language,
    mut text: PatternText,
    mut tree: Tree,
) -> Result<(PatternText, Tree), CompileError> {
    // A class member alone (`$M($$$) { $$$ }`) has no parse at the top level:
    // the grammar leaves an error beside what it recovered, or two nodes.
    // Where the same text is one clean declaration in a class body, that is
    // what it spells.
    let plain_root = effective_pattern_root(tree.root_node(), &text.source);
    if plain_root.has_error()
        || (is_pattern_wrapper(plain_root.kind()) && top_level_nodes(plain_root) > 1)
    {
        let member = text.map(wrap_in_class_body);
        let member_tree = parse_tree(language, &member.source)?;
        let member_root = effective_pattern_root(member_tree.root_node(), &member.source);
        if !member_root.has_error()
            && declares(member_root)
            && member_root
                .child_by_field_name("name")
                .is_none_or(|name| node_text(name, &member.source) != WRAP_MARKER)
        {
            text = member;
            tree = member_tree;
        }
    }
    // A `$$$` at a member or statement position parses in some grammars only
    // when terminated: before another member, or at the end of a C or C++
    // type's body. The terminated text counts when it leaves fewer errors.
    let plain_root = effective_pattern_root(tree.root_node(), &text.source);
    if plain_root.has_error()
        && let Some(source) = terminate_block_captures(&text.source, lang.expando())
    {
        let terminated = PatternText { source };
        let terminated_tree = parse_tree(language, &terminated.source)?;
        let terminated_root =
            effective_pattern_root(terminated_tree.root_node(), &terminated.source);
        if !terminated_root.is_error() && parse_errors(terminated_root) < parse_errors(plain_root) {
            text = terminated;
            tree = terminated_tree;
        }
    }
    Ok((text, tree))
}
