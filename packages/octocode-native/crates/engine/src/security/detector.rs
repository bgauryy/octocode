use super::patterns::PATTERNS;
use regex::Regex;
use std::sync::{LazyLock, OnceLock};

mod code_syntax;
mod credential_values;
mod key_facts;

use code_syntax::{CodeSyntax, SyntaxGates};
use credential_values::VALUE_TESTS;
use credential_values::{
    ValueTest, before_next_key, expanded_names, is_expanded_name, is_identifier_name,
    is_redaction_marker, is_sentence, is_value_of_its_own, literal_parts,
};
use key_facts::SpanKeys;

mod diff_view;
mod document;
mod every_value;
mod jwk;
mod long_line;
mod long_text;
mod named_values;
mod objects;
mod source_names;
mod split_literals;
pub(super) mod structured;
mod yaml_secret;
pub use every_value::{EveryValueScan, every_value_spans};
pub use jwk::hide_members as hide_jwk_members;
pub use long_line::{LONG_LINE_BYTES, LongLine};
#[cfg(feature = "test-hooks")]
pub use long_text::set_walk_budget;
pub use source_names::{
    Named, Objects, SecretForm, SecretSpan, SecretSpans, SourceNames, SourceWalk,
};

mod prescan;
mod regex_forms;
mod request;
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
pub(crate) use request::{holds_credential_as_request, literal_form};

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
/// test, and in a source-code file the file's syntax: where a configuration
/// rule's match lies, what a comment holds, which backslash escapes and where
/// a literal closes.
pub(super) struct Judge<'a> {
    /// The pattern's index in the registry.
    pattern: usize,
    value_test: ValueTest,
    syntax: Option<SyntaxOf<'a>>,
    /// The pattern's matches in code are statements, no credentials.
    code_is_no_config: bool,
    /// The pattern reads the text comments hold ([`READS_COMMENT_SYNTAX`]).
    reads_comments: bool,
    /// The text judged, and the names it expands as a variable
    /// ([`expanded_names`]), read once when a value first asks.
    text: &'a str,
    expanded: std::cell::OnceCell<std::collections::HashSet<Box<str>>>,
}

/// The syntax of the source a pass judges, read at the positions of the text
/// the earlier passes left (`edits` turn the source into it). Every pass judges
/// the source's own syntax: a placeholder is no part of it, and a rewritten
/// text that no longer parses would read its strings as comments.
#[derive(Clone, Copy)]
pub(super) struct SourceSyntax<'a> {
    pub(super) syntax: &'a CodeSyntax<'a>,
    pub(super) edits: &'a super::Edits,
}

/// The syntax a [`Judge`] reads: a parse of the text it judges, or the
/// source's, which every pass over one text shares, so a text parses once
/// however many patterns read it.
enum SyntaxOf<'a> {
    Own(CodeSyntax<'a>),
    Source(SourceSyntax<'a>),
}

impl SyntaxOf<'_> {
    fn place_of(&self, at: &std::ops::Range<usize>) -> code_syntax::Place {
        match self {
            Self::Own(syntax) => syntax.place_of(at),
            Self::Source(source) => source.syntax.place_of(&source.edits.input_range(at)),
        }
    }

    /// The source's comment lines at the judged text's positions: a key a
    /// replacement took is no key any more.
    fn comment_lines(&self) -> std::borrow::Cow<'_, [code_syntax::CommentLine]> {
        match self {
            Self::Own(syntax) => std::borrow::Cow::Borrowed(syntax.comment_lines()),
            Self::Source(source) if source.edits.is_empty() => {
                std::borrow::Cow::Borrowed(source.syntax.comment_lines())
            }
            Self::Source(source) => std::borrow::Cow::Owned(
                source
                    .syntax
                    .comment_lines()
                    .iter()
                    .filter_map(|line| line.through(source.edits))
                    .collect(),
            ),
        }
    }

    fn escapes_at(&self, at: usize) -> bool {
        match self {
            Self::Own(syntax) => syntax.escapes_at(at),
            Self::Source(source) => {
                let back = source.edits.input_at(at);
                // A placeholder's byte is none of the source's: fail closed.
                source.edits.output_at(back) != Some(at) || source.syntax.escapes_at(back)
            }
        }
    }

    /// Whether the grammar reads an escape sequence starting with the
    /// backslash at `at`: unlike [`SyntaxOf::escapes_at`], no parse, a
    /// placeholder's byte or a backslash the escape of another one holds
    /// (`\\"`) is no escape here. A reading of the escape's character
    /// ([`Judge::value_text`], [`Judge::value_end`]) is taken only where
    /// the source says so.
    fn escape_starts_at(&self, at: usize) -> bool {
        match self {
            Self::Own(syntax) => syntax.escape_starts_at(at),
            Self::Source(source) => {
                let back = source.edits.input_at(at);
                source.edits.output_at(back) == Some(at) && source.syntax.escape_starts_at(back)
            }
        }
    }

    fn literal_end(&self, at: usize, until: usize) -> Option<usize> {
        match self {
            Self::Own(syntax) => syntax.literal_end(at, until),
            Self::Source(source) => {
                let read = source.edits.input_range(&(at..until));
                let close = source.syntax.literal_end(read.start, read.end)?;
                Some(source.edits.output_end(close))
            }
        }
    }

    /// The source's literal lines ([`CodeSyntax::literal_lines`]) at the
    /// judged text's positions: a line whose start a replacement took is
    /// gone, and one a replacement cut ends where the replacement does.
    fn literal_lines(&self) -> std::borrow::Cow<'_, [std::ops::Range<usize>]> {
        match self {
            Self::Own(syntax) => std::borrow::Cow::Borrowed(syntax.literal_lines()),
            Self::Source(source) if source.edits.is_empty() => {
                std::borrow::Cow::Borrowed(source.syntax.literal_lines())
            }
            Self::Source(source) => std::borrow::Cow::Owned(
                source
                    .syntax
                    .literal_lines()
                    .iter()
                    .filter_map(|line| {
                        let start = source.edits.output_at(line.start)?;
                        Some(start..source.edits.output_end(line.end).max(start))
                    })
                    .collect(),
            ),
        }
    }
}

