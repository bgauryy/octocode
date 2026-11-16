//! Rust `cfg` predicates that compile an item only under test.

use crate::signatures::node_kinds::kind_of;
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

/// The attribute named `name` a `cfg_attr(predicate, ...)` applies, as written,
/// when its predicate holds only under test.
pub(super) fn cfg_attr_under_test<'t>(inner: &'t str, name: &str) -> Option<&'t str> {
    let args = call_args(inner, "cfg_attr")?;
    let (predicate, attributes) = args.split_first()?;
    requires_test(predicate).then_some(())?;
    attributes.iter().copied().find(|attribute| {
        attribute
            .strip_prefix(name)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

/// The attribute named `name` a `cfg_attr(predicate, ...)` applies, as written,
/// when its predicate is false under test: a build with `--test` leaves the item
/// without it.
pub(super) fn cfg_attr_outside_test<'t>(inner: &'t str, name: &str) -> Option<&'t str> {
    let args = call_args(inner, "cfg_attr")?;
    let (predicate, attributes) = args.split_first()?;
    excludes_test(predicate).then_some(())?;
    attributes.iter().copied().find(|attribute| {
        attribute
            .strip_prefix(name)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

/// Whether the named `children` of a node include an inner `#![cfg(test)]`.
pub(super) fn inner_cfg_requires_test_among(children: &[Node<'_>], content: &str) -> bool {
    children.iter().any(|&child| {
        kind_of(child) == "inner_attribute_item"
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

/// Whether `predicate` is decided by `test` alone and is false under it: plain
/// `not(test)`. Any other cfg beside it (`all(not(test), unix)`) also depends
/// on the host, so on some host the path applies in no build.
fn excludes_test(predicate: &str) -> bool {
    call_args(predicate.trim(), "not")
        .is_some_and(|args| matches!(args.as_slice(), [one] if one.trim() == "test"))
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
