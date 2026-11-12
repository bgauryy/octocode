//! Values under a credential's name in the layouts source writes them: the
//! name's word parts say "credential", an opaque value is one, and only the
//! value goes. Words, names, paths and versions under the same names stay.

use super::super::named_values::{is_opaque, names_a_credential};
use super::*;

/// Opaque values of the shapes a generator makes: mixed-case alphanumeric,
/// a generated password with symbols, base64, hex.
const TOKEN: &str = "kIXvkEgANsNycbRz4nTMVU4n";
const OTHER: &str = "Ue4Kr7SZZ8MF2rmkivoCY48H";
const PASSWORD: &str = "dWj3M!jhXT#NjPLv9";
const ENCODED: &str = "c3ZjOkFXVWt0YzlXWXJ4MzBETVpCcFY2";
const PADDED: &str = "eW9VdHFGdGRvTGgyWUtFTE9jM1k=";
const HEX: &str = "e80c9be5a2330c9377a7994dce1129fd237695238e9998b9";
const PLACEHOLDER: &str = "[REDACTED-CREDENTIALNAMEDVALUE]";

mod fields;
mod layouts;

/// That every layout loses its secrets and nothing else, in its file and in
/// the pathless net.
fn hides_only(rows: &[(&str, &str, &[&str])]) {
    for (content, path, secrets) in rows {
        for path in [Some(*path), None] {
            assert_redacts_each(content, path, secrets);
        }
    }
}

/// That `content` stays as written, in its file and in the pathless net.
fn stays(rows: &[(&str, &str)]) {
    for (content, path) in rows {
        for path in [Some(*path), None] {
            let result = detect(content, path);
            assert_eq!(&result.sanitized, content, "{path:?}");
        }
    }
}

/// A name's word parts make it a credential's: whole parts, in any case and
/// with any separator, a part written with no separator that ends in a
/// credential word, and the abbreviations people write.
#[test]
fn a_name_is_a_credentials_by_its_word_parts() {
    for name in [
        "apikey",
        "APIKEY",
        "pwd",
        "db_pwd",
        "dbPass",
        "adminPass",
        "pass_word",
        "tkn",
        "pat",
        "GH_PAT",
        "bearer",
        "SecretString",
        "secretValue",
        "stripe_sk",
        "x_auth",
        "_auth",
        "_authToken",
        "encryptionKey",
        "licenseKey",
        "masterKey",
        "secretKey1",
        "sshKey",
        "pass",
        "signature",
        "otpSeed",
        "clientSecret",
        "jwtSecret",
        "privateKey",
        "appSecretKeyProd",
        "userTokens",
        "ApiKey",
        "X-Api-Key",
        "PRIVATE-TOKEN",
        "--db-pass",
        "spring.datasource.password",
        "oauth",
        "sessionId",
        "Authorization",
        "NetworkCredential",
        "client.setToken",
        "creds",
    ] {
        assert!(names_a_credential(name), "{name}");
    }
    for name in [
        "path",
        "pattern",
        "patch",
        "author",
        "authority",
        "keyboard",
        "keywords",
        "tokenizer",
        "passed",
        "passenger",
        "secretary",
        "seeding",
        "sessions_count",
        "publicKey",
        "public_key",
        "pubkey",
        "PUBLIC_KEY_PEM",
        "secretName",
        "tokenType",
        "keyLength",
        "token_ttl",
        "passwordPolicy",
        "keyId",
        "client_id",
        "tokenUrl",
        "key_file",
        "secretPath",
        "privateKeyHash",
        "keyFingerprint",
        "description",
        "login",
        "skill",
        "task",
    ] {
        assert!(!names_a_credential(name), "{name}");
    }
}

