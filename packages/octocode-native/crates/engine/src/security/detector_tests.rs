use super::*;

#[test]
fn multiline_redaction_keeps_source_line_positions() {
    let regex = regex::Regex::new("BEGIN[\\s\\S]*?END").unwrap();
    let original = "before\nBEGIN\nsecret\nEND\nafter";
    let redacted = replace_preserving_lines(&regex, original, "[REDACTED]");
    assert_eq!(redacted, "before\n[REDACTED]\n\n\nafter");
    assert_eq!(redacted.lines().count(), original.lines().count());
    assert!(!redacted.contains("secret"));
}

#[test]
fn detect_single_returns_empty_on_blank_input() {
    let result = detect_single("", None);
    assert_eq!(result.sanitized, "");
    assert!(result.secrets_detected.is_empty());
}

#[test]
fn detect_single_no_match_returns_input_unchanged() {
    let input = "no secrets here just plain text";
    let result = detect_single(input, None);
    assert_eq!(result.sanitized, input);
    assert!(result.secrets_detected.is_empty());
}

#[test]
fn detect_single_redacts_github_token() {
    let input = "token: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let result = detect_single(input, None);
    assert!(result.sanitized.contains("[REDACTED-"));
    assert!(!result.secrets_detected.is_empty());
}

#[test]
fn detect_single_applies_file_context_when_path_matches() {
    // kubernetesSecrets pattern has file_context = r"\.ya?ml$"
    // Use a content that matches that pattern (kind: Secret … data:)
    let yaml = "kind: Secret\ndata:\n  password: c2VjcmV0cGFzc3dvcmQ=\n";
    let result_no_path = detect_single(yaml, None);
    let result_with_yaml = detect_single(yaml, Some("k8s/secret.yaml"));
    let result_with_ts = detect_single(yaml, Some("src/index.ts"));
    // With .yaml path → file-context pattern should fire
    assert!(result_with_yaml.has_secrets_or(&result_no_path));
    // With .ts path → file-context pattern should NOT fire
    assert_eq!(result_with_ts.sanitized, result_no_path.sanitized);
}

#[test]
fn content_net_redacts_file_context_secret_without_path() {
    // The global output net (tool results, error/metadata strings) has no
    // file path. A `kind: Secret` data block whose surrounding text carries
    // a `.yaml` reference matches the `\.ya?ml$` anchor against the CONTENT,
    // so it is redacted even though `file_path` is None.
    let yaml = "kind: Secret\ndata:\n  password: c2VjcmV0cGFzc3dvcmQ=\n# source: manifest.yaml";
    let result = detect_single(yaml, None);
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
    let result = detect_single(bare, None);
    assert_eq!(result.sanitized, bare, "bare UUID must not be redacted");
}

#[test]
fn mask_text_returns_empty_on_blank_input() {
    assert_eq!(mask_text(String::new()), "");
}

#[test]
fn mask_text_no_match_returns_input_unchanged() {
    let input = "no secrets here".to_string();
    assert_eq!(mask_text(input.clone()), input);
}

#[test]
fn mask_text_redacts_oversized_content_wholesale() {
    // Over-limit input must be redacted wholesale (mirroring sanitize_content)
    // instead of scanned, so maskSensitiveData can't be handed unbounded work.
    let input = "a".repeat(MAX_CONTENT_SIZE + 1);
    assert_eq!(mask_text(input), CONTENT_SIZE_LIMIT_PLACEHOLDER);
}

#[test]
fn find_char_boundary_at_end_returns_len() {
    let s = "hello";
    assert_eq!(find_char_boundary(s, 10), s.len());
}

#[test]
fn detect_chunked_no_match_returns_input_unchanged() {
    // Content with no secrets but length > CHUNK_SIZE to exercise the
    // pre-filter early-return path.
    let padding = "a".repeat(CHUNK_SIZE + 1);
    let result = detect_chunked(&padding, None);
    assert_eq!(result.sanitized, padding);
    assert!(result.secrets_detected.is_empty());
}

