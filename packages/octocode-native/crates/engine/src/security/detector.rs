use super::patterns::{PATTERNS, pattern_regex};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use std::sync::LazyLock;

/// Hard cap on content handed to the detector. Content above this is redacted
/// wholesale rather than scanned, bounding worst-case memory/time.
pub(crate) const MAX_CONTENT_SIZE: usize = 10_000_000;
/// Wholesale placeholder emitted when content exceeds `MAX_CONTENT_SIZE`.
pub(crate) const CONTENT_SIZE_LIMIT_PLACEHOLDER: &str = "[CONTENT-REDACTED-SIZE-LIMIT]";
// ---------------------------------------------------------------------------
// File-context regex cache — compiled once, index-aligned with PATTERNS.
// None means the pattern has no file-context constraint (always applicable).
// ---------------------------------------------------------------------------

// Built-in patterns are compile-time constants; an invalid one is a build bug.
#[allow(clippy::panic)]
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

// ---------------------------------------------------------------------------
// Literal prescan — the cheap gate in front of regex compilation.
//
// The old path lazily built a 309-branch RegexSet DFA plus 309 individual
// regexes on the FIRST sanitize call (~250-800 ms), a tax every fresh process
// paid even for entirely clean content. The prescan replaces it:
//
//   1. At first call, extract the *required prefix literals* of every pattern
//      via regex-syntax HIR (a parse, not a compile — milliseconds total).
//      A pattern gates on its literals only when extraction proves the literal
//      set is finite and every literal is a meaningful anchor (>= 3 bytes).
//   2. Build ONE case-insensitive aho-corasick automaton over those literals.
//   3. Per call: scan content once with aho-corasick; only patterns whose
//      literal appears (plus the small no-literal fallback bucket) get their
//      regex compiled (lazily, once per process) and confirmed with is_match.
//
// Correctness invariant: for a gated pattern, "regex matches content" implies
// "content contains one of its extracted prefix literals" (regex-syntax
// prefix-literal guarantee), and the automaton is ASCII-case-insensitive while
// literals are stored lowercased, so case-insensitive patterns cannot slip
// past the gate. Patterns whose literals cannot be proven (pure entropy
// shapes, UUIDs) are ALWAYS candidates — they are never gated out.
// ---------------------------------------------------------------------------

struct Prescan {
    /// One automaton over every gated pattern's literals (lowercased).
    ac: AhoCorasick,
    /// aho-corasick pattern id -> detector pattern index.
    literal_owner: Vec<usize>,
    /// Pattern indices with no provable literal — always candidates.
    fallback: Vec<usize>,
}

static PRESCAN: LazyLock<Prescan> = LazyLock::new(build_prescan);

/// Strip a single LEADING inline flag group like `(?i)`/`(?im)` — the
/// generator only ever emits whole-pattern leading flags (JS regex semantics).
/// Case-insensitivity is instead honored by the automaton itself.
fn strip_leading_flags(pattern: &str) -> &str {
    if let Some(rest) = pattern.strip_prefix("(?")
        && let Some(close) = rest.find(')')
    {
        let flags = &rest[..close];
        if !flags.is_empty() && flags.chars().all(|c| matches!(c, 'i' | 'm' | 's')) {
            return &rest[close + 1..];
        }
    }
    pattern
}

/// Extract the set of required (necessary) prefix literals for `pattern`, or
/// `None` when no trustworthy finite literal set exists (=> fallback bucket).
fn extract_gate_literals(pattern: &str) -> Option<Vec<Vec<u8>>> {
    use regex_syntax::hir::literal::{ExtractKind, Extractor};
    let stripped = strip_leading_flags(pattern);
    let hir = regex_syntax::ParserBuilder::new()
        .utf8(false)
        .build()
        .parse(stripped)
        .ok()?;
    let mut extractor = Extractor::new();
    extractor.kind(ExtractKind::Prefix);
    let seq = extractor.extract(&hir);
    let lits = seq.literals()?; // None => infinite set => cannot gate
    if lits.is_empty() {
        return None;
    }
    let mut out: Vec<Vec<u8>> = Vec::new();
    for lit in lits {
        let bytes = lit.as_bytes();
        // A shorter anchor gates too weakly to be worth trusting; send the
        // pattern to the fallback bucket instead of risking a false negative
        // on an anchor the extractor was unsure about.
        if bytes.len() < 3 {
            return None;
        }
        let lowered: Vec<u8> = bytes.to_ascii_lowercase();
        if !out.contains(&lowered) {
            out.push(lowered);
        }
    }
    if out.is_empty() || out.len() > 64 {
        return None;
    }
    Some(out)
}

fn build_prescan() -> Prescan {
    let mut literals: Vec<Vec<u8>> = Vec::new();
    let mut literal_owner: Vec<usize> = Vec::new();
    let mut fallback: Vec<usize> = Vec::new();
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        match extract_gate_literals(pattern.regex) {
            Some(lits) => {
                for lit in lits {
                    literals.push(lit);
                    literal_owner.push(idx);
                }
            }
            None => fallback.push(idx),
        }
    }
    // Aho-Corasick over a fixed literal set cannot fail to build.
    #[allow(clippy::expect_used)]
    let ac = AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build(&literals)
        .expect("prescan literal automaton must build");
    Prescan {
        ac,
        literal_owner,
        fallback,
    }
}