impl<'a> Judge<'a> {
    /// The judge of pattern `idx` over `text`; `source` is the syntax of the
    /// source `text` was made of when the caller holds it. Without it, a
    /// pattern that reads configuration or comments parses `text` itself.
    pub(super) fn of(
        idx: usize,
        scope: Scope<'a>,
        text: &'a str,
        source: Option<SourceSyntax<'a>>,
    ) -> Self {
        let syntax = scope.source_path().and_then(|path| match source {
            Some(source) => Some(SyntaxOf::Source(source)),
            None if READS_COMMENT_SYNTAX[idx] => CodeSyntax::of(path, text).map(SyntaxOf::Own),
            None => None,
        });
        Self {
            pattern: idx,
            value_test: VALUE_TESTS[idx],
            syntax,
            code_is_no_config: READS_CONFIG_SYNTAX[idx],
            reads_comments: READS_COMMENT_SYNTAX[idx],
            text,
            expanded: std::cell::OnceCell::new(),
        }
    }

    /// Whether the match `at`, whose value is `secret` after `key`, is a
    /// credential. In a source-code file a configuration rule's match is
    /// judged by where the grammar places it: in code it is no config entry,
    /// in a comment it is prose first ([`ValueTest::redacts_in_comment`]), in
    /// other text a config line that may be code ([`ValueTest::redacts_in_code_text`]).
    ///
    /// `secret` lies at `secret_at` in `text`, the text judged (the judge's
    /// own: [`Judge::of`]). The value is
    /// judged as the program holds it ([`Judge::value_text`]), and a
    /// constant's name the same text expands as a variable is no credential
    /// ([`is_expanded_name`]).
    pub(super) fn redacts(
        &self,
        text: &str,
        key: &str,
        secret_at: &std::ops::Range<usize>,
        at: &std::ops::Range<usize>,
    ) -> bool {
        let own = self.own_value(text, secret_at);
        let secret_at = &own;
        let value = self.value_text(text, secret_at);
        let secret = value.as_ref();
        let key = without_inner_comments(key, secret_at.start.saturating_sub(at.start));
        let key = key.as_ref();
        let verdict = match self.syntax.as_ref().filter(|_| self.code_is_no_config) {
            None => self.value_test.redacts(key, secret),
            Some(syntax) => match syntax.place_of(at) {
                code_syntax::Place::Code => false,
                code_syntax::Place::Comment => self.value_test.redacts_in_comment(key, secret),
                code_syntax::Place::Text => self.value_test.redacts_in_code_text(key, secret),
            },
        };
        debug_assert!(std::ptr::eq(text, self.text), "a judge judges its own text");
        verdict
            && !is_expanded_name(
                || self.expanded.get_or_init(|| expanded_names(self.text)),
                secret,
            )
    }

    /// The part of the literal at `secret` in `text` that is the value: all of
    /// it, or for pieces a quoted rule read side by side, what precedes a
    /// last piece that is the next entry's key ([`before_next_key`]).
    fn own_value(&self, text: &str, secret: &std::ops::Range<usize>) -> std::ops::Range<usize> {
        let end = (self.value_test == ValueTest::Quoted)
            .then(|| before_next_key(text, secret))
            .flatten()
            .unwrap_or(secret.end);
        secret.start..end
    }

