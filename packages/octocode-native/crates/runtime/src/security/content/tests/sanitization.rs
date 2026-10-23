use super::*;

#[test]
fn credential_keys_are_rejected_without_echoing_or_renaming_them() {
    let token = format!("ghp_{}", "a".repeat(36));
    let params = serde_json::json!({"state":[{"nested":{token.clone():"ordinary value"}}]});
    let checked = ContentSecurity::new().validate_state_parameters(&params, 32_000);
    assert!(!checked.is_valid);
    assert!(checked.has_secrets);
    assert!(!format!("{checked:?}").contains(&token));
}

#[test]
fn only_declared_state_inherits_the_current_evidence_limit() {
    let policy = ContentSecurity::new();
    let evidence = serde_json::json!({"state":{"draft":"x".repeat(20_000)}});
    assert!(policy.validate_state_parameters(&evidence, 32_000).is_valid);
    assert!(!policy.validate_input_parameters(&evidence).is_valid);
    for other in [
        serde_json::json!({"matchString":"x".repeat(20_000)}),
        serde_json::json!({"resources":[{"value":"x".repeat(20_000)}]}),
        serde_json::json!({"filter":{"state":"x".repeat(20_000)}}),
    ] {
        assert!(!policy.validate_state_parameters(&other, 32_000).is_valid);
    }
}

#[test]
fn state_limit_counts_complete_unicode_evidence_and_nested_arrays() {
    let policy = ContentSecurity::new();
    let unicode = serde_json::json!({"state":[[{"text":"😀".repeat(20_000)}]]});
    assert!(policy.validate_state_parameters(&unicode, 32_000).is_valid);
    let over = serde_json::json!({"state":["a".repeat(17_000),"b".repeat(17_000)]});
    assert!(!policy.validate_state_parameters(&over, 32_000).is_valid);
    let long = serde_json::json!({"state":"x".repeat(40_000)});
    assert!(!policy.validate_state_parameters(&long, 32_000).is_valid);
    assert!(policy.validate_state_parameters(&long, 256_000).is_valid);
    let token = format!("ghp_{}", "a".repeat(37));
    let value = serde_json::json!({"state":[[{"text":format!("{} {token}","a".repeat(20_000))}]]});
    let checked = policy.validate_state_parameters(&value, 32_000);
    assert!(checked.is_valid && checked.has_secrets);
    assert!(
        !serde_json::to_string(&checked.sanitized_params)
            .unwrap()
            .contains(&token)
    );
}

#[test]
fn nested_arrays_cannot_bypass_parameter_bounds() {
    let policy = ContentSecurity::new();
    assert!(
        !policy
            .validate_input_parameters(&serde_json::json!({"x":[["x".repeat(10_001)]]}))
            .is_valid
    );
    assert!(
        !policy
            .validate_input_parameters(&serde_json::json!({"x":[[(0..101).collect::<Vec<_>>()]]}))
            .is_valid
    );
    let mut deep = serde_json::json!("safe");
    for _ in 0..25 {
        deep = serde_json::json!([deep]);
    }
    assert!(
        !policy
            .validate_input_parameters(&serde_json::json!({"x":deep}))
            .is_valid
    );
}

#[test]
fn split_key_window_does_not_leak_through_sanitize_text() {
    let policy = ContentSecurity::new();
    let window = "config header line\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEA7Yn8xK2vJ9qLmN3pQrStUvWxYz0123456789AbCdEfGhIjKlMn";
    let result = policy.sanitize_text(window, Some(Path::new("secrets/key.pem")));
    assert!(result.has_secrets, "split key window must be flagged");
    assert!(
        !result.content.contains("MIIEpQIB"),
        "key body leaked from a split search/read window: {}",
        result.content
    );
}

#[test]
fn body_only_pem_window_does_not_leak_through_sanitize_text() {
    let policy = ContentSecurity::new();
    let key_body = "MIIEpQIBAAKCAQEA7Yn8xK2vJ9qLmN3pQrStUvWxYz0123456789AbCdEfGhIjKlMn";
    let result = policy.sanitize_text(key_body, Some(Path::new("secrets/deploy-key.pem")));
    assert!(result.has_secrets, "body-only PEM window must be flagged");
    assert!(
        !result.content.contains(key_body),
        "key body leaked from a body-only search/read window: {}",
        result.content
    );
    assert_eq!(
        result.content.lines().count(),
        key_body.lines().count(),
        "redaction must preserve source-line mapping"
    );
}

#[test]
/// Non-UTF-8 text decodes by the engine's one source rule, as localFetch
/// shows the same bytes: no valid UTF-8 sequence reads as Latin-1, and
/// mostly-UTF-8 text keeps UTF-8 with stray bytes replaced.
fn non_utf8_text_decodes_like_local_fetch_reads() {
    let policy = ContentSecurity::new();
    for (bytes, expected) in [
        (&[b'a', 0xff, b'b'][..], "a\u{ff}b"),
        (
            "é".as_bytes()
                .iter()
                .chain(&[0xff])
                .copied()
                .collect::<Vec<_>>()
                .leak(),
            "é\u{fffd}",
        ),
    ] {
        let result = policy
            .validate_text_bytes(bytes, None, 8)
            .expect("security test setup should succeed");
        assert_eq!(result.content, expected);
        assert_eq!(result.content, octocode_engine::text::decode(bytes).text);
    }
}

