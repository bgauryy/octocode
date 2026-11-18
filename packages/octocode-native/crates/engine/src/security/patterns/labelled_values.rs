//! Credentials named by their label: a key that says what it holds
//! (`password`, `client_secret`, `api_key`, `token`) assigned a value, in
//! source (`=`, `:=`, `=>`), JSON, YAML, `.ini` or `.properties`.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order. A label never follows a `:` (`arn:aws:…:secret:name`
//! names a resource, no assignment). Only the value is the match's `secret`; the label, its
//! quotes and the operator read as written. A label ends its key
//! (`max_tokens` is no `token`), and the value is a literal one reads as
//! text: a quoted run with no blank (a value everywhere), a URL query value
//! (`?token=…`, `&password=…`: a literal in code too), or a bare run of four
//! bytes at least (a value only in a config file: in code a bare word after a
//! label is an identifier, `ValueForm`). A bare value never starts with
//! `:` or `!` (a path `token::Brace`, a minified `!0`), `*` or `&` (a YAML
//! alias or anchor). A call, a subscript, a template or a placeholder
//! (`${X}`, `$X`, `{{x}}`, `<x>`, `%s`, `changeme`, a boolean or `null`,
//! `void`, a short number, a path, a publishable key a vendor documents as
//! public: Mapbox's `pk.eyJ…`, Stripe's `pk_live_…`) stands for a secret and
//! holds none, so its `template` branch hides nothing. An interpolation with
//! a default (`${MYSQL_PASS:petclinic}`) names where the secret comes from
//! and writes one: the default is the value when it is a literal of four
//! bytes at least, the variable and the braces read as written.

use super::SecretPattern;

/// A value that only stands for a secret, but for an interpolation (`${…}`).
macro_rules! placeholder_word {
    () => {
        r###"(?-i:\$[A-Z_][A-Z0-9_]*)|\{\{[^{}\r\n]+\}\}|<[^<>\r\n]+>|%s|%\(\w+\)s|\[REDACTED[^\]\r\n]*\]|(?:pass(?:word|wd)|pwd|secret|token|api_?key|client_?secret|private_?key|secret_?key)|changeme|true|false|null|none|nil|void|yes|no|on|off|[0-9]{1,4}|pk\.eyJ[\w.-]+|pk_(?:live|test)_\w+|[/~.][^\s"'`]*"###
    };
}

/// A value that only stands for a secret: an interpolation (`${X}`, or
/// `${X:default}` whose default is itself a placeholder or under four bytes;
/// a lookup `${env:HOME}`, `${vault:path}` names a source and holds no
/// default), or a [`placeholder_word!`].
macro_rules! stands_for_secret {
    () => {
        concat!(
            r###"\$\{(?:env|sys|ctx|java|jndi|date|file|docker|main|web|base64|lower|upper|bundle|properties|ref|op|vault|secret|secrets|ssm|aws|gcp|azure|k8s|kube|sm):[^{}\s]*\}|\$\{[^{}\s:]+(?::-?(?:"###,
            placeholder_word!(),
            r###"|[^{}\s]{0,3}))?\}|"###,
            placeholder_word!()
        )
    };
}

/// The operator and the value after a label that has been read: a template
/// (hides nothing), an interpolation's literal default, a quoted run, or a
/// bare word.
macro_rules! assigned_value {
    () => {
        concat!(
            r###"[ \t]*(?::=|=>|:|=)[ \t]*(?:["']?(?P<template>"###,
            stands_for_secret!(),
            r###")["']?(?:[\s,;)}\]&"']|\z)"###,
            r###"|["']?\$\{[^{}\s:]+:-?(?P<secret_default>[^{}\s]{4,})\}"###,
            r###"|"(?P<secret_dq>[^A-Za-z0-9"\s\\]*[A-Za-z0-9][^"\s\\]{3,})(?:"|(?m:$))"###,
            r###"|'(?P<secret_sq>[^A-Za-z0-9'\s\\]*[A-Za-z0-9][^'\s\\]{3,})(?:'|(?m:$))"###,
            r###"|(?P<secret_bare>[^\s"'`\\(){}<>\[\],;&|=:!*][^\s"'`\\(){}<>\[\],;&]{3,})(?:[\s,;)}\]&"']|\z))"###
        )
    };
}

