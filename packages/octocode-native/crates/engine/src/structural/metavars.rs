//! Metavariable and capture-name recognition for structural patterns.
//!
//! Pure, leaf-level recognizers that turn tree-sitter nodes and raw pattern
//! text into metavar/capture information (`$X`, `$_`, `$$$BODY`, `<$X>`,
//! `$K: $V`). The match pipeline calls into these; they never depend on the
//! pipeline's own state (`CompiledPattern`, `CaptureEnv`, `CandidatePlan`,
//! `ExecutionError`), only on the shared node/text helpers and the `Expando`
//! leading-character rules.

use tree_sitter::Node;

use super::language::Expando;
use super::octo::{named_children, node_text};

pub(super) fn html_tag_name_capture(pattern: &str) -> Option<String> {
    let trimmed = pattern.trim();
    let inner = trimmed.strip_prefix("<$")?.strip_suffix('>')?;
    if is_capture_name(inner) {
        return Some(inner.to_owned());
    }
    None
}

pub(super) fn key_value_pair_capture(pattern: &str) -> Option<(String, String)> {
    let (left, right) = pattern.trim().split_once(':')?;
    let key_capture = capture_name_from_token(left.trim())?;
    let value_capture = capture_name_from_token(right.trim())?;
    Some((key_capture, value_capture))
}

fn capture_name_from_token(token: &str) -> Option<String> {
    let name = token.strip_prefix('$')?;
    if is_capture_name(name) {
        return Some(name.to_owned());
    }
    None
}

pub(super) fn minimum_candidate_nodes(
    pattern_children: &[Node<'_>],
    source: &str,
    expando: Expando,
) -> usize {
    pattern_children
        .iter()
        .filter(|node| {
            !matches!(
                meta_from_node(**node, source, expando),
                Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti)
            )
        })
        .count()
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum MetaVar {
    Single(String),
    Multi(Option<String>),
    IgnoredSingle,
    IgnoredMulti,
}

/// C++ can parse `{ $$$BODY }` after a function declarator as an initializer
/// list. A statement terminator disambiguates only this sole-capture body;
/// ordinary variable initializers and concrete expression lists stay untouched.
pub(super) fn ambiguous_function_body_capture(
    root: Node<'_>,
    source: &str,
    expando: Expando,
) -> Option<usize> {
    if root.kind() != "declaration" {
        return None;
    }
    let initializer = root.child_by_field_name("declarator")?;
    if initializer.kind() != "init_declarator"
        || initializer.child_by_field_name("declarator")?.kind() != "function_declarator"
    {
        return None;
    }
    let body = initializer.child_by_field_name("value")?;
    if body.kind() != "initializer_list" {
        return None;
    }
    let named = named_children(body);
    let [capture] = named.as_slice() else {
        return None;
    };
    matches!(
        meta_from_node(*capture, source, expando),
        Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti)
    )
    .then_some(capture.end_byte())
}

/// Recover a metavariable from the grammar-specific expando prefix inserted
/// during pattern preprocessing.
pub(super) fn meta_from_node(node: Node<'_>, source: &str, expando: Expando) -> Option<MetaVar> {
    if node.kind() == "expression_statement" {
        let named = named_children(node);
        if let [capture] = named.as_slice() {
            let meta = meta_from_text(node_text(*capture, source), expando);
            if matches!(meta, Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti)) {
                return meta;
            }
        }
    }
    meta_from_text(node_text(node, source), expando)
}

fn meta_from_text(text: &str, expando: Expando) -> Option<MetaVar> {
    let mut chars = text.chars();
    let leading = chars.next()?;
    if !expando.matches_leading(leading) {
        return None;
    }

    let expando_len = text.chars().take_while(|ch| *ch == leading).count();
    let rest: String = text.chars().skip(expando_len).collect();
    match expando_len {
        1 if rest == "_" => Some(MetaVar::IgnoredSingle),
        1 if is_capture_name(&rest) => Some(MetaVar::Single(rest)),
        3 if rest.is_empty() => Some(MetaVar::IgnoredMulti),
        3 if is_capture_name(&rest) => Some(MetaVar::Multi(Some(rest))),
        _ => None,
    }
}

fn is_capture_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit())
}
