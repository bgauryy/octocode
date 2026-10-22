use super::*;

/// Former 500 KB chunk edge and 8 KiB overlap of the removed chunked path.
/// Large-input tests keep secrets at these offsets as regression positions.
const CHUNK_SIZE: usize = 500_000;
const CHUNK_OVERLAP: usize = 8_192;

#[test]
fn multiline_redaction_keeps_source_line_positions() {
    let regex = regex::Regex::new("BEGIN[\\s\\S]*?END").unwrap();
    let original = "before\nBEGIN\nsecret\nEND\nafter";
    let redacted = replace_secrets(&regex, original, "[REDACTED]", |_, _| true);
    assert_eq!(redacted, "before\n[REDACTED]\n\n\nafter");
    assert_eq!(redacted.lines().count(), original.lines().count());
    assert!(!redacted.contains("secret"));
}

#[test]
fn detect_returns_empty_on_blank_input() {
    let result = detect("", None);
    assert_eq!(result.sanitized, "");
    assert!(result.secrets_detected.is_empty());
}

#[test]
fn detect_no_match_returns_input_unchanged() {
    let input = "no secrets here just plain text";
    let result = detect(input, None);
    assert_eq!(result.sanitized, input);
    assert!(result.secrets_detected.is_empty());
}

#[test]
fn detect_redacts_github_token() {
    let input = "token: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let result = detect(input, None);
    assert!(result.sanitized.contains("[REDACTED-"));
    assert!(!result.secrets_detected.is_empty());
}

#[test]
fn detect_applies_file_context_when_path_matches() {
    // kubernetesSecrets pattern has file_context = r"\.ya?ml$"
    // Use a content that matches that pattern (kind: Secret … data:)
    let yaml = "kind: Secret\ndata:\n  password: c2VjcmV0cGFzc3dvcmQ=\n";
    let result_no_path = detect(yaml, None);
    let result_with_yaml = detect(yaml, Some("k8s/secret.yaml"));
    let result_with_ts = detect(yaml, Some("src/index.ts"));
    // With .yaml path → file-context pattern should fire
    assert!(result_with_yaml.has_secrets_or(&result_no_path));
    // With .ts path → file-context pattern should NOT fire (the YAML scalar
    // pattern is skipped for source code too, so the output differs from the
    // pathless net's).
    assert!(
        !result_with_ts
            .secrets_detected
            .contains(&"kubernetesSecrets".to_string()),
        "{:?}",
        result_with_ts.secrets_detected
    );
    assert!(
        result_with_yaml
            .secrets_detected
            .contains(&"kubernetesSecrets".to_string()),
        "{:?}",
        result_with_yaml.secrets_detected
    );
}

/// An unquoted `credentialName: Identifier;` is a type member in source
/// code (lib.dom.d.ts, minified plotly), never a literal secret; the same
/// line is a secret in a YAML file and in pathless content.
#[test]
fn yaml_secret_value_skips_source_code_paths() {
    let member = "interface Result {\n  password: URLPatternComponentResult;\n}\n";
    for path in ["lib.dom.d.ts", "src/plotly.min.js", "pkg/types.py"] {
        let result = detect(member, Some(path));
        assert_eq!(result.sanitized, member, "{path}");
        assert!(result.secrets_detected.is_empty(), "{path}");
    }
    let config = "db:\n  password: hunter2hunter2hunter2\n";
    for path in [Some("config/app.yaml"), Some("Procfile"), None] {
        let result = detect(config, path);
        assert!(
            result
                .secrets_detected
                .contains(&"yamlSecretValue".to_string()),
            "{path:?}: {:?}",
            result.secrets_detected
        );
    }
}

/// Output net: a string leaf of a result about a source file is scanned
/// knowing only its language. Config syntax is not a secret in code, while
/// every credential shape — and the config syntax in config files and in the
/// pathless net — stays redacted.
#[test]
fn leaf_of_source_file_skips_config_syntax_but_keeps_every_credential() {
    use super::Scope::{Leaf, Pathless};
    let member = "interface Cfg {\n  password: hunter2hunter2hunter2;\n}\n";
    for path in ["src/a.ts", "lib/b.rs", "pkg/c.py"] {
        let result = detect_in(member, Leaf(path));
        assert_eq!(result.sanitized, member, "{path}");
    }
    for scope in [
        Leaf("config/app.yaml"),
        Leaf(".env"),
        Leaf("Procfile"),
        Pathless,
    ] {
        let result = detect_in(member, scope);
        assert!(
            result
                .secrets_detected
                .contains(&"yamlSecretValue".to_string()),
            "{scope:?}: {:?}",
            result.secrets_detected
        );
    }
    let aws = format!("AKIA{}", "IOSFODNN7EXAMPLE");
    let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAxyz\n-----END RSA PRIVATE KEY-----";
    let credentials = [
        format!("const t = \"ghp_{}\";", "a".repeat(37)),
        format!("const k = \"{aws}\";"),
        pem.to_owned(),
    ];
    for credential in &credentials {
        let result = detect_in(credential, Leaf("src/secrets.ts"));
        assert!(!result.secrets_detected.is_empty(), "{credential}");
        assert!(
            result.sanitized.contains("[REDACTED-"),
            "{}",
            result.sanitized
        );
    }
}

