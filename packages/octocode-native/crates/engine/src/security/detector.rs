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
#[expect(
    clippy::panic,
    reason = "built-in patterns are compile-time constants; an invalid one is a build bug"
)]
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
        .map(|p| super::redaction_placeholder(p.name))
        .collect()
});

/// Patterns that read configuration syntax (`name: value`). In a source-code
/// file the same line is a type member, parameter or object key, never a
/// literal secret (quoted literals are caught by `envVarSecrets`).
const CONFIG_SYNTAX_PATTERNS: &[&str] = &["yamlSecretValue"];

/// Index-aligned with PATTERNS: the pattern is skipped for source-code paths.
static SKIPS_SOURCE_CODE: LazyLock<Vec<bool>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .map(|p| CONFIG_SYNTAX_PATTERNS.contains(&p.name))
        .collect()
});

/// Patterns that take any value assigned to a credential-named key. Only the
/// key's name says "credential", so a value that is not one is kept: a value
/// that spells a code identifier
/// (`"x-kubernetes-patch-merge-key": "resourceClaimName"`) names a field, and
/// a hex digest keyed by a file path (`"bin/with_credentials.mjs": "<sha256>"`)
/// is a checksum manifest entry. See [`assigns_a_credential`].
const CREDENTIAL_ASSIGNMENT_PATTERNS: &[&str] =
    &["envVarSecrets", "dotenvSecretAssignment", "yamlSecretValue"];

/// Index-aligned with PATTERNS: the pattern keeps values that are not credentials.
static KEEPS_NON_CREDENTIAL_VALUES: LazyLock<Vec<bool>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .map(|p| CREDENTIAL_ASSIGNMENT_PATTERNS.contains(&p.name))
        .collect()
});

/// Whether a credential-assignment match assigns a credential: its `secret`
/// value is not an identifier name ([`is_identifier_name`]) and not a hex
/// digest keyed by a file path ([`is_file_digest`]). `key` is the line's
/// text before the value (ending in the key, its quotes and its operator).
fn assigns_a_credential(key: &str, secret: &str) -> bool {
    !is_identifier_name(secret) && !is_file_digest(key, secret) && !reads_a_name(key, secret)
}

/// Whether an unquoted `secret` ends a code statement that assigns another
/// name: an optional `!`, `&` or `*`, a dotted path of identifier words
/// ([`is_identifier_word`]) and the `;` terminator
/// (`frame.expect_key = frame.object;`). Dotenv, INI, properties and shell
/// values carry no statement terminator, and a quoted value (`key` ends in
/// its opening quote) is a literal whatever it spells.
fn reads_a_name(key: &str, secret: &str) -> bool {
    !key.ends_with(['"', '\''])
        && secret.strip_suffix(';').is_some_and(|name| {
            name.trim_start_matches(['!', '&', '*'])
                .split('.')
                .all(is_identifier_word)
        })
}

/// One segment of a code name: a camelCase name ([`is_identifier_name`]) or
/// snake_case / SCREAMING_SNAKE words that each read as a word
/// ([`pronounceable_word`]): `object`, `inside_lines`, `API_KEY`.
fn is_identifier_word(segment: &str) -> bool {
    is_identifier_name(segment)
        || (!segment.is_empty()
            && segment
                .split('_')
                .filter(|word| !word.is_empty())
                .all(|word| pronounceable_word(word.to_ascii_lowercase().as_bytes()))
            && segment.bytes().any(|byte| byte != b'_'))
}

/// Whether `secret` is a hex digest (MD5, SHA-1, SHA-256 or SHA-512 length)
/// assigned to a key that names a file: a path whose last segment is a file
/// name with an extension (`config\\credentials.yaml`), or a source file
/// name (`with_credentials.mjs`). A checksum manifest maps files to digests,
/// and a credential-named file is still a file; a credential under a key
/// that is not a file name, a secret path (`secret/data/prod/datadog_api_key`)
/// included, stays redacted whatever its shape.
fn is_file_digest(key: &str, secret: &str) -> bool {
    let digest = matches!(secret.len(), 32 | 40 | 64 | 128)
        && secret.bytes().all(|byte| byte.is_ascii_hexdigit());
    let name = key
        .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ':' | '=' | '>'))
        .rsplit(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '{' | ',' | '('))
        .next()
        .unwrap_or_default();
    digest
        && ((name.contains(['/', '\\']) && has_file_extension(name)) || is_source_code_path(name))
}

/// Whether the last segment of `path` is a file name with an extension: a
/// stem, then a dot and up to 10 ASCII letters or digits, a letter among them.
fn has_file_extension(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && (1..=10).contains(&ext.len())
            && ext.bytes().all(|byte| byte.is_ascii_alphanumeric())
            && ext.bytes().any(|byte| byte.is_ascii_alphabetic())
    })
}

/// Whether `value` is a camelCase or PascalCase name of two or more
/// pronounceable words (`resourceClaimName`, `createdAt`, `ContainerPort`):
/// letters only, each capital starts a word of at least two letters, and
/// every word has a vowel and no run of more than three consonants. A random
/// token of credential length almost never has this shape (about 1 in 15,000
/// of 16 random letters, far fewer with digits); field, type and property
/// names do.
pub(crate) fn is_identifier_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !bytes.iter().all(u8::is_ascii_alphabetic) {
        return false;
    }
    let starts: Vec<usize> = std::iter::once(0)
        .chain((1..bytes.len()).filter(|&at| bytes[at].is_ascii_uppercase()))
        .collect();
    starts.len() >= 2
        && starts.iter().enumerate().all(|(index, &start)| {
            let end = starts.get(index + 1).copied().unwrap_or(bytes.len());
            pronounceable_word(&bytes[start..end])
        })
}

