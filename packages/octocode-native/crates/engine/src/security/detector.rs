use super::patterns::{PATTERN_COUNT, PATTERNS};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use regex::Regex;
use std::sync::{LazyLock, OnceLock};

mod credential_values;

#[cfg(test)]
use credential_values::is_identifier_name;
use credential_values::{KEEPS_NON_CREDENTIAL_VALUES, assigns_a_credential};

/// Lazily-compiled per-pattern Regex instances (compile on first use per index).
/// Sized from `PATTERNS.len()` — never a hardcoded count — so the cell table
/// can never be shorter than the pattern set it indexes.
static PATTERN_REGEX_CELLS: LazyLock<Vec<OnceLock<Regex>>> =
    LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());

/// Rewrite Unicode `\b` to ASCII `(?-u:\b)`. A Unicode word boundary makes the
/// regex crate abandon its fast DFA on any non-ASCII haystack (12–20× slower on
/// multi-MB files). The ASCII boundary also fires next to non-ASCII letters, so
/// it only widens redaction around the ASCII token shapes these patterns match.
fn ascii_word_boundaries(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut chars = pattern.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('b') => out.push_str("(?-u:\\b)"),
            Some(escaped) => {
                out.push('\\');
                out.push(escaped);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Get (compiling at most once) the Regex for pattern `idx`.
fn pattern_regex(idx: usize) -> &'static Regex {
    // Built-in pattern strings are constants validated by the pattern tests; a
    // compile failure is a build-time bug, not a runtime condition.
    #[expect(
        clippy::expect_used,
        reason = "built-in patterns are valid by construction"
    )]
    PATTERN_REGEX_CELLS[idx].get_or_init(|| {
        Regex::new(&ascii_word_boundaries(PATTERNS[idx].regex)).expect(PATTERNS[idx].name)
    })
}

/// Hard cap on content handed to the detector. Content above this is redacted
/// wholesale rather than scanned, bounding worst-case memory/time.
pub(crate) const MAX_CONTENT_SIZE: usize = 10_000_000;
/// Wholesale placeholder emitted when content exceeds `MAX_CONTENT_SIZE`.
pub(crate) const CONTENT_SIZE_LIMIT_PLACEHOLDER: &str = "[CONTENT-REDACTED-SIZE-LIMIT]";
/// File-context regex of pattern `idx`, compiled on first use: only the
/// candidates a text reaches compile theirs. `None`: the pattern has no
/// file-context constraint (always applicable).
fn file_context_regex(idx: usize) -> Option<&'static Regex> {
    static CELLS: LazyLock<Vec<OnceLock<Regex>>> =
        LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());
    let context = PATTERNS[idx].file_context?;
    // Built-in patterns are compile-time constants; an invalid one is a build bug.
    #[expect(
        clippy::panic,
        reason = "built-in patterns are compile-time constants; an invalid one is a build bug"
    )]
    Some(CELLS[idx].get_or_init(|| {
        Regex::new(context)
            .unwrap_or_else(|e| panic!("invalid file_context regex '{context}': {e}"))
    }))
}

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
//   1. At build time (`build.rs`), extract every pattern's gate via
//      regex-syntax HIR: its *required prefix literals*, else literals its
//      structure requires inside every match. A pattern gates on its
//      literals only when extraction proves the set is finite and every
//      literal is a meaningful anchor (>= 3 bytes).
//   2. At first call, build ONE case-insensitive aho-corasick automaton over
//      those literals.
//   3. Per call: scan content once with aho-corasick; only patterns whose
//      literal appears (plus the small no-literal fallback bucket) get their
//      regex compiled (lazily, once per process) and confirmed with is_match.
//
// Correctness invariant: for a gated pattern, "regex matches content" implies
// "content contains one of its gate literals" (a match begins with a prefix
// literal, and contains a match of every concatenated part, of one branch of
// an alternation, and of a repetition's body when its minimum is one), and
// patterns are parsed with their flags, so a `(?i)` pattern's literals carry
// every Unicode case variant its regex matches (`ſ` for `s`); the automaton
// is ASCII-case-insensitive over literals stored lowercased. Patterns whose
// literals cannot be proven (pure entropy shapes, UUIDs) are ALWAYS
// candidates — they are never gated out.
// ---------------------------------------------------------------------------

/// The prescan gate `build.rs` derives from [`PATTERNS`]: `GATE_LITERALS`
/// (lowercased literal, pattern index) and the `FALLBACK` pattern indices.
mod gates {
    include!(concat!(env!("OUT_DIR"), "/secret_gates.rs"));
}

