use std::path::Path;
use std::sync::Arc;

use octocode_engine::security::types::SanitizationResult;
use serde_json::{Map, Value};

use super::SecurityRegistry;
use crate::policy::{PolicyError, PolicyErrorCode};

const MAX_STRING_LENGTH: usize = 10_000;
const MAX_ARRAY_LENGTH: usize = 100;
const MAX_DEPTH: usize = 20;

/// Secret label recorded when the split-private-key window guard fires.
const SPLIT_KEY_SECRET: &str = "privateKeyFragment";

/// A PEM/OpenSSH/PGP *private-key* boundary line (BEGIN or END). Public
/// certificates (`-----BEGIN CERTIFICATE-----`) are intentionally excluded —
/// they are not secret.
fn is_private_key_boundary(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.ends_with("-----")
        && trimmed.contains("PRIVATE KEY")
        && (trimmed.starts_with("-----BEGIN") || trimmed.starts_with("-----END"))
}

/// A base64 body line as emitted inside PEM/OpenSSH/PGP key blocks (wrapped at
/// 64–76 chars). The 80-char cap avoids redacting ordinary long source lines.
fn is_key_body_line(line: &str) -> bool {
    let trimmed = line.trim();
    (16..=80).contains(&trimmed.len())
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
}

/// Redact private-key material from a single sanitized leaf when a key boundary
/// marker survived full-block redaction — the signal that a bounded read or
/// search-context window split the key across its BEGIN/END boundary, so the
/// anchored built-in patterns (which need a complete block) could not match.
/// Returns `None` when no boundary marker is present, so `fullContent` reads
/// (already collapsed to one marker) and non-key content stay byte-identical.
/// Redaction is line-for-line, preserving the leaf's line count so the tool's
/// reported line ranges remain accurate.
fn redact_split_private_key(text: &str) -> Option<String> {
    if !text.lines().any(is_private_key_boundary) {
        return None;
    }
    let mut changed = false;
    let redacted: Vec<String> = text
        .lines()
        .map(|line| {
            if is_private_key_boundary(line) || is_key_body_line(line) {
                changed = true;
                format!("[REDACTED-{}]", SPLIT_KEY_SECRET.to_uppercase())
            } else {
                line.to_owned()
            }
        })
        .collect();
    if !changed {
        return None;
    }
    let mut out = redacted.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ValidationResult {
    pub sanitized_params: Map<String, Value>,
    pub is_valid: bool,
    pub has_secrets: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ContentSecurity {
    registry: Arc<SecurityRegistry>,
}

/// Conservative email shape for opt-in masking of gh outputs. Local-part
/// and domain are both masked; the match is intentionally simple (no
/// quoted local-parts) — commit metadata uses plain addresses.
fn email_pattern() -> &'static regex::Regex {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        #[allow(clippy::expect_used)]
        regex::Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+")
            .expect("static email pattern compiles")
    })
}

impl ContentSecurity {
    pub fn new(registry: Arc<SecurityRegistry>) -> Self {
        Self { registry }
    }

    /// Opt-in (R11): mask every email address in `text`. Callers gate this
    /// on `output.redactEmails` / `OCTOCODE_REDACT_EMAILS`; default output
    /// is unchanged.
    pub fn redact_emails(&self, text: &str) -> String {
        email_pattern()
            .replace_all(text, "[REDACTED-EMAIL]")
            .into_owned()
    }

    pub fn sanitize_text(&self, content: &str, file_path: Option<&Path>) -> SanitizationResult {
        let path = file_path.map(|path| path.to_string_lossy());
        let native = octocode_engine::portable::sanitize_content(content, path.as_deref())
            .unwrap_or_else(|error| SanitizationResult {
                content: "[CONTENT-REDACTED-SANITIZER-FAILURE]".to_owned(),
                has_secrets: true,
                secrets_detected: vec!["sanitizer-failure".to_owned()],
                warnings: vec![error.to_string()],
            });
        if self.registry.secret_patterns().is_empty() {
            // Even with no registry patterns, guard against a multi-line private
            // key that a bounded read/search window split across its BEGIN/END
            // boundary (the anchored built-in patterns only match a complete
            // block, so a single-boundary window would otherwise leak the body).
            if let Some(guarded) = redact_split_private_key(&native.content) {
                let mut secrets = native.secrets_detected;
                secrets.push(SPLIT_KEY_SECRET.to_owned());
                return SanitizationResult {
                    content: guarded,
                    has_secrets: true,
                    warnings: vec![format!("{} secret(s) redacted", secrets.len())],
                    secrets_detected: secrets,
                };
            }
            return native;
        }
        let mut sanitized = native.content;
        let mut secrets = native.secrets_detected;
        for pattern in self.registry.secret_patterns() {
            if let Some(context) = &pattern.file_context {
                let Some(path) = path.as_deref() else {
                    continue;
                };
                if !context.is_match(path) {
                    continue;
                }
            }
            if pattern.regex.is_match(&sanitized) {
                secrets.push(pattern.name.clone());
                let replacement = format!("[REDACTED-{}]", pattern.name.to_uppercase());
                sanitized = if pattern.global {
                    pattern
                        .regex
                        .replace_all(&sanitized, replacement)
                        .into_owned()
                } else {
                    pattern.regex.replace(&sanitized, replacement).into_owned()
                };
            }
        }
        if let Some(guarded) = redact_split_private_key(&sanitized) {
            if !secrets.iter().any(|name| name == SPLIT_KEY_SECRET) {
                secrets.push(SPLIT_KEY_SECRET.to_owned());
            }
            sanitized = guarded;
        }
        let has_secrets = !secrets.is_empty();
        SanitizationResult {
            content: sanitized,
            has_secrets,
            warnings: if has_secrets {
                vec![format!("{} secret(s) redacted", secrets.len())]
            } else {
                Vec::new()
            },
            secrets_detected: secrets,
        }
    }

