use super::patterns::PATTERNS;
use regex::Regex;
use std::sync::{LazyLock, OnceLock};

mod code_syntax;
mod credential_values;
mod key_facts;

use code_syntax::CodeSyntax;
use credential_values::VALUE_TESTS;
use credential_values::{ValueTest, is_identifier_name};

mod document;
mod every_value;
mod jwk;
mod long_line;
mod long_text;
mod objects;
mod source_names;
pub(super) mod structured;
mod yaml_secret;
pub use every_value::every_value_spans;
pub use jwk::hide_members as hide_jwk_members;
pub use long_line::{LONG_LINE_BYTES, LongLine};
pub use source_names::{Named, SecretForm, SecretSpan, SecretSpans, SourceNames, SourceWalk};

mod prescan;
mod regex_forms;
mod tail_captures;
mod tail_reach;
mod tail_reads;

use prescan::{PatternSet, prescan_candidates};

/// The prescan gate `build.rs` derives from [`PATTERNS`]: `GATE_LITERALS`
/// (lowercased literal, pattern index) and the `FALLBACK` pattern indices
/// ([`prescan`]).
mod gates {
    include!(concat!(env!("OUT_DIR"), "/secret_gates.rs"));
}

mod file_context;

use file_context::BARE_SHAPE;
pub use file_context::WholeSourcePatterns;
pub use gates::LONGEST_BOUNDED_MATCH;

use regex_forms::{anchored_pattern_regex, pattern_regex_for};
#[cfg(test)]
use regex_forms::{ascii_word_boundaries, pattern_regex};

/// The largest source a read holds whole, and the text this pass judges pattern by
/// pattern. A longer one is judged in one pass over its spans ([`long_text`]).
pub const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024;
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

/// Patterns that read configuration syntax (`name: value`, `NAME=value`). In
/// a source-code file the same line is a statement, a type member, a parameter
/// or an object key, never a config entry: only text the grammar places in a
/// string or a comment is read as config ([`code_syntax`]). Quoted literals in
/// code are `envVarSecrets`' own.
const CONFIG_SYNTAX_PATTERNS: &[&str] = &[
    "yamlSecretValue",
    "dotenvFileCredential",
    "propertiesFileCredential",
];

/// Patterns beside [`CONFIG_SYNTAX_PATTERNS`] that read a line-anchored
/// `NAME=value`: in a source-code file they too find a credential in a comment
/// wherever the comment sits on its line.
const COMMENT_SYNTAX_PATTERNS: &[&str] = &["dotenvSecretAssignment"];

/// Index-aligned with PATTERNS: the pattern reads text a comment holds.
static READS_COMMENT_SYNTAX: LazyLock<Vec<bool>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .map(|p| {
            CONFIG_SYNTAX_PATTERNS.contains(&p.name) || COMMENT_SYNTAX_PATTERNS.contains(&p.name)
        })
        .collect()
});

/// Index-aligned with PATTERNS: the pattern reads configuration syntax.
static READS_CONFIG_SYNTAX: LazyLock<Vec<bool>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .map(|p| CONFIG_SYNTAX_PATTERNS.contains(&p.name))
        .collect()
});

/// Whether `path` names a source-code file: its extension belongs to a
/// grammar language the engine parses.
fn is_source_code_path(path: &str) -> bool {
    code_syntax::grammar_of(path).is_some()
}

/// How the matches of one pattern in one text are judged: the pattern's value
/// test, and for a pattern that reads configuration syntax in a source-code
/// file, the file's syntax.
pub(super) struct Judge<'a> {
    /// The pattern's index in the registry.
    pattern: usize,
    value_test: ValueTest,
    syntax: Option<SyntaxOf<'a>>,
    /// The pattern's matches in code are statements, no credentials.
    code_is_no_config: bool,
}

/// The syntax a [`Judge`] reads: its own parse, or the one every judge of the
/// same text shares, so one text parses once however many patterns read it.
enum SyntaxOf<'a> {
    Own(CodeSyntax<'a>),
    Shared(&'a CodeSyntax<'a>),
}

impl<'a> std::ops::Deref for SyntaxOf<'a> {
    type Target = CodeSyntax<'a>;