#[test]
fn detect_chunked_redacts_token_spanning_chunk_boundary() {
    // Place a GitHub PAT near the CHUNK_SIZE boundary so it straddles the
    // overlap window and must still be redacted by the chunked path.
    let prefix = "a".repeat(CHUNK_SIZE - 10);
    let token = "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let input = format!("{prefix} token={token}");
    let result = detect_chunked(&input, None);
    assert!(
        result.sanitized.contains("[REDACTED-"),
        "chunked path must redact token near chunk boundary"
    );
    assert!(!result.secrets_detected.is_empty());
}

#[test]
fn detect_chunked_redacts_long_secret_spanning_chunk_boundary() {
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
    let result = detect_chunked(&input, None);
    assert!(
        result.sanitized.contains("[REDACTED-"),
        "chunked path must redact a >1 KB secret straddling the chunk boundary"
    );
    assert!(!result.sanitized.contains("-----BEGIN RSA PRIVATE KEY-----"));
    assert!(!result.secrets_detected.is_empty());
}

#[test]
fn next_chunk_start_snaps_overlap_to_char_boundary() {
    let s = format!("{}😀tail", "a".repeat(10));
    let inside_emoji = 11;

    let next = next_chunk_start(&s, CHUNK_OVERLAP + inside_emoji);

    assert_eq!(next, 10);
    assert!(s.is_char_boundary(next));
}

#[test]
fn detect_chunked_preserves_canonical_pattern_order() {
    let input = format!(
        "{} {} {} {}",
        "sk-1234567890abcdefghijklmnopqrstuvwxyzT3BlbkFJABCDEFGHIJKLMNO",
        "AKIAIOSFODNN7EXAMPLE",
        "ghp_1234567890abcdefghijklmnopqrstuvwxyz123456",
        "x".repeat(CHUNK_SIZE)
    );

    let result = detect_chunked(&input, None);

    assert_eq!(
        result.secrets_detected,
        vec![
            "openaiApiKeyLegacy".to_string(),
            "awsAccessKeyId".to_string(),
            "githubTokens".to_string(),
        ]
    );
}

#[test]
fn detect_chunked_matches_detect_single_on_same_input() {
    // Both paths must produce the same redacted output for content that
    // fits in a single chunk (use a small string so both paths are tested).
    let input = "token: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let single = detect_single(input, None);
    let chunked = detect_chunked(input, None);
    assert_eq!(single.sanitized, chunked.sanitized);
    assert_eq!(
        single
            .secrets_detected
            .iter()
            .collect::<std::collections::HashSet<_>>(),
        chunked
            .secrets_detected
            .iter()
            .collect::<std::collections::HashSet<_>>(),
    );
}

#[test]
fn find_char_boundary_snaps_to_valid_boundary() {
    let s = "héllo";
    let pos = 2; // middle of the 2-byte 'é'
    let b = find_char_boundary(s, pos);
    assert!(s.is_char_boundary(b));
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
        format!("postgres://user:s3cr3t@db.example.com:5432/app"),
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
        let prescanned = matching_pattern_indices(content);
        assert_eq!(
            reference, prescanned,
            "prescan diverged from reference RegexSet on: {content:?}"
        );
    }
}

