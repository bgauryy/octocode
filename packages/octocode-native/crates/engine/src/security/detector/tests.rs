use super::*;

/// Former 500 KB chunk edge and 8 KiB overlap of the removed chunked path.
/// Large-input tests keep secrets at these offsets as regression positions.
const CHUNK_SIZE: usize = 500_000;
const CHUNK_OVERLAP: usize = 8_192;

// ghp_ + 36 alphanum satisfies githubTokens regex {36,255}.
const FAKE_GH_TOKEN: &str = "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
// AKIA + 16 uppercase alphanum satisfies awsAccessKeyId regex.
const FAKE_AWS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";

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
        // Its regex only names a document, and still does: nothing is left to hide.
        if let Some(form) = document::Form::of(idx) {
            assert_eq!(
                form.redact(&result.sanitized, replacement_for(idx)).0,
                result.sanitized,
                "pattern `{name}` still hides something in sanitized output"
            );
            continue;
        }
        // A value-tested pattern matches values its test keeps (a reference,
        // a redaction marker): what must not remain is one it would redact.
        let value_test = super::credential_values::VALUE_TESTS[idx];
        let left = if value_test == super::credential_values::ValueTest::Every {
            pattern_regex(idx).is_match(&result.sanitized)
        } else {
            replace_secrets(
                pattern_regex(idx),
                &result.sanitized,
                "",
                |key, secret, _| value_test.redacts(key, secret),
            )
            .0 != result.sanitized
        };
        assert!(
            !left,
            "pattern `{name}` still matches sanitized output — post-condition violated"
        );
    }
}

/// [`redact`] over `file_path`'s content (`None`: the pathless net).
fn detect(content: &str, file_path: Option<&str>) -> DetectResult {
    detect_in(content, file_path.map_or(Scope::Pathless, Scope::File))
}

/// [`redact`] with clean content returned unchanged. Every result's edits
/// place its text: what they did not replace is the content's own bytes.
fn detect_in(content: &str, scope: Scope<'_>) -> DetectResult {
    let result = redact(content, scope).unwrap_or_else(|| DetectResult {
        sanitized: content.to_owned(),
        secrets_detected: Vec::new(),
        edits: crate::security::Edits::default(),
    });
    assert!(
        result.edits.maps(content, &result.sanitized),
        "edits {:?} do not place {:?} in {content:?}",
        result.edits,
        result.sanitized
    );
    result
}

impl DetectResult {
    fn has_secrets_or(&self, other: &Self) -> bool {
        !self.secrets_detected.is_empty() || !other.secrets_detected.is_empty()
    }
}

/// The prescan candidates of `content`, in pattern order.
fn candidate_pattern_indices(content: &str) -> Vec<usize> {
    prescan_candidates(content).iter().collect()
}

/// The candidates that apply to `scope`, in pattern order.
fn applicable_candidates(content: &str, scope: Scope<'_>) -> Vec<usize> {
    applicable_patterns(content, scope).iter().collect()
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

mod ascii_boundaries;
mod comment_tails;
mod credential_values;
mod document_forms;
mod every_pattern;
mod glued_tags;
mod key_armors;
mod large_input;
mod markerless;
mod one_long_line;
mod prescan;
mod regex_forms;
mod sampling;
mod scope;
mod source_names;
mod span_keys;

/// Two found ranges that overlap join into one that ends where the later
/// one does: no tail of either stays readable (B165 round 10, R6 F5).
#[test]
fn overlapping_secrets_join_and_none_is_dropped() {
    let found = vec![
        (0..10, 4..10),
        (6..20, 8..20),
        (20..30, 25..30),
        (2..9, 4..9),
    ];
    let joined: Vec<_> = super::joined(found)
        .into_iter()
        .map(|(_, secret)| secret)
        .collect();
    assert_eq!(joined, vec![4..20, 25..30]);
}