#[test]
fn builtin_sanitization_result_is_lossless_through_policy_wrapper() {
    let policy = ContentSecurity::new();
    let corpus = [
        (format!("const token = \"ghp_{}\";", "a".repeat(37)), None),
        (
            "requestId = 550e8400-e29b-41d4-a716-446655440000".to_owned(),
            None,
        ),
        (
            "apiVersion: v1\nkind: Secret\ndata:\n  password: c3VwZXJzZWNyZXQ=".to_owned(),
            Some(Path::new("k8s/secret.yaml")),
        ),
    ];
    for (content, path) in corpus {
        let expected = octocode_engine::security::sanitizer::sanitize_content(
            &content,
            path.map(|path| path.to_string_lossy()).as_deref(),
        )
        .expect("portable sanitizer");
        let actual = policy.sanitize_text(&content, path);
        assert_eq!(actual.content, expected.content);
        assert_eq!(actual.has_secrets, expected.has_secrets);
        assert_eq!(actual.secrets_detected, expected.secrets_detected);
        assert_eq!(actual.warnings, expected.warnings);
    }
}
#[test]
fn nested_array_string_leaves_are_sanitized() {
    let policy = ContentSecurity::new();
    let token = format!("ghp_{}", "a".repeat(37));
    let result = policy.validate_input_parameters(&serde_json::json!({
        "x": [[token]]
    }));
    assert!(result.has_secrets, "nested-array secret must be detected");
    assert!(
        !serde_json::to_string(&result.sanitized_params)
            .expect("serializable sanitized map")
            .contains("ghp_"),
        "token leaked from a nested array"
    );
}

#[test]
fn parameters_reject_dangerous_keys_and_keep_safe_partial_data() {
    let policy = ContentSecurity::new();
    let result = policy.validate_input_parameters(&serde_json::json!({"ok":"x", "prototype": {}}));
    assert!(!result.is_valid);
    assert_eq!(result.sanitized_params["ok"], "x");
    assert!(!result.sanitized_params.contains_key("prototype"));
}

#[test]
fn secret_fields_name_every_rewritten_leaf() {
    let policy = ContentSecurity::new();
    let token = format!("ghp_{}", "a".repeat(36));
    let result = policy.validate_input_parameters(&serde_json::json!({
        "matchString": token,
        "keywords": ["safe", token],
        "outer": {"key": token},
        "grid": [[token]],
        "rows": [{"inner": token}],
        "clean": "needle",
    }));
    assert!(result.has_secrets);
    assert_eq!(
        result.secret_fields,
        [
            "grid[]",
            "keywords[]",
            "matchString",
            "outer.key",
            "rows[].inner"
        ]
    );
    let clean = policy.validate_input_parameters(&serde_json::json!({"matchString": "needle"}));
    assert!(clean.secret_fields.is_empty());
}

#[test]
fn continuation_shaped_values_are_not_credentials() {
    let policy = ContentSecurity::new();
    for value in [
        "lexical-live-v2:845e0d1f0b51e9d91d398dac8fdc9d3a0b88cb954e6434e78a56ff2b753a9a16",
        "824df86de3bc3a3ff0c6201874d42955e8b5b12a0",
        "3bc3a3ff0",
        "MAX_STRING_LENGTH",
        "fn validate_input_parameters",
    ] {
        let result = policy.validate_input_parameters(&serde_json::json!({"field": value}));
        assert!(
            result.secret_fields.is_empty(),
            "{value} flagged as a secret"
        );
    }
}

#[test]
fn frozen_input_bounds_and_nested_secret_projection_match_reference() {
    let policy = ContentSecurity::new();
    let oversized = serde_json::json!({"text": "x".repeat(10_001)});
    let result = policy.validate_input_parameters(&oversized);
    assert!(!result.is_valid);
    assert!(!result.sanitized_params.contains_key("text"));

    let oversized_array = serde_json::json!({"items": (0..101).collect::<Vec<_>>()});
    let result = policy.validate_input_parameters(&oversized_array);
    assert!(!result.is_valid);
    assert!(!result.sanitized_params.contains_key("items"));

    let token = format!("ghp_{}", "a".repeat(37));
    let nested = serde_json::json!({"outer": {"key": token}});
    let result = policy.validate_input_parameters(&nested);
    assert!(result.has_secrets);
    assert!(result.is_valid);
    assert!(
        !serde_json::to_string(&result.sanitized_params)
            .expect("serializable sanitized map")
            .contains("ghp_")
    );
}
