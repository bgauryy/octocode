//! Credential assignments, encoded keys and rule boundaries.

use super::*;

/// A base64-encoded PEM private key (Kubernetes `data:`, Helm values,
/// JSON) is redacted in any window, without `kind: Secret` context.
#[test]
fn base64_encoded_private_key_is_redacted_without_manifest_context() {
    for kind in ["", "RSA ", "EC ", "OPENSSH ", "ENCRYPTED ", "PGP "] {
        let suffix = if kind == "PGP " { " BLOCK" } else { "" };
        let pem = format!(
            "-----BEGIN {kind}PRIVATE KEY{suffix}-----\n{}\n-----END {kind}PRIVATE KEY{suffix}-----\n",
            fake_b64(64)
        );
        let encoded = b64(&pem);
        for content in [
            format!("  tls.key: {encoded}\n"),
            format!("{{\"key\":\"{encoded}\"}}"),
            format!("key: |\n  {}\n  {}\n", &encoded[..76], &encoded[76..]),
        ] {
            let result = detect(&content, None);
            assert!(
                !result.sanitized.contains(&encoded[20..60]),
                "{kind}: base64 key body leaked: {}",
                result.sanitized
            );
            assert!(!result.secrets_detected.is_empty());
            assert_eq!(result.sanitized.lines().count(), content.lines().count());
        }
    }
    let cert = b64(&format!(
        "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
        fake_b64(64)
    ));
    let line = format!("  tls.crt: {cert}\n");
    assert_eq!(
        detect(&line, None).sanitized,
        line,
        "public certificates stay readable"
    );
}

/// Credential-named assignments in camelCase, JSON keys, YAML and
/// unquoted dotenv lines redact only the value and keep the identifier.
#[test]
fn credential_assignments_redact_only_the_value() {
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    let cases = [
        (
            format!("const awsSecret = \"{value}\";"),
            "const awsSecret = \"",
        ),
        (
            format!("const awsSecret: \"{value}\""),
            "const awsSecret: \"",
        ),
        (
            format!("{{\"clientSecret\": \"{value}\"}}"),
            "{\"clientSecret\": \"",
        ),
        (
            format!("AWS_SECRET_ACCESS_KEY = \"{value}\""),
            "AWS_SECRET_ACCESS_KEY = \"",
        ),
        (format!("API_SECRET={value}\n"), "API_SECRET="),
        (
            format!("export DB_PASSWORD={value} # prod\n"),
            "export DB_PASSWORD=",
        ),
        (format!("  client_secret: {value}\n"), "  client_secret: "),
        (format!("  apiKey: {value}\n"), "  apiKey: "),
        (format!("jwt_secret = \"{value}\";"), "jwt_secret = \""),
    ];
    for (content, kept) in cases {
        let result = detect(&content, None);
        assert!(
            !result.sanitized.contains(&value[..16]),
            "value leaked: {content:?} -> {:?}",
            result.sanitized
        );
        assert!(
            result.sanitized.starts_with(kept),
            "identifier lost: {content:?} -> {:?}",
            result.sanitized
        );
    }
}

/// The credential-assignment rules leave code and config references alone.
#[test]
fn credential_assignments_skip_references_and_plain_keys() {
    for content in [
        "SECRET_KEY = os.environ[\"DJANGO_SECRET_KEY\"]\n",
        "API_TOKEN=${API_TOKEN_FROM_VAULT}\n",
        "  token: ${{ secrets.GITHUB_TOKEN }}\n",
        "  - key: node-role.kubernetes.io/control-plane\n",
        "  secretName: my-ingress-tls-secret\n",
        "const tokenType = \"Bearer\";\n",
        "  password: short\n",
    ] {
        assert_eq!(detect(content, None).sanitized, content);
    }
}