/// A leaf's path never loosens file-context patterns: they stay content-gated
/// like the pathless net (a `.ts` leaf is not exempt from a keyword-anchored
/// pattern the pathless net applies).
#[test]
fn leaf_scope_keeps_content_gated_file_context_patterns() {
    use super::Scope::{Leaf, Pathless};
    let content = "kind: Secret\ndata:\n  password: c2VjcmV0cGFzc3dvcmQ=\n# source: manifest.yaml";
    assert_eq!(
        detect_in(content, Leaf("src/a.ts")).sanitized,
        detect_in(content, Pathless).sanitized
    );
}

#[test]
fn content_net_redacts_file_context_secret_without_path() {
    // The global output net (tool results, error/metadata strings) has no
    // file path. A `kind: Secret` data block whose surrounding text carries
    // a `.yaml` reference matches the `\.ya?ml$` anchor against the CONTENT,
    // so it is redacted even though `file_path` is None.
    let yaml = "kind: Secret\ndata:\n  password: c2VjcmV0cGFzc3dvcmQ=\n# source: manifest.yaml";
    let result = detect(yaml, None);
    assert!(
        result.sanitized.contains("[REDACTED-"),
        "k8s Secret data block must be redacted via the content net with path None: {}",
        result.sanitized
    );
    assert!(
        result
            .secrets_detected
            .contains(&"kubernetesSecrets".to_string()),
        "expected kubernetesSecrets, got: {:?}",
        result.secrets_detected
    );
}

#[test]
fn content_net_does_not_redact_bare_uuid_without_keyword_context() {
    // A bare UUID with no `.env|config|settings|secrets` keyword context in
    // the content must NOT be redacted by the keyword-gated generics (e.g.
    // azureSubscriptionId) — the content net keeps those FP-prone patterns
    // gated so ordinary UUIDs/SHAs pass through untouched.
    let bare = "requestId = 550e8400-e29b-41d4-a716-446655440000";
    let result = detect(bare, None);
    assert_eq!(result.sanitized, bare, "bare UUID must not be redacted");
}

#[test]
fn detect_large_no_match_returns_input_unchanged() {
    // Content with no secrets but length > CHUNK_SIZE to exercise the
    // pre-filter early-return path.
    let padding = "a".repeat(CHUNK_SIZE + 1);
    let result = detect(&padding, None);
    assert_eq!(result.sanitized, padding);
    assert!(result.secrets_detected.is_empty());
}

#[test]
fn detect_large_redacts_token_spanning_chunk_boundary() {
    // Place a GitHub PAT near the CHUNK_SIZE boundary so it straddles the
    // overlap window and must still be redacted.
    let prefix = "a".repeat(CHUNK_SIZE - 10);
    let token = "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let input = format!("{prefix} token={token}");
    let result = detect(&input, None);
    assert!(
        result.sanitized.contains("[REDACTED-"),
        "must redact token near chunk boundary"
    );
    assert!(!result.secrets_detected.is_empty());
}

#[test]
fn detect_large_redacts_long_secret_spanning_chunk_boundary() {
    // A multi-line PEM private key block is far longer than 1 KB and matches
    // via `[\s\S]*?`. Straddle it across the CHUNK_SIZE boundary so BEGIN sits
    // ~1.5 KB before the edge and END after it — beyond the old 1 KB overlap,
    // within the current one. Proves the widened overlap catches secrets that
    // exceed the previous window.
    let key_body = "MIIBODEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789\n".repeat(30);
    let key = format!("-----BEGIN RSA PRIVATE KEY-----\n{key_body}-----END RSA PRIVATE KEY-----");
    assert!(
        key.len() > 1_000,
        "key block must exceed the old 1 KB overlap"
    );
    let prefix = "a".repeat(CHUNK_SIZE - 1_500);
    let input = format!("{prefix}{key}\n tail");
    let result = detect(&input, None);
    assert!(
        result.sanitized.contains("[REDACTED-"),
        "must redact a >1 KB secret straddling the chunk boundary"
    );
    assert!(!result.sanitized.contains("-----BEGIN RSA PRIVATE KEY-----"));
    assert!(!result.secrets_detected.is_empty());
}