/// An opaque value is what a generator makes; what a person writes to be
/// read is none, whatever digits and capitals it holds.
#[test]
fn a_value_is_opaque_by_its_shape() {
    for value in [
        TOKEN,
        OTHER,
        PASSWORD,
        ENCODED,
        HEX,
        "wLsbUAFXniClAUADRuS4",
        "7iR9AeopxGmjeBfT4XEA",
        "gHBlIzrlXDtpB5EB",
        "bDSC8tseiIuzhG1ZNmugwuAh",
        "i8umeat7fyudmaohvb0xe9hg",
        "P2FCVL6T2SDHA3ZXBQE2PYATFZS3JO54",
        "4527d0fe-c322-1f89-8652-8f228b0c8b0c",
        "tm/sapD8AiLDbn084OHzudIehyVnkGl3NacYaTozOTE5",
        "eW9VdHFGdGRvTGgyWUtFTE9jM1k=",
    ] {
        assert!(is_opaque(value, 16), "{value}");
    }
    for value in [
        "resourceClaimName",
        "getOAuth2TokenV1ForUser",
        "GitHub-OAuth2-App",
        "SOME_CONSTANT_NAME_HERE",
        "your-api-key-here-1234",
        "node18-alpine3.19-arm64v8",
        "x86_64-unknown-linux-gnu",
        "python3.11-dev-2ubuntu1",
        "utf8mb4_0900_ai_ci",
        "octocode-mcp@20.2.0-beta.1",
        "application/vnd.github.v3+json",
        "src/pkg/cfg/v2/Main.rs",
        "node_modules/.pnpm/registry.npmjs.org+js-tokens@4.0.0/node_modules",
        "ec2-54-123-45-67.compute-1.amazonaws.com",
        "Summer2024Vacation!",
        "xxxxxxxxxxxxxxxxxxxxxxxx",
        "sk-xxxxxxxxxxxxxxxxxxxx",
        "abcdef1234567890abcdef",
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        "deadbeefdeadbeefdeadbeef",
        "1234567890123456",
        "192.168.100.200:8080",
        "0x9E37_79B9_7F4A_7C15",
        "0x2545_f491_4f6c_dd1d_u64",
        "2026-11-10T22:13:22Z",
    ] {
        assert!(!is_opaque(value, 16), "{value}");
    }
}

/// A name the assignment rules did not read holds its opaque value in every
/// file kind: `name: value`, `name=value` with no blank, a JSON member, a
/// code literal.
#[test]
fn an_opaque_value_under_a_credential_name_is_hidden() {
    for name in [
        "apikey",
        "APIKEY",
        "pwd",
        "db_pwd",
        "dbPass",
        "adminPass",
        "pass_word",
        "tkn",
        "pat",
        "bearer",
        "SecretString",
        "secretValue",
        "stripe_sk",
        "x_auth",
        "encryptionKey",
        "licenseKey",
        "masterKey",
        "secretKey1",
        "sshKey",
        "clientSecret",
        "jwtSecret",
        "privateKey",
        "signature",
        "otpSeed",
        "pass",
    ] {
        for (content, path) in [
            (format!("{name}: {TOKEN}\n"), "deploy/names.yaml"),
            (format!("{name}={TOKEN}\n"), "notes/names.txt"),
            (
                format!("{{\n  \"{name}\": \"{TOKEN}\"\n}}\n"),
                "conf/row.json",
            ),
            (
                format!("const config = {{\n  {name}: \"{TOKEN}\",\n}};\n"),
                "src/names.ts",
            ),
        ] {
            for path in [Some(path), None] {
                assert_redacts_only(&content, path, TOKEN);
            }
        }
    }
}

/// A short or ambiguous name holds a credential only when its value is
/// opaque: a word, a name, a path, a version, a number or a template under
/// it stays, and so does any value under a name that only resembles one.
#[test]
fn readable_values_and_lookalike_names_stay() {
    let values = [
        "resourceClaimName",
        "user-list-page",
        "home.title.main",
        "ctrl+shift+p",
        "x-api-key",
        "node18-alpine3.19-arm64v8",
        "packages/octocode-native/crates/engine",
        "https://example.com/oauth/token",
        "${API_KEY}",
        "process.env.SECRET_KEY",
        "<generated-per-request>",
        "1234567890123456",
        "0x9E37_79B9_7F4A_7C15",
        "true",
        "none",
    ];
    for name in ["pat", "pass", "key", "auth", "tkn", "seed", "sk", "session"] {
        for value in values {
            for (content, path) in [
                (format!("{name}: {value}\n"), "deploy/names.yaml"),
                (format!("{name}={value}\n"), "notes/names.txt"),
                (format!("--{name} {value}\n"), "notes/run.txt"),
                (format!("the {name} is {value} for now\n"), "docs/notes.md"),
            ] {
                let result = detect(&content, Some(path));
                assert!(
                    !result.sanitized.contains(PLACEHOLDER),
                    "{path}: {content:?} -> {:?}",
                    result.sanitized
                );
            }
        }
    }
    for name in [
        "path",
        "pattern",
        "author",
        "keyboard",
        "tokenizer",
        "publicKey",
        "secretName",
        "keyId",
        "tokenUrl",
        "privateKeyHash",
    ] {
        for (content, path) in [
            (format!("{name}: {TOKEN}\n"), "notes/names.txt"),
            (format!("{name}={TOKEN}\n"), "notes/names.txt"),
            (format!("{name} {TOKEN}\n"), "notes/names.txt"),
        ] {
            let result = detect(&content, Some(path));
            assert_eq!(result.sanitized, content, "{name}");
        }
    }
    stays(&[
        (
            "# public key: age1qpzry9x8gf2tvdw0s3jn54khce6mua7lqpzry9x8gf2tvdw0s3jn54khce\n",
            "keys.txt",
        ),
        ("let mut seed = 0x2545_f491_4f6c_dd1d_u64;\n", "src/rng.rs"),
        (
            "state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,\n",
            "src/rng.rs",
        ),
        (
            "(\"x-oauth-client-id\", \"178c6fc778ccc68e1d6a\"),\n",
            "src/headers.rs",
        ),
        ("| sortKey | name desc size asc |\n", "docs/table.md"),
        (
            "The token is validated against the session store on every request.\n",
            "docs/auth.md",
        ),
        (
            "Pass --token to authenticate, or set the key in the settings file.\n",
            "docs/cli.md",
        ),
        (
            "it(\"redacts a password\", async () => {\n  const id = \"a8F3kL9pQ2zX7vB4\";\n});\n",
            "src/a.test.ts",
        ),
    ]);
}