/// An unquoted assignment that ends a code statement (`;`) with a name reads
/// another variable or member, not a literal: it stays verbatim in any file,
/// while the same keys in dotenv, INI, properties and shell syntax keep their
/// values redacted.
#[test]
fn credential_assignments_keep_code_statement_references() {
    for (content, path) in [
        ("        frame.expect_key = frame.object;\n", None),
        (
            "    held.key_lines = point.inside_lines;\n",
            Some("src/large_source.rs"),
        ),
        (
            "  this.secret_token = !options.secret_token;\n",
            Some("a.ts"),
        ),
        ("api_key = default_api_key;\n", None),
    ] {
        let result = detect(content, path);
        assert_eq!(result.sanitized, content, "{path:?}");
        assert!(result.secrets_detected.is_empty(), "{content:?}");
    }
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    for (content, path) in [
        (format!("API_SECRET={value}\n"), Some(".env")),
        (format!("export DB_PASSWORD={value}\n"), Some("deploy.sh")),
        (format!("db.password={value}\n"), Some("app.properties")),
        (
            format!("[db]\nsecret_key = {value}\n"),
            Some("settings.ini"),
        ),
        ("PASSWORD=correct.horse.battery.staple\n".to_owned(), None),
        ("API_TOKEN=abcdefghijklmnopqrst;\n".to_owned(), Some(".env")),
    ] {
        let result = detect(&content, path);
        assert!(
            result
                .secrets_detected
                .iter()
                .any(|name| name == "dotenvSecretAssignment"),
            "not redacted: {content:?} -> {:?}",
            result.sanitized
        );
    }
}

/// A credential-named key whose value spells a code identifier names a
/// field, so it stays verbatim in every assignment form; a credential value
/// under the same keys is still redacted.
#[test]
fn credential_assignments_keep_identifier_values() {
    for content in [
        "\"x-kubernetes-patch-merge-key\": \"resourceClaimName\",\n",
        "const sortKey = \"createdAtTimestamp\";\n",
        "PRIMARY_KEY=ResourceClaimName\n",
        "  apiKey: resourceClaimName\n",
    ] {
        let result = detect(content, None);
        assert_eq!(result.sanitized, content);
        assert!(result.secrets_detected.is_empty(), "{content:?}");
    }
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    for (content, secret) in [
        (
            format!("\"x-kubernetes-patch-merge-key\": \"{value}\""),
            value.as_str(),
        ),
        (
            "const sortKey = \"resourceClaimName7\";".to_owned(),
            "resourceClaimName7",
        ),
        (
            "const apiToken = \"resourceCLaimName\";".to_owned(),
            "resourceCLaimName",
        ),
        (
            "const apiToken = \"abcdefghijklmnopqrstuvwx\";".to_owned(),
            "abcdefghijklmnopqrstuvwx",
        ),
    ] {
        let result = detect(&content, None);
        assert!(!result.sanitized.contains(secret), "leaked: {content:?}");
        assert!(
            result
                .secrets_detected
                .iter()
                .any(|name| name == "envVarSecrets"),
            "{content:?}"
        );
    }
}

#[test]
fn identifier_names_are_whole_camel_case_words() {
    for name in [
        "resourceClaimName",
        "ContainerPort",
        "createdAtTimestamp",
        "userId",
    ] {
        assert!(is_identifier_name(name), "{name}");
    }
    for value in [
        "abcdefghijklmnopqrstuvwx",
        "resourceClaimName7",
        "resourceCLaimName",
        "aBcDeFgHiJkLmNoPq",
        "qXzvbnTrwkpLmnbvc",
        "resource_claim_name",
        "",
    ] {
        assert!(!is_identifier_name(value), "{value}");
    }
}

/// A Kubernetes Secret keeps its `kind:`/`data:` lines and line count;
/// every data value is redacted whichever key comes first.
#[test]
fn kubernetes_secret_redacts_every_data_value_in_either_key_order() {
    let one = fake_b64(24);
    let two = format!("{}xyz", fake_b64(20));
    let kind_first = format!(
        "apiVersion: v1\nkind: Secret\ndata:\n  username: {one}\n  password: {two}\nmetadata:\n  name: db\n"
    );
    let data_first = format!(
        "apiVersion: v1\ndata:\n  username: {one}\n  password: {two}\nkind: Secret\nmetadata:\n  name: db\n"
    );
    for content in [kind_first, data_first] {
        let result = detect(&content, Some("k8s/secret.yaml"));
        assert!(!result.sanitized.contains(&one), "{}", result.sanitized);
        assert!(!result.sanitized.contains(&two), "{}", result.sanitized);
        assert!(result.sanitized.contains("kind: Secret\n"));
        assert!(result.sanitized.contains("\ndata:\n"));
        assert!(result.sanitized.contains("  name: db\n"));
        assert_eq!(result.sanitized.lines().count(), content.lines().count());
    }
}