/// Review M1: an open-ended secret whose first part fits inside chunk 1
/// must not be redacted only up to the slice end (the slice end satisfies
/// `\b`), leaving its tail in clear text after `[REDACTED-*]`.
#[test]
fn detect_large_does_not_leak_tail_of_open_ended_secret_at_chunk_end() {
    let body: String = "Ab3Cd5Ef7Gh9".repeat(5); // 60 alphanumerics
    let secret = format!("sk-{body}");
    // 44 body characters land inside the first 500 KB chunk, 16 after it.
    let prefix = format!("{} ", "x".repeat(CHUNK_SIZE - 48));
    let input = format!("{prefix}{secret} tail");
    assert_eq!(input.find(&secret), Some(CHUNK_SIZE - 47));
    let result = detect(&input, None);
    let tail = &body[44..];
    assert!(
        !result.sanitized.contains(tail),
        "secret tail leaked after the chunk boundary"
    );
    assert!(!result.sanitized.contains(&secret));
}

#[test]
fn detect_large_preserves_canonical_pattern_order() {
    let input = format!(
        "{} {} {} {}",
        "sk-1234567890abcdefghijklmnopqrstuvwxyzT3BlbkFJABCDEFGHIJKLMNO",
        "AKIAIOSFODNN7EXAMPLE",
        "ghp_1234567890abcdefghijklmnopqrstuvwxyz123456",
        "x".repeat(CHUNK_SIZE)
    );

    let result = detect(&input, None);

    assert_eq!(
        result.secrets_detected,
        vec![
            "openaiApiKeyLegacy".to_string(),
            "awsAccessKeyId".to_string(),
            "githubTokens".to_string(),
        ]
    );
}

// ghp_ + 36 alphanum satisfies githubTokens regex {36,255}.
const FAKE_GH_TOKEN: &str = "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
// AKIA + 16 uppercase alphanum satisfies awsAccessKeyId regex.
const FAKE_AWS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";

/// Differential guardrail: the prescan-gated matcher must agree exactly
/// with the complete reference set across gated patterns, fallback-bucket
/// patterns, case-folded keywords, and clean content.
#[test]
fn prescan_agrees_with_reference_regex_set_on_corpus() {
    let reference_set = regex::RegexSetBuilder::new(PATTERNS.iter().map(|p| p.regex))
        .size_limit(256 * 1024 * 1024)
        .dfa_size_limit(256 * 1024 * 1024)
        .build()
        .expect("All patterns must be valid Rust regex");
    let jwt = format!(
        "eyJ{}.eyJ{}.{}",
        "a".repeat(20),
        "b".repeat(20),
        "c".repeat(20)
    );
    let corpus: Vec<String> = vec![
        format!("token={FAKE_GH_TOKEN}"),
        format!("AWS_ACCESS_KEY_ID={FAKE_AWS_KEY}"),
        format!("aws_secret_access_key = {}", "A".repeat(40)),
        jwt,
        "postgres://user:s3cr3t@db.example.com:5432/app".to_string(),
        format!(
            "-----BEGIN RSA PRIVATE KEY-----\nMII{}\n-----END RSA PRIVATE KEY-----",
            "A".repeat(64)
        ),
        format!("uuid-ish {}", "123e4567-e89b-12d3-a456-426614174000"),
        format!(
            "telegram 123456789:{}",
            "AAExampleExampleExampleExampleAAAAA"
        ),
        "perfectly clean prose with no secrets at all".to_string(),
        format!("sk-proj-{}", "x".repeat(48)),
        format!("xoxb-1234567890-1234567890123-{}", "x".repeat(24)),
        format!("SLACK_TOKEN = \"xoxp-{}\"", "1".repeat(30)),
    ];
    for content in &corpus {
        let reference: Vec<usize> = reference_set.matches(content).into_iter().collect();
        let mut prescanned = candidate_pattern_indices(content);
        prescanned.retain(|&idx| pattern_regex(idx).is_match(content));
        assert_eq!(
            reference, prescanned,
            "prescan diverged from reference RegexSet on: {content:?}"
        );
    }
}