    /// The value at `secret` in `text` as the program holds it: each quote
    /// the grammar reads as escaped (`\"` in `"…Password={value}\";"`) is the
    /// quote it writes. Only an escape the source's syntax holds is read so
    /// ([`SyntaxOf::escape_starts_at`]); without one the text is judged as
    /// written. Other escapes stay as written: the value tests read them
    /// (`\n` framing a name, an escape counting as one character).
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "`secret` is a regex match; the offsets are ASCII backslashes and quotes in it"
        )
    )]
    fn value_text<'t>(
        &self,
        text: &'t str,
        secret: &std::ops::Range<usize>,
    ) -> std::borrow::Cow<'t, str> {
        let raw = &text[secret.clone()];
        let Some(syntax) = self.syntax.as_ref() else {
            return std::borrow::Cow::Borrowed(raw);
        };
        let escaped_quote = |at: usize| {
            raw.as_bytes()
                .get(at + 1)
                .is_some_and(|byte| matches!(byte, b'"' | b'\'' | b'`'))
                && syntax.escape_starts_at(secret.start + at)
        };
        let mut decoded = String::new();
        let mut from = 0;
        for (at, _) in raw.match_indices('\\') {
            if at >= from && escaped_quote(at) {
                decoded.push_str(&raw[from..at]);
                from = at + 1;
            }
        }
        if from == 0 {
            return std::borrow::Cow::Borrowed(raw);
        }
        decoded.push_str(&raw[from..]);
        std::borrow::Cow::Owned(decoded)
    }

    /// Where the value of a secret found at `secret` in `text` ends, `text`
    /// lying at `base` in the judged text: its continued lines and an escape
    /// it ends inside ([`continued_end`]), and no further than the close of
    /// the string literal it starts in. A config line in a literal ends with
    /// the literal (`r"…Password=x\"`, `` `…Pwd=x` ``), so a rule reading to the
    /// line's end never takes the closing quote or the code after it. A
    /// control-character escape inside it (`"<value>\r\nHost: …"`, `\t`,
    /// `\u000a`) ends it when what precedes the escape is still a credential
    /// (`credential`, given that part's range): the text after is the next
    /// field's ([`Judge::before_inner_escape`]). Otherwise a line-ending escape
    /// it ends with is no part of it ([`Judge::before_escaped_line_end`]).
    pub(super) fn value_end(
        &self,
        text: &str,
        base: usize,
        secret: &std::ops::Range<usize>,
        credential: impl Fn(&std::ops::Range<usize>) -> bool,
    ) -> usize {
        let end = self.read_end(text, base, secret);
        let end = self
            .opaque_word_end(text, secret.start, end, &credential)
            .unwrap_or(end);
        let end = self.before_member_end(text, secret.start, end);
        self.before_inner_escape(text, base, secret.start, end)
            .filter(|&at| credential(&(secret.start..at)))
            .unwrap_or_else(|| self.before_escaped_line_end(text, base, secret.start, end))
    }

    /// Where a plain scalar ([`ValueTest::Scalar`], no quote opening it)
    /// read from `start` to `end` ends when its first word, up to a blank or
    /// a `;`, is one opaque word ([`named_values::is_opaque_word`]) and a
    /// credential alone (`credential`): the value is that token, ending at
    /// its first blank, and what follows (`<token> release1`, `<token>; more`)
    /// is the line's own text. A scalar of words (`correct horse 9`) is one
    /// value.
    fn opaque_word_end(
        &self,
        text: &str,
        start: usize,
        end: usize,
        credential: &impl Fn(&std::ops::Range<usize>) -> bool,
    ) -> Option<usize> {
        let quoted = start
            .checked_sub(1)
            .and_then(|before| text.as_bytes().get(before))
            .is_some_and(|byte| matches!(byte, b'"' | b'\''));
        if self.value_test != ValueTest::Scalar || quoted {
            return None;
        }
        let value = text.get(start..end)?;
        let cut = value.find(|c: char| c.is_whitespace() || c == ';')?;
        let token = start..start + cut;
        (cut > 0
            && value.get(..cut).is_some_and(named_values::is_opaque_word)
            && credential(&token))
        .then_some(token.end)
    }

    /// Where a plain scalar ([`ValueTest::Scalar`], no quote opening it) read
    /// from `start` to `end` ends without the `,` it ends with: the `,` that
    /// ends an object's member (`password: <value>,`) is the member's, no
    /// part of its value.
    fn before_member_end(&self, text: &str, start: usize, end: usize) -> usize {
        let quoted = start
            .checked_sub(1)
            .and_then(|before| text.as_bytes().get(before))
            .is_some_and(|byte| matches!(byte, b'"' | b'\''));
        let trimmed = text
            .get(start..end)
            .map_or(0, |value| value.len() - value.trim_end().len());
        let member = self.value_test == ValueTest::Scalar
            && !quoted
            && end - trimmed > start + 1
            && text.as_bytes().get(end - trimmed - 1) == Some(&b',');
        if member { end - trimmed - 1 } else { end }
    }

    /// Where the value read from `start` to `end` ends at a control-character
    /// escape inside it, as the named-value rule ends one
    /// ([`named_values::field_end`]): where a source's syntax is known, only
    /// at a backslash it reads as an escape (in a raw string the backslash is
    /// the value's own); in any other text at every one.
    fn before_inner_escape(&self, text: &str, base: usize, start: usize, end: usize) -> Option<usize> {
        let at = start + named_values::field_end(text.get(start..end)?)?;
        (at > start
            && self
                .syntax
                .as_ref()
                .is_none_or(|syntax| syntax.escape_starts_at(base + at)))
        .then_some(at)
    }

    /// Where [`Judge::value_end`] reads the value at `secret` to.
    fn read_end(&self, text: &str, base: usize, secret: &std::ops::Range<usize>) -> usize {
        let own = self.own_value(text, secret);
        if own.end < secret.end {
            return own.end;
        }
        let end = continued_end(text, secret.end, |at| {
            self.syntax
                .as_ref()
                .is_none_or(|syntax| syntax.escapes_at(base + at))
        });
        let end = inline_code_end(text, secret.start, end);
        let end = self.field_line_end(text, base, secret.start, end);
        // A literal closes with a quote: a value with none in it ends inside.
        if memchr::memchr3(b'"', b'\'', b'`', &text.as_bytes()[secret.start..end]).is_none() {
            return end;
        }
        self.syntax
            .as_ref()
            .and_then(|syntax| syntax.literal_end(base + secret.start, base + end))
            .and_then(|close| close.checked_sub(base))
            .filter(|&close| secret.start < close && close < end)
            .unwrap_or(end)
    }

    /// Where a value read from `start` to `end` ends without the line-ending
    /// escapes (`\r\n`, `\n`) it ends with (`api_key = "<value>\r\n"`), each
    /// one the source's syntax holds as an escape: the line break it writes
    /// ends the value's line. What the value holds before them stays its
    /// own, and a backslash the syntax does not read as an escape (a raw
    /// string, no parse) stays in the value.
    fn before_escaped_line_end(&self, text: &str, base: usize, start: usize, end: usize) -> usize {
        let Some(syntax) = self.syntax.as_ref() else {
            return end;
        };
        let bytes = text.as_bytes();
        let mut end = end;
        while let Some(at) = end.checked_sub(2).filter(|at| *at > start) {
            let escape = bytes.get(at) == Some(&b'\\')
                && matches!(bytes.get(at + 1), Some(b'n' | b'r'))
                && syntax.escape_starts_at(base + at);
            if !escape {
                break;
            }
            end = at;
        }
        end
    }

    /// Where a field of a value's own grammar ([`ValueTest::Field`]: a
    /// connection string's password field) that starts at `start` and was read
    /// to `end` ends: at the first escaped line break (`\n`, `\r`) the
    /// source's syntax holds inside it, since the program's text has a line
    /// break there and a connection string is one line. A backslash the
    /// syntax does not read as an escape (a raw string, no parse, a
    /// placeholder) ends nothing, so the field keeps its whole read.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "`start` is a regex match bound and `end` a char boundary from `continued_end`"
        )
    )]
    fn field_line_end(&self, text: &str, base: usize, start: usize, end: usize) -> usize {
        let Some(syntax) = self
            .syntax
            .as_ref()
            .filter(|_| self.value_test == ValueTest::Field)
        else {
            return end;
        };
        text[start..end]
            .match_indices('\\')
            .map(|(at, _)| start + at)
            .find(|&at| {
                at > start
                    && matches!(text.as_bytes().get(at + 1), Some(b'n' | b'r'))
                    && syntax.escape_starts_at(base + at)
            })
            .unwrap_or(end)
    }

    /// Calls `found(whole, secret)` for each credential `regex` finds in
    /// `text`: [`accepted_secrets`] over the text, and in a source-code file
    /// over each comment's credential-word tails, which the text's own
    /// line anchors do not reach (`x = 1; // password: …`), and over each
    /// line a string literal holds that starts past its physical line's
    /// start ([`CodeSyntax::literal_lines`]: `let s = "password = …";`),
    /// read from where the literal's line starts, by every rule but a
    /// file-class one ([`ValueTest::FileClass`]). Ranges come in order and
    /// none overlaps another: overlapping ones join.
    pub(super) fn secrets(
        &self,
        regex: &regex::Regex,
        text: &str,
        mut found: impl FnMut(std::ops::Range<usize>, std::ops::Range<usize>),
    ) {
        let found = |whole: std::ops::Range<usize>, secret: std::ops::Range<usize>| {
            // A shell parameter expansion keeps its syntax: its default is
            // the value, and one a pass already replaced is judged.
            let secret = match expansion_default(text, &secret) {
                Some(word)
                    if word.is_empty()
                        || text.get(word.clone()).is_some_and(is_redaction_marker) =>
                {
                    return;
                }
                Some(word) => word,
                None => secret,
            };
            match self.own_parts(text, &secret) {
                Some(parts) => {
                    for part in parts {
                        found(whole.clone(), part);
                    }
                }
                None => found(whole, secret),
            }
        };
        self.judged_secrets(regex, text, found);
    }

    /// The parts of the quoted value at `secret` in `text` that are values
    /// of their own ([`is_value_of_its_own`]), each judged alone as the
    /// rule judges a value: a literal's value ends at its closing quote, so
    /// a piece joined after it (`"<key>" + "-suffix"`) or a hole it
    /// interpolates (`` `<key>${id}` ``) is the program's text. When no part
    /// of it is a value alone, the pieces joined are judged as one value
    /// ([`ValueTest::Quoted`]) and each piece goes on its own, what joins
    /// them staying (`"…" \⏎ "…"`). `None` when the value is one piece. A
    /// part an earlier pass replaced stays as it is.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "`secret` is a found range and its parts lie on ASCII quotes and braces"
        )
    )]
    fn own_parts(
        &self,
        text: &str,
        secret: &std::ops::Range<usize>,
    ) -> Option<Vec<std::ops::Range<usize>>> {
        if self.value_test != ValueTest::Quoted {
            return None;
        }
        let value = text.get(secret.clone())?;
        let line = text[..secret.start]
            .rfind(LINE_END_CHARS)
            .map_or(0, |at| at + 1);
        let key = &text[line..secret.start];
        let parts = literal_parts(key, value);
        if parts.len() == 1 && parts[0] == (0..value.len()) {
            return None;
        }
        let shift =
            |part: &std::ops::Range<usize>| secret.start + part.start..secret.start + part.end;
        // A part an earlier pass replaced is a value it judged alone.
        let alone = |part: &std::ops::Range<usize>| {
            let at = shift(part);
            text.get(at.clone()).is_some_and(|part| {
                is_redaction_marker(part)
                    || (is_value_of_its_own(key, part) && self.redacts(text, key, &at, &at))
            })
        };
        // The pieces joined are the value when none is one alone, or when
        // together they spell a token a shape names (`"ghp_" \⏎ "…"`): each
        // piece goes, and what joins them (quotes, blanks, a line's
        // continuation, a hole an interpolation writes) stays as written.
        let joined: String = parts.iter().filter_map(|part| value.get(part.clone())).collect();
        if !parts.iter().any(alone) || shape_of(&joined).is_some() {
            return Some(
                parts
                    .iter()
                    .map(shift)
                    .filter(|at| {
                        text.get(at.clone())
                            .is_some_and(|part| !part.is_empty() && !is_redaction_marker(part))
                    })
                    .collect(),
            );
        }
        Some(
            parts
                .iter()
                .map(shift)
                .filter(|at| {
                    text.get(at.clone())
                        .is_some_and(|part| !is_redaction_marker(part))
                        && self.redacts(text, key, at, at)
                })
                .collect(),
        )
    }

    /// [`Judge::secrets`] before a quoted value is cut into its own parts.
    fn judged_secrets(
        &self,
        regex: &regex::Regex,
        text: &str,
        mut found: impl FnMut(std::ops::Range<usize>, std::ops::Range<usize>),
    ) {
        // A name's values are walked, not only matched: the walk judges them.
        if self.value_test == ValueTest::Named {
            named_values::secrets(regex, text, found);
            return;
        }
        let Some(syntax) = self.syntax.as_ref().filter(|_| self.reads_comments) else {
            accepted_secrets(
                regex,
                text,
                line_start_of,
                |k, s, at| self.redacts(text, k, s, at),
                |secret, credential| self.value_end(text, 0, secret, credential),
                found,
            );
            return;
        };
        let mut all = Vec::new();
        accepted_secrets(
            regex,
            text,
            line_start_of,
            |k, s, at| self.redacts(text, k, s, at),
            |secret, credential| self.value_end(text, 0, secret, credential),
            |whole, secret| {
                all.push((whole, secret));
            },
        );
        all.extend(self.tail_secrets(text, &syntax.comment_lines(), &mut 0));
        // A literal's line is a literal of code first: a file-class rule
        // reads its value as a dotenv or properties file holds it, which a
        // line of source code is not, and the value a rule finds there is a
        // credential only when it is one as a code literal too
        // ([`ValueTest::Quoted`]: long enough, or a chosen secret). A
        // message is prose there as in a comment: after `name:` a sentence
        // is no value ([`is_sentence`]).
        let literal_lines = if self.value_test == ValueTest::FileClass {
            std::borrow::Cow::Borrowed(&[][..])
        } else {
            syntax.literal_lines()
        };
        for line in literal_lines.iter() {
            // A line with an escape is written otherwise than the program
            // holds it (`\"` for `"`): a rule would read its backslashes.
            let Some(window) = text.get(line.clone()).filter(|line| !line.contains('\\')) else {
                continue;
            };
            let base = line.start;
            let shift = |at: &std::ops::Range<usize>| base + at.start..base + at.end;
            accepted_secrets(
                regex,
                window,
                line_start_of,
                |k, s, at| {
                    window.get(s.clone()).is_some_and(|value| {
                        ValueTest::Quoted.redacts(k, value)
                            && !is_sentence(&SpanKeys::new(k).key(0), value.trim())
                            && self.redacts(text, k, &shift(s), &shift(at))
                    })
                },
                |secret, credential| self.value_end(window, base, secret, credential),
                |whole, secret| all.push((shift(&whole), shift(&secret))),
            );
        }
        for (whole, secret) in joined(all) {
            found(whole, secret);
        }
    }
}

