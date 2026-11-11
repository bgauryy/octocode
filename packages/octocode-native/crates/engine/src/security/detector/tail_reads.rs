//! The credentials a pattern finds in the comment lines of a source file
//! (B165): `x = 1; // password: …`, which the text's own line anchors do not
//! reach.
//!
//! The pattern reads a key's value to the end of its line, so a read of every
//! key to the line's end costs the rest of the line from each key. A key is
//! read over its own segment (to the next key) unless the line's text after it
//! can change the match; only then is it read to the line's end, and only if
//! the pattern can match from it at all, which the DFA's walk tells ([`Reach`])
//! at the cost of the line once per state. That read is a walk of the pattern's
//! NFA that stops where an earlier key's walk stood in the same threads
//! ([`TailCaptures`]), so the line's ends are read once per list of threads,
//! not once per key. A key inside a secret found before is not read.

use super::code_syntax::{CommentLine, Key};
use super::key_facts::{KeyBits, SpanKeys};
use super::regex_forms::{anchored_dfa, pattern_nfa};
use super::tail_captures::{TailCaptures, group};
use super::tail_reach::Reach;
use super::*;

type Span = std::ops::Range<usize>;

/// The `(whole, secret)` ranges of a read.
type Found = Vec<(Span, Span)>;

/// One text's reads of one pattern's comment lines.
struct Reads<'a> {
    judge: &'a Judge<'a>,
    text: &'a str,
    regex: &'static Regex,
    groups: Vec<usize>,
    /// Whether the pattern reads a diff line's `+` or `-` prefix.
    has_diff: bool,
    reach: Reach,
    /// The memoized walks of the line's reads to its end.
    captures: TailCaptures,
    /// The keys of the span being read, from its first key's start to its
    /// value: what [`ValueTest::admits_key_in_comment`] reads of each.
    span_keys: Option<(Key, SpanKeys<'a>)>,
    /// The reads to the line's end of the span being read.
    shared: Vec<SpanRead<'a>>,
    /// The bytes handed to the pattern.
    scanned: usize,
}

/// A read of a span's value to the line's end, which every key of the span
/// that the pattern matches from shares: they differ in where they start, not
/// in what the value is, so the match is made once and each key's verdict is
/// read off its facts ([`SpanKeys`]).
struct SpanRead<'a> {
    /// Where the span's value starts: what tells its keys apart from another
    /// span's.
    value: usize,
    /// The keys that start with a diff prefix's `+` or `-` read the match
    /// with that prefix, so they share a read of their own.
    diff_start: bool,
    /// Where the first key that needed the read starts.
    first: usize,
    secret: Span,
    whole_end: usize,
    /// Where the secret ends once a continued value is read whole.
    end: usize,
    /// The match is no placeholder and no diff line outside a hunk.
    kept: bool,
    keys: SpanKeys<'a>,
    /// The verdict of each distinct set of facts a key had.
    judged: Vec<(KeyBits, bool)>,
}

impl SpanRead<'_> {
    /// The secret of `key`, if its value is one: its verdict is that of the
    /// keys with the same facts.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "a secret's bounds are regex match bounds, char boundaries of the text"
        )
    )]
    fn found(&mut self, text: &str, key: Key, test: ValueTest) -> Found {
        if !self.kept {
            return Vec::new();
        }
        let secret = &text[self.secret.clone()];
        // The facts of a key are shared when its text is the first key's with
        // its head cut off, and the operator is in what the cut leaves.
        let accepted = if self.secret.start > key.operator && key.start >= self.first {
            let bits = self.keys.key(key.start - self.first).bits();
            if let Some(&(_, verdict)) = self.judged.iter().find(|(seen, _)| *seen == bits) {
                verdict
            } else {
                let verdict = test.redacts_in_comment_by(&bits, secret);
                self.judged.push((bits, verdict));
                verdict
            }
        } else {
            let from = key.start.min(self.secret.start);
            test.redacts_in_comment(&text[from..self.secret.start], secret)
        };
        if !accepted {
            return Vec::new();
        }
        vec![(
            key.start..self.whole_end.max(self.end),
            self.secret.start..self.end,
        )]
    }
}

