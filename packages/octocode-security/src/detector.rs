use crate::patterns::{PATTERNS, PATTERN_REGEXES, REGEX_SET};

pub(crate) const CHUNK_SIZE: usize = 500_000;
const CHUNK_OVERLAP: usize = 1_000;

// ---------------------------------------------------------------------------
// File-context regex cache — compiled once, index-aligned with PATTERNS.
// None means the pattern has no file-context constraint (always applicable).
// ---------------------------------------------------------------------------

static FILE_CONTEXT_REGEXES: std::sync::LazyLock<Vec<Option<regex::Regex>>> =
    std::sync::LazyLock::new(|| {
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
    let matched_indices: Vec<usize> = REGEX_SET.matches(content).into_iter().collect();

    if matched_indices.is_empty() {
        return DetectResult {
            sanitized: content.to_string(),
            secrets_detected: vec![],
        };
    }

    let mut sanitized = content.to_string();
    let mut secrets_detected = Vec::with_capacity(matched_indices.len());

    for idx in matched_indices {
        if !should_apply(idx, file_path) {
            continue;
        }
        let pattern = &PATTERNS[idx];
        let regex = &PATTERN_REGEXES[idx];
        let replacement = format!("[REDACTED-{}]", pattern.name.to_uppercase());
        let result = regex.replace_all(&sanitized, replacement.as_str());
        if result != sanitized.as_str() {
            secrets_detected.push(pattern.name.to_string());
            sanitized = result.into_owned();
        }
    }

    DetectResult { sanitized, secrets_detected }
}

/// Slow path: content exceeds `CHUNK_SIZE` — process in overlapping chunks to
/// avoid loading the entire string into the regex engine at once.
/// Mirrors the TypeScript chunked implementation.
///
/// After each replacement the string length may change; `effective_end` tracks
/// the real end of the new chunk so the overlap window is computed correctly.
pub(crate) fn detect_chunked(content: &str, file_path: Option<&str>) -> DetectResult {
    let mut sanitized = content.to_string();
    let mut secrets_detected_set: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    for (idx, pattern) in PATTERNS.iter().enumerate() {
        if !should_apply(idx, file_path) {
            continue;
        }

        let regex = &PATTERN_REGEXES[idx];
        let mut chunk_start = 0usize;
        let mut found_in_pattern = false;

        while chunk_start < sanitized.len() {
            let chunk_end = find_char_boundary(
                &sanitized,
                (chunk_start + CHUNK_SIZE).min(sanitized.len()),
            );
            let chunk = &sanitized[chunk_start..chunk_end];

            // Track the effective end after replacement so the next chunk_start
            // is correct even when the replacement changes the string length.
            let effective_end = if regex.is_match(chunk) {
                found_in_pattern = true;
                let replacement = format!("[REDACTED-{}]", pattern.name.to_uppercase());
                let new_chunk = regex.replace_all(chunk, replacement.as_str()).into_owned();
                let new_len = new_chunk.len();
                sanitized = format!(
                    "{}{}{}",
                    &sanitized[..chunk_start],
                    new_chunk,
                    &sanitized[chunk_end..]
                );
                chunk_start + new_len
            } else {
                chunk_end
            };

            let next = effective_end.saturating_sub(CHUNK_OVERLAP);
            if next <= chunk_start {
                break;
            }
            chunk_start = next;
        }

        if found_in_pattern {
            secrets_detected_set.insert(pattern.name.to_string());
        }
    }

    DetectResult {
        sanitized,
        secrets_detected: secrets_detected_set.into_iter().collect(),
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

    let mut matches: Vec<(usize, usize)> = Vec::new();
    for (idx, regex) in PATTERN_REGEXES.iter().enumerate() {
        if PATTERNS[idx].file_context.is_some() {
            continue;
        }
        for m in regex.find_iter(&text) {
            matches.push((m.start(), m.end()));
        }
    }

    if matches.is_empty() {
        return text;
    }

    matches.sort_unstable_by_key(|m| m.0);

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
    fn find_char_boundary_snaps_to_valid_boundary() {
        let s = "héllo";
        let pos = 2; // middle of the 2-byte 'é'
        let b = find_char_boundary(s, pos);
        assert!(s.is_char_boundary(b));
    }
}