/// The default of the shell parameter expansion the value at `secret` in
/// `text` is, its quotes aside (`"${API_KEY:-<default>}"`, `${KEY=…}`), by
/// byte position: the name it expands is a reference, the operator syntax,
/// and only the default a value. `None` for a value that is no such
/// expansion.
fn expansion_default(
    text: &str,
    secret: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let value = text.get(secret.clone())?;
    let inner = credential_values::QUOTES
        .into_iter()
        .find_map(|quote| value.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(value);
    let body = inner.strip_prefix("${")?.strip_suffix('}')?;
    let name = body
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|&end| end > 0)?;
    let rest = body.get(name..)?;
    let operator = rest.strip_prefix(':').unwrap_or(rest);
    let word = operator.strip_prefix(['-', '=', '?', '+'])?;
    if word.contains(['{', '}']) {
        return None;
    }
    let start = secret.start + (value.len() - inner.len()) / 2 + 2 + (body.len() - word.len());
    Some(start..start + word.len())
}

/// Where a value that starts at `start` and was read to `end` ends when
/// prose quotes it as code: a backtick that opens a span before it on its
/// line (`` the string (`Server=db;Password=x`) sits under … ``) closes at
/// the next one, and the words after it are the sentence's, no part of the
/// value. The backtick nearest before the value opens a span when no word
/// character precedes it; one that closes a span follows the span's text.
/// A value no backtick opens keeps its whole read.
fn inline_code_end(text: &str, start: usize, end: usize) -> usize {
    let Some(close) = text
        .get(start..end)
        .and_then(|value| value.find('`'))
        .filter(|close| *close > 0)
    else {
        return end;
    };
    let before = text.get(..start).unwrap_or_default();
    let opens = before
        .rfind(['`', '\n', '\r'])
        .filter(|at| before.as_bytes().get(*at) == Some(&b'`'))
        .is_some_and(|at| {
            !before
                .get(..at)
                .is_some_and(|head| head.ends_with(|c: char| c.is_alphanumeric() || c == '_'))
        });
    if opens { start + close } else { end }
}

