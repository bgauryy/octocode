//! The keys of a comment line (B165): each is read as a line of its own, over
//! its segment of the line and at most the next, so reading them costs what the
//! line's bytes cost, however many credential words it holds.

use super::*;
use crate::security::detector::code_syntax::{
    CodeSyntax, Place, grammar_of, has_credential_assignment,
};
use std::ops::Range;

/// The `(whole, secret)` ranges a read finds.
type Found = Vec<(Range<usize>, Range<usize>)>;

/// A comment line of a size.
type Line = Box<dyn Fn(usize) -> String>;

fn pattern(name: &str) -> usize {
    PATTERNS
        .iter()
        .position(|pattern| pattern.name == name)
        .expect("a registered pattern")
}

/// The bytes handed to pattern `name` for the tails of the TypeScript `text`,
/// and the secrets it found there.
fn read_tails(name: &str, text: &str) -> (usize, Found) {
    let syntax = CodeSyntax::of("a.ts", text).expect("a TypeScript grammar");
    let judge = Judge::of(pattern(name), Scope::File("a.ts"), text, Some(&syntax));
    let mut scanned = 0;
    let found = judge.tail_secrets(text, &syntax, &mut scanned);
    (scanned, found)
}

/// The comment lines the work tests read: shapes whose keys each lie in the
/// value of the one before, or no operator ends, or one operator ends all.
fn work_lines() -> Vec<(&'static str, Line)> {
    vec![
        // Words that are no key: no operator follows them.
        (
            "yamlSecretValue",
            Box::new(|n| format!("// {}password: Zq7xK2mPlanted9\n", "pass ".repeat(n))),
        ),
        // Keys of one path, which no operator ends, and a value that is rejected.
        (
            "yamlSecretValue",
            Box::new(|n| format!("// {}password: a b\n", "secret/".repeat(n))),
        ),
        // The same path, and a value that is a secret.
        (
            "yamlSecretValue",
            Box::new(|n| format!("// {}password: Zq7xK2mPlanted9\n", "secret/".repeat(n))),
        ),
        // Each key in the value of the one before it, no value that ends it.
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {} a b\n", "token=".repeat(n))),
        ),
        // Each key in the value of the one before it.
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {}Zq7xK2mPlanted9\n", "Zq7xK2mPl_token=".repeat(n))),
        ),
        // One key, however long its run of words.
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {}pass=Zq7xK2mPlanted9\n", "pass_".repeat(n))),
        ),
        // Secrets that each end where a trailing comment starts.
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {}\n", "token=Zq7xK2mPlanted9 # ".repeat(n))),
        ),
        (
            "yamlSecretValue",
            Box::new(|n| format!("// {}\n", "token: 1 # ".repeat(n))),
        ),
        // Keys, each with a value that no secret test accepts.
        (
            "yamlSecretValue",
            Box::new(|n| format!("// {}\n", "token: x ".repeat(n))),
        ),
        // A pattern that reads a path of `[n]` positions, from a key in its
        // middle: a rejected value, then a secret.
        (
            "propertiesFileCredential",
            Box::new(|n| format!("// {}password=a b\n", "token[0].".repeat(n))),
        ),
        (
            "propertiesFileCredential",
            Box::new(|n| {
                format!(
                    "// pass/xsecret{}api_key:Zq7xK2mPlanted9\n",
                    "api_key.-[0]-_".repeat(n)
                )
            }),
        ),
        // A pattern that reads no path: only the key nearest the value matches.
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {}password=a b\n", "secret/".repeat(n))),
        ),
        // Keys that name a locator, each in the value of the one before: the
        // key alone rejects every value, in a path of its own or in a run.
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {}\n", "token_file=/a/".repeat(n))),
        ),
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {}\n", "token_path=a_".repeat(n))),
        ),
        // Keys each in the value of the one before, whose value runs to the
        // line's end and is rejected by the value test (a backtick in it): the
        // reads to the line's end share their walks.
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {}\n", "token=`a".repeat(n))),
        ),
        (
            "yamlSecretValue",
            Box::new(|n| format!("// {}\n", "token: `a".repeat(n))),
        ),
        // The review's shapes (review/b165.md, rounds 3 and 4).
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {}\n", "token=token_".repeat(n))),
        ),
        (
            "yamlSecretValue",
            Box::new(|n| format!("// {}\n", "token: ".repeat(n))),
        ),
        (
            "yamlSecretValue",
            Box::new(|n| format!("// {}\n", "password: a b, ".repeat(n))),
        ),
        (
            "yamlSecretValue",
            Box::new(|n| format!("// {}\n", "token: changeme ".repeat(n))),
        ),
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {}\n", "token=x token=y: ".repeat(n))),
        ),
        (
            "dotenvSecretAssignment",
            Box::new(|n| format!("// {}\n", "password=a b ".repeat(n))),
        ),
        (
            "dotenvFileCredential",
            Box::new(|n| format!("// {}\n", "token=`a".repeat(n))),
        ),
        (
            "propertiesFileCredential",
            Box::new(|n| format!("// {}\n", "token=`a".repeat(n))),
        ),
    ]
}

