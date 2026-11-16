//! Credentials named by their label: a key that says what it holds
//! (`password`, `client_secret`, `api_key`, `token`) assigned a value, in
//! source (`=`, `:=`, `=>`), JSON, YAML, `.ini` or `.properties`.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order. A label never follows a `:` (`arn:aws:…:secret:name`
//! names a resource, no assignment). Only the value is the match's `secret`; the label, its
//! quotes and the operator read as written. A label ends its key
//! (`max_tokens` is no `token`), and the value is a literal one reads as
//! text: a quoted run with no blank (a value everywhere), or a bare run of
//! four bytes at least (a value only in a config file: in code a bare word
//! after a label is an identifier, `ValueForm`). A call, a subscript, a
//! template or a placeholder
//! (`${X}`, `$X`, `{{x}}`, `<x>`, `%s`, `changeme`, a boolean or `null`, a
//! short number, a path, a publishable key a vendor documents as public:
//! Mapbox's `pk.eyJ…`, Stripe's `pk_live_…`) stands for a secret and holds none, so its
//! `template` branch hides nothing.

use super::SecretPattern;

/// A value that only stands for a secret.
macro_rules! stands_for_secret {
    () => {
        r###"\$\{[^{}\s]+\}|(?-i:\$[A-Z_][A-Z0-9_]*)|\{\{[^{}\r\n]+\}\}|<[^<>\r\n]+>|%s|%\(\w+\)s|\[REDACTED[^\]\r\n]*\]|(?:pass(?:word|wd)|pwd|secret|token|api_?key|client_?secret|private_?key|secret_?key)|changeme|true|false|null|none|nil|yes|no|on|off|[0-9]{1,4}|pk\.eyJ[\w.-]+|pk_(?:live|test)_\w+|[/~.][^\s"'`]*"###
    };
}

/// The operator and the value after a label that has been read: a template
/// (hides nothing), a quoted run, or a bare word, each with a letter-only
/// word left out.
macro_rules! assigned_value {
    () => {
        concat!(
            r###"[ \t]*(?::=|=>|:|=)[ \t]*(?:["']?(?P<template>"###,
            stands_for_secret!(),
            r###")["']?(?:[\s,;)}\]&"']|\z)"###,
            r###"|"(?P<secret_dq>[^A-Za-z0-9"\s\\]*[A-Za-z0-9][^"\s\\]{3,})(?:"|(?m:$))"###,
            r###"|'(?P<secret_sq>[^A-Za-z0-9'\s\\]*[A-Za-z0-9][^'\s\\]{3,})(?:'|(?m:$))"###,
            r###"|(?P<secret_bare>[^\s"'`\\(){}<>\[\],;&|=][^\s"'`\\(){}<>\[\],;&]{3,})(?:[\s,;)}\]&"']|\z))"###
        )
    };
}

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "passwordAssignment",
        // `pwd` is short enough to be an ordinary word: only quoted.
        regex: concat!(
            r###"(?i)(?:(?m:^)|[^:])(?:pass(?:word|wd)["'`]?|["']pwd["'])"###,
            assigned_value!()
        ),
    },
    SecretPattern {
        name: "secretAssignment",
        regex: concat!(
            r###"(?i)(?:(?m:^)|[^:])(?:(?:client[_-]?)?secret(?:[_-]?key)?|private[_-]?key)["'`]?"###,
            assigned_value!()
        ),
    },
    SecretPattern {
        name: "tokenAssignment",
        regex: concat!(
            r###"(?i)(?:(?m:^)|[^:])(?:token|api[_-]?key)["'`]?"###,
            assigned_value!()
        ),
    },
];