/// `key`, the text before a value whose last `matched` bytes are the
/// pattern's own match, without the comments that match reads through
/// (`dbPassword /* c1 */ : /* c2 */ "`, a `// …` or `# …` line between the
/// operator and a value on the next line): a comment between a name, its
/// operator and its value is no part of the key, so the value tests read the
/// name and the operator as if it were not there. A block comment leaves a
/// blank, a line comment its line break, and so does a backslash that
/// continues the line; an opener that closes nowhere
/// in the match (`r#"`) is text. The text before the match is the line's
/// own and stays as written.
fn without_inner_comments(key: &str, matched: usize) -> std::borrow::Cow<'_, str> {
    let (before, own) = key
        .split_at_checked(key.len().saturating_sub(matched))
        .unwrap_or((key, ""));
    if !own.contains(['/', '#', '\\']) {
        return std::borrow::Cow::Borrowed(key);
    }
    let mut read = String::with_capacity(key.len());
    read.push_str(before);
    let mut rest = own;
    while let Some(at) = rest.find(['/', '#', '\\']) {
        let (text, from) = rest.split_at(at);
        let continued = from
            .strip_prefix('\\')
            .is_some_and(|after| after.starts_with(LINE_END_CHARS));
        let comment_len = if continued {
            Some(1)
        } else if from.starts_with("/*") {
            from.get(2..)
                .and_then(|body| body.find("*/"))
                .map(|close| close + 4)
        } else if from.starts_with("//") || from.starts_with('#') {
            from.find(LINE_END_CHARS)
        } else {
            None
        };
        read.push_str(text);
        match comment_len.and_then(|len| from.get(len..)) {
            Some(after) => {
                read.push(' ');
                rest = after;
            }
            None => {
                let mark = from.chars().next().map_or(0, char::len_utf8);
                let (mark, after) = from.split_at(mark);
                read.push_str(mark);
                rest = after;
            }
        }
    }
    read.push_str(rest);
    std::borrow::Cow::Owned(read)
}