/// A comment line's bytes are each handed to its pattern a bounded number of
/// times: doubling it doubles the bytes the tails
/// cost, however its keys lie. A search from each key start reads
/// the rest of the line again from each, which quadruples them.
#[test]
fn tails_cost_what_their_line_costs() {
    for (name, line) in &work_lines() {
        let (small, _) = read_tails(name, &line(2_000));
        let (large, _) = read_tails(name, &line(4_000));
        assert!(
            large <= 3 * small,
            "{name} {:?}: {small} bytes, then {large}",
            line(2)
        );
    }
}

fn lcg() -> impl FnMut(usize) -> usize {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    move |bound| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(state >> 33).expect("a 31-bit number") % bound
    }
}

const WORDS: [&str; 26] = [
    "password",
    "token",
    "secret",
    "key",
    "x",
    "Zq7xK2mPlanted9",
    "Zq7xK2mPl_token",
    "=",
    ":",
    " ",
    "  ",
    "/",
    "[0]",
    ".",
    "-",
    "_",
    "\"",
    "'",
    "#",
    "a_token",
    "api_key",
    " # ",
    "*/",
    "pass",
    "`",
    "`a",
];

/// The keys of a comment line are read as before: the secrets of the two
/// patterns a source file's comments reach equal those of a read of each
/// credential word's tail to the line's end with the unanchored pattern, and
/// the file-class patterns, whose files are no source files, leave nothing
/// readable that read redacts (they may redact more).
#[test]
fn the_tails_read_find_what_every_word_tail_finds() {
    let grammar = grammar_of("a.ts").expect("a TypeScript grammar");
    let mut next = lcg();
    let mut compared = 0;
    let cases: usize = std::env::var("OCTOCODE_TAIL_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(6_000);
    for _ in 0..cases {
        let words = 1 + next(14);
        let body: String = (0..words).map(|_| WORDS[next(WORDS.len())]).collect();
        let lead = ["// ", "x = 1; // ", "//"][next(3)];
        let text = format!("{lead}{body}\n");
        if !has_credential_assignment(&text, grammar) {
            continue;
        }
        let comment_at = text.find("//").expect("a comment");
        for name in [
            "yamlSecretValue",
            "dotenvSecretAssignment",
            "dotenvFileCredential",
            "propertiesFileCredential",
        ] {
            let judge = Judge::of(pattern(name), Scope::File("a.ts"), &text, None);
            let (_, found) = read_tails(name, &text);
            let expected = every_word_tail(&judge, name, &text, comment_at);
            let (found, expected) = (deduped(found), deduped(expected));
            // Nothing the per-word reads redact stays readable; a read of a
            // key by its own segment may redact more.
            assert!(
                expected.iter().all(|old| found
                    .iter()
                    .any(|new| new.start <= old.start && old.end <= new.end)),
                "{name}: {text:?}: {expected:?} not in {found:?}"
            );
            compared += 1;
        }
    }
    assert!(compared > 1_000, "{compared} comparisons");
}

/// A value that runs over three or more keys is read whole: the secrets the
/// per-word reads find are all redacted. Each text was a loss of a window that
/// ended at the next key or the one after.
#[test]
fn a_value_over_several_keys_is_found_whole() {
    for (name, text) in [
        ("dotenvSecretAssignment", "//token=token=xpass=pass\n"),
        ("dotenvFileCredential", "//password=\"\"pass=\n"),
        ("dotenvFileCredential", "// token=#passpassx:a_token:\n"),
        (
            "dotenvFileCredential",
            "// tokenZq7xK2mPlanted9Zq7xK2mPlanted9api_keytoken:api_key=secret:\n",
        ),
        ("dotenvSecretAssignment", "//token=abc/pass=defghijklmno\n"),
        // A quote the line closes after the next key.
        ("dotenvFileCredential", "//token=' #key:x # /token='\n"),
        // The match starts at a credential word that is neither the first nor
        // the last of its path.
        (
            "propertiesFileCredential",
            "// pass/xsecretapi_key.-[0]-_api_key:Zq7xK2mPlanted9\n",
        ),
    ] {
        let judge = Judge::of(pattern(name), Scope::File("a.ts"), text, None);
        let (_, found) = read_tails(name, text);
        let expected = deduped(every_word_tail(&judge, name, text, 0));
        assert!(!expected.is_empty(), "{name} {text:?}: a secret to find");
        let found = deduped(found);
        assert!(
            expected.iter().all(|old| found
                .iter()
                .any(|new| new.start <= old.start && old.end <= new.end)),
            "{name} {text:?}: {expected:?} not in {found:?}"
        );
    }
}

/// What a read of the tail of every word of the comment line that holds a
/// credential word finds, each with the unanchored pattern.
fn every_word_tail(judge: &Judge<'_>, name: &str, text: &str, comment_at: usize) -> Found {
    let line_end = text.find('\n').unwrap_or(text.len());
    let body = &text[comment_at..line_end];
    let closed = if body.trim_end().ends_with("*/") {
        body.trim_end().len() - 2
    } else {
        body.len()
    };
    let end = comment_at + closed;
    let regex = pattern_regex_for(pattern(name), text);
    let is_key = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-');
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = comment_at;
    while at < end {
        if !is_key(bytes[at]) {
            at += 1;
            continue;
        }
        let word_end = (at..end).find(|&i| !is_key(bytes[i])).unwrap_or(end);
        let word = text[at..word_end].to_ascii_lowercase();
        if crate::security::patterns::CREDENTIAL_WORDS
            .split('|')
            .any(|credential| word.contains(credential))
        {
            accepted_secrets(
                regex,
                &text[at..end],
                line_start_of,
                |key, secret, _| judge.value_test.redacts_in_comment(key, secret),
                |whole, secret| {
                    let shift = |range: Range<usize>| range.start + at..range.end + at;
                    found.push((shift(whole), shift(secret)));
                },
            );
        }
        at = word_end;
    }
    found
}

/// The secrets that stay once the later of two that start inside the first is
/// dropped, as the detector does.
fn deduped(mut all: Found) -> Vec<Range<usize>> {
    all.sort_by_key(|(_, secret)| (secret.start, std::cmp::Reverse(secret.end)));
    let mut covered_to = 0;
    let mut kept = Vec::new();
    for (_, secret) in all {
        if secret.start >= covered_to {
            covered_to = secret.end;
            kept.push(secret);
        }
    }
    kept
}

/// A character is code unless a string, a comment or a label spans it: the
/// verdict of every position of a text, from the ranges of its data nodes.
#[test]
fn a_position_is_code_unless_a_string_or_a_comment_spans_it() {
    let text = "const a = 1; // note\nconst s = \"in a string\";\nconst t = `a\n${b} c`;\n/* block\n over lines */ let z = 2;\n";
    let syntax = CodeSyntax::of("a.ts", text).expect("a TypeScript grammar");
    let at = |needle: &str| {
        let start = text.find(needle).expect("a needle");
        start..start + needle.len()
    };
    for (needle, code) in [
        ("const a", true),
        ("1;", true),
        ("// note", false),
        ("note", false),
        ("in a string", false),
        ("const t", true),
        ("a\n${b}", false),
        ("c`", false),
        ("block", false),
        ("over lines", false),
        ("let z", true),
        ("2;", true),
    ] {
        assert_eq!(
            syntax.place_of(&at(needle)) == Place::Code,
            code,
            "{needle}"
        );
    }
    // Blanks before a match are no part of its verdict.
    assert_ne!(
        syntax.place_of(&(at("note").start - 1..at("note").end)),
        Place::Code
    );
}

/// Every key of a comment line that holds a secret has it redacted, the later
/// ones after the first's trailing comment or `;` too.
#[test]
fn each_secret_of_one_comment_line_is_found() {
    for (name, text, secrets) in [
        (
            "dotenvSecretAssignment",
            "x = 1; // token=Zq7xK2mPlanted9 # password=Abcdefghijkl12\n",
            ["Zq7xK2mPlanted9", "Abcdefghijkl12"].as_slice(),
        ),
        (
            "yamlSecretValue",
            "x = 1; // password: Zq7xK2mPlanted9 # token: Abcdefghijkl12\n",
            ["Zq7xK2mPlanted9", "Abcdefghijkl12"].as_slice(),
        ),
        (
            "dotenvSecretAssignment",
            "x = 1; // a=1 password=Abcdefghijkl12\n",
            ["Abcdefghijkl12"].as_slice(),
        ),
        (
            "yamlSecretValue",
            "x = 1; // token: Zq7xK2mPlanted9; secret: Abcdefghijkl12\n",
            ["Zq7xK2mPlanted9", "Abcdefghijkl12"].as_slice(),
        ),
    ] {
        let (_, found) = read_tails(name, text);
        for secret in secrets {
            let at = text.find(secret).expect("a planted secret");
            assert!(
                found
                    .iter()
                    .any(|(_, range)| range.start <= at && at + secret.len() <= range.end),
                "{name} {text:?}: {secret} not in {found:?}"
            );
        }
    }
}
