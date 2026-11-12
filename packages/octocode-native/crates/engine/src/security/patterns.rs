//! Canonical ordered secret-detection patterns: plain data, also read by the
//! engine build script to derive the detector's prescan gate. Each child
//! module holds one contiguous slice of the order; [`PATTERNS`] joins them.
//!
//! A pattern's left word boundary keeps a shape from matching inside a longer
//! word. A token a vendor opens with its own tag (`ghp_`, `glpat-`, `AKIA`:
//! four or more bytes, or a short tag opening a fixed inner literal of four or
//! more, such as OpenAI's `sk-…T3BlbkFJ`) is a secret wherever it starts,
//! glued to letters, digits or `_` (minified text, a joined name,
//! `Bearerghp_…`), so its pattern spells no left boundary. The regex is the
//! one source of that choice: the detector's `glued_tags` tests derive each
//! pattern's start from it and pin the set of tagged patterns by name.
//!
//! A tag can still occur by chance inside ordinary text (`AQE`, `EAA` inside
//! base64). A new entry in that set therefore needs a measured cost of zero:
//! count the pattern's matches with ripgrep (the same Rust regex engine) over
//! a pinned ordinary-text corpus (installed `node_modules`, a sample of the
//! Yarn cache, docs, skills and this repository's sources), once with the
//! leading `\b` and once without; the unbounded form may add no glued match.
//!
//! The right end follows the same rule. A tagged token of fixed length is a
//! secret whatever word characters follow it (`npm_<36>xyz`), so its pattern
//! spells no trailing `\b`, and the `glued_tags` tests pin the set of tagged
//! patterns open on the right. A new entry there needs the same measurement,
//! with and without the trailing `\b`. AWS key IDs keep theirs: dropping it
//! adds matches inside base64.

/// The credential words of the assignment rules, as one regex alternation:
/// the patterns spell it with `concat!`, the value tests split
/// [`CREDENTIAL_WORDS`].
macro_rules! credential_words {
    () => {
        "secret|password|passwd|passphrase|pass|pwd|key|token|credential"
    };
}

/// The comment marker a line-anchored assignment rule reads through
/// (`// KEY=value`, `# key: value`, ` * key: value`, `! key=value`, and the
/// markers of grammarless files: `-- key: value` (SQL, Lua), `; key = value`
/// (INI), `/* key: value`, `<!-- key: value`): a commented-out
/// credential is still one. In a source file the detector's syntax judgment
/// places the line in its comment.
macro_rules! comment_lead {
    () => {
        r"(?:(?://+|#+|;+|/\*+|<!--|--+[ \t]|[*!]+)[ \t]*)?"
    };
}

/// What source code may write between a credential's name, its operator and
/// its value, and between the pieces of one literal: blanks, line breaks, a
/// backslash that continues the line, and comments (`/* … */`, and `// …` or
/// `# …` to the end of its line). None of
/// it is part of the name or of the value, so an assignment is read through
/// it; the detector drops the comments from the key it judges
/// (`without_inner_comments`).
macro_rules! code_gap {
    () => {
        r"(?:\s|\\\r?\n|/\*(?:[^*]|\*+[^*/])*\*+/|(?://|#)[^\r\n]*(?:\r?\n|\r))*"
    };
}

/// The digits that number a credential's name (`apiKey2`, `API_KEY2`): the
/// name still says what its value is.
macro_rules! name_number {
    () => {
        "[0-9]*"
    };
}

/// The delimiter that closes a block comment after a value written inside
/// it (`/* token: … */`, `<!-- token: … -->`), a blank before it: the
/// comment's own, no part of the value.
macro_rules! comment_close {
    () => {
        r"(?:[ \t]+(?:\*+/|-->))?"
    };
}

/// The characters of an opaque run, as a class body: what tokens, encoded
/// keys and generated passwords are made of, no blank, quote, bracket or
/// separator among them.
macro_rules! opaque_chars {
    () => {
        r###"A-Za-z0-9+/_.~!@#$%^*-"###
    };
}

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

/// [`opaque_chars`] as text: the detector reads the same set.
pub(crate) const OPAQUE_CHARS: &str = opaque_chars!();

/// [`code_gap`] as text.
pub(crate) const CODE_GAP: &str = code_gap!();

/// [`credential_words`] as text.
pub(crate) const CREDENTIAL_WORDS: &str = credential_words!();

mod ai_and_analytics;
mod auth_and_cloud;
mod config_files;
mod dev_tooling;
mod git_hosting;
mod keys_and_databases;
mod named_values;
mod observability_and_payments;
mod platforms;
mod workspace_and_messaging;

/// The optional backup suffix a file class accepts (`.bak`, `.old`, `~`).
macro_rules! backup_suffix {
    () => {
        r"(?:\.(?:old|bak|backup|orig|save)|~)?"
    };
}

/// The dotenv name class (`.env`, `.env.*`, `.envrc`, `.flaskenv`, `*.env`,
/// each with an optional backup suffix such as `.bak` or `~`), any case: the
/// one rule behind the dotenv file pattern's `file_context` and
/// [`crate::security::is_dotenv_name`].
pub(crate) const DOTENV_FILE_CONTEXT: &str = concat!(
    r"(?i)(?:^|[/\\])(?:\.env(?:\.[^/\\]+)?|\.envrc|\.flaskenv|[^/\\]+\.env)",
    backup_suffix!(),
    "$"
);

/// The settings-file name class: Java properties (`*.properties`) and INI
/// files (`*.ini`), with an optional backup suffix, any case. `.conf` and
/// `.cfg` name no one format (WireGuard, nginx), so they stay out. The `file_context` of the settings file pattern: both read
/// `key=value` and `key: value` lines, where a credential name means a
/// credential. Such a file is no credential file by name, so it is not in
/// [`DOTENV_FILE_CONTEXT`] and the path policy still reads it.
pub(crate) const PROPERTIES_FILE_CONTEXT: &str = concat!(
    r"(?i)(?:^|[/\\])[^/\\]+\.(?:properties|ini)",
    backup_suffix!(),
    "$"
);

#[derive(Debug, Clone, Copy)]
pub struct SecretPattern {
    pub name: &'static str,
    pub file_context: Option<&'static str>,
    pub regex: &'static str,
}

/// Secret patterns in evaluation order; a pattern's index is its identity.
pub static PATTERNS: &[SecretPattern] = &join::<PATTERN_COUNT>(&SLICES);

/// How many patterns [`PATTERNS`] holds.
pub(crate) const PATTERN_COUNT: usize = total(&SLICES);

/// The slices of [`PATTERNS`], in evaluation order.
const SLICES: [&[SecretPattern]; 10] = [
    config_files::PATTERNS,
    ai_and_analytics::PATTERNS,
    auth_and_cloud::PATTERNS,
    platforms::PATTERNS,
    keys_and_databases::PATTERNS,
    dev_tooling::PATTERNS,
    observability_and_payments::PATTERNS,
    workspace_and_messaging::PATTERNS,
    git_hosting::PATTERNS,
    named_values::PATTERNS,
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
        file_context: None,
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
