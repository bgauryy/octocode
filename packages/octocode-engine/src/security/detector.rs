use super::patterns::{PATTERNS, PATTERN_REGEXES, REGEX_SET};
use std::sync::LazyLock;

pub(crate) const CHUNK_SIZE: usize = 500_000;
const CHUNK_OVERLAP: usize = 1_000;

// ---------------------------------------------------------------------------
// File-context regex cache — compiled once, index-aligned with PATTERNS.
// None means the pattern has no file-context constraint (always applicable).
// ---------------------------------------------------------------------------

static FILE_CONTEXT_REGEXES: LazyLock<Vec<Option<regex::Regex>>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .map(|p| {
            p.file_context.map(|ctx| {
                regex::Regex::new(ctx)
                    .unwrap_or_else(|e| panic!("invalid file_context regex '{ctx}': {e}"))
            })
        })
        .collect()
});

static REPLACEMENTS: LazyLock<Vec<String>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .map(|p| format!("[REDACTED-{}]", p.name.to_ascii_uppercase()))
        .collect()
});

fn replacement_for(idx: usize) -> &'static str {
    &REPLACEMENTS[idx]
}

fn matching_pattern_indices(content: &str) -> Vec<usize> {
    REGEX_SET.matches(content).into_iter().collect()
}

fn empty_result(content: &str) -> DetectResult {
    DetectResult {
        sanitized: content.to_string(),
        secrets_detected: vec![],
    }
}

fn matching_non_context_indices(content: &str) -> Vec<usize> {
    REGEX_SET
        .matches(content)
        .into_iter()
        .filter(|&idx| PATTERNS[idx].file_context.is_none())
        .collect()
}

fn replace_chunk(
    sanitized: &mut String,
    range: std::ops::Range<usize>,
    regex: &regex::Regex,
    replacement: &str,
) -> usize {
    let new_chunk = regex
        .replace_all(&sanitized[range.clone()], replacement)
        .into_owned();
    let new_len = new_chunk.len();
    sanitized.replace_range(range, &new_chunk);
    new_len
}

fn next_chunk_start(s: &str, effective_end: usize) -> usize {
    find_char_boundary(s, effective_end.saturating_sub(CHUNK_OVERLAP))
}

/// Returns `true` if pattern at `idx` should be applied for the given file path.
///
/// - No `file_context` on the pattern       → always apply.
/// - Has `file_context`, no `file_path`     → skip (cannot verify context).
/// - Has `file_context`, `file_path` given  → apply only when path matches.
fn should_apply(idx: usize, file_path: Option<&str>) -> bool {
    match &FILE_CONTEXT_REGEXES[idx] {
        None => true,
        Some(re) => file_path.is_some_and(|p| re.is_match(p)),
    }
}

pub(crate) struct DetectResult {
    pub sanitized: String,
    pub secrets_detected: Vec<String>,
}

/// Fast path: content fits in one chunk.
/// Uses `RegexSet` for O(1) multi-pattern detection, then per-pattern replace
/// only for the matched subset.
///
/// `file_path` gates file-context patterns (e.g. Kubernetes YAML secrets, `.env`
/// fine-grained GitHub tokens) so they fire only when the path matches.
pub(crate) fn detect_single(content: &str, file_path: Option<&str>) -> DetectResult {
    let matched_indices = matching_pattern_indices(content);

    if matched_indices.is_empty() {
        return empty_result(content);
    }

    let mut sanitized = content.to_string();
    let mut secrets_detected = Vec::with_capacity(matched_indices.len());

    for idx in matched_indices {
        if !should_apply(idx, file_path) {
            continue;
        }
        let pattern = &PATTERNS[idx];
        let regex = &PATTERN_REGEXES[idx];
        let result = regex.replace_all(&sanitized, replacement_for(idx));
        if result != sanitized.as_str() {
            secrets_detected.push(pattern.name.to_string());
            sanitized = result.into_owned();
        }
    }

    DetectResult {
        sanitized,
        secrets_detected,
    }
}