    fn deref(&self) -> &CodeSyntax<'a> {
        match self {
            Self::Own(syntax) => syntax,
            Self::Shared(syntax) => syntax,
        }
    }
}

impl<'a> Judge<'a> {
    /// The judge of pattern `idx` over `text`; `shared` is the syntax of
    /// that same text when the caller already holds it.
    pub(super) fn of(
        idx: usize,
        scope: Scope<'a>,
        text: &'a str,
        shared: Option<&'a CodeSyntax<'a>>,
    ) -> Self {
        let syntax = scope
            .source_path()
            .filter(|_| READS_COMMENT_SYNTAX[idx])
            .and_then(|path| match shared {
                Some(syntax) => Some(SyntaxOf::Shared(syntax)),
                None => CodeSyntax::of(path, text).map(SyntaxOf::Own),
            });
        Self {
            pattern: idx,
            value_test: VALUE_TESTS[idx],
            syntax,
            code_is_no_config: READS_CONFIG_SYNTAX[idx],
        }
    }

    /// Whether the match `at`, whose value is `secret` after `key`, is a
    /// credential. In a source-code file a configuration rule's match is
    /// judged by where the grammar places it: in code it is no config entry,
    /// in a comment it is prose first ([`ValueTest::redacts_in_comment`]), in
    /// other text a config line that may be code ([`ValueTest::redacts_in_code_text`]).
    pub(super) fn redacts(&self, key: &str, secret: &str, at: &std::ops::Range<usize>) -> bool {
        let Some(syntax) = self.syntax.as_ref().filter(|_| self.code_is_no_config) else {
            return self.value_test.redacts(key, secret);
        };
        match syntax.place_of(at) {
            code_syntax::Place::Code => false,
            code_syntax::Place::Comment => self.value_test.redacts_in_comment(key, secret),
            code_syntax::Place::Text => self.value_test.redacts_in_code_text(key, secret),
        }
    }

    /// Calls `found(whole, secret)` for each credential `regex` finds in
    /// `text`: [`accepted_secrets`] over the text, and in a source-code file
    /// over each comment's credential-word tails, which the text's own
    /// line anchors do not reach (`x = 1; // password: …`). Ranges come in
    /// order and none overlaps another.
    pub(super) fn secrets(
        &self,
        regex: &regex::Regex,
        text: &str,
        mut found: impl FnMut(std::ops::Range<usize>, std::ops::Range<usize>),
    ) {
        let Some(syntax) = &self.syntax else {
            accepted_secrets(
                regex,
                text,
                line_start_of,
                |k, s, at| self.redacts(k, s, at),
                found,
            );
            return;
        };
        let mut all = Vec::new();
        accepted_secrets(
            regex,
            text,
            line_start_of,
            |k, s, at| self.redacts(k, s, at),
            |whole, secret| {
                all.push((whole, secret));
            },
        );
        all.extend(self.tail_secrets(text, syntax, &mut 0));
        all.sort_by_key(|(_, secret)| (secret.start, std::cmp::Reverse(secret.end)));
        let mut covered_to = 0;
        for (whole, secret) in all {
            if secret.start >= covered_to {
                covered_to = secret.end;
                found(whole, secret);
            }
        }
    }
}

fn replacement_for(idx: usize) -> &'static str {
    &REPLACEMENTS[idx]
}

/// Candidates that apply to `scope` ([`should_apply`]), checked before any
/// pattern regex compiles: a pattern its file context rules out is never built
/// for that text, so a process pays only for the detectors its output reaches.
fn applicable_patterns(content: &str, scope: Scope<'_>) -> PatternSet {
    prescan_candidates(content).filter(|idx| should_apply(idx, scope, content))
}

/// The patterns that may redact in `content`: those whose regex matches it.
/// A pattern that also reads the comments of a source-code file stays a
/// candidate there whatever the regex says of the whole text: its line
/// anchors do not reach a comment after code ([`Judge::secrets`]).
fn matching_patterns(content: &str, scope: Scope<'_>) -> PatternSet {
    let reads_comments = scope
        .source_path()
        .and_then(code_syntax::grammar_of)
        .is_some_and(|grammar| code_syntax::has_credential_assignment(content, grammar));
    applicable_patterns(content, scope).filter(|idx| {
        (reads_comments && READS_COMMENT_SYNTAX[idx])
            || pattern_regex_for(idx, content).is_match(content)
    })
}

