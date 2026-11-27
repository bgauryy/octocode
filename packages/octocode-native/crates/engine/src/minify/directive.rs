//! Comments a tool reads, which compact keeps: build constraints, compiler and
//! checker pragmas, triple-slash references. One shape covers them in every
//! grammar: a comment whose first word is machine syntax, not prose.

/// The comment openers of the linked grammars, longest first.
const OPENERS: &[&str] = &["///", "//!", "//", "/**", "/*!", "/*", "#", ";"];

/// Whether `comment` is read by a tool. Its first word decides:
/// - written against the opener in lower case (`//go:build`, `//nolint`,
///   `//export F`), Go's machine-comment form;
/// - `@word` (`@ts-ignore`, `@flow`, `@__PURE__`), `#word` (a cgo
///   `#include`), `+word` (`+build`, `+kubebuilder:`), `<reference`;
/// - a lower-case `tool:` or `tool-word` (`type: ignore`, `noqa:`,
///   `eslint-disable-next-line`, `prettier-ignore`, `webpackChunkName:`);
/// - the `-*-` of an editor or encoding line (`-*- coding: utf-8 -*-`).
pub(super) fn is_directive(comment: &str) -> bool {
    let body = OPENERS
        .iter()
        .find_map(|opener| comment.strip_prefix(opener))
        .unwrap_or(comment);
    let first_line = body
        .lines()
        .map(|line| line.trim_start_matches(|c: char| c.is_whitespace() || c == '*'))
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let word = first_line.split_whitespace().next().unwrap_or("");
    let lower = word.starts_with(|c: char| c.is_ascii_lowercase());
    let against_opener = !body.starts_with(char::is_whitespace);
    let sigil = |sigil: char| {
        word.strip_prefix(sigil)
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_'))
    };
    let tool_word = lower
        && word
            .trim_end_matches(':')
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && (word.ends_with(':') || word.contains('-'));
    (against_opener && lower)
        || sigil('@')
        || sigil('#')
        || sigil('+')
        || word.starts_with("<reference")
        || word == "-*-"
        || tool_word
}

#[cfg(test)]
mod tests;