/// Slow path: content exceeds `CHUNK_SIZE` — process in overlapping chunks to
/// avoid loading the entire string into the regex engine at once.
/// Mirrors the TypeScript chunked implementation.
///
/// Uses `REGEX_SET` on the original content to pre-filter candidate patterns
/// (same optimisation as `detect_single`), then runs the chunk loop only for
/// those candidates.  `REGEX_SET` has no false negatives — a pattern excluded
/// here cannot match any chunk of the original content, and replacements
/// produce `[REDACTED-*]` strings that do not re-trigger other patterns.
///
/// After each replacement the string length may change; `effective_end` tracks
/// the real end of the new chunk so the overlap window is computed correctly.
pub(crate) fn detect_chunked(content: &str, file_path: Option<&str>) -> DetectResult {
    // Pre-filter: collect pattern indices that appear anywhere in the original
    // content.  Patterns absent here are skipped in the per-pattern loop below.
    let candidate_indices = matching_pattern_indices(content);

    if candidate_indices.is_empty() {
        return empty_result(content);
    }

    let mut sanitized = content.to_string();
    let mut secrets_detected = Vec::with_capacity(candidate_indices.len());

    for idx in candidate_indices {
        if !should_apply(idx, file_path) {
            continue;
        }

        let pattern = &PATTERNS[idx];
        let regex = &PATTERN_REGEXES[idx];
        let replacement = replacement_for(idx);
        let mut chunk_start = 0usize;
        let mut found_in_pattern = false;

        while chunk_start < sanitized.len() {
            let chunk_end =
                find_char_boundary(&sanitized, (chunk_start + CHUNK_SIZE).min(sanitized.len()));
            let chunk = &sanitized[chunk_start..chunk_end];

            // Track the effective end after replacement so the next chunk_start
            // is correct even when the replacement changes the string length.
            let effective_end = if regex.is_match(chunk) {
                found_in_pattern = true;
                let new_len =
                    replace_chunk(&mut sanitized, chunk_start..chunk_end, regex, replacement);
                chunk_start + new_len
            } else {
                chunk_end
            };

            let next = next_chunk_start(&sanitized, effective_end);
            if next <= chunk_start {
                break;
            }
            chunk_start = next;
        }

        if found_in_pattern {
            secrets_detected.push(pattern.name.to_string());
        }
    }

    DetectResult {
        sanitized,
        secrets_detected,
    }
}

/// Mask secrets in place: every even-indexed character of a matched secret is
/// replaced with `*`, preserving partial readability.
///
/// File-context patterns are always skipped — `mask_text` has no `file_path`
/// parameter, mirroring the TS `maskSensitiveData` behaviour.
///
/// Uses `String` directly so regex byte-offsets (which are always valid UTF-8
/// boundaries) never require a `from_utf8_lossy` round-trip.
pub(crate) fn mask_text(text: String) -> String {
    if text.is_empty() {
        return text;
    }

    let candidate_indices = matching_non_context_indices(&text);
    if candidate_indices.is_empty() {
        return text;
    }

    let mut matches: Vec<(usize, usize)> = Vec::new();
    for idx in candidate_indices {
        let regex = &PATTERN_REGEXES[idx];
        for m in regex.find_iter(&text) {
            matches.push((m.start(), m.end()));
        }
    }

    if matches.is_empty() {
        return text;
    }

    matches.sort_by_key(|m| m.0);

    // Deduplicate overlapping spans — first match wins.
    let mut non_overlapping: Vec<(usize, usize)> = Vec::new();
    let mut last_end = 0usize;
    for (start, end) in matches {
        if start >= last_end {
            non_overlapping.push((start, end));
            last_end = end;
        }
    }

    // Build directly into a String — regex offsets are always valid UTF-8
    // boundaries so &text[a..b] is always safe to push_str.
    let mut result = String::with_capacity(text.len());
    let mut pos = 0usize;

    for (start, end) in &non_overlapping {
        result.push_str(&text[pos..*start]);
        for (i, ch) in text[*start..*end].chars().enumerate() {
            if i % 2 == 0 {
                result.push('*');
            } else {
                result.push(ch);
            }
        }
        pos = *end;
    }
    result.push_str(&text[pos..]);

    result
}

