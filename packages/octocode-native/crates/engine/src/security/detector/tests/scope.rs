//! Detection by scope: file-context patterns, source-code paths and the content net.

use super::*;

#[test]
fn multiline_redaction_keeps_source_line_positions() {
    let regex = regex::Regex::new("BEGIN[\\s\\S]*?END").unwrap();
    let original = "before\nBEGIN\nsecret\nEND\nafter";
    let (redacted, replaced) = replace_secrets(&regex, original, "[REDACTED]", |_, _, _| true);
    assert_eq!(replaced, vec![(7..23, "[REDACTED]\n\n".len())]);
    assert_eq!(redacted, "before\n[REDACTED]\n\n\nafter");
    assert_eq!(redacted.lines().count(), original.lines().count());
    assert!(!redacted.contains("secret"));
}

/// A leaf judged whole ([`Scope::Judged`]) is never judged by fewer patterns
/// than the leaf net: every pattern the leaf net applies is applied by the
/// whole-source pass or by the judged leaf's own pass, for every pattern,
/// leaf path, source path and text that can trigger a file context.
#[test]
fn the_whole_source_pass_and_the_judged_leaf_apply_every_leaf_pattern() {
    let paths = [
        "src/a.rs",
        "/w/src/a.rs",
        "app/.env",
        ".env.local",
        "config/values.yaml",
        "docker-compose.yml",
        "settings.py",
        "notes.txt",
    ];
    let texts = [
        "",
        "plain text",
        ".env config settings secrets",
        "values.yaml docker-compose.yml wandb mollie postmark",
        "apiVersion: v1\nkind: Secret\n",
    ];
    let mut checked = 0;
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        for leaf in paths.iter().copied().map(Some).chain([None]) {
            for source in paths {
                for text in texts {
                    let whole = should_apply(idx, Scope::File(source), text);
                    let judged = should_apply(idx, Scope::Judged { leaf, source }, text);
                    let name = pattern.name;
                    assert!(
                        !(whole && judged),
                        "{name} runs twice: leaf {leaf:?}, source {source}, text {text:?}"
                    );
                    if !should_apply(idx, Scope::of_leaf(leaf), text) {
                        assert!(!judged, "{name} runs past the leaf net: {leaf:?} {text:?}");
                        continue;
                    }
                    checked += 1;
                    assert!(
                        whole || judged,
                        "{name}: leaf {leaf:?}, source {source}, text {text:?}"
                    );
                }
            }
        }
    }
    assert!(checked > PATTERNS.len());
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
    // dockerComposeSecrets has file_context = r"docker-compose\.ya?ml$".
    let yaml = "services:\n  db:\n    environment:\n      - POSTGRES_PASSWORD=s3cretpw\n";
    let result_with_yaml = detect(yaml, Some("deploy/docker-compose.yaml"));
    let result_with_ts = detect(yaml, Some("src/index.ts"));
    let compose = "dockerComposeSecrets".to_string();
    assert!(
        result_with_yaml.secrets_detected.contains(&compose),
        "{:?}",
        result_with_yaml.secrets_detected
    );
    assert!(
        !result_with_ts.secrets_detected.contains(&compose),
        "{:?}",
        result_with_ts.secrets_detected
    );
}

/// D2 (round 13): a Kubernetes Secret says what it is, so its `data` and
/// `stringData` values are hidden in any text that holds it, by content:
/// a `.yaml` file, a `.txt` copy, a source file's string, pathless text.
#[test]
fn a_kubernetes_secret_is_known_by_its_content() {
    let manifest = "apiVersion: v1\nkind: Secret\nmetadata:\n  name: app\nstringData:\n  extra: Fz8kQ2mL9xW4\ndata:\n  username: YWRtaW4=\n";
    for path in [
        None,
        Some("k8s/secret.yaml"),
        Some("k8s/secret.txt"),
        Some("notes.md"),
    ] {
        let result = detect(manifest, path);
        assert_eq!(
            result.sanitized,
            "apiVersion: v1\nkind: Secret\nmetadata:\n  name: app\nstringData:\n  extra: [REDACTED-KUBERNETESSECRETS]\ndata:\n  username: [REDACTED-KUBERNETESSECRETS]\n",
            "{path:?}"
        );
    }
    // A ConfigMap's data is no secret anywhere.
    let config = "kind: ConfigMap\ndata:\n  mode: YWRtaW4=\n";
    assert_eq!(detect(config, None).sanitized, config);
    assert_eq!(detect(config, Some("k8s/cm.txt")).sanitized, config);
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
    for (path, code) in [
        ("src/a.ts", member),
        (
            "lib/b.rs",
            "struct Cfg {\n    password: hunter2hunter2hunter2,\n}\n",
        ),
        (
            "pkg/c.py",
            "class Cfg:\n    password: hunter2hunter2hunter2\n",
        ),
    ] {
        let result = detect_in(code, Leaf(path));
        assert_eq!(result.sanitized, code, "{path}");
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