    pub fn validate_text_bytes(
        &self,
        bytes: &[u8],
        file_path: Option<&Path>,
        max_bytes: usize,
    ) -> Result<SanitizationResult, PolicyError> {
        if bytes.len() > max_bytes {
            return Err(PolicyError::new(
                PolicyErrorCode::InputTooLarge,
                format!("Content exceeds maximum length ({max_bytes} bytes)"),
            ));
        }
        if bytes.contains(&0) {
            return Err(PolicyError::new(
                PolicyErrorCode::BinaryContent,
                "Binary content is not allowed for text output",
            ));
        }
        Ok(self.sanitize_text(&String::from_utf8_lossy(bytes), file_path))
    }

    pub fn validate_input_parameters(&self, params: &Value) -> ValidationResult {
        let Some(object) = params.as_object() else {
            return ValidationResult {
                sanitized_params: Map::new(),
                is_valid: false,
                has_secrets: false,
                warnings: vec!["Invalid parameters: must be an object".to_owned()],
            };
        };
        self.validate_object(object, 0)
    }

    fn validate_object(&self, object: &Map<String, Value>, depth: usize) -> ValidationResult {
        if depth > MAX_DEPTH {
            return ValidationResult {
                sanitized_params: Map::new(),
                is_valid: false,
                has_secrets: false,
                warnings: vec!["Maximum nesting depth exceeded".to_owned()],
            };
        }
        let mut sanitized = Map::new();
        let mut warnings = Vec::new();
        let mut valid = true;
        let mut has_secrets = false;
        for (key, value) in object {
            if key.trim().is_empty() {
                warnings.push(format!("Invalid parameter key: {key}"));
                valid = false;
                continue;
            }
            if matches!(key.as_str(), "__proto__" | "constructor" | "prototype") {
                warnings.push(format!("Dangerous parameter key blocked: {key}"));
                valid = false;
                continue;
            }
            match value {
                Value::String(text) => {
                    if text.encode_utf16().count() > MAX_STRING_LENGTH {
                        warnings.push(format!(
                            "Parameter {key} exceeds maximum length (10,000 characters)"
                        ));
                        valid = false;
                        continue;
                    }
                    let result = self.sanitize_text(text, None);
                    if result.has_secrets {
                        has_secrets = true;
                        for secret in result.secrets_detected {
                            warnings.push(format!("Secrets detected in {key}: {secret}"));
                        }
                    }
                    sanitized.insert(key.clone(), Value::String(result.content));
                }
                Value::Array(values) => {
                    if values.len() > MAX_ARRAY_LENGTH {
                        warnings.push(format!(
                            "Parameter {key} array exceeds maximum length (100 items)"
                        ));
                        valid = false;
                        continue;
                    }
                    let mut array = Vec::new();
                    for item in values {
                        match item {
                            Value::String(text)
                                if text.encode_utf16().count() > MAX_STRING_LENGTH =>
                            {
                                warnings.push(format!(
                                    "Parameter {key}[] exceeds maximum length (10,000 characters)"
                                ));
                                valid = false;
                            }
                            Value::String(text) => {
                                let result = self.sanitize_text(text, None);
                                has_secrets |= result.has_secrets;
                                array.push(Value::String(result.content));
                            }
                            Value::Object(nested) => {
                                let result = self.validate_object(nested, depth + 1);
                                has_secrets |= result.has_secrets;
                                valid &= result.is_valid;
                                warnings.extend(
                                    result
                                        .warnings
                                        .into_iter()
                                        .map(|warning| format!("{key}[]: {warning}")),
                                );
                                array.push(Value::Object(result.sanitized_params));
                            }
                            // Nested arrays would otherwise be cloned verbatim,
                            // leaving `{"x":[["ghp_…"]]}` unscanned. Recurse so
                            // string leaves at any array depth are sanitized.
                            Value::Array(inner) => {
                                array.push(Value::Array(self.sanitize_nested_array(
                                    inner,
                                    depth + 1,
                                    &mut has_secrets,
                                )));
                            }
                            _ => array.push(item.clone()),
                        }
                    }
                    sanitized.insert(key.clone(), Value::Array(array));
                }
                Value::Object(nested) => {
                    let result = self.validate_object(nested, depth + 1);
                    has_secrets |= result.has_secrets;
                    valid &= result.is_valid;
                    warnings.extend(result.warnings.iter().map(|warning| {
                        format!("Invalid nested object in parameter {key}: {warning}")
                    }));
                    sanitized.insert(key.clone(), Value::Object(result.sanitized_params));
                }
                _ => {
                    sanitized.insert(key.clone(), value.clone());
                }
            }
        }
        warnings.sort();
        warnings.dedup();
        ValidationResult {
            sanitized_params: sanitized,
            is_valid: valid,
            has_secrets,
            warnings,
        }
    }