#[test]
fn detect_redacts_aws_access_key_id() {
    let input = format!("AWS_ACCESS_KEY_ID={FAKE_AWS_KEY}");
    let result = detect(&input, None);
    assert!(
        result.sanitized.contains("[REDACTED-AWSACCESSKEYID]"),
        "expected redaction, got: {}",
        result.sanitized
    );
    assert!(
        result
            .secrets_detected
            .contains(&"awsAccessKeyId".to_string())
    );
}

// ── Straddle-proofing post-condition tests ────────────────────────────────
//
// These pin the guarantee that `detect`'s output never still matches
// a candidate pattern, even when a secret is longer than CHUNK_OVERLAP and
// lands across a 500 KB chunk boundary (invisible to every chunk slice).

/// Build an RSA-private-key block whose body is `body_lines` × 64 chars, so
/// the whole block comfortably exceeds a chosen byte size. Matches the
/// unbounded `rsaPrivateKey` regex (`[\s\S]*?` between BEGIN/END markers).
fn rsa_key(body_lines: usize) -> String {
    let body =
        "MIIBODEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789\n".repeat(body_lines);
    format!("-----BEGIN RSA PRIVATE KEY-----\n{body}-----END RSA PRIVATE KEY-----")
}

/// Post-condition assertion: for every reported pattern, its regex must no
/// longer match the sanitized output — the airtight guarantee.
fn assert_no_pattern_matches(result: &DetectResult) {
    for name in &result.secrets_detected {
        let idx = PATTERNS
            .iter()
            .position(|p| p.name == *name)
            .expect("reported pattern must exist in PATTERNS");
        assert!(
            !pattern_regex(idx).is_match(&result.sanitized),
            "pattern `{name}` still matches sanitized output — post-condition violated"
        );
    }
}

#[test]
fn detect_large_redacts_oversized_secret_straddling_boundary() {
    // A >8 KiB secret placed so it straddles the 500 KB chunk boundary with
    // BEGIN before the edge and END after it, both markers landing OUTSIDE
    // the 8 KiB overlap window. No single chunk slice contains the whole
    // block, so the chunk fast path can't see it — only the full-content
    // post-condition catches it.
    let key = rsa_key(300); // ~19 KiB, well over CHUNK_OVERLAP
    let half = key.len() / 2;
    assert!(
        half > CHUNK_OVERLAP,
        "each half of the key must exceed the overlap window so no chunk contains the whole block"
    );
    let prefix = "a".repeat(CHUNK_SIZE - half);
    let input = format!("{prefix}{key}\n tail");
    assert!(
        input.len() > CHUNK_SIZE,
        "input must exceed the former chunk edge"
    );

    let result = detect(&input, None);

    assert!(
        !result.sanitized.contains("-----BEGIN RSA PRIVATE KEY-----"),
        "oversized straddling key must be redacted"
    );
    assert!(result.sanitized.contains("[REDACTED-RSAPRIVATEKEY]"));
    assert!(
        result
            .secrets_detected
            .contains(&"rsaPrivateKey".to_string())
    );
    assert_no_pattern_matches(&result);
}

#[test]
fn detect_large_redacts_both_in_chunk_and_straddling_matches() {
    // One key fully inside chunk 1 (redacted by the fast path, so
    // `found_in_pattern` is set) AND a second oversized key straddling the
    // chunk 1/2 boundary beyond the overlap window (invisible to every
    // chunk). `found_in_pattern` alone would mark the pattern detected while
    // leaving the straddling instance in the output — the post-condition
    // must redact it too.
    let key_early = rsa_key(5); // small, fully inside chunk 1
    let key_straddle = rsa_key(300); // ~19 KiB, straddles the boundary
    let half = key_straddle.len() / 2;
    assert!(half > CHUNK_OVERLAP);
    let filler = "a".repeat(CHUNK_SIZE - half - key_early.len());
    let input = format!("{key_early}{filler}{key_straddle}\n tail");

    let result = detect(&input, None);

    assert!(
        !result.sanitized.contains("-----BEGIN RSA PRIVATE KEY-----"),
        "both the in-chunk and straddling keys must be redacted"
    );
    assert_eq!(
        result.sanitized.matches("[REDACTED-RSAPRIVATEKEY]").count(),
        2,
        "both key instances must be replaced"
    );
    assert_eq!(
        result
            .secrets_detected
            .iter()
            .filter(|n| *n == "rsaPrivateKey")
            .count(),
        1,
        "pattern must be reported exactly once"
    );
    assert_no_pattern_matches(&result);
}

/// Fake, deterministic base64 body (never a real key).
fn fake_b64(len: usize) -> String {
    "Zm9vYmFyYmF6cXV4".chars().cycle().take(len).collect()
}