/// The group a pattern puts around a diff line's `+` or `-` prefix.
const DIFF_PREFIX_GROUP: &str = "diff";

/// Which matches of a pattern with a [`DIFF_PREFIX_GROUP`] are diff lines
/// outside every hunk, so no match: a `+` or `-` before a line makes it a
/// diff line only after a hunk header (`@@ -1,3 +1,4 @@`) in the same text.
/// Elsewhere it is part of the text (`-DMAX_TOKENS=4096`, a compiler flag).
pub(super) struct DiffLines<'t> {
    group: Option<usize>,
    text: &'t str,
    first_hunk: std::cell::OnceCell<Option<usize>>,
}

impl<'t> DiffLines<'t> {
    pub(super) fn new(regex: &Regex, text: &'t str) -> Self {
        Self {
            group: regex
                .capture_names()
                .position(|name| name == Some(DIFF_PREFIX_GROUP)),
            text,
            first_hunk: std::cell::OnceCell::new(),
        }
    }

    /// The index of the pattern's [`DIFF_PREFIX_GROUP`], if it has one.
    pub(super) fn group(&self) -> Option<usize> {
        self.group
    }

    /// Whether the match `captures` holds a diff line's prefix.
    pub(super) fn prefixed(&self, captures: &regex::Captures<'_>) -> bool {
        self.group
            .is_some_and(|group| captures.get(group).is_some())
    }

    /// Whether a match on the line starting at `line_start`, `prefixed` when
    /// its diff group took part ([`DiffLines::prefixed`]), is a diff line
    /// before any hunk header.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "offsets are a regex match's, or the start of a line after an ASCII line-break char"
        )
    )]
    pub(super) fn outside_hunk(&self, prefixed: bool, line_start: usize) -> bool {
        #[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
        static HUNK_HEADER: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?m)^@@ -[0-9]+(?:,[0-9]+)? \+[0-9]+(?:,[0-9]+)? @@").expect("hunk header")
        });
        // A `-` and a blank open a YAML list item in plain text; only in a
        // hunk is it a removed line.
        let list_item = self.text[line_start..]
            .strip_prefix('-')
            .is_some_and(|rest| rest.starts_with([' ', '\t']));
        prefixed
            && !list_item
            && self
                .first_hunk
                .get_or_init(|| HUNK_HEADER.find(self.text).map(|header| header.end()))
                .is_none_or(|end| end > line_start)
    }
}

/// The line breaks of `text`, each as written: `\n`, `\r\n` or a lone `\r`.
/// The one definition of a line end here: line starts, kept breaks and
/// continued values all read it.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are `find` results and `break_len` of ASCII break chars"
    )
)]
fn line_breaks(text: &str) -> impl Iterator<Item = &str> {
    let mut at = 0;
    std::iter::from_fn(move || {
        let start = at + text[at..].find(LINE_END_CHARS)?;
        at = start + break_len(&text[start..])?;
        Some(&text[start..at])
    })
}

/// The characters a line break is made of.
const LINE_END_CHARS: [char; 2] = ['\n', '\r'];

/// The length of the line break `text` starts with, if it starts with one.
fn break_len(text: &str) -> Option<usize> {
    if text.starts_with("\r\n") {
        Some(2)
    } else {
        text.starts_with(LINE_END_CHARS).then_some(1)
    }
}

/// The line breaks a redacted `secret` spanned, each as it was, so its
/// replacement keeps the source's lines and line ends.
fn kept_line_breaks(secret: &str) -> String {
    line_breaks(secret).collect()
}

/// The end of the value a secret ending at `end` continues into. A secret
/// that ends in an odd number of backslashes ends inside an escape. Before a
/// line break the line goes on at the next one (Java properties, a shell
/// reading a dotenv file): each such line and the one that ends the value are
/// the value's own, in whatever rule found its first line. Before any other
/// character the escape is that character's (`Password=Pw0rd9q\";` in a code
/// literal, where the rule stopped at the quote), so the secret takes it: a
/// replacement that left the backslash would escape what follows it and end
/// the literal there, and every later rule would read a text of broken syntax.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are a regex match end, `find` results and ASCII break lengths"
    )
)]
fn continued_end(text: &str, mut end: usize) -> usize {
    loop {
        let backslashes = text.as_bytes()[..end]
            .iter()
            .rev()
            .take_while(|byte| **byte == b'\\')
            .count();
        if backslashes % 2 == 0 {
            return end;
        }
        let Some(len) = break_len(&text[end..]) else {
            return end + text[end..].chars().next().map_or(0, char::len_utf8);
        };
        let next = end + len;
        end = text[next..]
            .find(LINE_END_CHARS)
            .map_or(text.len(), |at| next + at);
    }
}