impl<'a> Reads<'a> {
    /// Whether the value test can accept a value under `key`, read off its
    /// span's facts: the keys of a span lie in text order, its first key
    /// first, so the facts of each span are read once.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "a key's start and its value's start are char boundaries of the text"
        )
    )]
    fn admits(&mut self, key: Key) -> bool {
        let text = self.text;
        let (first, keys) = match &self.span_keys {
            Some((first, keys)) if first.value == key.value => (*first, keys),
            _ => {
                let keys = SpanKeys::new(&text[key.start..key.value]);
                let (first, keys) = self.span_keys.insert((key, keys));
                (*first, &*keys)
            }
        };
        self.judge
            .value_test
            .admits_key_in_comment(&keys.key(key.start - first.start))
    }

    /// The credentials of the line `text[from..to]` read as a line of its own.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "window bounds are char boundaries of the text: key starts and a comment line's end"
        )
    )]
    fn read(&mut self, from: usize, to: usize) -> Found {
        self.scanned += to - from;
        let window = &self.text[from..to];
        let Some((raw, true)) = anchored_match(self.regex, &self.groups, window) else {
            return Vec::new();
        };
        let key = &window[..raw.secret.start];
        if !self
            .judge
            .value_test
            .redacts_in_comment(key, &window[raw.secret.clone()])
        {
            return Vec::new();
        }
        let end = self.judge.value_end(window, from, &raw.secret, |part| {
            window
                .get(part.clone())
                .is_some_and(|value| self.judge.value_test.redacts_in_comment(key, value))
        });
        let shift = |at: usize| at + from;
        vec![(
            shift(raw.whole.start)..shift(raw.whole.end.max(end)),
            shift(raw.secret.start)..shift(end),
        )]
    }

    /// What the pattern finds from `line.keys[at]` over the rest of the line:
    /// the span's one read, judged for this key.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "key starts and a match's bounds are char boundaries of the text"
        )
    )]
    fn read_shared(&mut self, line: &CommentLine, at: usize) -> Found {
        let key = line.keys[at];
        let text = self.text;
        let diff_start = self.has_diff && matches!(text.as_bytes()[key.start], b'+' | b'-');
        let slot = match self
            .shared
            .iter()
            .position(|read| read.value == key.value && read.diff_start == diff_start)
        {
            Some(slot) => slot,
            None => {
                // The next span: the reads of the one before are no more shared.
                if self.shared.iter().any(|read| read.value != key.value) {
                    self.shared.clear();
                }
                let window = &text[key.start..line.range.end];
                let Some((raw, kept)) = self.line_end_match(key.start, line.range.end) else {
                    return Vec::new();
                };
                let secret = key.start + raw.secret.start..key.start + raw.secret.end;
                self.shared.push(SpanRead {
                    value: key.value,
                    diff_start,
                    first: key.start,
                    whole_end: key.start + raw.whole.end,
                    // One read shared by the span's keys: no one key judges
                    // a part of it.
                    end: key.start + self.judge.value_end(window, key.start, &raw.secret, |_| false),
                    kept,
                    keys: SpanKeys::new(&text[key.start..secret.start]),
                    secret,
                    judged: Vec::new(),
                });
                self.shared.len() - 1
            }
        };
        self.shared[slot].found(text, key, self.judge.value_test)
    }

    /// The match of the pattern anchored at `from` in `text[from..to]` read as
    /// a line of its own, its ranges relative to `from`, and whether it is a
    /// candidate ([`RawMatch::is_kept`]): what [`anchored_match`] answers for
    /// that window, from the line's shared walks ([`TailCaptures`]).
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "`from` is a key start and `to` a comment line's end, char boundaries of the text"
        )
    )]
    fn line_end_match(&mut self, from: usize, to: usize) -> Option<(RawMatch, bool)> {
        let slots = self.captures.find(self.text.as_bytes(), from, to)?;
        let relative = |span: Span| span.start - from..span.end - from;
        let whole = relative(group(&slots, 0)?);
        let secret = self
            .groups
            .iter()
            .find_map(|&index| group(&slots, index))
            .map_or_else(|| whole.clone(), relative);
        let window = &self.text[from..to];
        let diff_lines = DiffLines::new(self.regex, window);
        let prefixed = diff_lines
            .group()
            .is_some_and(|index| group(&slots, index).is_some());
        let raw = RawMatch { whole, secret };
        let kept = raw.is_kept(&diff_lines, prefixed, window, 0);
        Some((raw, kept))
    }

    /// Where the value of the pattern's match of `text[from..to]` ends, whatever
    /// the value test says of it; `None` if the pattern does not match there.
    fn value_end(&mut self, from: usize, to: usize) -> Option<usize> {
        self.scanned += to - from;
        let captures = self.regex.captures(self.text.get(from..to)?)?;
        let value = self
            .groups
            .iter()
            .find_map(|&index| captures.get(index))
            .or_else(|| captures.get(0))?;
        Some(from + value.end())
    }

    /// Whether the value of the window `text[key..end]`, which read `found`,
    /// is followed in it by a comment, and no quote opens in the window that
    /// text after it may close.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "a value's end is a regex match bound, a char boundary of the text"
        )
    )]
    fn ends_in_comment(&mut self, key: usize, end: usize, found: &Found) -> bool {
        let value_end = match found.first() {
            Some((_, secret)) => Some(secret.end),
            None => self.value_end(key, end),
        };
        value_end.is_some_and(|value_end| {
            self.text[value_end..end].contains('#')
                && !self.text[key..end].contains(['"', '\'', '`'])
        })
    }

    /// What the pattern finds from `line.keys[at]`: what a read of the line
    /// from the key to the line's end finds.
    ///
    /// The key's own segment answers for it when its value is followed in the
    /// segment by a comment, which takes the rest of the line whatever it holds
    /// (`token=S1 # password=S2`), or when the pattern cannot match the whole
    /// line from the key: the segment is then read as a line of its own
    /// (`token: S1; secret: S2`). Otherwise text after the segment may change
    /// the match (a value that runs on, a quote the line closes later), and the
    /// line is read to its end.
    fn key(&mut self, line: &CommentLine, at: usize) -> Found {
        if !self.admits(line.keys[at]) {
            return Vec::new();
        }
        let key = line.keys[at].start;
        let end = line
            .keys
            .get(at + 1)
            .map_or(line.range.end, |next| next.start);
        let found = self.read(key, end);
        if end >= line.range.end
            || self.ends_in_comment(key, end, &found)
            || !self
                .reach
                .matches(self.text.as_bytes(), key, line.range.end)
        {
            return found;
        }
        self.read_shared(line, at)
    }
}

