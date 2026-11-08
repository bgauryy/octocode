//! The keys of one path are judged from the facts of the first (B165): every
//! key of a path, judged by the facts read off the first key's text with its
//! head cut off, gets the verdict the key's own text gets.

use super::*;
use crate::security::detector::key_facts::SpanKeys;

fn lcg() -> impl FnMut(usize) -> usize {
    let mut state = 0xD1B5_4A32_D192_ED03_u64;
    move |bound| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(state >> 33).expect("a 31-bit number") % bound
    }
}

const PIECES: [&str; 14] = [
    "secret", "a", "token", "password", "db.pass", "Path", "src.rs", "x_dir", "my-file", "api_key",
    "keyFile", "data", "app.url", "pass",
];

const JOINS: [&str; 5] = ["/", "/", "[0]", "[1].", "\\"];

/// What follows a path's last word up to the value: the operator and what a
/// rule reads after it (a type, a fallback read, an opening quote).
const TAILS: [&str; 12] = [
    ": ",
    "=",
    " = ",
    ": \"",
    "= '",
    ": &str = \"",
    " = env.DB_PASS || \"",
    ": os.getenv(\"x\", \"",
    "= a or '",
    " : ",
    "=r#\"",
    ": Bearer ",
];

const SECRETS: [&str; 9] = [
    "Zq7xK2mPlanted9",
    "a b",
    "abc/def.rs",
    "d41d8cd98f00b204e9800998ecf8427e",
    "x y",
    "${A}",
    "Correct Horse 9",
    "quoted-literal-12345678",
    "/run/secrets/db",
];

/// Where each key of `path` starts: its words, which `/`, `[` and `]` end.
fn key_starts(path: &str) -> Vec<usize> {
    let bytes = path.as_bytes();
    (0..bytes.len())
        .filter(|&at| {
            bytes[at].is_ascii_alphanumeric()
                && (at == 0 || matches!(bytes[at - 1], b'/' | b'[' | b']' | b'\\'))
        })
        .collect()
}

#[test]
fn a_key_of_a_path_is_judged_as_its_own_text_is() {
    let mut next = lcg();
    let mut compared = 0;
    for _ in 0..4_000 {
        let mut path = String::new();
        for _ in 0..1 + next(6) {
            if !path.is_empty() {
                path.push_str(JOINS[next(JOINS.len())]);
            }
            path.push_str(PIECES[next(PIECES.len())]);
        }
        let text = format!("{path}{}", TAILS[next(TAILS.len())]);
        let starts = key_starts(&path);
        let span = SpanKeys::new(&text);
        for &start in &starts {
            let own = SpanKeys::new(&text[start..]);
            let (cut, owned) = (span.key(start), own.key(0));
            assert_eq!(cut.bits(), owned.bits(), "{text:?} from {start}");
            for test in [
                ValueTest::Every,
                ValueTest::KeyName,
                ValueTest::Scalar,
                ValueTest::Quoted,
                ValueTest::FileClass,
            ] {
                for secret in SECRETS {
                    assert_eq!(
                        test.redacts_in_comment_by(&cut.bits(), secret),
                        test.redacts_in_comment(&text[start..], secret),
                        "{test:?} {text:?} from {start}: {secret:?}"
                    );
                    compared += 1;
                }
            }
        }
    }
    assert!(compared > 10_000, "{compared} comparisons");
}