    /// Recursively sanitize the string leaves of a (possibly deeply) nested
    /// array so no credential survives at any array depth. Objects delegate to
    /// [`validate_object`]; scalars are passed through unchanged. Depth is
    /// bounded by `MAX_DEPTH` to match the object walk and cap recursion.
    fn sanitize_nested_array(
        &self,
        values: &[Value],
        depth: usize,
        has_secrets: &mut bool,
    ) -> Vec<Value> {
        if depth > MAX_DEPTH {
            return values.to_vec();
        }
        values
            .iter()
            .map(|item| match item {
                Value::String(text) => {
                    let result = self.sanitize_text(text, None);
                    *has_secrets |= result.has_secrets;
                    Value::String(result.content)
                }
                Value::Object(nested) => {
                    let result = self.validate_object(nested, depth + 1);
                    *has_secrets |= result.has_secrets;
                    Value::Object(result.sanitized_params)
                }
                Value::Array(inner) => {
                    Value::Array(self.sanitize_nested_array(inner, depth + 1, has_secrets))
                }
                _ => item.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::SensitiveDataPattern;
    #[test]
    fn split_private_key_guard_targets_only_key_markers() {
        // Body-only base64 with no key marker → guard is a no-op (must not
        // redact ordinary base64 that happens to appear in source).
        assert!(redact_split_private_key("aGVsbG8gd29ybGQ=\nc29tZSBkYXRhIGhlcmU=").is_none());
        // A window holding only the BEGIN boundary + body (END is in the next
        // window) → the body must be redacted.
        let out = redact_split_private_key(
            "config header line\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEA7Yn8xK2vJ9qLmN3pQrSt",
        )
        .expect("split private key must be redacted");
        assert!(!out.contains("MIIEpQIB"), "key body leaked: {out}");
        assert!(
            out.contains("config header line"),
            "non-secret context dropped"
        );
        assert_eq!(out.lines().count(), 3, "line count must be preserved");
        // A public certificate boundary is not a private key → no-op.
        assert!(redact_split_private_key("-----BEGIN CERTIFICATE-----\nMIIBpayload").is_none());
    }

    #[test]
    fn split_key_window_does_not_leak_through_sanitize_text() {
        let policy = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
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
    fn custom_patterns_follow_builtin_then_custom_order() {
        let mut registry = SecurityRegistry::default();
        registry
            .add_secret_patterns([SensitiveDataPattern::compile(
                "custom",
                "",
                "secret-[0-9]+",
                false,
                false,
                None,
            )
            .expect("security test setup should succeed")])
            .expect("security test setup should succeed");
        let policy = ContentSecurity::new(Arc::new(registry));
        let result = policy.sanitize_text("secret-123", None);
        assert_eq!(result.content, "[REDACTED-CUSTOM]");
        assert_eq!(result.secrets_detected, ["custom"]);
    }
    #[test]
    fn malformed_utf8_is_lossily_decoded_like_node_reads() {
        let policy = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        let result = policy
            .validate_text_bytes(&[b'a', 0xff, b'b'], None, 3)
            .expect("security test setup should succeed");
        assert_eq!(result.content, "a�b");
    }

    #[test]
    fn custom_regex_preserves_javascript_global_replacement_flag() {
        let mut registry = SecurityRegistry::default();
        registry
            .add_secret_patterns([
                SensitiveDataPattern::compile_js("one", "", "token", "i", None)
                    .expect("compatible non-global regex"),
            ])
            .expect("mutable registry");
        let policy = ContentSecurity::new(Arc::new(registry));
        assert_eq!(
            policy.sanitize_text("TOKEN token", None).content,
            "[REDACTED-ONE] token"
        );
    }

    #[test]
    fn builtin_sanitization_result_is_lossless_through_policy_wrapper() {
        let policy = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
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
            let expected = octocode_engine::portable::sanitize_content(
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
        let policy = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
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
        let policy = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        let result =
            policy.validate_input_parameters(&serde_json::json!({"ok":"x", "prototype": {}}));
        assert!(!result.is_valid);
        assert_eq!(result.sanitized_params["ok"], "x");
        assert!(!result.sanitized_params.contains_key("prototype"));
    }

    #[test]
    fn frozen_input_bounds_and_nested_secret_projection_match_reference() {
        let policy = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
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
}