/// Capture group name prefix marking the secret part of a match that also
/// spells its non-secret context (an assignment's identifier).
const SECRET_GROUP: &str = "secret";

/// The capture groups of `regex` that name a match's secret: one per branch of
/// an alternation (`secret`, `secret_…`).
fn secret_groups(regex: &regex::Regex) -> Vec<usize> {
    regex
        .capture_names()
        .enumerate()
        .filter_map(|(index, name)| {
            name.filter(|name| name.starts_with(SECRET_GROUP))
                .map(|_| index)
        })
        .collect()
}

/// Where a line's key starts when the line starts it: the line's start.
fn line_start_of(_match_start: usize, line_start: usize) -> usize {
    line_start
}

/// Calls `found(whole, secret)` for each match of `regex` in `text` whose
/// secret is a credential: the one judgment every scanner of a pattern makes.
/// A pattern with a `secret` group names that group's range, else the whole
/// match. Left out are a placeholder (a secret an earlier pattern already
/// replaced, as a URL's password that two URL patterns name: counted once, its
/// label stays), a match outside a diff's hunks, and one `redacts` turns away
/// (given the text from where the line's key starts, which `key_start` says
/// from the match's start and its line's start, to the secret, and the secret).
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are regex match bounds and a `rfind` of an ASCII break char"
    )
)]
fn accepted_secrets(
    regex: &regex::Regex,
    text: &str,
    key_start: impl Fn(usize, usize) -> usize,
    redacts: impl Fn(&str, &str, &std::ops::Range<usize>) -> bool,
    mut found: impl FnMut(std::ops::Range<usize>, std::ops::Range<usize>),
) {
    let groups = secret_groups(regex);
    // Matches come in order, so only the text since the last one can hold a
    // later line start: searching back to the start of the text from each
    // match costs quadratic time on one long line.
    let (mut line_start, mut searched_to, mut covered_to) = (0, 0, 0);
    let diff_lines = DiffLines::new(regex, text);
    for captures in regex.captures_iter(text) {
        let Some(raw) = RawMatch::of(&captures, &groups) else {
            continue;
        };
        if let Some(at) = text[searched_to..raw.whole.start].rfind(LINE_END_CHARS) {
            line_start = searched_to + at + 1;
        }
        searched_to = raw.whole.start;
        let secret = &raw.secret;
        if secret.start >= covered_to
            && raw.is_kept(
                &diff_lines,
                diff_lines.prefixed(&captures),
                text,
                line_start,
            )
            && redacts(
                &text[key_start(raw.whole.start, line_start).min(secret.start)..secret.start],
                &text[secret.clone()],
                &raw.whole,
            )
        {
            let end = continued_end(text, secret.end);
            covered_to = end;
            found(raw.whole.start..raw.whole.end.max(end), secret.start..end);
        }
    }
}

/// A match of a pattern before its value is judged: where it lies, and the
/// secret group's range (the whole match for a pattern with none). The
/// judgment of the value ([`ValueTest`]) is a step of its own, so one match
/// can be judged against several keys.
pub(super) struct RawMatch {
    pub(super) whole: std::ops::Range<usize>,
    pub(super) secret: std::ops::Range<usize>,
}

impl RawMatch {
    /// The match `captures` holds: `None` for one with no whole match.
    fn of(captures: &regex::Captures<'_>, groups: &[usize]) -> Option<Self> {
        let whole = captures.get(0)?;
        let secret = groups
            .iter()
            .find_map(|&index| captures.get(index))
            .unwrap_or(whole);
        Some(Self {
            whole: whole.range(),
            secret: secret.range(),
        })
    }

