//! Values of several pieces under opaque-string keys that are data, not
//! prose: a serialized structure, `name:value` pairs, an auth scheme and its
//! credential, grouped codes, lines an escape separates.

use super::*;

/// The keys of the retest rows, one per credential word that names an opaque
/// string, in each assignment form a reader meets.
fn rows(key: &str, value: &str) -> [(String, Option<&'static str>); 3] {
    let json = value.replace('\\', "\\\\").replace('"', "\\\"");
    [
        (
            format!("{{\"{key}\": \"{json}\"}}\n"),
            Some("fixtures/row.json"),
        ),
        (format!("{{\"{key}\": \"{json}\"}}\n"), None),
        (format!("  {key}: '{value}'\n"), Some("deploy/values.yaml")),
    ]
}

fn assert_redacts(key: &str, value: &str, secrets: &[&str]) {
    for (content, path) in rows(key, value) {
        let result = detect(&content, path);
        for secret in secrets {
            assert!(
                !result.sanitized.contains(secret),
                "{path:?} leaked {secret:?}: {content:?} -> {:?}",
                result.sanitized
            );
        }
        assert!(!result.secrets_detected.is_empty(), "{path:?}: {content:?}");
        assert!(
            result.sanitized.contains(key) && result.sanitized.ends_with('\n'),
            "the key and the line stay: {:?}",
            result.sanitized
        );
    }
}

/// A serialized structure is data whatever words it holds: a credential blob
/// stored as a string is no description.
#[test]
fn a_serialized_structure_under_an_opaque_string_key_is_redacted() {
    for (key, value, secrets) in [
        (
            "dbCredentials",
            r#"{"username": "admin", "password": "letmein"}"#,
            &["letmein", "admin"][..],
        ),
        (
            "serviceCredentials",
            r#"{"user": "deploy", "pass": "opensesame"}"#,
            &["opensesame", "deploy"],
        ),
        (
            "privateKey",
            r#"[{"user": "admin"}, {"pass": "opensesame"}]"#,
            &["opensesame"],
        ),
        (
            "licenseKey",
            "user: admin, password: letmein",
            &["letmein", "admin"],
        ),
        (
            "clientKey",
            "login: deploy; pass: opensesame",
            &["opensesame", "deploy"],
        ),
    ] {
        assert_redacts(key, value, secrets);
    }
}

/// A bare `name:value` pair has no blank after its colon as a sentence's
/// colon has, and an auth scheme's word is followed by its credential: one
/// word beside one marked piece is no sentence.
#[test]
fn pairs_and_auth_schemes_under_an_opaque_string_key_are_redacted() {
    for (key, value, secrets) in [
        (
            "apiCredentials",
            "admin:opensesame backup:welcome",
            &["opensesame", "welcome"][..],
        ),
        (
            "encryptionKey",
            "admin:letmein root:toor",
            &["letmein", "toor"],
        ),
        ("signingKey", "Basic admin:opensesame", &["opensesame"]),
        (
            "apiCredentials",
            "please use the pair admin:opensesame for the first login",
            &["opensesame"],
        ),
        (
            "accessToken",
            "Bearer tofa.bime.redu.loka",
            &["tofa.bime", "redu.loka"],
        ),
        ("accessToken", "Bearer abc.def.ghi", &["abc.def"]),
        (
            "encryptionKey",
            "a.1.b.2 c.3.d.4 e.5.f.6",
            &["a.1.b.2", "c.3.d.4"],
        ),
        (
            "authToken",
            "Zebra.Mango.Quilt.Otter Piano.Rocket",
            &["Mango.Quilt"],
        ),
    ] {
        assert_redacts(key, value, secrets);
    }
}

/// Groups of digits or of letters, all of one length, are a code a person
/// reads off (a recovery code, a card number, an app password): a sentence's
/// words and numbers vary in length.
#[test]
fn grouped_codes_under_an_opaque_string_key_are_redacted() {
    for (key, value, secrets) in [
        (
            "recoveryKey",
            "backup codes 4821-9930 1177-5512 9034-2281",
            &["4821-9930", "1177-5512", "9034-2281"][..],
        ),
        (
            "apiKey",
            "my key 1234-5678-9012-3456",
            &["1234-5678", "9012-3456"],
        ),
        ("token", "code 4821-9930-1177 pin 5512", &["4821-9930"]),
        (
            "token",
            "1234 5678 9012 3456 card number",
            &["1234 5678", "9012 3456"],
        ),
        (
            "recoveryKey",
            "the backup codes are 4821-9930 and 1177-5512 for now",
            &["4821-9930", "1177-5512"],
        ),
        (
            "privateKey",
            "abcd efgh ijkl mnop",
            &["abcd efgh", "ijkl mnop"],
        ),
        (
            "clientKey",
            "qhxv ktzm wjrp ysbn",
            &["qhxv ktzm", "wjrp ysbn"],
        ),
        (
            "appToken",
            "mocu wika jive zaxo",
            &["mocu wika", "jive zaxo"],
        ),
        (
            "licenseKey",
            "KaToFiMe-ReDuLoKa-PiZoWaNe CuHyTeBo",
            &["KaToFiMe", "ReDuLoKa", "PiZoWaNe"],
        ),
    ] {
        assert_redacts(key, value, secrets);
    }
}

/// Words that come out of one length are still words: every group is
/// pronounceable, they count through no alphabet and hold few of the letters
/// words seldom use. No dictionary is read, so a generated code with all
/// three (`tofa bime redu loka`, about one in thirty) reads as words too.
#[test]
fn same_length_words_under_an_opaque_string_key_stay() {
    for (key, value) in [
        ("orderToken", "date time size name"),
        ("groupKey", "east west north south"),
        ("sortToken", "NAME DATE SIZE KIND"),
        ("dedupeKey", "path line side role"),
        ("clientKey", "tofa bime redu loka"),
    ] {
        for (content, path) in rows(key, value) {
            let result = detect(&content, path);
            assert_eq!(result.sanitized, content, "{path:?}");
        }
    }
}

/// A run of letters as long as an opaque literal is no dictionary word, and
/// an escaped line break separates lines, each judged alone: a token on its
/// own line is a token.
#[test]
fn long_runs_and_escaped_lines_under_an_opaque_string_key_are_redacted() {
    for (key, value, secrets) in [
        (
            "credentials",
            "token tofabimereduloka",
            &["tofabimereduloka"][..],
        ),
        (
            "clientKey",
            "tofabimereduloka pizowanecuhyte",
            &["tofabimereduloka", "pizowanecuhyte"],
        ),
    ] {
        assert_redacts(key, value, secrets);
    }
    for (content, path) in [
        (
            "{\"accessToken\":\"live\\ntofabimereduloka\"}\n",
            Some("fixtures/row.json"),
        ),
        (
            "{\"accessToken\":\"live\\ntofabimerelok\"}\n",
            Some("fixtures/row.json"),
        ),
        ("{\"accessToken\":\"live\\ttofabimerelok\"}\n", None),
        (
            "ACCESS_TOKEN=live\\ntofabimereduloka\n",
            Some("notes/prose.txt"),
        ),
        ("ACCESS_TOKEN=live\\ntofabimereduloka\n", None),
    ] {
        let result = detect(content, path);
        assert!(
            !result.sanitized.contains("tofabime"),
            "{path:?} leaked: {content:?}"
        );
        assert!(
            result.sanitized.contains("ACCESS_TOKEN") || result.sanitized.contains("accessToken")
        );
    }
}

/// The descriptions these rules must leave readable: a sentence with an
/// example inside it, its words outnumbering its marked pieces.
#[test]
fn descriptions_under_an_opaque_string_key_stay_readable() {
    for value in [
        "each hit line key path, e.g. the name",
        "each hit line key path such as devDependencies.vite here",
        "Listens on 127.0.0.1:8080 by default, i.e. loopback only.",
        "meta form value 1",
        "Default: none",
        "Note: the first line holds the name.\\nThe second line holds its value.",
        "see page 7519 and section 4.1 of the guide for the claim names",
        "one of read-only, read-write or left-hand-side",
    ] {
        for content in [
            format!("{{\"matchKeys\":\"{value}\"}}\n"),
            format!("{{\"signingKey\":\"{value}\"}}\n"),
            format!("  access_token: \"{value}\"\n"),
        ] {
            for path in [None, Some("fixtures/row.json"), Some("src/model.rs")] {
                let result = detect(&content, path);
                assert_eq!(result.sanitized, content, "{path:?}");
            }
        }
    }
}

/// Every description of the generated CLI presentation contract reads whole:
/// no key of it names a credential's value.
#[test]
fn the_cli_presentation_contract_holds_no_placeholder() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../octocode-config/contract/cli-presentation.json"
    );
    let content = std::fs::read_to_string(path).expect("the generated contract");
    let result = detect(
        &content,
        Some("packages/octocode-config/contract/cli-presentation.json"),
    );
    assert!(
        result.secrets_detected.is_empty(),
        "{:?}",
        result.secrets_detected
    );
    assert_eq!(result.sanitized, content);
    let pathless = detect(&content, None);
    assert_eq!(
        pathless.sanitized, content,
        "{:?}",
        pathless.secrets_detected
    );
}