/// One long line of names and values costs what its bytes cost: no value
/// reads back to its line's start or on to its end.
#[test]
fn one_long_line_of_named_values_costs_what_its_bytes_cost() {
    let unit = format!("tkn=\"{TOKEN}\";pat='{OTHER}:x';key=plainWordsForAName;");
    let few = unit.repeat(200);
    let many = unit.repeat(3_200);
    assert_scales_linearly(&few, &many, |text| {
        let idx = PATTERNS
            .iter()
            .position(|pattern| pattern.name == "credentialNamedValue")
            .expect("the pattern");
        let mut found = 0;
        super::super::named_values::secrets(pattern_regex(idx), text, |_, _| found += 1);
        assert!(found > 0);
    });
}

/// Collections nested in one another, each named like a credential and
/// closed nowhere, cost what their bytes cost: a walk that held no value is
/// paid for from a budget of the text's own length.
#[test]
fn nested_collections_cost_what_their_bytes_cost() {
    let unit = "key=[";
    let few = unit.repeat(2_000);
    let many = unit.repeat(32_000);
    assert_scales_linearly(&few, &many, |text| {
        std::hint::black_box(redact(text, Scope::Pathless));
    });
}

/// A value goes whole: characters past the opaque run that still belong to
/// it (a `:` or a bracket inside a quoted or a bare value) go with it, and a
/// sentence's closing mark does not.
#[test]
fn no_tail_of_a_value_stays_readable() {
    let quoted = format!("{TOKEN}:{OTHER},x9");
    let bare = format!("{TOKEN}:Zq7xK2m(9");
    let in_quotes = format!("dbPass = \"{quoted}\"\n");
    let unquoted = format!("adminPass={bare}\n");
    let sentence = format!("The tkn is {TOKEN}. Rotate it: {OTHER}?\n");
    hides_only(&[
        (&in_quotes, "conf/app.txt", &[&quoted]),
        (&unquoted, "conf/app.txt", &[&bare]),
        (&sentence, "docs/notes.md", &[TOKEN]),
    ]);
    stays(&[
        (
            "dbPass = \"the value is set at deploy time, see the runbook\"\n",
            "conf/app.txt",
        ),
        (
            "adminPass=someFunctionCallForTheSecret(argument)\n",
            "conf/app.txt",
        ),
    ]);
}