    /// Whether what the text says of the match leaves it a candidate: no
    /// placeholder, and no diff line before any hunk. `line_start` is where
    /// the match's line starts; `prefixed`, whether its diff group took part.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "a secret's bounds are regex match bounds"
        )
    )]
    fn is_kept(
        &self,
        diff_lines: &DiffLines<'_>,
        prefixed: bool,
        text: &str,
        line_start: usize,
    ) -> bool {
        !is_placeholder(&text[self.secret.clone()])
            && !diff_lines.outside_hunk(prefixed, line_start)
    }
}

/// The match of `regex` at the start of `text`, if it matches there, and
/// whether it is a candidate ([`RawMatch::is_kept`]).
pub(super) fn anchored_match(
    regex: &regex::Regex,
    groups: &[usize],
    text: &str,
) -> Option<(RawMatch, bool)> {
    let captures = regex.captures(text)?;
    let raw = RawMatch::of(&captures, groups)?;
    let diff_lines = DiffLines::new(regex, text);
    let kept = raw.is_kept(&diff_lines, diff_lines.prefixed(&captures), text, 0);
    Some((raw, kept))
}

/// A redacted multiline match still occupies its source lines. File reads
/// report and classify source-line scopes, so collapsing a secret block into
/// one line would shift every later focus window.
///
/// Only the secrets [`accepted_secrets`] finds are redacted, and only their
/// secret group: an assignment (`API_TOKEN = "…"`) keeps its identifier and
/// quotes, so redacted source still reads, and parses, as the same statement.
/// A rejected match stays verbatim. Also returns each replaced range of
/// `content` with the bytes it became.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(clippy::string_slice, reason = "offsets are regex match bounds")
)]
fn replace_judged(
    content: &str,
    replacement: &str,
    scan: impl FnOnce(&mut dyn FnMut(std::ops::Range<usize>, std::ops::Range<usize>)),
) -> (String, Vec<(std::ops::Range<usize>, usize)>) {
    let mut text = String::with_capacity(content.len());
    let mut replaced = Vec::new();
    let mut from = 0;
    scan(&mut |_, secret| {
        let line_breaks = kept_line_breaks(&content[secret.clone()]);
        text.push_str(&content[from..secret.start]);
        text.push_str(replacement);
        text.push_str(&line_breaks);
        replaced.push((secret.clone(), replacement.len() + line_breaks.len()));
        from = secret.end;
    });
    text.push_str(&content[from..]);
    (text, replaced)
}

/// [`replace_judged`] over the matches of `regex` that `redacts` accepts.
#[cfg(test)]
fn replace_secrets(
    regex: &regex::Regex,
    content: &str,
    replacement: &str,
    redacts: impl Fn(&str, &str, &std::ops::Range<usize>) -> bool,
) -> (String, Vec<(std::ops::Range<usize>, usize)>) {
    replace_judged(content, replacement, |found| {
        accepted_secrets(regex, content, line_start_of, redacts, found);
    })
}

/// `text` is exactly one pattern's placeholder.
fn is_placeholder(text: &str) -> bool {
    text.starts_with(super::REDACTION_PREFIX)
        && REPLACEMENTS.iter().any(|placeholder| placeholder == text)
}