/// `found` (`(whole, secret)` ranges) in order, each secret joined with every
/// later one it overlaps: a range that starts inside another and ends past it
/// extends it, so no part of either stays readable.
fn joined(
    mut found: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>,
) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    found.sort_by_key(|(_, secret)| (secret.start, std::cmp::Reverse(secret.end)));
    let mut out: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> = Vec::new();
    for (whole, secret) in found {
        match out.last_mut() {
            Some((last_whole, last)) if secret.start < last.end => {
                last.end = last.end.max(secret.end);
                last_whole.end = last_whole.end.max(whole.end);
            }
            _ => out.push((whole, secret)),
        }
    }
    out
}

fn replacement_for(idx: usize) -> &'static str {
    &REPLACEMENTS[idx]
}

/// The placeholder pattern `idx` writes for `value`, a secret it found: one
/// kind per value, the most specific detector's, in every view. A value
/// that a pattern knowing its shape alone ([`ValueTest::Every`]: a GitHub
/// token, a cloud key) finds whole goes under that pattern's kind, as the
/// same value read alone (a hover, a search row) does; a value no shape
/// names goes under `idx`'s own (a name's, a place's).
fn kind_for(idx: usize, value: &str) -> &'static str {
    if VALUE_TESTS[idx] == ValueTest::Every || value.contains('\n') {
        return replacement_for(idx);
    }
    shape_of(value).map_or_else(|| replacement_for(idx), replacement_for)
}

