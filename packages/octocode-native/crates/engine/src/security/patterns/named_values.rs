//! The value a credential's name holds, wherever source lays it out.
//!
//! The last slice of [`super::PATTERNS`]: every pattern that knows a
//! provider's shape or a file's syntax reads first, and names what it finds.

use super::SecretPattern;

/// The word parts that make a name a credential's, as the pattern spells
/// them: a superset the detector's `named_values` narrows to whole word
/// parts (`pat`, not `path`). The last three are the verbs of a call that
/// takes credentials by position (`login("svc", "…")`): only a call reads
/// them.
macro_rules! name_words {
    () => {
        "pass|pwd|secret|token|tkn|key|auth|cred|signature|seed|bearer|pat|cookie|session|[_.-]sk|log[io]n|sign_?in|connect"
    };
}

/// One opaque run: the characters tokens, encoded keys and generated
/// passwords are made of, no blank, quote, bracket or separator among them,
/// ending in none a sentence closes with, base64 padding aside.
macro_rules! opaque_run {
    () => {
        concat!(
            "[",
            opaque_chars!(),
            "]{15,}",
            r###"[A-Za-z0-9+/_~!@#$%^*-]={0,3}"###
        )
    };
}

/// An assignment after a credential's name: `=` (the name may carry a
/// type), a comparison, `:=`, `=>`, or a colon.
macro_rules! name_assignment {
    () => {
        concat!(
            r###"(?:(?:[ \t]*:[ \t]*[A-Za-z_&\[*][^=\r\n"'`;(){}]{0,60}?)?[ \t]*(?:[!=]==?|=>|:=|\|\|=|\?\?=|=)[ \t]*"###,
            r###"|[ \t]*:[ \t]*)"###
        )
    };
}

/// What else sits between a credential's name and its value: a comma (an
/// argument or item after the name's), a table's bar, the `value` member
/// beside a `name` one, a block's `default`, a method called on the read, the
/// arguments before it in a call the name makes (`login("svc", "…")`) or in
/// the `VALUES` of the columns it ends, or a few plain words (`the API key
/// is …`, `ENV NAME …`, `--token …`).
macro_rules! name_gap {
    () => {
        concat!(
            r###"(?:[ \t]*\|[ \t]*"###,
            r###"|[ \t]*,\s*"###,
            r###"|[ \t]*[:=,]?\s*\{?\s*["']?value["']?[ \t]*[:=][ \t]*"###,
            r###"|[ \t]*\{[^{}]{0,300}?\bdefault[ \t]*=[ \t]*"###,
            r###"|[ \t]*\.[A-Za-z_][A-Za-z0-9_]*\([ \t]*"###,
            r###"|(?:[ \t]+(?i-u:values)[ \t]*)?\((?:[ \t]*(?:"[^"\r\n]{0,40}"|'[^'\r\n]{0,40}'|[A-Za-z_][A-Za-z0-9_.]*)[ \t]*,){0,3}[ \t]*"###,
            r###"|(?:[ \t]+[a-z]{1,12}){0,3}[ \t]+)"###
        )
    };
}

/// One opaque run and what may wrap it: a YAML anchor or tag, a call, a
/// command substitution, a literal's quote and prefix, an auth scheme, an
/// interpolation's brace. `$group` names the run.
macro_rules! wrapped_run {
    ($group:literal) => {
        concat!(
            r###"(?:&[A-Za-z0-9_-]+[ \t]+|!![a-z]+[ \t]+)?"###,
            r###"(?:%[qQ]?\(|\$\([a-z]+[ \t]+(?:-[a-z]+[ \t]+)?|(?:\[\])?[A-Za-z_][A-Za-z0-9_.:]*!?\([ \t]*)?"###,
            r###"(?:[A-Za-z$@]{0,2}["'`]|[A-Za-z_][A-Za-z0-9_.]*`)?(?:(?i-u:bearer|basic|token)[ \t]+)?(?:\{["'])?"###,
            "(?P<",
            $group,
            ">",
            opaque_run!(),
            ")"
        )
    };
}

/// What may open the value in the run's place: a bracket (a call's or a
/// macro's too), a YAML block scalar or block list, a triple quote, a
/// heredoc, or a string continued on the next line.
macro_rules! value_opener {
    () => {
        concat!(
            r###"(?P<open>(?:[A-Za-z_][A-Za-z0-9_.:]*!?)?[\[({]|[|>][+-]?[ \t]*\r?\n"###,
            r###"|[A-Za-z]{0,2}(?:"{3}|'{3})|<<[-~]?["']?[A-Za-z_][A-Za-z0-9_]*["']?"###,
            r###"|"\\\r?\n|\r?\n[ \t]*-[ \t])"###
        )
    };
}

pub(super) const PATTERNS: &[SecretPattern] = &[SecretPattern {
    name: "credentialNamedValue",
    file_context: None,
    // A name with a credential's word part (`dbPass`, `x_auth`, `APIKEY`,
    // `--token`), what its layout puts after it, and one opaque run
    // (`wrapped_run`). After an assignment the name may instead open a
    // collection or a block (`open`), and a heredoc's tag may be the name
    // (`tag`): the detector's `named_values` walks those, and judges every
    // name and value. An assignment may instead hold an expression whose
    // operands are literals (`cond ? "…" : "…"`, `env.X || "…"`): `expr`
    // starts it, and the detector judges each literal of it as the named
    // value. A sentence may state the value after `is` or `was`, a colon
    // after them or not, or after a bare `:` or `=` (`stated`), a word of
    // any characters.
    regex: concat!(
        r###"\b(?P<name>[A-Za-z0-9_.$-]*(?i-u:"###,
        name_words!(),
        r###")[A-Za-z0-9_.-]*)["'`]?[\])]{0,3}(?:"###,
        name_assignment!(),
        "(?:",
        wrapped_run!("secret"),
        "|",
        value_opener!(),
        r###"|(?P<expr>[^"'`;\r\n]*?)["'`]"###,
        ")|",
        name_gap!(),
        wrapped_run!("secret_after_gap"),
        // A sentence that states the name's value (`the password is …`,
        // `my password was: …`) with no run of those characters after it:
        // the word itself, which the detector judges as a request judges it.
        r###"|(?:[ \t]+(?i-u:is|was)[ \t]*:?[ \t]+|[ \t]*[:=][ \t]*)(?P<stated>\S+)"###,
        r###")|<<[-~]?["']?(?P<tag>[A-Za-z0-9_]*(?i-u:"###,
        name_words!(),
        r###")[A-Za-z0-9_]*)["']?[ \t]*\r?\n"###
    ),
}];
