//! Which Rust expression macros have arguments the graph reads as code,
//! and queueing a macro call's body for that re-read.
//!
//! A macro's token tree is flat tokens to tree-sitter, so a call inside
//! `ready!(poll_proceed(cx))` is no call node. Re-parsing a body is only
//! sound when the macro evaluates its arguments as expressions in place:
//! a DSL (`async_assert_fn!(f(&str): Send)`), a pattern (`matches!(x,
//! Foo(_))`) or code generation (`quote!`) re-parses into clean call nodes
//! that are no calls. So only the std/core macros known to forward their
//! arguments are admitted, by name; every other macro keeps its flat body.

use tree_sitter::Node;

use super::GraphAccumulator;
use super::macro_bodies::record_macro_body;
use super::positions::NodePositions;
use super::walk::{RustNodeContext, is_item_level};
use crate::graph::GraphParseAssessment;
use crate::signatures::nodes::node_text;

/// How an admitted macro's arguments are read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MacroArgs {
    /// Every argument is an expression evaluated in place (`assert_eq!`,
    /// `println!`, `vec!`, `ready!`).
    Forwarding,
    /// Only the first argument is an expression; the rest is a pattern and
    /// guard (`matches!(scrutinee, Pattern if guard)`).
    Scrutinee,
}

/// Paths an admitted macro may be spelled under: bare (a prelude or `use`d
/// name) or through the crate that defines it.
const QUALIFIERS: &[&str] = &[
    "",
    "std",
    "core",
    "alloc",
    "std::task",
    "core::task",
    "futures",
    "futures_core",
    "futures_util",
];

/// The std/core macros that evaluate their expression arguments in place.
const FORWARDING: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "dbg",
    "print",
    "println",
    "eprint",
    "eprintln",
    "format",
    "format_args",
    "write",
    "writeln",
    "panic",
    "unreachable",
    "todo",
    "unimplemented",
    "vec",
    "ready",
    "try_ready",
];

const SCRUTINEE: &[&str] = &["matches", "assert_matches", "debug_assert_matches"];

/// Nesting bound for expression macros inside expression macros.
pub(super) const MAX_MACRO_DEPTH: usize = 16;

/// How the macro spelled `path` (`ready`, `std::task::ready`, `::std::vec`)
/// reads its arguments; `None` for a macro whose arguments stay opaque.
pub(super) fn admit(path: &str) -> Option<MacroArgs> {
    let path = path.strip_prefix("::").unwrap_or(path);
    let (qualifier, name) = path.rsplit_once("::").unwrap_or(("", path));
    if !QUALIFIERS.contains(&qualifier) {
        return None;
    }
    if FORWARDING.contains(&name) {
        Some(MacroArgs::Forwarding)
    } else if SCRUTINEE.contains(&name) {
        Some(MacroArgs::Scrutinee)
    } else {
        None
    }
}

/// Whether a bare macro name of `chain` is also a name this file binds to
/// another macro (`declared`: its `macro_rules!` names and the local names
/// of its [`foreign_use`]s): that macro, not the std one, would expand there.
pub(super) fn shadowed<'a>(chain: &[String], mut declared: impl Iterator<Item = &'a str>) -> bool {
    let bare = chain
        .iter()
        .filter(|path| !path.contains("::"))
        .collect::<Vec<_>>();
    !bare.is_empty() && declared.any(|name| bare.iter().any(|path| path.as_str() == name))
}

/// Whether a `use` of `specifier` may bind a macro other than an admitted
/// std one under its local name: `use my_ready as ready`, `use
/// crate::m::ready`, `use pretty_assertions::assert_eq`.
pub(super) fn foreign_use(specifier: &str) -> bool {
    admit(specifier).is_none()
}

/// Queue a Rust macro call's body for re-reading. Only item-level macro
/// calls can generate `mod`/`use` items; their bodies are re-read as items.
/// Elsewhere only an admitted expression macro's arguments are re-read, for
/// the calls they hold ([`admit`]); a body re-parses clean
/// on its own, so it carries the recovery its macro call sits in.
pub(super) fn record_rust_macro(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
    active_name: Option<&str>,
    rust: &RustNodeContext<'_>,
) {
    let container = || {
        active_decl
            .zip(active_name)
            .map(|(id, name)| (id.to_owned(), name.to_owned()))
    };
    if acc.macro_chain.is_none() && is_item_level(node) {
        record_macro_body(node, acc, container(), rust.module_scope, None);
        return;
    }
    if acc.declarations_only || acc.macro_chain.as_ref().map_or(0, Vec::len) >= MAX_MACRO_DEPTH {
        return;
    }
    let Some(path) = node
        .child_by_field_name("macro")
        .and_then(|name| node_text(name, content))
        .map(|text| text.split_whitespace().collect::<String>())
    else {
        return;
    };
    if let Some(args) = admit(&path) {
        let recovered = acc.macro_recovered.clone().or_else(|| {
            let assessment = super::validity::assess(node, ancestors, line_index);
            matches!(assessment, GraphParseAssessment::Recovered { .. }).then_some(assessment)
        });
        record_macro_body(
            node,
            acc,
            container(),
            rust.module_scope,
            Some((path, args, recovered)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_keys_on_the_last_segment_and_a_known_qualifier() {
        assert_eq!(admit("ready"), Some(MacroArgs::Forwarding));
        assert_eq!(admit("std::task::ready"), Some(MacroArgs::Forwarding));
        assert_eq!(admit("::std::vec"), Some(MacroArgs::Forwarding));
        assert_eq!(admit("core::assert_eq"), Some(MacroArgs::Forwarding));
        assert_eq!(admit("matches"), Some(MacroArgs::Scrutinee));
        assert_eq!(admit("tokio::ready"), None);
        assert_eq!(admit("tracing::info"), None);
        assert_eq!(admit("async_assert_fn"), None);
        assert_eq!(admit("quote"), None);
        assert_eq!(admit("stringify"), None);
        assert_eq!(admit("assert_ready"), None);
    }

    #[test]
    fn only_bare_names_are_shadowed() {
        let chain = |paths: &[&str]| paths.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>();
        assert!(shadowed(
            &chain(&["assert", "ready"]),
            ["ready"].into_iter()
        ));
        assert!(!shadowed(
            &chain(&["std::task::ready"]),
            ["ready"].into_iter()
        ));
        assert!(!shadowed(&chain(&["ready"]), ["other"].into_iter()));
    }

    #[test]
    fn only_std_paths_bind_an_admitted_name() {
        assert!(!foreign_use("std::task::ready"));
        assert!(!foreign_use("futures::ready"));
        assert!(foreign_use("my_ready"));
        assert!(foreign_use("crate::m::ready"));
        assert!(foreign_use("pretty_assertions::assert_eq"));
    }
}
