//! The known secret formats: one ordered list of provider token shapes,
//! private-key armors, connection strings and well-known credential keys
//! with a fixed value shape. Plain data, also read by the engine build script
//! to derive the detector's prescan gate. Each child module holds one
//! contiguous slice of the order; [`PATTERNS`] joins them.
//!
//! A pattern names a format, never a guess: no rule reads a generic
//! `…Key`/`…Token`/`…Secret` name or judges a value by its length or look.
//! A match redacts its `secret` capture group (or `secret_…`, one per branch
//! of an alternation) when it has one, else the whole match: an assignment
//! keeps its name, quotes and operator. A match whose `template` group took
//! part hides nothing: that branch spells a value that only stands for a
//! secret (`url_password!`).
//!
//! A pattern's left word boundary keeps a shape from matching inside a longer
//! word. A token a vendor opens with its own tag (`ghp_`, `glpat-`, `AKIA`) is
//! a secret wherever it starts, glued to letters, digits or `_` (minified
//! text, `Bearerghp_…`), so its pattern spells no left boundary; a token of
//! fixed length likewise spells no trailing `\b`. AWS key IDs keep theirs:
//! dropping it adds matches inside base64.

/// The header lines an armor may carry between its BEGIN line and its body
/// (PEM's `Proc-Type:` and `DEK-Info:`, PGP's `Version:` and `Comment:`, RFC
/// 4716's `Comment:` continued by a closing `\`), then the blank line that
/// ends them: they name the key and hold none of it, so a body's `secret`
/// group starts after them.
macro_rules! armor_headers {
    () => {
        r"(?:[ \t]*[A-Za-z][A-Za-z0-9_-]*:(?:[^\r\n]*\\\r?\n)*[^\r\n]*\r?\n)*(?:[ \t]*\r?\n)?"
    };
}

/// A URL's userinfo password: the `template` branch when it is wholly an
/// interpolation or a template placeholder (`${VAR}`, an environment
/// variable's `$VAR` in capitals even where the pattern ignores case, `{{…}}`,
/// `<…>`, `%s`, `%(name)s`), which names where the secret comes from and
/// holds none of it, else `secret`, spelled by `$secret` (a class and its
/// repetition). The URL's `@` follows, so a password that holds such text
/// beside other characters is a secret.
macro_rules! url_password {
    ($secret:literal) => {
        concat!(
            r###"(?:(?P<template>\$\{[^{}@\s]+\}|(?-i:\$[A-Z_][A-Z0-9_]*)|\{\{[^{}@\r\n]+\}\}|<[^<>@\r\n]+>|%s|%\([A-Za-z_][A-Za-z0-9_]*\)s)|(?P<secret>"###,
            $secret,
            r###"))@"###
        )
    };
}

mod ai_and_analytics;
mod auth_and_cloud;
mod dev_tooling;
mod git_hosting;
mod keys_and_databases;
mod observability_and_payments;
mod platforms;
mod workspace_and_messaging;

#[derive(Debug, Clone, Copy)]
pub struct SecretPattern {
    pub name: &'static str,
    pub regex: &'static str,
}

/// Secret patterns in evaluation order; a pattern's index is its identity.
pub static PATTERNS: &[SecretPattern] = &join::<PATTERN_COUNT>(&SLICES);

/// How many patterns [`PATTERNS`] holds.
pub(crate) const PATTERN_COUNT: usize = total(&SLICES);

/// The slices of [`PATTERNS`], in evaluation order.
const SLICES: [&[SecretPattern]; 8] = [
    ai_and_analytics::PATTERNS,
    auth_and_cloud::PATTERNS,
    platforms::PATTERNS,
    keys_and_databases::PATTERNS,
    dev_tooling::PATTERNS,
    observability_and_payments::PATTERNS,
    workspace_and_messaging::PATTERNS,
    git_hosting::PATTERNS,
];

/// The number of patterns in `slices`.
const fn total(slices: &[&[SecretPattern]]) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < slices.len() {
        count += slices[index].len();
        index += 1;
    }
    count
}

/// `slices` joined in order into one table of exactly `N` patterns.
const fn join<const N: usize>(slices: &[&[SecretPattern]]) -> [SecretPattern; N] {
    let mut joined = [SecretPattern {
        name: "",
        regex: "",
    }; N];
    let mut filled = 0;
    let mut slice = 0;
    while slice < slices.len() {
        let mut index = 0;
        while index < slices[slice].len() {
            joined[filled] = slices[slice][index];
            filled += 1;
            index += 1;
        }
        slice += 1;
    }
    assert!(filled == N, "pattern count mismatch");
    joined
}
