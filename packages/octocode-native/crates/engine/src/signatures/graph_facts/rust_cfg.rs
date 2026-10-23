//! Rust `cfg` predicates that compile an item only under test.

use tree_sitter::Node;

use crate::signatures::nodes::node_text;

/// Whether the attribute body `inner` (the text between `#[` / `#![` and
/// `]`) is a `cfg` whose predicate holds only when `test` is set:
/// `cfg(test)`, `cfg(all(test, …))`, `cfg(any(test, all(test, …)))`.
/// `not(test)` and `any(test, x)` also compile outside tests.
pub(super) fn cfg_requires_test(inner: &str) -> bool {
    call_args(inner, "cfg")
        .is_some_and(|args| matches!(args.as_slice(), [one] if requires_test(one)))
}

/// Whether `node` holds an inner `#![cfg(test)]` among its named children.
pub(super) fn inner_cfg_requires_test(node: Node<'_>, content: &str) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).any(|child| {
        child.kind() == "inner_attribute_item"
            && node_text(child, content)
                .and_then(|text| text.trim().strip_prefix("#!["))
                .and_then(|text| text.strip_suffix(']'))
                .is_some_and(cfg_requires_test)
    })
}

fn requires_test(predicate: &str) -> bool {
    let predicate = predicate.trim();
    if predicate == "test" {
        return true;
    }
    if let Some(args) = call_args(predicate, "all") {
        return args.iter().any(|arg| requires_test(arg));
    }
    if let Some(args) = call_args(predicate, "any") {
        return !args.is_empty() && args.iter().all(|arg| requires_test(arg));
    }
    false
}

/// The top-level, comma-separated arguments of `name(…)`, or `None` when
/// `text` is not that call.
fn call_args<'t>(text: &'t str, name: &str) -> Option<Vec<&'t str>> {
    let body = text
        .trim()
        .strip_prefix(name)?
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    let mut args = Vec::new();
    let (mut depth, mut quoted, mut start) = (0_u32, false, 0);
    for (at, ch) in body.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth = depth.saturating_sub(1),
            ',' if !quoted && depth == 0 => {
                args.push(&body[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    args.push(&body[start..]);
    Some(
        args.into_iter()
            .map(str::trim)
            .filter(|arg| !arg.is_empty())
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::cfg_requires_test;

    #[test]
    fn only_test_only_predicates_require_test() {
        for gated in [
            "cfg(test)",
            "cfg( test )",
            "cfg(all(test, feature = \"x\"))",
            "cfg(any(test, all(test, unix)))",
        ] {
            assert!(cfg_requires_test(gated), "{gated}");
        }
        for open in [
            "cfg(not(test))",
            "cfg(any(test, feature = \"x\"))",
            "cfg(feature = \"test\")",
            "cfg(unix)",
            "cfg_attr(test, allow(dead_code))",
            "allow(dead_code)",
            "cfg()",
        ] {
            assert!(!cfg_requires_test(open), "{open}");
        }
    }
}
