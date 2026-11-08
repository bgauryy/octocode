//! Short quoted literals (D6 round 4): under a key naming a secret a person
//! chooses (`password`, `passwd`, `passphrase`, `secret`), a quoted
//! literal shorter than an opaque token is a credential when it has no
//! whitespace and holds a digit or a symbol, in every file class.

use super::*;

/// Where such an assignment is read: code, docs and a pathless PR body.
const PLACES: &[Option<&str>] = &[
    Some("src/db.rs"),
    Some("web/app.ts"),
    Some("docs/setup.md"),
    None,
];

#[test]
fn short_chosen_secrets_are_redacted_in_every_file_class() {
    let cases = [
        ("let password = \"{v}\";\n", "hunter2Sec!x"),
        ("db_password: '{v}'\n", "s3cr3t-99"),
        ("  \"client_secret\": \"{v}\",\n", "Pa55w0rd"),
        ("dbPasswd := \"{v}\"\n", "tr0ub4dor"),
        ("passwd => \"{v}\"\n", "open$esame"),
        ("ssh_passphrase = '{v}'\n", "kitty#cat"),
        ("Set `password = \"{v}\"` in the config.\n", "hunter2Sec!x"),
    ];
    let mut leaks = Vec::new();
    for (shape, value) in cases {
        let content = shape.replace("{v}", value);
        for place in PLACES {
            let result = detect(&content, *place);
            if result.sanitized.contains(value) {
                leaks.push(format!("{place:?}: {}", result.sanitized));
            }
        }
    }
    assert_eq!(leaks, Vec::<String>::new());
}

/// A short literal that is no chosen secret stays: letters only, a template
/// (`<…>`, `${…}`, `{{…}}`), a value with whitespace, a key that says only
/// `pass` (`bypass`, `passport`) or names an opaque string (`token`).
#[test]
fn short_literals_that_are_no_chosen_secret_stay_readable() {
    let contents = [
        "let password = \"hunterxyz\";\n",
        "password: \"<db-pass-1>\"\n",
        "password = \"${DB_PASS1}\"\n",
        "secret: \"{{vault_1}}\"\n",
        "password = \"hunt 3r!x\"\n",
        "dbpass = \"hunter2Sec!x\"\n",
        "passport_no = \"X1234567!\"\n",
        "token = \"abc123!xyz\"\n",
        // `pwd` is a working directory: its paths stay (G1).
        "pwd = \"/Users/x/MyApp\"\n",
        "let pwd = \"C:\\\\work\\\\app\";\n",
        "pwd: '~/Projects/x'\n",
        // An escape is one character of a quoted value (G2): a regex source
        // of 9 escapes and a `+` is short, under any key.
        "tokens.variable: '\\$\\.\\-\\w+\\d\\s\\b\\(\\)'\n",
        "token = \"\\$\\.\\-\\w+\\d\\s\\b\\(\"\n",
    ];
    let mut redacted = Vec::new();
    for content in contents {
        for place in PLACES {
            let result = detect(content, *place);
            if result.sanitized != content {
                redacted.push(format!("{place:?}: {}", result.sanitized));
            }
        }
    }
    assert_eq!(redacted, Vec::<String>::new());
}
