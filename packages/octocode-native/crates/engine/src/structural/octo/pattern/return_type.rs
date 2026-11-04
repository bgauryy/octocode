//! The annotated spelling of a pattern: a pattern matches the parts it spells,
//! so `function $N($$$) { $$$ }` does not match a function with a return type.
//! The same pattern with a capture in each function's empty return slot
//! matches exactly those.
//!
//! The grammar decides: a function is any node with a `parameters` field, and
//! a spelling counts only when the grammar parses it cleanly into that node's
//! return field. A grammar without an optional return type (JavaScript, or
//! Java, whose type is required before the name) parses none of them.

use tree_sitter::Node;

use super::CompiledPattern;
use super::qualified::free_capture;
use crate::structural::language::AgLanguage;
use crate::structural::octo::matching::{named_children, node_text};

/// The fields grammars hold a function's return type in. The runtime
/// `Language` lists no fields per node kind, so the names are stated and the
/// re-parse proves which node takes one.
const RETURN_FIELDS: [&str; 2] = ["return_type", "result"];

/// What separates a return type from the parameters before it (`: T`,
/// `-> T`, Go's bare `T`). A `Language` exposes no field's spelling; the first
/// the grammar parses into a return field is the one.
const SEPARATORS: [&str; 3] = [": ", " -> ", " "];

/// The capture name the annotated spelling binds.
const RETURN_CAPTURE: &str = "R";

fn parameters(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    // Curried definitions spell several; the return type follows the last.
    node.children_by_field_name("parameters", &mut cursor)
        .last()
}

fn return_slot(node: Node<'_>) -> Option<Node<'_>> {
    RETURN_FIELDS
        .iter()
        .find_map(|field| node.child_by_field_name(field))
}

/// `pattern` with a fresh capture in the return slot of each of its outermost
/// functions that leave it empty; `None` when it has none the grammar lets it
/// fill. A function nested in another's body stays as spelled.
pub(in crate::structural::octo) fn with_return_types(
    lang: &AgLanguage,
    pattern: &str,
) -> Option<String> {
    let mut text = pattern.to_owned();
    let mut index = 0;
    loop {
        let compiled = CompiledPattern::new(lang, &text).ok()?;
        if compiled.is_special() {
            return None;
        }
        let functions = compiled.functions()?;
        let Some(function) = functions.get(index) else {
            break;
        };
        if return_slot(*function).is_none()
            && let Some(annotated) = compiled.annotated(lang, &text, index, *function)
        {
            text = annotated;
        }
        index += 1;
    }
    (text != pattern).then_some(text)
}

impl CompiledPattern {
    /// The outermost nodes with parameters under the root, in source order.
    fn functions(&self) -> Option<Vec<Node<'_>>> {
        let mut found = Vec::new();
        let mut pending = vec![self.root()?];
        while let Some(node) = pending.pop() {
            if parameters(node).is_some() {
                found.push(node);
            } else {
                pending.extend(named_children(node).into_iter().rev());
            }
        }
        Some(found)
    }

    /// `text` (this pattern's own) with a capture after the parameters of its
    /// `index`th function, spelled as the grammar parses into a return field:
    /// the same root and function kinds, and no syntax error the pattern did
    /// not already have.
    fn annotated(
        &self,
        lang: &AgLanguage,
        text: &str,
        index: usize,
        function: Node<'_>,
    ) -> Option<String> {
        let root = self.root()?;
        let before = self
            .source
            .get(root.start_byte()..parameters(function)?.end_byte())?;
        // The compiled source may wrap the pattern and respells its captures.
        let before = self.expando.restore(before);
        let at = text.find(&before)? + before.len();
        let capture = format!("${}", free_capture(text, RETURN_CAPTURE));
        SEPARATORS.iter().find_map(|separator| {
            let spelled = format!("{}{separator}{capture}{}", &text[..at], &text[at..]);
            let compiled = Self::new(lang, &spelled).ok()?;
            let spelled_root = compiled.root()?;
            let functions = compiled.functions()?;
            let filled = return_slot(*functions.get(index)?)?;
            let holds_capture = compiled
                .expando
                .restore(node_text(filled, &compiled.source))
                .ends_with(&capture);
            (holds_capture
                && spelled_root.kind() == root.kind()
                && functions[index].kind() == function.kind()
                && (root.has_error() || !spelled_root.has_error()))
            .then_some(spelled)
        })
    }
}
