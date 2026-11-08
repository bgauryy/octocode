//! Rules that read one config-file class's own value grammar.
//!
//! The leading slice of [`super::PATTERNS`]: a rule here runs before every
//! generic rule, because a value's grammar can span lines and a generic
//! rule that redacts the first line (a long value, a vendor token, a Spring
//! Boot key) takes its trailing `\` with it, so the file's rule no longer
//! sees that the value continues and its tail stays readable. The detector's
//! `properties_rule_leads_the_registry` test pins the position.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[SecretPattern {
    name: "propertiesFileCredential",
    // A Java `.properties` file is a config file where a credential name
    // means a credential, as in a dotenv file, however short the value.
    file_context: Some(super::PROPERTIES_FILE_CONTEXT),
    // `key=value` or `key: value` (a diff line inside a hunk too) whose
    // dotted key holds a credential word, with `[n]` list positions
    // (`users[0].password`) and an optional list marker (`  - key=value`);
    // which keys count, and which values stay
    // readable, is the file-class value test's (`ValueTest::FileClass`).
    // Java reads the value as everything up to the line end: quotes and
    // ` #` are value text, not syntax. A line that ends in an odd number of
    // backslashes continues on the next, which the detector's continued-value
    // rule reads for every pattern alike. A line `key value`
    // (whitespace as the separator) is not covered. Under a key that
    // only qualifies a credential (`db.password.old`) a number or a
    // boolean stays readable by design.
    regex: concat!(
        r###"(?m)^(?P<diff>[+\-])?[ \t]*"###,
        comment_lead!(),
        r###"(?:-[ \t]+)?(?:[A-Za-z0-9_.\-]|\[[0-9]*\])*?(?i:"###,
        credential_words!(),
        r###")(?:[A-Za-z0-9_.\-]|\[[0-9]*\])*[ \t]*[=:][ \t]*(?P<secret>[^\r\n]*?)[ \t]*\r?$"###
    ),
}];