#[test]
fn detect_single_redacts_aws_access_key_id() {
    let input = format!("AWS_ACCESS_KEY_ID={FAKE_AWS_KEY}");
    let result = detect_single(&input, None);
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

#[test]
fn mask_text_fully_masks_matched_secret() {
    let output = mask_text(FAKE_GH_TOKEN.to_string());
    // Must differ from input and preserve byte length (ASCII '*' == 1 byte).
    assert_ne!(output, FAKE_GH_TOKEN);
    assert_eq!(
        output.len(),
        FAKE_GH_TOKEN.len(),
        "masking must not change byte length"
    );
    // The entire matched span is masked — no character of the secret leaks.
    assert!(
        output.chars().all(|c| c == '*'),
        "every character of the matched secret must be masked: {output}"
    );
    assert!(
        !output.contains("ghp_"),
        "no portion of the token prefix may survive: {output}"
    );
}

#[test]
fn mask_text_preserves_non_matching_prefix_and_suffix() {
    // Use spaces as separators: '_' is a word-char and would break the \b boundary.
    let input = format!("token: {FAKE_GH_TOKEN}, rest");
    let output = mask_text(input.clone());
    assert!(output.starts_with("token: "), "prefix must be untouched");
    assert!(output.ends_with(", rest"), "suffix must be untouched");
    assert!(output.contains('*'), "match region must be masked");
}

// ── Straddle-proofing post-condition tests ────────────────────────────────
//
// These pin the guarantee that `detect_chunked`'s output never still matches
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
fn detect_chunked_redacts_oversized_secret_straddling_boundary() {
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
        "input must exceed CHUNK_SIZE so detect_chunked actually chunks"
    );

    let result = detect_chunked(&input, None);

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
fn detect_chunked_redacts_both_in_chunk_and_straddling_matches() {
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

    let result = detect_chunked(&input, None);

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

// ── Property tests ───────────────────────────────────────────────────────
//
// Two complementary checks (both proptest!):
//
// 1. `prop_chunked_matches_single_small`: byte-identical equivalence across
//    small, randomly shaped inputs, including multibyte characters.
//
// 2. `prop_chunked_matches_single_boundary`: the same
//    equivalence on ~500KB inputs with the token placed at boundary-relevant
//    offsets, including a multibyte character near the chunk edge. The
//    literal prescan bounds regex work sufficiently for this to run in the
//    default suite; it no longer depends on the removed RegexSet path.
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

    /// `detect_chunked` and `detect_single` agree on small, randomly-shaped
    /// inputs (incl. multi-byte chars interspersed around the token). Fast —
    /// keeps the default suite quick; the chunk-boundary mega-input case is
    /// covered by the dedicated unit tests and the #[ignore] property below.
    #[test]
    fn prop_chunked_matches_single_small(
        pre in "[ a-z]{0,16}",
        post in "[ a-z]{0,16}",
        token_idx in 0usize..4,
        mb_before in any::<bool>(),
        mb_after in any::<bool>(),
    ) {
        let token = match token_idx {
            0 => FAKE_GH_TOKEN.to_string(),
            1 => FAKE_AWS_KEY.to_string(),
            2 => format!("sk-{}T3BlbkFJ{}", "a".repeat(20), "a".repeat(20)),
            _ => format!("gho_{}", "a".repeat(36)),
        };
        let before = if mb_before { format!("{pre}é") } else { pre };
        let after = if mb_after { format!("é{post}") } else { post };
        let input = format!("{before} {token} {after}");

        let single = detect_single(&input, None);
        let chunked = detect_chunked(&input, None);
        prop_assert_eq!(single.sanitized, chunked.sanitized);
        let s: std::collections::HashSet<_> = single.secrets_detected.iter().collect();
        let c: std::collections::HashSet<_> = chunked.secrets_detected.iter().collect();
        prop_assert_eq!(s, c);
    }

    /// ~500KB chunk-boundary equivalence, including UTF-8 overlap windows.
    /// Keep the same case count as the small-input properties so the large
    /// input path remains part of ordinary correctness validation.
    #[test]
    fn prop_chunked_matches_single_boundary(
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

        let single = detect_single(&input, None);
        let chunked = detect_chunked(&input, None);
        prop_assert_eq!(single.sanitized, chunked.sanitized);
        let s: std::collections::HashSet<_> = single.secrets_detected.iter().collect();
        let c: std::collections::HashSet<_> = chunked.secrets_detected.iter().collect();
        prop_assert_eq!(s, c);
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
        let out = detect_single(&input, None);
        // The redacted form is `[REDACTED-GITHUBTOKENS]` — it must NOT contain
        // the bare `ghp_` prefix followed by token chars.
        prop_assert!(
            !out.sanitized.contains("ghp_"),
            "raw token leaked into sanitized output: {:?}",
            out.sanitized
        );
        // And mask_text must preserve total byte length for this ASCII input
        // (even-indexed chars become '*'; ASCII '*' == 1 byte, so length holds).
        let masked = mask_text(input.clone());
        prop_assert_eq!(masked.len(), input.len());
    }
}
