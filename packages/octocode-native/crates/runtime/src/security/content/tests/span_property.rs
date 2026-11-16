//! For any text with known secrets inserted at random places among ordinary
//! text, hashes, UUIDs and base64, the output is the input with exactly the
//! secret spans replaced: nothing else changes and no secret byte survives.

use super::*;

enum Piece {
    Plain(String),
    Secret(String),
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn plain(text: &str) -> Piece {
    Piece::Plain(text.to_owned())
}

fn ordinary(rng: &mut Rng) -> Vec<Piece> {
    const LINES: &[&str] = &[
        "const a = compute(1);\n",
        "let sha = \"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\";\n",
        "export const lock = { integrity: \"sha512-Z2VuZXJhdGVkRmFrZUludGVncml0eUhhc2hGb3JUZXN0c09ubHlOb3RBS2V5\" };\n",
        "id: 550e8400-e29b-41d4-a716-446655440000\n",
        "logo: iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==\n",
        "const HEADER = \"-----BEGIN RSA PRIVATE KEY-----\";\n",
        "const FOOTER = \"-----END RSA PRIVATE KEY-----\";\n",
        "// the token handling lives in auth.rs\n",
        "max_tokens: 100\n",
        "password = getPassword()\n",
        "token: Token\n",
        "commit 3755d3a1c2b4e5f60718293a4b5c6d7e8f901234\n",
        "\n",
    ];
    vec![plain(LINES[rng.below(LINES.len())])]
}

fn key_line(rng: &mut Rng) -> String {
    (0..64)
        .map(|_| char::from(BASE64[rng.below(BASE64.len())]))
        .collect()
}

fn secret(rng: &mut Rng) -> Vec<Piece> {
    const TOKENS: &[&str] = &[
        "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8",
        "AKIAIOSFODNN7EXAMPLE",
        "xoxb-123456789012-1234567890123-abcdefghijklmnopqrstuvwx",
        "sk_live_4eC39HqLyjWDarjtT1zdp7dc",
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c",
    ];
    match rng.below(3) {
        0 => vec![
            plain("const k = \""),
            Piece::Secret(TOKENS[rng.below(TOKENS.len())].to_owned()),
            plain("\";\n"),
        ],
        1 => vec![
            plain("config.password = \""),
            Piece::Secret(format!("hunter{}x!", rng.below(1000))),
            plain("\"; // note\n"),
        ],
        _ => {
            let mut pieces = vec![plain("-----BEGIN RSA PRIVATE KEY-----\n")];
            for line in 0..1 + rng.below(3) {
                if line > 0 {
                    pieces.push(plain("\n"));
                }
                pieces.push(Piece::Secret(key_line(rng)));
            }
            pieces.push(plain("\n-----END RSA PRIVATE KEY-----\n"));
            pieces
        }
    }
}

/// The statements of a text: each one holds a whole secret or ordinary text.
fn text(rng: &mut Rng) -> Vec<Vec<Piece>> {
    (0..3 + rng.below(10))
        .map(|_| {
            if rng.below(3) == 0 {
                secret(rng)
            } else {
                ordinary(rng)
            }
        })
        .collect()
}

fn flat(statements: &[Vec<Piece>]) -> Vec<&Piece> {
    statements.iter().flatten().collect()
}

fn joined(pieces: &[&Piece]) -> String {
    pieces
        .iter()
        .map(|piece| match piece {
            Piece::Plain(text) | Piece::Secret(text) => text.as_str(),
        })
        .collect()
}

/// `out` is `pieces` with each secret replaced by one placeholder and
/// nothing else changed.
fn assert_spans_only(pieces: &[&Piece], out: &str, what: &str) {
    let placeholders = octocode_engine::security::placeholder_regex();
    let mut at = 0;
    for piece in pieces {
        match piece {
            Piece::Plain(text) => {
                assert!(
                    out[at..].starts_with(text.as_str()),
                    "{what}: plain text changed at {at}\n--- expected {text:?}\n--- output {out}"
                );
                at += text.len();
            }
            Piece::Secret(secret) => {
                let found = placeholders.find(&out[at..]);
                assert!(
                    found.is_some_and(|found| found.start() == 0),
                    "{what}: no placeholder for {secret:?} at {at}\n--- output {out}"
                );
                at += found.map_or(0, |found| found.end());
            }
        }
    }
    assert_eq!(at, out.len(), "{what}: text added\n--- output {out}");
    for piece in pieces {
        if let Piece::Secret(secret) = piece {
            assert!(
                !out.contains(secret.as_str()),
                "{what}: {secret:?} survived"
            );
        }
    }
}

#[test]
fn only_the_secret_spans_of_a_text_are_replaced() {
    let policy = ContentSecurity::new();
    let mut rng = Rng(0x5eed);
    for case in 0..300 {
        let statements = text(&mut rng);
        let pieces = flat(&statements);
        let source = joined(&pieces);
        let out = policy.sanitize_text(&source, None).content.into_owned();
        assert_spans_only(&pieces, &out, &format!("whole text, case {case}"));
    }
}

#[test]
fn only_the_secret_spans_of_a_window_cut_from_a_text_are_replaced() {
    // A window cut at statement boundaries is judged as a leaf: the path a
    // match, a hover or a page cut takes.
    let policy = ContentSecurity::new();
    let mut rng = Rng(0xc07);
    for case in 0..300 {
        let statements = text(&mut rng);
        let from = rng.below(statements.len());
        let to = from + 1 + rng.below(statements.len() - from);
        let window = flat(&statements[from..to]);
        let source = joined(&window);
        let out = policy
            .sanitize_leaf(&source)
            .unwrap_or_else(|| source.clone());
        assert_spans_only(&window, &out, &format!("window, case {case}"));
    }
}

#[test]
fn a_key_whose_begin_lies_before_the_window_has_its_body_hidden() {
    let policy = ContentSecurity::new();
    let mut rng = Rng(0xbe9);
    for _ in 0..50 {
        let lines: Vec<String> = (0..4).map(|_| key_line(&mut rng)).collect();
        let whole = format!(
            "-----BEGIN RSA PRIVATE KEY-----\n{}\n-----END RSA PRIVATE KEY-----\nconst tail = 1;\n",
            lines.join("\n")
        );
        let hidden = policy.sanitize_text(&whole, None).content.into_owned();
        assert!(lines.iter().all(|line| !hidden.contains(line.as_str())));
        // A window cut from the raw text, from the second body line on.
        let window = whole.lines().skip(2).collect::<Vec<_>>().join("\n");
        let out = policy.sanitize_leaf(&window).unwrap_or(window);
        assert!(
            lines[1..].iter().all(|line| !out.contains(line.as_str())),
            "{out}"
        );
        assert!(out.contains("const tail = 1;"), "{out}");
    }
}