/// A Stripe-shaped `sk_live_` key carries the Stripe label.
#[test]
fn stripe_secret_key_is_labelled_stripe() {
    let key = format!("sk_{}_{}", "live", "a1B2c3D4e5".repeat(3));
    let result = detect(&format!("STRIPE = '{key}'"), None);
    assert!(!result.sanitized.contains(&key));
    assert_eq!(
        result.secrets_detected.first().map(String::as_str),
        Some("stripeSecretKey")
    );
}

/// Rules bounded by quotes fire when the quote is followed by punctuation
/// (`"…";`, `"…"}`): a `\b` beside a quote needs a word character there.
#[test]
fn quote_bounded_rules_fire_before_punctuation() {
    let id = "a1B2c3D4e5F6g7H8i9J0k1L2";
    let result = detect(&format!("{{\"client_secret\":\"{id}\"}}"), None);
    assert!(!result.sanitized.contains(id), "{}", result.sanitized);
    assert!(result.sanitized.starts_with("{\"client_secret\":\""));
}

/// A hex digest keyed by a file path is a checksum manifest entry, even when
/// the file's name holds a credential word; the same digest under a
/// credential key that names no file, or a non-digest value under a file key,
/// is still redacted.
#[test]
fn credential_assignments_keep_digests_keyed_by_file_paths() {
    let sha256 = "fdb33b8b86243923b54ae97ca58536e1fba694b759d1bf966fc5f5fb974484b6";
    for (content, path) in [
        (
            format!("   \"packages/bench/compare/bin/with_credentials.mjs\": \"{sha256}\",\n"),
            Some("runs/manifest.json"),
        ),
        (format!("{{\"with_credentials.mjs\": \"{sha256}\"}}"), None),
        (
            format!("  config\\credentials.yaml: \"{}\"\n", &sha256[..40]),
            None,
        ),
        // The same manifest entry as unquoted YAML.
        (
            format!("packages/bench/compare/bin/with_credentials.mjs: {sha256}\n"),
            None,
        ),
    ] {
        let result = detect(&content, path);
        assert_eq!(result.sanitized, content);
        assert!(result.secrets_detected.is_empty(), "{content:?}");
    }
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    for (content, secret) in [
        (format!("\"credentials\": \"{sha256}\""), sha256),
        (format!("const apiToken = \"{sha256}\";"), sha256),
        (format!("API_TOKEN=\"{}\"", &sha256[..32]), &sha256[..32]),
        (
            format!("\"bin/with_credentials.mjs\": \"{value}\""),
            value.as_str(),
        ),
        (
            format!("\"bin/with_credentials.mjs\": \"{}\"", &sha256[..50]),
            &sha256[..50],
        ),
        // Vault/Consul-style path keys name a secret, not a file: a bare-hex
        // credential (Datadog, Twilio) under them stays redacted.
        (
            format!(
                "\"secret/data/prod/datadog_api_key\": \"{}\"",
                &sha256[..32]
            ),
            &sha256[..32],
        ),
        (
            format!("\"config/myapp/twilio_auth_token\": \"{}\"", &sha256[..40]),
            &sha256[..40],
        ),
        (
            format!("secret/data/prod/datadog_app_key: {}\n", &sha256[..40]),
            &sha256[..40],
        ),
        // An unquoted YAML path key that no provider pattern knows: the
        // credential-named key alone says "credential".
        (
            format!("config/app/api_token: {}\n", &sha256[..40]),
            &sha256[..40],
        ),
    ] {
        let result = detect(&content, None);
        assert!(!result.sanitized.contains(secret), "leaked: {content:?}");
    }
}