/// The pattern that knows `value` by its shape alone and finds all of it,
/// read as a text of its own; `None` when none does. The read of `value`
/// names its own finds by their patterns: a value is asked once.
fn shape_of(value: &str) -> Option<usize> {
    thread_local! {
        static ASKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if ASKING.with(|asking| asking.replace(true)) {
        return None;
    }
    let result = redact(value, Scope::Pathless);
    ASKING.with(|asking| asking.set(false));
    let result = result?;
    let mut runs = result.edits.redacted();
    if runs.next()? != (0..value.len()) || runs.next().is_some() {
        return None;
    }
    let name = result.secrets_detected.first()?;
    let idx = PATTERNS.iter().position(|pattern| pattern.name == name)?;
    (VALUE_TESTS[idx] == ValueTest::Every
        && PATTERNS[idx].file_context.is_none()
        && document::Form::of(idx).is_none())
    .then_some(idx)
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
/// anchors do not reach a comment after code, or a config line a literal
/// opens mid-line ([`Judge::secrets`]).
#[cfg(test)]
fn matching_patterns(content: &str, scope: Scope<'_>) -> PatternSet {
    matching_patterns_gated(content, scope).0
}

/// [`matching_patterns`], and what the lexical gates it read said of
/// `content`, for the parse of the same text ([`CodeSyntax::gated`]). The
/// patterns that apply are picked first: with none (a leaf whose source's
/// whole pass applied every one, [`Scope::Judged`]) nothing is left to
/// match, and with none that reads comments no gate decides anything, so
/// the text is not lexed for an answer no pattern asks for.
fn matching_patterns_gated(content: &str, scope: Scope<'_>) -> (PatternSet, SyntaxGates) {
    let gates = SyntaxGates::default();
    let applicable = applicable_patterns(content, scope);
    if applicable.is_empty() {
        return (applicable, gates);
    }
    let reads_comments = applicable.iter().any(|idx| READS_COMMENT_SYNTAX[idx])
        && scope
            .source_path()
            .and_then(code_syntax::grammar_of)
            .is_some_and(|grammar| gates.comment(content, grammar) || gates.literal(content));
    let matched = applicable.filter(|idx| {
        (reads_comments && READS_COMMENT_SYNTAX[idx])
            || pattern_regex_for(idx, content).is_match(content)
    });
    (matched, gates)
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

/// The line breaks a redacted `secret` spanned, each as it was and with the
/// blanks that indent the line it starts, so its replacement keeps the
/// source's lines, line ends and indentation: an indented block (a key body
/// in a YAML literal or a string) hides its text, not its layout.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are the ends of `line_breaks` slices and ASCII blank runs"
    )
)]
fn kept_line_breaks(secret: &str) -> String {
    // Most secrets sit on one line and keep none.
    if memchr::memchr2(b'\n', b'\r', secret.as_bytes()).is_none() {
        return String::new();
    }
    let base = secret.as_ptr() as usize;
    let mut kept = String::new();
    for line_break in line_breaks(secret) {
        let after = line_break.as_ptr() as usize - base + line_break.len();
        let rest = &secret[after..];
        let blanks = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        kept.push_str(line_break);
        kept.push_str(&rest[..blanks]);
    }
    kept
}