impl Judge<'_> {
    /// The credentials of the pattern in comment `lines`
    /// ([`super::code_syntax::CodeSyntax::comment_lines`], at `text`'s positions), as
    /// `(whole, secret)` ranges of `text`,
    /// with the bytes handed to the pattern and stepped over by its DFA added to
    /// `scanned`.
    ///
    /// A key inside a secret an earlier key found is not read.
    pub(super) fn tail_secrets(
        &self,
        text: &str,
        lines: &[CommentLine],
        scanned: &mut usize,
    ) -> Found {
        let regex = anchored_pattern_regex(self.pattern, text.is_ascii());
        let mut reads = Reads {
            judge: self,
            text,
            regex,
            groups: secret_groups(regex),
            has_diff: regex
                .capture_names()
                .any(|name| name == Some(DIFF_PREFIX_GROUP)),
            shared: Vec::new(),
            span_keys: None,
            reach: Reach::new(anchored_dfa(self.pattern)),
            captures: TailCaptures::new(pattern_nfa(self.pattern)),
            scanned: 0,
        };
        let mut all = Vec::new();
        for line in lines {
            reads.reach.begin_line();
            reads.captures.begin_line();
            let mut covered = 0;
            for (at, &Key { value, .. }) in line.keys.iter().enumerate() {
                // A key whose value starts well inside a secret found before
                // finds one that starts inside it too.
                if value + 1 < covered {
                    continue;
                }
                let found = reads.key(line, at);
                covered = found
                    .iter()
                    .fold(covered, |covered, (_, secret)| covered.max(secret.end));
                all.extend(found);
            }
        }
        *scanned += reads.scanned + reads.reach.steps() + reads.captures.steps();
        all
    }
}