/// One word of [`is_identifier_name`]: an optional capital, then lowercase.
fn pronounceable_word(word: &[u8]) -> bool {
    let vowel = |byte: u8| b"aeiouy".contains(&byte.to_ascii_lowercase());
    let mut consonants = 0;
    word.len() >= 2
        && word[1..].iter().all(u8::is_ascii_lowercase)
        && word.iter().any(|&byte| vowel(byte))
        && word.iter().all(|&byte| {
            consonants = if vowel(byte) { 0 } else { consonants + 1 };
            consonants <= 3
        })
}

/// Whether `path` names a source-code file: its extension belongs to a
/// grammar language the engine parses.
fn is_source_code_path(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        crate::signatures::languages::find_entry(&ext.to_ascii_lowercase()).is_some()
    })
}

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
    #[expect(
        clippy::expect_used,
        reason = "built from static patterns, valid by construction"
    )]
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

/// Candidates that apply to `scope` ([`should_apply`]), checked before any
/// pattern regex compiles: a pattern its file context rules out (the
/// multi-millisecond Kubernetes Secret automaton outside YAML) is never built
/// for that text, so a process pays only for the detectors its output reaches.
fn applicable_candidates(content: &str, scope: Scope<'_>) -> Vec<usize> {
    let mut candidates = candidate_pattern_indices(content);
    candidates.retain(|&idx| should_apply(idx, scope, content));
    candidates
}

fn matching_pattern_indices(content: &str, scope: Scope<'_>) -> Vec<usize> {
    let mut candidates = applicable_candidates(content, scope);
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
/// still reads, and parses, as the same statement. An alternation names one
/// group per branch (`secret`, `secret_…`); the participating one is redacted.
/// Only the secrets `redacts` accepts (given the line's text before the
/// secret, and the secret) are redacted; a rejected match stays verbatim.
fn replace_secrets(
    regex: &regex::Regex,
    content: &str,
    replacement: &str,
    redacts: impl Fn(&str, &str) -> bool,
) -> String {
    let secret_groups: Vec<usize> = regex
        .capture_names()
        .enumerate()
        .filter_map(|(index, name)| {
            name.filter(|name| name.starts_with(SECRET_GROUP))
                .map(|_| index)
        })
        .collect();
    regex
        .replace_all(content, |captures: &regex::Captures<'_>| {
            let Some(whole) = captures.get(0) else {
                return String::new();
            };
            let secret = secret_groups
                .iter()
                .find_map(|&index| captures.get(index))
                .unwrap_or(whole);
            let line_start = content[..whole.start()].rfind('\n').map_or(0, |at| at + 1);
            if !redacts(&content[line_start..secret.start()], secret.as_str()) {
                return whole.as_str().to_owned();
            }
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

/// Capture group name prefix marking the secret part of a match that also
/// spells its non-secret context (an assignment's identifier).
const SECRET_GROUP: &str = "secret";

/// Returns `true` if pattern at `idx` should be applied for the given file path
/// and content.
///
/// - A config-syntax pattern ([`CONFIG_SYNTAX_PATTERNS`]) and a source-code
///   `file_path`                            → never apply.
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
fn should_apply(idx: usize, scope: Scope<'_>, content: &str) -> bool {
    if SKIPS_SOURCE_CODE[idx] && scope.source_path().is_some_and(is_source_code_path) {
        return false;
    }
    match &FILE_CONTEXT_REGEXES[idx] {
        None => true,
        Some(re) => match scope {
            Scope::File(path) => re.is_match(path),
            Scope::Pathless | Scope::Leaf(_) => re.is_match(content),
        },
    }
}

/// Where the scanned text came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope<'a> {
    /// No known origin: the global output net.
    Pathless,
    /// The text is the content of the file at this path; the path gates
    /// file-context patterns.
    File(&'a str),
    /// The text is one leaf of a result about the file at this path (a
    /// returned line, match or snippet). The path says only what language
    /// the text is in; file-context patterns stay content-gated exactly as
    /// in the pathless net, so the net is never weaker than without a path.
    Leaf(&'a str),
}

impl<'a> Scope<'a> {
    fn source_path(self) -> Option<&'a str> {
        match self {
            Scope::Pathless => None,
            Scope::File(path) | Scope::Leaf(path) => Some(path),
        }
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
#[cfg(test)]
pub(crate) fn detect(content: &str, file_path: Option<&str>) -> DetectResult {
    detect_in(content, file_path.map_or(Scope::Pathless, Scope::File))
}

/// [`detect`] for text of a known [`Scope`].
pub(crate) fn detect_in(content: &str, scope: Scope<'_>) -> DetectResult {
    let matched_indices = matching_pattern_indices(content, scope);

    if matched_indices.is_empty() {
        return empty_result(content);
    }

    let mut sanitized = content.to_string();
    let mut secrets_detected = Vec::with_capacity(matched_indices.len());

    for idx in matched_indices {
        let pattern = &PATTERNS[idx];
        let regex = pattern_regex(idx);
        let keeps_values = KEEPS_NON_CREDENTIAL_VALUES[idx];
        let result = replace_secrets(regex, &sanitized, replacement_for(idx), |key, secret| {
            !keeps_values || assigns_a_credential(key, secret)
        });
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