/// A URL query value after a label's `=`: the same template (hides nothing)
/// and a literal of four bytes at least that ends at the next `&`, `#`, blank
/// or quote. A URL's literal is a value in code too, never an identifier.
macro_rules! query_value {
    () => {
        concat!(
            r###"=(?:(?P<template_query>"###,
            stands_for_secret!(),
            r###")(?:[\s,;)}\]&#"'`]|\z)"###,
            r###"|(?P<secret_query>[^\s"'`\\(){}<>\[\],;&|=:!*#][^\s"'`\\(){}<>\[\],;&#]{3,}))"###
        )
    };
}

/// A known secret key after `?` or `&` in a URL's query: the bare label, or
/// the label behind one prefix of a closed list, each a documented secret
/// parameter: `access_token` (RFC 6750 section 2.3), `refresh_token`,
/// `id_token` and `client_secret` (RFC 6749, OpenID Connect), `x-amz-security-token`
/// (AWS Signature Version 4), `x-api-key`, `x-auth-token`, `session_token`,
/// `bearer_token`, `private_key`. Any other key that merely ends in the label
/// (`page_token`, `next_token`, `sync_token`) names no credential.
macro_rules! query_key {
    ($label:literal) => {
        concat!(
            r###"[?&](?:x[_-])?(?:(?:access|auth|refresh|id|session|bearer|api|client|private|oauth|amz[_-]security)[_.-]?)?"###,
            $label
        )
    };
}

/// An unlisted query key that ends in the label (`page_token=`): a match
/// through it hides nothing, and it takes the key off the label-assignment
/// branch (which reads `token=` after any word character).
macro_rules! unlisted_query_key {
    ($label:literal) => {
        concat!(
            r###"[?&][\w.-]*"###,
            $label,
            r###"=(?P<template_unlisted_key>)"###
        )
    };
}

/// A quoted label word that is a ternary branch (`c ? 'password' : 'text'`),
/// not a key: a match through it hides nothing. It starts at the `?`, before
/// the label-assignment branch could start at the quote, so the branch's value
/// is never read as an assigned one.
macro_rules! ternary_branch {
    ($label:literal) => {
        concat!(
            r###"\?[ \t]*["'`]"###,
            $label,
            r###"["'`][ \t]*:(?P<template_ternary>)"###
        )
    };
}

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "passwordAssignment",
        // `pwd` is short enough to be an ordinary word: only quoted.
        regex: concat!(
            r###"(?i)(?:(?:(?m:^)|[^:?&])(?:pass(?:word|wd)["'`]?|["']pwd["'])"###,
            assigned_value!(),
            "|",
            query_key!(r"pass(?:word|wd)"),
            query_value!(),
            "|",
            unlisted_query_key!(r"pass(?:word|wd)"),
            "|",
            ternary_branch!(r"pass(?:word|wd)"),
            ")"
        ),
    },
    SecretPattern {
        name: "secretAssignment",
        regex: concat!(
            r###"(?i)(?:(?:(?m:^)|[^:?&])(?:(?:client[_-]?)?secret(?:[_-]?key)?|private[_-]?key)["'`]?"###,
            assigned_value!(),
            "|",
            query_key!(r"(?:(?:client[_-]?)?secret(?:[_-]?key)?|private[_-]?key)"),
            query_value!(),
            "|",
            unlisted_query_key!(r"(?:(?:client[_-]?)?secret(?:[_-]?key)?|private[_-]?key)"),
            "|",
            ternary_branch!(r"(?:(?:client[_-]?)?secret(?:[_-]?key)?|private[_-]?key)"),
            ")"
        ),
    },
    SecretPattern {
        name: "tokenAssignment",
        regex: concat!(
            r###"(?i)(?:(?:(?m:^)|[^:?&])(?:token|api[_-]?key)["'`]?"###,
            assigned_value!(),
            "|",
            query_key!(r"(?:token|api[_-]?key)"),
            query_value!(),
            "|",
            unlisted_query_key!(r"(?:token|api[_-]?key)"),
            "|",
            ternary_branch!(r"(?:token|api[_-]?key)"),
            ")"
        ),
    },
];
