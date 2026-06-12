#![deny(clippy::all)]

mod patterns;

use napi_derive::napi;
use patterns::{PATTERNS, PATTERN_REGEXES, REGEX_SET};

const MAX_CONTENT_SIZE: usize = 10_000_000;
const CHUNK_SIZE: usize = 500_000;
const CHUNK_OVERLAP: usize = 1_000;

// ---------------------------------------------------------------------------
// Public types (mirrored from octocode-security-utils TypeScript types)
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct SanitizationResult {
    pub content: String,
    pub has_secrets: bool,
    pub secrets_detected: Vec<String>,
    pub warnings: Vec<String>,
}

#[napi(object)]
pub struct MaskResult {
    pub text: String,
    pub matched_count: u32,
}

// ---------------------------------------------------------------------------
// sanitize_content — detect and redact secrets in a string
// ---------------------------------------------------------------------------

/// Detect and redact all secrets from `content`.
/// Returns the sanitized string with REDACTED placeholders, plus metadata.
///
/// Mirrors ContentSanitizer.sanitizeContent() from octocode-security-utils.
#[napi]
pub fn sanitize_content(content: String, _file_path: Option<String>) -> SanitizationResult {
    if content.len() > MAX_CONTENT_SIZE {
        return SanitizationResult {
            content: "[CONTENT-REDACTED-SIZE-LIMIT]".to_string(),
            has_secrets: true,
            secrets_detected: vec!["content-size-exceeded".to_string()],
            warnings: vec![format!(
                "Content exceeds {} character limit — redacted for safety",
                MAX_CONTENT_SIZE
            )],
        };
    }

    let result = if content.len() > CHUNK_SIZE {
        detect_chunked(&content)
    } else {
        detect_single(&content)
    };

    let warnings = if result.secrets_detected.is_empty() {
        vec![]
    } else {
        vec![format!("{} secret(s) redacted", result.secrets_detected.len())]
    };

    SanitizationResult {
        content: result.sanitized,
        has_secrets: !result.secrets_detected.is_empty(),
        secrets_detected: result.secrets_detected,
        warnings,
    }
}

// ---------------------------------------------------------------------------
// mask_sensitive_data — mask (not redact) secrets with * every other char
// ---------------------------------------------------------------------------

/// Mask secrets in place: every even character of a matched secret is
/// replaced with '*', preserving partial readability.
///
/// Mirrors maskSensitiveData() from octocode-security-utils.
#[napi]
pub fn mask_sensitive_data(text: String) -> String {
    if text.is_empty() {
        return text;
    }

    // Collect all matches from all patterns (non-overlapping, sorted by offset)
    let mut matches: Vec<(usize, usize)> = Vec::new();
    for (idx, regex) in PATTERN_REGEXES.iter().enumerate() {
        // Skip file-context patterns — they require a specific filename to apply
        // (mirrors the TS maskSensitiveData which filters resolvePatterns by fileContext)
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

    // Sort by start offset, then deduplicate overlapping spans
    matches.sort_unstable_by_key(|m| m.0);
    let mut non_overlapping: Vec<(usize, usize)> = Vec::new();
    let mut last_end = 0usize;
    for (start, end) in matches {
        if start >= last_end {
            non_overlapping.push((start, end));
            last_end = end;
        }
    }

    // Build masked string by replacing every even-indexed char with '*'
    let text_bytes = text.as_bytes();
    let mut result = Vec::with_capacity(text.len());
    let mut pos = 0usize;

    for (start, end) in &non_overlapping {
        // Copy unmasked region before this match
        result.extend_from_slice(&text_bytes[pos..*start]);
        // Mask the matched region: even positions → '*'
        let matched = &text[*start..*end];
        for (i, ch) in matched.chars().enumerate() {
            if i % 2 == 0 {
                result.push(b'*');
            } else {
                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                result.extend_from_slice(s.as_bytes());
            }
        }
        pos = *end;
    }
    result.extend_from_slice(&text_bytes[pos..]);

    String::from_utf8_lossy(&result).into_owned()
}

// ---------------------------------------------------------------------------
// pattern_count — utility exposed for testing / benchmarking
// ---------------------------------------------------------------------------

/// Returns the number of loaded patterns.
#[napi]
pub fn pattern_count() -> u32 {
    PATTERNS.len() as u32
}

// ---------------------------------------------------------------------------
// Internal detection logic
// ---------------------------------------------------------------------------

struct DetectResult {
    sanitized: String,
    secrets_detected: Vec<String>,
}

/// Fast path: content fits in one chunk — use RegexSet for detection,
/// then per-pattern replace only for matched patterns.
fn detect_single(content: &str) -> DetectResult {
    // One pass with RegexSet to find which patterns matched
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
        let pattern = &PATTERNS[idx];

        // Skip file-context patterns (they require filename — not applicable here)
        if pattern.file_context.is_some() {
            continue;
        }

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

/// Slow path: content > CHUNK_SIZE — process in overlapping chunks.
/// Mirrors the TypeScript chunked implementation exactly.
fn detect_chunked(content: &str) -> DetectResult {
    let mut sanitized = content.to_string();
    let mut secrets_detected_set: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    for (idx, pattern) in PATTERNS.iter().enumerate() {
        if pattern.file_context.is_some() {
            continue;
        }

        let regex = &PATTERN_REGEXES[idx];
        let mut chunk_start = 0usize;
        let mut found_in_pattern = false;

        while chunk_start < sanitized.len() {
            let chunk_end = (chunk_start + CHUNK_SIZE).min(sanitized.len());
            // Snap to char boundary
            let chunk_end = find_char_boundary(&sanitized, chunk_end);
            let chunk = &sanitized[chunk_start..chunk_end];

            if regex.is_match(chunk) {
                found_in_pattern = true;
                let replacement = format!("[REDACTED-{}]", pattern.name.to_uppercase());
                let new_chunk = regex.replace_all(chunk, replacement.as_str()).into_owned();
                sanitized = format!("{}{}{}", &sanitized[..chunk_start], new_chunk, &sanitized[chunk_end..]);
            }

            let next = chunk_end.saturating_sub(CHUNK_OVERLAP);
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

/// Find the nearest valid UTF-8 char boundary at or before `pos`.
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