fn b64(text: &str) -> String {
    use std::fmt::Write;
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let n =
            chunk.iter().fold(0u32, |acc, b| (acc << 8) | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            if i <= chunk.len() {
                let _ = write!(
                    out,
                    "{}",
                    ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char
                );
            } else {
                out.push('=');
            }
        }
    }
    out
}

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

// ── Property tests ───────────────────────────────────────────────────────
//
// `prop_large_input_redacts_token_at_former_boundaries`: ~500KB inputs with
// the token at the former chunk-edge offsets, including a multibyte character
// nearby, are always fully redacted.
//
// `prop_sanitized_has_no_raw_token_shape` pins the no-re-trigger guarantee:
// redaction output never re-exposes a raw `ghp_` token shape that a later
// pattern could match on a subsequent pass.
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    })]

    /// ~500KB inputs with the token at the former chunk edge/overlap offsets,
    /// including a multibyte character nearby: the token is always redacted.
    #[test]
    fn prop_large_input_redacts_token_at_former_boundaries(
        offset_idx in 0usize..5,
        token_idx in 0usize..4,
    ) {
        let token = match token_idx {
            0 => FAKE_GH_TOKEN.to_string(),
            1 => FAKE_AWS_KEY.to_string(),
            2 => format!("sk-{}T3BlbkFJ{}", "a".repeat(20), "a".repeat(20)),
            _ => format!("gho_{}", "a".repeat(36)),
        };
        let base = match offset_idx {
            0 => 0,
            1 => CHUNK_SIZE - token.len() - 8,
            2 => CHUNK_SIZE - token.len() / 2,
            3 => CHUNK_SIZE + CHUNK_OVERLAP / 2,
            _ => CHUNK_SIZE + CHUNK_OVERLAP + 4,
        };
        let mut prefix: String = "x".repeat(base);
        if prefix.len() > 1000 {
            // One multi-byte char well inside the prefix so the overlap
            // window crosses a non-ASCII byte (stresses find_char_boundary),
            // while the chunk edge itself stays a clean ASCII boundary.
            let pos = prefix.len() - 500;
            prefix.replace_range(pos..pos, "é");
        }
        let input = format!("{prefix}token={token}\n tail");

        let result = detect(&input, None);
        prop_assert!(!result.sanitized.contains(&token), "token leaked");
        prop_assert!(!result.secrets_detected.is_empty());
        assert_no_pattern_matches(&result);
    }

    /// Sanitized output must contain no raw secret-token prefix that a later
    /// pattern could re-match — the no-re-trigger invariant. Uses the
    /// FAKE_GH_TOKEN shape proven to redact in the unit tests above.
    #[test]
    fn prop_sanitized_has_no_raw_token_shape(
        wrap in "[ .,]{0,4}",
        rest in "[ -~]{0,40}", // printable ASCII so we don't re-invent secrets
    ) {
        let input = format!("{wrap}{FAKE_GH_TOKEN}{rest}");
        let out = detect(&input, None);
        // The redacted form is `[REDACTED-GITHUBTOKENS]` — it must NOT contain
        // the bare `ghp_` prefix followed by token chars.
        prop_assert!(
            !out.sanitized.contains("ghp_"),
            "raw token leaked into sanitized output: {:?}",
            out.sanitized
        );
    }
}

/// A detector its file context rules out is never compiled for that text:
/// the Kubernetes Secret automaton (tens of milliseconds to build) and other
/// context-gated detectors cost nothing on a call whose output never reaches
/// them, while the same text in their context still selects them.
#[test]
fn context_gated_detectors_are_selected_only_where_they_apply() {
    let index = |name: &str| {
        PATTERNS
            .iter()
            .position(|pattern| pattern.name == name)
            .expect(name)
    };
    let (kubernetes, wandb) = (index("kubernetesSecrets"), index("wandbApiKey"));
    let message = "No tool is named nope; use one of localSearch, localFetch.";
    let candidates = applicable_candidates(message, Scope::Pathless);
    assert!(!candidates.contains(&kubernetes) && !candidates.contains(&wandb));
    let manifest = "kind: Secret\ndata:\n  password: c2VjcmV0\n";
    assert!(applicable_candidates(manifest, Scope::File("k8s/secret.yaml")).contains(&kubernetes));
    assert!(!applicable_candidates(manifest, Scope::File("k8s/secret.txt")).contains(&kubernetes));
    let detected = detect(manifest, Some("k8s/secret.yaml"));
    assert_eq!(detected.secrets_detected, ["kubernetesSecrets"]);
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