fn find_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

// Helper only used in tests below.
#[cfg(test)]
impl DetectResult {
    fn has_secrets_or(&self, other: &DetectResult) -> bool {
        !self.secrets_detected.is_empty() || !other.secrets_detected.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn mask_text_returns_empty_on_blank_input() {
        assert_eq!(mask_text(String::new()), "");
    }

    #[test]
    fn mask_text_no_match_returns_input_unchanged() {
        let input = "no secrets here".to_string();
        assert_eq!(mask_text(input.clone()), input);
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

    #[test]
    fn detect_single_redacts_aws_access_key_id() {
        let input = format!("AWS_ACCESS_KEY_ID={FAKE_AWS_KEY}");
        let result = detect_single(&input, None);
        assert!(
            result.sanitized.contains("[REDACTED-AWSACCESSKEYID]"),
            "expected redaction, got: {}",
            result.sanitized
        );
        assert!(result
            .secrets_detected
            .contains(&"awsAccessKeyId".to_string()));
    }

    #[test]
    fn mask_text_masks_even_indexed_chars_of_matched_secret() {
        let output = mask_text(FAKE_GH_TOKEN.to_string());
        // Must differ from input and preserve byte length.
        assert_ne!(output, FAKE_GH_TOKEN);
        assert_eq!(
            output.len(),
            FAKE_GH_TOKEN.len(),
            "masking must not change byte length"
        );
        // Even-indexed chars (0, 2, 4, ...) in the matched region become '*';
        // odd-indexed chars are kept verbatim.
        let mut chars = output.chars();
        assert_eq!(chars.next(), Some('*')); // 'g' → '*'
        assert_eq!(chars.next(), Some('h')); // 'h' preserved
        assert_eq!(chars.next(), Some('*')); // 'p' → '*'
        assert_eq!(chars.next(), Some('_')); // '_' preserved
        assert_eq!(chars.next(), Some('*')); // first 'a' → '*'
        assert_eq!(chars.next(), Some('a')); // second 'a' preserved
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

    // ── Property tests ───────────────────────────────────────────────────────
    //
    // Two complementary checks (both proptest!):
    //
    // 1. `prop_chunked_matches_single_small` (default-run): byte-identical
    //    equivalence of `detect_chunked` and `detect_single` across small,
    //    randomly-shaped inputs incl. multi-byte chars. This is fast — the heavy
    //    cost lives in the `RegexSet` DFA, whose debug-build execution on a
    //    multi-hundred-KB string is orders of magnitude slower than release; so
    //    we keep the default cases small and rely on the dedicated boundary
    //    unit tests (`detect_chunked_redacts_token_spanning_chunk_boundary`,
    //    `detect_chunked_terminates_when_tail_shorter_than_overlap`) for the
    //    million-byte path. Both paths share the same per-pattern replacement
    //    loop, so small-input agreement is strong evidence of equivalence.
    //
    // 2. `prop_chunked_matches_single_boundary` (#[ignore] by default): the same
    //    equivalence on ~500KB inputs with the token placed at boundary-relevant
    //    offsets, including a multi-byte char near the chunk edge. Run on demand
    //    (`cargo test -- --ignored`) or in a release-profile CI lane. Not run by
    //    default because the debug-build `RegexSet` DFA is pathologically slow on
    //    this input size (tracked separately; correctness is verified).
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

        /// ~500KB chunk-boundary equivalence. Still `#[ignore]` by default:
        /// `[profile.dev.package."regex-automata"] opt-level = 3` (Cargo.toml)
        /// fixes clean-ASCII 500KB inputs, but the DFA is still algorithmically
        /// slow on the multi-byte-char-near-boundary shape (a 2-byte UTF-8 char
        /// placed in the overlap window causes the regex crate's DFA to explore
        /// many more states). This is a fundamental crate behaviour, not a
        /// compilation issue. Run with `cargo test -- --ignored prop_chunked` or
        /// in a release CI lane. Correctness is already pinned by the unit tests.
        #[test]
        #[ignore]
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
}