/// Returns `true` if pattern at `idx` should be applied for the given file path
/// and content.
///
/// - No `file_context` on the pattern       → always apply.
/// - Has `file_context`, `file_path` given  → apply only when the path matches.
/// - Has `file_context`, stored text        → see [`Scope::Stored`].
/// - Has `file_context`, no `file_path`     → the global output net (tool
///   results, error/metadata strings) has no path, so fall back to matching the
///   file-context anchor against the CONTENT itself. This re-enables
///   content-keyword patterns (e.g. `.env|config|settings|secrets`, `wandb`,
///   `mollie`, `postmark`) whose context lives in the surrounding text, while
///   UUID/SHA-shaped generics (e.g. `azureSubscriptionId`) still fire only when
///   that keyword context is present — a bare UUID/SHA never triggers them, so
///   the global net does not reintroduce mass false positives. Strict
///   filename-glob anchors (e.g. `\.ya?ml$`, `docker-compose\.ya?ml$`) do not
///   match arbitrary content, so those remain effectively path-gated. A leaf
///   about a file also applies what that file's path gates, as its read does,
///   except a bare shape ([`BARE_SHAPE`]): a leaf may be a field beside the
///   file's text rather than the text.
fn should_apply(idx: usize, scope: Scope<'_>, content: &str) -> bool {
    if let Scope::Judged { leaf, source } = scope {
        return should_apply(idx, Scope::of_leaf(leaf), content)
            && !should_apply(idx, Scope::File(source), content);
    }
    match file_context_regex(idx) {
        None => true,
        Some(re) => match scope {
            Scope::File(path) => re.is_match(path),
            Scope::Pathless => re.is_match(content),
            Scope::Leaf(path) => re.is_match(content) || (!BARE_SHAPE[idx] && re.is_match(path)),
            Scope::Stored(path) => {
                !BARE_SHAPE[idx]
                    || re.is_match(content)
                    || path.is_some_and(|path| re.is_match(path))
            }
            // Split into its two nets above.
            Scope::Judged { .. } => false,
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
    /// The text is about to be kept, and the read that will show it may name
    /// any file (`path`, when known, is only the best guess): every file-context
    /// pattern applies, so nothing a read would redact is kept. A bare shape
    /// still needs its context named here, or every commit SHA would count.
    Stored(Option<&'a str>),
    /// The text is a leaf of a result about the file at `leaf` (`None`: no
    /// known file), cut from text that was judged whole as [`Scope::File`] at
    /// `source`. Only the patterns the leaf net ([`Scope::of_leaf`]) applies
    /// and that whole pass did not apply run here, so the two passes together
    /// apply every pattern the leaf net applies: one detector set, split by
    /// [`should_apply`] itself, never a copy of it.
    Judged {
        leaf: Option<&'a str>,
        source: &'a str,
    },
}

impl<'a> Scope<'a> {
    /// The output net of a leaf about the file at `path`, if any.
    pub(crate) fn of_leaf(path: Option<&'a str>) -> Self {
        path.map_or(Scope::Pathless, Scope::Leaf)
    }

    fn source_path(self) -> Option<&'a str> {
        match self {
            Scope::Pathless => None,
            Scope::File(path) | Scope::Leaf(path) => Some(path),
            Scope::Stored(path) | Scope::Judged { leaf: path, .. } => path,
        }
    }
}

pub(crate) struct DetectResult {
    pub sanitized: String,
    pub secrets_detected: Vec<String>,
    /// What `sanitized` replaced of the content, by content byte position.
    pub edits: super::Edits,
}

/// [`redact_within`] at [`MAX_CONTENT_SIZE`].
pub(crate) fn redact(content: &str, scope: Scope<'_>) -> Option<DetectResult> {
    redact_within(content, scope, MAX_CONTENT_SIZE)
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
/// A text past `limit` is judged in one pass over its spans.
pub(crate) fn redact_within(content: &str, scope: Scope<'_>, limit: usize) -> Option<DetectResult> {
    if content.len() > limit {
        return long_text::redact_long(content, scope);
    }
    let matched = matching_patterns(content, scope);
    if matched.is_empty() {
        return None;
    }
    let mut sanitized: Option<String> = None;
    let mut secrets_detected = Vec::new();
    let mut edits = super::Edits::default();
    // One parse of the original text for every judge that reads its syntax.
    let syntax = scope
        .source_path()
        .and_then(|path| CodeSyntax::of(path, content));
    for idx in matched.iter() {
        let current = sanitized.as_deref().unwrap_or(content);
        let (result, replaced) = if let Some(form) = document::Form::of(idx) {
            form.redact(current, replacement_for(idx))
        } else {
            let regex = pattern_regex_for(idx, current);
            let judge = Judge::of(
                idx,
                scope,
                current,
                syntax.as_ref().filter(|_| sanitized.is_none()),
            );
            replace_judged(current, replacement_for(idx), |found| {
                judge.secrets(regex, current, found);
            })
        };
        if result != current {
            secrets_detected.push(PATTERNS[idx].name.to_string());
            // Each pass runs over what the earlier ones left.
            edits = edits.then(&super::Edits::of_pass(replaced));
            sanitized = Some(result);
        }
    }
    sanitized.map(|sanitized| DetectResult {
        sanitized,
        secrets_detected,
        edits,
    })
}

#[cfg(test)]
mod tests;