/// The end of the value a secret ending at `end` continues into. A secret
/// that ends in an odd number of backslashes ends inside an escape. Before a
/// line break the line goes on at the next one (Java properties, a shell
/// reading a dotenv file): each such line and the one that ends the value are
/// the value's own, in whatever rule found its first line. Before any other
/// character the escape is that character's (`Password=Pw0rd9q\";` in a code
/// literal, where the rule stopped at the quote), so the secret takes it: a
/// replacement that left the backslash would escape what follows it and end
/// the literal there. Where the backslash escapes nothing (`escapes` says, for
/// the backslash's position: a raw string, a comment) the secret ends at it.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are a regex match end, `find` results and ASCII break lengths"
    )
)]
fn continued_end(text: &str, mut end: usize, escapes: impl Fn(usize) -> bool) -> usize {
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
            if !escapes(end - 1) {
                return end;
            }
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
/// from the match's start and its line's start, to the secret, the secret's
/// range and the match's). A match that starts inside the lines an earlier
/// secret continued into and ends past them joins that secret.
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
    redacts: impl Fn(&str, &std::ops::Range<usize>, &std::ops::Range<usize>) -> bool,
    value_end: impl Fn(&std::ops::Range<usize>, &dyn Fn(&std::ops::Range<usize>) -> bool) -> usize,
    mut found: impl FnMut(std::ops::Range<usize>, std::ops::Range<usize>),
) {
    let groups = secret_groups(regex);
    // Matches come in order, so only the text since the last one can hold a
    // later line start: searching back to the start of the text from each
    // match costs quadratic time on one long line.
    let (mut line_start, mut searched_to) = (0, 0);
    // The last secret found, held until no later one can join it: a match
    // never starts inside an earlier match, but it can start inside the lines
    // an earlier secret continued into, and then it extends that secret.
    let mut pending: Option<(std::ops::Range<usize>, std::ops::Range<usize>)> = None;
    let diff_lines = DiffLines::new(regex, text);
    // Where the next search starts: past the match, or where an accepted
    // value ends when it ends before the secret the pattern read (an escaped
    // line break, a literal's close: [`Judge::value_end`]). The text past
    // that end is no part of the value, so it is searched again: it may hold
    // a secret of its own (`…Password=x\nCONN4=Pwd=ab12;…`).
    let mut from = 0;
    while from <= text.len() {
        let Some(captures) = regex.captures_at(text, from) else {
            break;
        };
        let Some(raw) = RawMatch::of(&captures, &groups) else {
            break;
        };
        from = if raw.whole.is_empty() {
            raw.whole.end
                + text[raw.whole.end..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8)
        } else {
            raw.whole.end
        };
        if let Some(at) =
            memchr::memrchr2(b'\n', b'\r', &text.as_bytes()[searched_to..raw.whole.start])
        {
            line_start = searched_to + at + 1;
        }
        searched_to = raw.whole.start;
        let secret = &raw.secret;
        let covered_to = pending.as_ref().map_or(0, |(_, last)| last.end);
        // A match within what is hidden already hides nothing more.
        if secret.end <= covered_to {
            continue;
        }
        let key = &text[key_start(raw.whole.start, line_start).min(secret.start)..secret.start];
        if raw.is_kept(
            &diff_lines,
            diff_lines.prefixed(&captures),
            text,
            line_start,
        ) && redacts(key, secret, &raw.whole)
        {
            let end = value_end(secret, &|part| redacts(key, part, &raw.whole));
            if secret.start < end && end < secret.end {
                from = end;
            }
            let whole = raw.whole.start..raw.whole.end.max(end);
            match pending.as_mut() {
                Some((last_whole, last)) if secret.start < last.end => {
                    last.end = last.end.max(end);
                    last_whole.end = last_whole.end.max(whole.end);
                }
                _ => {
                    if let Some((whole, secret)) = pending.replace((whole, secret.start..end)) {
                        found(whole, secret);
                    }
                }
            }
        }
    }
    if let Some((whole, secret)) = pending {
        found(whole, secret);
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
    /// placeholder ([`is_redaction_marker`]: the tool's own text, its quotes,
    /// a line's escape and the next field aside), and no diff line before any
    /// hunk. `line_start` is where
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
        // A secret of several lines (a key block's body) is hidden already
        // when each of its lines that holds text is a placeholder; an empty
        // one (a block with no body) hides nothing.
        let secret = &text[self.secret.clone()];
        let hidden = if secret.contains(['\n', '\r']) {
            let mut lines = secret.lines().map(str::trim).filter(|line| !line.is_empty());
            lines.clone().next().is_some() && lines.all(is_redaction_marker)
        } else {
            is_redaction_marker(secret)
        };
        !secret.is_empty() && !hidden && !diff_lines.outside_hunk(prefixed, line_start)
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
fn replace_judged<'r>(
    content: &str,
    replacement: impl Fn(&str) -> &'r str,
    scan: impl FnOnce(&mut dyn FnMut(std::ops::Range<usize>, std::ops::Range<usize>)),
) -> (String, Vec<(std::ops::Range<usize>, usize)>) {
    let mut text = String::with_capacity(content.len());
    let mut replaced = Vec::new();
    let mut from = 0;
    scan(&mut |_, secret| {
        let line_breaks = kept_line_breaks(&content[secret.clone()]);
        let replacement = replacement(&content[secret.clone()]);
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
    replace_judged(content, |_| replacement, |found| {
        accepted_secrets(
            regex,
            content,
            line_start_of,
            |key, secret, whole| redacts(key, &content[secret.clone()], whole),
            |secret, _| continued_end(content, secret.end, |_| true),
            found,
        );
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
/// fine-grained GitHub tokens) so they fire only where they apply. A last
/// pass reads the literals written in pieces ([`split_literals`]).
/// A text past `limit` is judged in one pass over its spans.
///
/// A unified diff's hunks are judged as the two texts they change, old and
/// new, their marker column aside ([`diff_view`]), and each value is replaced
/// in its own line.
pub(crate) fn redact_within(content: &str, scope: Scope<'_>, limit: usize) -> Option<DetectResult> {
    match diff_view::DiffView::of(content, diff_view::Side::New) {
        Some(view) => {
            diff_view::redact_sides(content, &view, |text| redact_text(text, scope, limit))
        }
        None => redact_text(content, scope, limit),
    }
}

/// [`redact_within`] of a text that is no diff, or a diff's view.
fn redact_text(content: &str, scope: Scope<'_>, limit: usize) -> Option<DetectResult> {
    if content.len() > limit {
        return long_text::redact_long(content, scope);
    }
    let (matched, gates) = matching_patterns_gated(content, scope);
    let mut sanitized: Option<String> = None;
    let mut secrets_detected = Vec::new();
    let mut edits = super::Edits::default();
    // One parse of the source for every pass: each judges the source's own
    // syntax, at the positions of the text the earlier passes left.
    let syntax = scope
        .source_path()
        .filter(|_| !matched.is_empty())
        .and_then(|path| CodeSyntax::gated(path, content, gates));
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
                syntax.as_ref().map(|syntax| SourceSyntax {
                    syntax,
                    edits: &edits,
                }),
            );
            replace_judged(current, |value| kind_for(idx, value), |found| {
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
    // A leaf cut from a source judged whole had its pieces judged there,
    // with the pieces on the lines around it.
    let pieces = if matches!(scope, Scope::Judged { .. }) {
        None
    } else {
        split_literals::redact_pieces(sanitized.as_deref().unwrap_or(content))
    };
    if let Some(pieces) = pieces {
        for name in pieces.patterns {
            if !secrets_detected.iter().any(|detected| detected == name) {
                secrets_detected.push(name.to_string());
            }
        }
        edits = edits.then(&super::Edits::of_pass(pieces.replaced));
        sanitized = Some(pieces.text);
    }
    sanitized.map(|sanitized| DetectResult {
        sanitized,
        secrets_detected,
        edits,
    })
}

#[cfg(test)]
mod tests;