/// A line-ending escape (`\r`, `\n`) ends the line a value lies on, as a
/// line break does: the value goes, and the escape, the literal's close and
/// the next header stay, in one literal or across the pieces of one. A
/// backslash another escapes (`\\n`) ends nothing, and a value that is no
/// credential up to the escape goes whole: the backslash may be its own.
#[test]
fn a_line_ending_escape_ends_the_value() {
    let long = format!("{TOKEN}{}", &OTHER[..16]);
    let api_key = format!(
        "const req = \"GET / HTTP/1.1\\r\\nX-Api-Key: {long}\\r\\nHost: example.com\\r\\n\";\n"
    );
    let pieces = format!(
        "const req = \"GET / HTTP/1.1\\r\\n\" +\n  \"X-Api-Key: {long}\\r\\n\" +\n  \"Host: example.com\\r\\n\";\n"
    );
    let lf = format!("headers = 'X-Auth-Token: {TOKEN}\\nAccept: */*'\n");
    let quoted = format!("const apiKey = \"{TOKEN}\\r\\n\";\n");
    let assigned = format!("api_key = \"{TOKEN}\"\n");
    let escaped_slash = format!("x = \"X-Api-Key: {TOKEN}\\\\nZq7xK2m\";\n");
    let worded = format!("correct-horse-battery\\n{TOKEN}");
    let bare = format!("adminPass={worded}\n");
    hides_only(&[
        (&api_key, "src/req.js", &[&long]),
        (&pieces, "src/req.js", &[&long]),
        (&lf, "src/req.py", &[TOKEN]),
        (&assigned, "src/req.py", &[TOKEN]),
        (
            &escaped_slash,
            "src/req.js",
            &[&format!("{TOKEN}\\\\nZq7xK2m")],
        ),
        (&bare, "conf/app.txt", &[&worded]),
    ]);
    // A literal that holds only the value and its line break: where the
    // file's syntax reads the escape, the break stays.
    assert_redacts_only(&quoted, Some("src/req.js"), TOKEN);
    // The auth-header rules hide the token alone: the header's name, its
    // scheme and what follows the value stay.
    let bearer = format!("send(\"Authorization: Bearer {TOKEN}\\r\\nAccept: */*\\r\\n\");\n");
    hides_keeping_what_follows(&bearer, "src/req.js", TOKEN);
}

/// That `content` at `path` loses `secret` and keeps, as written, all the
/// text after it.
fn hides_keeping_what_follows(content: &str, path: &str, secret: &str) {
    let after = content.split_once(secret).map_or("", |(_, after)| after);
    for path in [Some(path), None] {
        let result = detect(content, path);
        assert!(
            !result.sanitized.contains(secret),
            "{path:?}: {:?}",
            result.sanitized
        );
        assert!(
            result.sanitized.ends_with(after),
            "{path:?}: {:?}",
            result.sanitized
        );
    }
}

/// A header of the HTTP `Sec-` namespace (`Sec-<feature>-<field>`) is a field
/// the user agent fills per request (Fetch's forbidden request-header names),
/// which no application holds: its `Key` names no credential, so the
/// WebSocket handshake's nonce stays. A `Sec` that only abbreviates a name's
/// word (`SEC_KEY`, `Sec-Key`) and every credential header still hide theirs.
#[test]
fn a_sec_namespace_header_names_no_credential() {
    let handshake = "const req = \"GET /chat HTTP/1.1\\r\\nUpgrade: websocket\\r\\n\" +\n  \"Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\\r\\n\" +\n  \"Sec-WebSocket-Version: 13\\r\\n\\r\\n\";\n";
    let response = "HTTP/1.1 101 Switching Protocols\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\n";
    stays(&[
        (handshake, "src/ws.js"),
        (handshake, "docs/issue.md"),
        (response, "docs/issue.md"),
    ]);
    assert!(!names_a_credential("Sec-WebSocket-Key"));
    assert!(!names_a_credential("sec-websocket-key"));
    for name in [
        "SEC_KEY",
        "Sec-Key",
        "sec_token",
        "secKey",
        "X-Sec-Token",
        "Proxy-Authorization",
    ] {
        assert!(names_a_credential(name), "{name}");
    }
    let long = format!("{TOKEN}{}", &OTHER[..16]);
    let headers = format!("X-Api-Key: {long}\nX-Auth-Token: {OTHER}\nSec-Key: {PADDED}\n");
    hides_only(&[(&headers, "docs/issue.md", &[&long, OTHER, PADDED])]);
    let bearer = format!("Authorization: Bearer {TOKEN}\nHost: example.com\n");
    let proxy = format!("Proxy-Authorization: Basic {ENCODED}\nHost: example.com\n");
    hides_keeping_what_follows(&bearer, "docs/issue.md", TOKEN);
    hides_keeping_what_follows(&proxy, "docs/issue.md", ENCODED);
    stays(&[
        (
            "def login(password: str, token: Token) -> None:\n",
            "src/auth.py",
        ),
        ("let dispatchToken: string;\n", "src/store.ts"),
    ]);
}