/// One automaton over every gated pattern's literals; its pattern id indexes
/// `gates::GATE_LITERALS`.
static PRESCAN: LazyLock<AhoCorasick> = LazyLock::new(|| {
    // Aho-Corasick over a fixed literal set cannot fail to build.
    #[expect(
        clippy::expect_used,
        reason = "built from static patterns, valid by construction"
    )]
    AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build(gates::GATE_LITERALS.iter().map(|(literal, _)| literal))
        .expect("prescan literal automaton must build")
});

/// A set of pattern indices, one bit per pattern, kept on the stack: picking
/// a text's candidates allocates nothing.
#[derive(Clone, Copy, Default)]
struct PatternSet([u64; PATTERN_COUNT.div_ceil(64)]);

impl PatternSet {
    fn insert(&mut self, idx: usize) {
        self.0[idx / 64] |= 1 << (idx % 64);
    }

    fn is_empty(&self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }

    /// The members, in canonical pattern order.
    fn iter(self) -> impl Iterator<Item = usize> {
        self.0.into_iter().enumerate().flat_map(|(at, word)| {
            let mut rest = word;
            std::iter::from_fn(move || {
                (rest != 0).then(|| {
                    let bit = rest.trailing_zeros() as usize;
                    rest &= rest - 1;
                    at * 64 + bit
                })
            })
        })
    }

    /// The members `keep` accepts.
    fn filter(self, mut keep: impl FnMut(usize) -> bool) -> Self {
        let mut kept = Self::default();
        for idx in self.iter().filter(|&idx| keep(idx)) {
            kept.insert(idx);
        }
        kept
    }
}

/// Candidate patterns for `content`: gated patterns whose literal appears,
/// plus the always-on fallback bucket. Callers confirm each with the real
/// pattern regex (compiled lazily, only for candidates), so they still see
/// exact match semantics.
fn prescan_candidates(content: &str) -> PatternSet {
    let mut candidates = PatternSet::default();
    // Overlapping scan: a literal nested inside another match span must still
    // register its own pattern, or that pattern would be silently gated out.
    for hit in PRESCAN.find_overlapping_iter(content) {
        candidates.insert(gates::GATE_LITERALS[hit.pattern().as_usize()].1);
    }
    for &idx in gates::FALLBACK {
        candidates.insert(idx);
    }
    candidates
}

/// Candidates that apply to `scope` ([`should_apply`]), checked before any
/// pattern regex compiles: a pattern its file context rules out (the
/// multi-millisecond Kubernetes Secret automaton outside YAML) is never built
/// for that text, so a process pays only for the detectors its output reaches.
fn applicable_patterns(content: &str, scope: Scope<'_>) -> PatternSet {
    prescan_candidates(content).filter(|idx| should_apply(idx, scope, content))
}

fn matching_patterns(content: &str, scope: Scope<'_>) -> PatternSet {
    applicable_patterns(content, scope).filter(|idx| pattern_regex(idx).is_match(content))
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
    match file_context_regex(idx) {
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

/// Detects and redacts secrets over the whole content; `None` when nothing
/// was redacted, so clean content is never copied.
///
/// One literal prescan selects candidate patterns, then each candidate runs one
/// linear `replace_all` over the full haystack. Content is never split into
/// chunks: a slice end satisfies `\b`/`$`, so an open-ended secret cut by a
/// chunk edge was redacted only up to the edge and its tail leaked (review M1).
///
/// `scope` gates file-context patterns (e.g. Kubernetes YAML secrets, `.env`
/// fine-grained GitHub tokens) so they fire only where they apply.
pub(crate) fn redact(content: &str, scope: Scope<'_>) -> Option<DetectResult> {
    let matched = matching_patterns(content, scope);
    if matched.is_empty() {
        return None;
    }
    let mut sanitized: Option<String> = None;
    let mut secrets_detected = Vec::new();
    for idx in matched.iter() {
        let current = sanitized.as_deref().unwrap_or(content);
        let keeps_values = KEEPS_NON_CREDENTIAL_VALUES[idx];
        let result = replace_secrets(
            pattern_regex(idx),
            current,
            replacement_for(idx),
            |key, secret| !keeps_values || assigns_a_credential(key, secret),
        );
        if result != current {
            secrets_detected.push(PATTERNS[idx].name.to_string());
            sanitized = Some(result);
        }
    }
    sanitized.map(|sanitized| DetectResult {
        sanitized,
        secrets_detected,
    })
}

#[cfg(test)]
mod tests;
