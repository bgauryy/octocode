//! Private keys no armor marks: the context on their own line, or near them,
//! says what they are, and the public half of each format stays readable.

use super::*;

const WG: &str = "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=";
const WG_PUBLIC: &str = "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=";
const FIELD: &str = "Fake0Scalar9Not4Real8Value7Long6Enough5Xy";
const OTHER: &str = "Other1Scalar2Not3Real4Value5Long6Enough7Zz";
const PUBLIC: &str = "Public0Coordinate9Stays8Readable7Always6Ab";
const BECH: &str = "QPZRY9X8GF2TVDW0S3JN54KHCE6MUA7LQPZRY9X8GF2TVDW0S3JN54KHCE";
const SIGNED: &str = "RWQfZm9vYmFyYmF6cXV4Zm9vYmFyYmF6cXV4Zm9vYmFyYmF6cXV4Zm9vYmFyYmF6";

fn scan(text: &str) -> String {
    detect(text, Some("conf/app.conf")).sanitized
}

/// Hidden in the ASCII and the full form of its pattern, and through the
/// prescan gate; `kept` stays beside it.
fn hides(text: &str, secrets: &[&str], kept: &[&str]) {
    for haystack in [text.to_owned(), format!("caf\u{e9}\n{text}\n\u{e9}")] {
        let result = detect(&haystack, Some("conf/app.conf"));
        for secret in secrets {
            assert!(
                !result.sanitized.contains(secret),
                "{secret} in {}",
                result.sanitized
            );
        }
        for piece in kept {
            assert!(
                result.sanitized.contains(piece),
                "{piece} lost: {}",
                result.sanitized
            );
        }
        assert_eq!(
            result.sanitized.split('\n').count(),
            haystack.split('\n').count(),
            "{}",
            result.sanitized
        );
        assert_no_pattern_matches(&result);
    }
}

fn unchanged(text: &str) {
    assert_eq!(scan(text), text);
}

#[test]
fn a_wireguard_config_hides_its_private_and_preshared_keys_only() {
    hides(
        &format!(
            "[Interface]\nPrivateKey = {WG}\nAddress = 10.0.0.1/24\n\n[Peer]\nPublicKey = {WG_PUBLIC}\nPresharedKey = {WG}\n"
        ),
        &[WG],
        &[WG_PUBLIC, "Address = 10.0.0.1/24"],
    );
    hides(&format!("privatekey={WG}\r\n"), &[WG], &[]);
    hides(&format!("# PrivateKey = {WG}\n"), &[WG], &[]);
}

#[test]
fn wireguard_near_misses_stay() {
    unchanged(&format!("[Peer]\nPublicKey = {WG_PUBLIC}\n"));
    unchanged("PrivateKey = <your key here>\nPrivateKey = (hidden)\n");
    unchanged("PostUp = wg set %i private-key /etc/wireguard/key\n");
    unchanged(&format!("Checksum = {WG}\nMyPrivateKeyHash = {WG}\n"));
    // Not 32 bytes: its padding cannot end a WireGuard key.
    unchanged("PrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBm1=\n");
}