/// Candidate pattern indices for `content`: gated patterns whose literal
/// appears, plus the always-on fallback bucket — each CONFIRMED with the real
/// pattern regex (compiled lazily, only for candidates) so callers still see
/// exact match semantics, sorted in canonical pattern order.
fn candidate_pattern_indices(content: &str) -> Vec<usize> {
    let prescan = &*PRESCAN;
    let mut seen = vec![false; PATTERNS.len()];
    // Overlapping scan: a literal nested inside another match span must still
    // register its own pattern, or that pattern would be silently gated out.
    for hit in prescan.ac.find_overlapping_iter(content) {
        seen[prescan.literal_owner[hit.pattern().as_usize()]] = true;
    }
    for &idx in &prescan.fallback {
        seen[idx] = true;
    }
    (0..PATTERNS.len()).filter(|&idx| seen[idx]).collect()
}

fn matching_pattern_indices(content: &str) -> Vec<usize> {
    let mut candidates = candidate_pattern_indices(content);
    candidates.retain(|&idx| pattern_regex(idx).is_match(content));
    candidates
}

fn empty_result(content: &str) -> DetectResult {
    DetectResult {
        sanitized: content.to_string(),
        secrets_detected: vec![],
    }
}

/// A redacted multiline match still occupies its source lines. File reads
/// report and classify source-line scopes, so collapsing a secret block into
/// one line would shift every later focus window.
///
/// A pattern with a `secret` group redacts only that group: an assignment
/// (`API_TOKEN = "…"`) keeps its identifier and quotes, so redacted source
/// still reads, and parses, as the same statement.
fn replace_preserving_lines(regex: &regex::Regex, content: &str, replacement: &str) -> String {
    regex
        .replace_all(content, |captures: &regex::Captures<'_>| {
            let Some(whole) = captures.get(0) else {
                return String::new();
            };
            let secret = captures.name(SECRET_GROUP).unwrap_or(whole);
            let line_breaks = secret
                .as_str()
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            let mut redacted = String::with_capacity(whole.len() + replacement.len());
            redacted.push_str(&content[whole.start()..secret.start()]);
            redacted.push_str(replacement);
            redacted.extend(std::iter::repeat_n('\n', line_breaks));
            redacted.push_str(&content[secret.end()..whole.end()]);
            redacted
        })
        .into_owned()
}

/// Capture group naming the secret part of a match that also spells its
/// non-secret context (an assignment's identifier).
const SECRET_GROUP: &str = "secret";

/// Returns `true` if pattern at `idx` should be applied for the given file path
/// and content.
///
/// - No `file_context` on the pattern       → always apply.
/// - Has `file_context`, `file_path` given  → apply only when the path matches.
/// - Has `file_context`, no `file_path`     → the global output net (tool
///   results, error/metadata strings) has no path, so fall back to matching the
///   file-context anchor against the CONTENT itself. This re-enables
///   content-keyword patterns (e.g. `.env|config|settings|secrets`, `wandb`,
///   `mollie`, `postmark`) whose context lives in the surrounding text, while
///   UUID/SHA-shaped generics (e.g. `azureSubscriptionId`) still fire only when
///   that keyword context is present — a bare UUID/SHA never triggers them, so
///   the global net does not reintroduce mass false positives. Strict
///   filename-glob anchors (e.g. `\.ya?ml$`, `docker-compose\.ya?ml$`) do not
///   match arbitrary content, so those remain effectively path-gated.
fn should_apply(idx: usize, file_path: Option<&str>, content: &str) -> bool {
    match &FILE_CONTEXT_REGEXES[idx] {
        None => true,
        Some(re) => match file_path {
            Some(path) => re.is_match(path),
            None => re.is_match(content),
        },
    }
}

pub(crate) struct DetectResult {
    pub sanitized: String,
    pub secrets_detected: Vec<String>,
}

/// Detects and redacts secrets over the whole content.
///
/// One literal prescan selects candidate patterns, then each candidate runs one
/// linear `replace_all` over the full haystack. Content is never split into
/// chunks: a slice end satisfies `\b`/`$`, so an open-ended secret cut by a
/// chunk edge was redacted only up to the edge and its tail leaked (review M1).
///
/// `file_path` gates file-context patterns (e.g. Kubernetes YAML secrets, `.env`
/// fine-grained GitHub tokens) so they fire only when the path matches.
pub(crate) fn detect(content: &str, file_path: Option<&str>) -> DetectResult {
    let matched_indices = matching_pattern_indices(content);

    if matched_indices.is_empty() {
        return empty_result(content);
    }

    let mut sanitized = content.to_string();
    let mut secrets_detected = Vec::with_capacity(matched_indices.len());

    for idx in matched_indices {
        if !should_apply(idx, file_path, content) {
            continue;
        }
        let pattern = &PATTERNS[idx];
        let regex = pattern_regex(idx);
        let result = replace_preserving_lines(regex, &sanitized, replacement_for(idx));
        if result != sanitized {
            secrets_detected.push(pattern.name.to_string());
            sanitized = result;
        }
    }

    DetectResult {
        sanitized,
        secrets_detected,
    }
}

// Helper only used in tests below.
#[cfg(test)]
impl DetectResult {
    fn has_secrets_or(&self, other: &Self) -> bool {
        !self.secrets_detected.is_empty() || !other.secrets_detected.is_empty()
    }
}

#[cfg(test)]
#[path = "detector_tests.rs"]
mod tests;