#[test]
fn a_jwk_hides_its_private_members_and_keeps_the_public_ones() {
    let ec = |d: &str| {
        format!(r#"{{"kty":"EC","crv":"P-256","x":"{PUBLIC}","y":"{PUBLIC}","d":"{d}"}}"#)
    };
    hides(&ec(FIELD), &[FIELD], &[PUBLIC, "P-256"]);
    // Sorted keys put `d` above `kty`, and a pretty printer on its own line.
    hides(
        &format!(
            "{{\n  \"crv\": \"P-256\",\n  \"d\": \"{FIELD}\",\n  \"kty\": \"EC\",\n  \"x\": \"{PUBLIC}\"\n}}\n"
        ),
        &[FIELD],
        &[PUBLIC],
    );
    // Every private member of an RSA key, either side of `kty`.
    hides(
        &format!(
            r#"{{"d":"{FIELD}","dp":"{OTHER}","dq":"{FIELD}","e":"AQAB","kty":"RSA","n":"{PUBLIC}","p":"{OTHER}","q":"{FIELD}","qi":"{OTHER}"}}"#
        ),
        &[FIELD, OTHER],
        &[PUBLIC, "AQAB"],
    );
    hides(
        r#"{"kty":"oct","k":"GawgguFyGrWKav7AX4VKUg"}"#,
        &["GawgguFyGrWKav7AX4VKUg"],
        &[],
    );
    hides(
        &format!(r#"{{"kty":"AKP","pub":"{PUBLIC}","priv":"{FIELD}"}}"#),
        &[FIELD],
        &[PUBLIC],
    );
    // Each key of a set, and a key held as a string inside other JSON.
    hides(
        &format!(r#"{{"keys":[{},{}]}}"#, ec(FIELD), ec(OTHER)),
        &[FIELD, OTHER],
        &[PUBLIC],
    );
    hides(
        &format!(r#"{{"env":"{{\"kty\":\"EC\",\"x\":\"{PUBLIC}\",\"d\":\"{FIELD}\"}}"}}"#),
        &[FIELD],
        &[PUBLIC],
    );
    // Source code and YAML spell the names bare.
    hides(
        &format!("const k = {{\n  kty: 'EC',\n  x: '{PUBLIC}',\n  d: '{FIELD}',\n}};\n"),
        &[FIELD],
        &[PUBLIC],
    );
    hides(
        &format!("keys:\n  - kty: EC\n    x: {PUBLIC}\n    d: {FIELD}\n"),
        &[FIELD],
        &[PUBLIC],
    );
}

#[test]
fn jwk_near_misses_stay() {
    // Public keys.
    unchanged(&format!(
        r#"{{"kty":"EC","crv":"P-256","x":"{PUBLIC}","y":"{PUBLIC}","kid":"{PUBLIC}"}}"#
    ));
    unchanged(&format!(
        r#"{{"kty":"RSA","n":"{PUBLIC}","e":"AQAB","alg":"RS256"}}"#
    ));
    // The same member names with no `kty` in their object.
    unchanged(&format!(
        r#"{{"d":"{FIELD}","k":"{FIELD}","p":"{FIELD}","q":"{FIELD}"}}"#
    ));
    unchanged(&format!(
        r#"{{"a":{{"kty":"EC","x":"{PUBLIC}"}},"b":{{"d":"{FIELD}"}}}}"#
    ));
    // A name that only ends in a member's letter, a short value, code reading a name.
    unchanged(&format!(r#"{{"kty":"EC","id":"{FIELD}","pk":"{FIELD}"}}"#));
    unchanged(r#"A JWK has "kty": "EC" and "d": "...", the private scalar."#);
    unchanged("const k = { kty: 'EC', d: privateScalarEncoded };\n");
    // `blockType:` is no `kty`.
    unchanged(&format!("const m = {{ blockType: 'a', d: '{FIELD}' }};\n"));
}

#[test]
fn a_minisign_or_signify_secret_key_file_is_hidden() {
    for comment in [
        "minisign encrypted secret key",
        "minisign secret key",
        "rsign encrypted secret key",
        "signify secret key",
    ] {
        let value = format!("{FIELD}{OTHER}{FIELD}");
        hides(
            &format!("untrusted comment: {comment}\n{value}\n"),
            &[&value],
            &[comment],
        );
        hides(
            &format!("untrusted comment: {comment}\r\n{value}\r\n"),
            &[&value],
            &[comment],
        );
    }
    // A comment of the owner's own: the key's header says what it is.
    for head in ["RWRTY0Iy", "RWQAAEIy", "RWRCSwAA"] {
        let value = format!("{head}{FIELD}{OTHER}");
        hides(
            &format!("untrusted comment: release key\n{value}\n"),
            &[&value],
            &["release key"],
        );
    }
    // Held in a JSON string, or base64 of the whole file as Tauri takes it.
    let value = format!("{FIELD}{OTHER}{FIELD}");
    hides(
        &format!(r#"{{"k":"untrusted comment: rsign encrypted secret key\n{value}\n"}}"#),
        &[&value],
        &[],
    );
    let tauri =
        "dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5ClJXUlRZMEl5ZmFrZWZha2VmYWtl";
    hides(&format!("{tauri}\n"), &[tauri], &[]);
}

#[test]
fn minisign_near_misses_stay() {
    // The header alone, too short to hold a key.
    unchanged("RWRTY0IyFake0Scalar9Not4Real\n");
    unchanged(&format!(
        "untrusted comment: minisign public key 26E9D2B3A1C8F4D0\n{SIGNED}\n"
    ));
    unchanged(&format!(
        "untrusted comment: signify public key\n{SIGNED}\n"
    ));
    unchanged(&format!(
        "untrusted comment: signature from minisign secret key\n{SIGNED}\ntrusted comment: timestamp:1\n{SIGNED}\n"
    ));
    unchanged(&format!(
        "untrusted comment: signature from signify secret key\n{SIGNED}\n"
    ));
    unchanged(&format!(
        "untrusted comment: verify with key.pub\n{SIGNED}\n"
    ));
    unchanged(
        "The first line reads `untrusted comment: minisign encrypted secret key`.\nThe next line holds the key.\n",
    );
    // Tauri's public key is the same encoding of the public comment.
    unchanged(
        "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDI2RTlEMkIzQTFDOEY0RDAKUldRZmFrZQ==\n",
    );
}

#[test]
fn age_identities_of_every_kind_are_hidden_and_recipients_stay() {
    for identity in [
        format!("AGE-SECRET-KEY-1{BECH}"),
        format!("AGE-SECRET-KEY-PQ-1{BECH}{BECH}"),
        format!("AGE-PLUGIN-YUBIKEY-1{BECH}"),
    ] {
        hides(
            &format!("# created: 2026-01-01\n{identity}\n"),
            &[&identity],
            &["# created"],
        );
    }
    let lower = BECH.to_ascii_lowercase();
    unchanged(&format!(
        "# public key: age1{lower}\nage1{lower}\nage1pq1{lower}\n"
    ));
    unchanged("AGE-SECRET-KEY-1 goes in the identity file.\n");
}

#[test]
fn wallet_keys_are_hidden_and_their_public_forms_stay() {
    let lower = BECH.to_ascii_lowercase();
    let nsec = format!("nsec1{lower}");
    hides(&format!("key: {nsec}\n"), &[&nsec], &["key: "]);
    unchanged(&format!("npub1{lower}\n"));
    let base58 = "9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPPqjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHi";
    for head in ["xprv", "yprv", "zprv", "tprv"] {
        let key = format!("{head}{base58}");
        hides(&format!("root = {key}\n"), &[&key], &["root = "]);
    }
    unchanged(&format!("root = xpub{base58}\n"));
}

#[test]
fn a_url_encoded_private_key_is_hidden_to_the_end_of_its_value() {
    const BODY: &str = "MIIEpAIBAAKCAQEAfakebodylinefakebodylinefakebodyline";
    for (open, close) in [
        (
            "-----BEGIN%20RSA%20PRIVATE%20KEY-----%0A",
            "%0A-----END%20RSA%20PRIVATE%20KEY-----%0A",
        ),
        (
            "-----BEGIN+PRIVATE+KEY-----%0A",
            "%0A-----END+PRIVATE+KEY-----%0A",
        ),
        (
            "%2D%2D%2D%2D%2DBEGIN%20OPENSSH%20PRIVATE%20KEY%2D%2D%2D%2D%2D%0A",
            "%0A%2D%2D%2D%2D%2DEND%20OPENSSH%20PRIVATE%20KEY%2D%2D%2D%2D%2D%0A",
        ),
        (
            "%2d%2d%2d%2d%2dBEGIN%20EC%20PRIVATE%20KEY%2d%2d%2d%2d%2d%0a",
            "%0a%2d%2d%2d%2d%2dEND%20EC%20PRIVATE%20KEY%2d%2d%2d%2d%2d%0a",
        ),
        (
            "-----BEGIN%2520RSA%2520PRIVATE%2520KEY-----%250A",
            "%250A-----END%2520RSA%2520PRIVATE%2520KEY-----%250A",
        ),
        (
            "----%20BEGIN%20SSH2%20PRIVATE%20KEY%20----%0A",
            "%0A----%20END%20SSH2%20PRIVATE%20KEY%20----%0A",
        ),
    ] {
        hides(
            &format!("https://h/x?pem={open}{BODY}{close}&next=keepme\n"),
            &[BODY],
            &["next=keepme", "https://h/x?pem="],
        );
        // No END: the value ends where its URL does.
        hides(&format!("pem={open}{BODY} tail\n"), &[BODY], &[" tail"]);
    }
}

#[test]
fn url_encoded_near_misses_stay() {
    const BODY: &str = "MIIEpAIBAAKCAQEAfakebodylinefakebodylinefakebodyline";
    unchanged(&format!(
        "cert=-----BEGIN%20CERTIFICATE-----%0A{BODY}%0A-----END%20CERTIFICATE-----%0A\n"
    ));
    unchanged(&format!(
        "pub=-----BEGIN%20PUBLIC%20KEY-----%0A{BODY}%0A-----END%20PUBLIC%20KEY-----%0A\n"
    ));
    unchanged("A link: https://h/search?q=BEGIN%20PRIVATE%20KEY&lang=en\n");
}

/// A Secret as `kubectl -o json` writes it names its kind beside its data, in
/// any file; an annotation holds the same manifest as an escaped string.
#[test]
fn a_kubernetes_secret_in_json_hides_its_data() {
    const VALUE: &str = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
    let applied = format!(
        r#""{{\"apiVersion\":\"v1\",\"data\":{{\"k\":\"{VALUE}\"}},\"kind\":\"Secret\"}}""#
    );
    for text in [
        format!(
            "{{\n  \"apiVersion\": \"v1\",\n  \"data\": {{\n    \"tls.crt\": \"{VALUE}\",\n    \"tls.key\": \"{VALUE}\"\n  }},\n  \"kind\": \"Secret\",\n  \"metadata\": {{ \"name\": \"x\" }}\n}}\n"
        ),
        format!(
            r#"{{"kind":"Secret","metadata":{{"name":"x"}},"stringData":{{"password":"{VALUE}"}}}}"#
        ),
        format!(
            r#"{{"data":{{"k":"{VALUE}"}},"kind":"Secret","metadata":{{"annotations":{{"last":{applied}}}}}}}"#
        ),
    ] {
        let result = detect(&text, Some("conf/secret.json"));
        assert!(!result.sanitized.contains(VALUE), "{}", result.sanitized);
        assert_eq!(result.sanitized.lines().count(), text.lines().count());
        assert!(result.sanitized.contains("Secret"), "{}", result.sanitized);
        let bare = detect(&text, None);
        assert!(!bare.sanitized.contains(VALUE), "{}", bare.sanitized);
    }
    // A List: each item's kind speaks for its own data.
    let listed = detect(
        &format!(
            r#"{{"items":[{{"data":{{"blob":"Plain0ConfigMap9Value8Stays"}},"kind":"ConfigMap"}},{{"data":{{"k":"{VALUE}"}},"kind":"Secret"}}],"kind":"List"}}"#
        ),
        None,
    );
    assert!(!listed.sanitized.contains(VALUE), "{}", listed.sanitized);
    assert!(
        listed.sanitized.contains("Plain0ConfigMap9Value8Stays"),
        "{}",
        listed.sanitized
    );
    for text in [
        format!(r#"{{"apiVersion":"v1","data":{{"blob":"{VALUE}"}},"kind":"ConfigMap"}}"#),
        format!(r#"{{"data":{{"blob":"{VALUE}"}},"kind":"Report"}}"#),
        format!(r#"{{"secret":"short","data":{{"blob":"{VALUE}"}}}}"#),
    ] {
        unchanged(&text);
    }
}

/// Armors naming a secret or static key rather than a private one are whole
/// blocks like the rest: a short last line goes with them.
#[test]
fn a_secret_or_static_key_armor_is_a_block() {
    for (open, close) in [
        (
            "-----BEGIN PGP SECRET KEY BLOCK-----",
            "-----END PGP SECRET KEY BLOCK-----",
        ),
        ("-----BEGIN SECRET KEY-----", "-----END SECRET KEY-----"),
        (
            "-----BEGIN OpenVPN Static key V1-----",
            "-----END OpenVPN Static key V1-----",
        ),
    ] {
        hides(
            &format!("{open}\n\n{FIELD}\nshortLastLine==\n{close}\nafter\n"),
            &[FIELD, "shortLastLine"],
            &["after"],
        );
    }
    unchanged(&format!(
        "-----BEGIN PGP PUBLIC KEY BLOCK-----\n{FIELD}\n-----END PGP PUBLIC KEY BLOCK-----\n"
    ));
}
