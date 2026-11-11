//! The values a name holds: the one after it, and those of the collection
//! or block it opens.

use std::ops::Range;

use super::names::{names_a_credential, names_no_value};
use super::opaque::{is_opaque, is_run};
use super::text::{field_end, is_public_armor, line_end, string_end};
use super::{Named, Opener};
use crate::security::detector::credential_values::{OPAQUE_LITERAL_CHARS, QUOTES, ValueTest};
use crate::security::detector::{LINE_END_CHARS, is_placeholder, jwk};

/// Characters of one string of a credential's collection that is a
/// credential when opaque: the length of a chosen secret's shortest opaque
/// word. A value beside a name's looser layouts needs
/// [`OPAQUE_LITERAL_CHARS`].
const ELEMENT_CHARS: usize = 8;

/// The member that holds a credential's record's value (`"secret":
/// {"value": "…"}`).
const VALUE_MEMBER: &str = "value";

/// The text a scan reads, and what its walks have cost.
pub(super) struct Walk<'t> {
    text: &'t str,
    /// Bytes walked for collections that held no value: past the text's
    /// length times [`REWALKS`], a walk takes its text whatever it holds, so
    /// collections nested in one another cost no more than that.
    spent: usize,
    line_start: usize,
    searched_to: usize,
}

/// The characters that end a bare value: what closes or separates the
/// layouts a value sits in (a quote, a list's `,`, a statement's `;`, a
/// query's `&`, a pipe, a closing bracket).
const BARE_ENDS: [char; 12] = ['"', '\'', '`', ',', ';', '&', '|', '<', '>', ')', ']', '}'];

/// How many times over a text's bytes may be walked for collections that
/// hold no value before each walk takes what it reads.
const REWALKS: usize = 8;

impl<'t> Walk<'t> {
    /// A scan of `text` from its start.
    pub(super) fn new(text: &'t str) -> Self {
        Self {
            text,
            spent: 0,
            line_start: 0,
            searched_to: 0,
        }
    }

    /// Where the line of `at` starts. Matches come in order, so only the
    /// text since the last one is searched.
    pub(super) fn line_start_of(&mut self, at: usize) -> usize {
        let from = self.searched_to.min(at);
        if let Some(found) = self
            .text
            .get(from..at)
            .and_then(|since| since.rfind(LINE_END_CHARS))
        {
            self.line_start = from + found + 1;
        }
        self.searched_to = at;
        self.line_start
    }

    /// The value the run at `run` opens under the name at `name`, if it is a
    /// credential: all of it, so no tail of it stays readable. A value a
    /// quote opens runs to the quote that closes it on its line; a bare one
    /// to the first blank or [`BARE_ENDS`] character, less the marks a
    /// sentence closes with. A control-character escape in it ends the value's
    /// field as a line break or a tab does ([`field_end`]: `"X-Api-Key:
    /// <value>\r\nHost: …"`), so the value ends there when it is a credential
    /// up to there;
    /// otherwise the backslash may be its own character, and the value is
    /// read whole. A value is one opaque string, no placeholder and no value
    /// the key-name rules keep under the key the text spells from the name
    /// and the path it ends ([`ValueTest::KeyName`]), or it is no value of
    /// this rule's.
    pub(super) fn scalar(&self, name: usize, run: Range<usize>) -> Option<Range<usize>> {
        let bytes = self.text.as_bytes();
        let quote = run
            .start
            .checked_sub(1)
            .and_then(|before| bytes.get(before))
            .filter(|byte| matches!(byte, b'"' | b'\'' | b'`'));
        let rest = self.text.get(run.end..)?;
        // Where the value read to `to` bytes past the run ends at a field's
        // escape, counted from the run's end as `to` is.
        let field_end_after = |to: usize| {
            field_end(self.text.get(run.start..run.end + to)?)?.checked_sub(run.len())
        };
        let (whole, line) = match quote {
            Some(&quote) => {
                let close = rest
                    .find([char::from(quote), '\n', '\r'])
                    .filter(|close| rest.as_bytes().get(*close) == Some(&quote))?;
                (close, field_end_after(close))
            }
            None => {
                let end = rest
                    .find(|c: char| !c.is_ascii_graphic() || BARE_ENDS.contains(&c))
                    .unwrap_or(rest.len());
                let bare = |end: usize| {
                    rest.get(..end)
                        .map_or(0, |value| value.trim_end_matches(['.', ':', '?']).len())
                };
                let line = field_end_after(end).map(bare);
                (bare(end), line)
            }
        };
        let key = self.text.get(self.path_start(name)?..run.start)?;
        let credential = |more: usize| {
            let span = run.start..run.end + more;
            self.text
                .get(span.clone())
                .filter(|value| {
                    is_opaque(value, OPAQUE_LITERAL_CHARS)
                        && !is_placeholder(value)
                        && ValueTest::KeyName.redacts(key, value)
                })
                .map(|_| span)
        };
        line.and_then(credential).or_else(|| credential(whole))
    }

    /// Where the key of the name at `name` starts: where its path starts,
    /// since a name is the last part of one (`config/credentials.yaml:
    /// "<digest>"`), and a path says more.
    pub(super) fn path_start(&self, name: usize) -> Option<usize> {
        let path = self
            .text
            .get(..name)?
            .rfind(|c: char| {
                !(c.is_alphanumeric() || matches!(c, '/' | '\\' | '_' | '.' | '-' | '$'))
            })
            .map_or(0, |at| at + 1);
        (path..=name).find(|at| self.text.is_char_boundary(*at))
    }

    /// Calls `found` for each value `opener` holds under `name`, and returns
    /// where the walk ended when the text to there is taken: it held a
    /// value, or the walks have spent their budget.
    pub(super) fn values(
        &mut self,
        opener: &Opener<'_>,
        name: &Named,
        found: &mut impl FnMut(Range<usize>, Range<usize>),
    ) -> Option<usize> {
        let mut any = false;
        let mut emit = |span: Range<usize>| {
            any = true;
            found(name.at..span.end, span);
        };
        let indent = name.indent;
        let (start, end) = match *opener {
            Opener::Bracket { at } => (at, self.bracket(at, name.several, &mut emit)),
            Opener::BlockScalar { from } => {
                let end = self.block_end(from, indent);
                self.words(from..end, &mut emit);
                (from, end)
            }
            Opener::BlockList { from } => (from, self.list_items(from, indent, &mut emit)),
            Opener::Triple { quote, from } => {
                let end = self.until(from, |rest| rest.find(quote));
                self.words(from..end, &mut emit);
                (from, end)
            }
            Opener::Heredoc { tag, from, rest } => {
                let body = if rest { self.next_line(from) } else { from };
                let end = self.heredoc_end(body, tag);
                self.words(body..end, &mut emit);
                (from, end)
            }
            Opener::Continued { from } => {
                let end = self.until(from, |rest| rest.find('"'));
                self.words(from..end, &mut emit);
                (from, end)
            }
            Opener::Expression { from, key } => (from, self.expression(from, key, &mut emit)),
        };
        if any {
            return Some(end);
        }
        self.spent += end.saturating_sub(start);
        (self.spent > self.text.len().saturating_mul(REWALKS)).then_some(end)
    }

    /// The literals of the expression from `from` to the end of its
    /// statement (a `;` or `,`, a line break or a closing bracket outside
    /// every bracket it opens, or a comment), each judged as the value the
    /// name `key` holds ([`ValueTest::Named`]): a ternary's arms, a
    /// fallback's operand, a concatenation's pieces. A literal that
    /// interpolates is judged by its text outside the holes
    /// ([`Walk::literal_text`]). Returns where the statement ends.
    fn expression(&self, from: usize, key: &str, emit: &mut impl FnMut(Range<usize>)) -> usize {
        let bytes = self.text.as_bytes();
        let mut depth = 0usize;
        let mut at = from;
        while let Some(&byte) = bytes.get(at) {
            match byte {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => match depth.checked_sub(1) {
                    Some(outer) => depth = outer,
                    None => return at,
                },
                b';' | b',' | b'\n' | b'\r' if depth == 0 => return at,
                b'/' if bytes.get(at + 1) == Some(&b'/') => return line_end(bytes, at),
                b'#' if at == 0 || bytes[at - 1].is_ascii_whitespace() => {
                    return line_end(bytes, at);
                }
                b'"' | b'\'' | b'`' => {
                    let close = string_end(bytes, at);
                    for piece in self.literal_text(at + 1..close) {
                        // One run, `=` only as its padding: a literal that
                        // holds `name=value` is the name's own to judge.
                        let judged = self.text.get(piece.clone()).is_some_and(|value| {
                            is_run(value)
                                && !value.trim_end_matches('=').contains('=')
                                && !is_placeholder(value)
                                && ValueTest::Named.redacts(key, value)
                        });
                        if judged {
                            emit(piece);
                        }
                    }
                    at = close;
                }
                _ => {}
            }
            at += 1;
        }
        bytes.len()
    }

    /// The text of the literal whose body lies at `body`, outside the
    /// holes an interpolation writes in it (`${…}`): what the source spells,
    /// each piece of it apart.
    fn literal_text(&self, body: Range<usize>) -> Vec<Range<usize>> {
        let Some(text) = self.text.get(body.clone()) else {
            return Vec::new();
        };
        let mut pieces = Vec::new();
        let mut start = 0;
        while let Some(open) = text.get(start..).and_then(|rest| rest.find("${")) {
            let open = start + open;
            let Some(close) = text.get(open..).and_then(|rest| rest.find('}')) else {
                break;
            };
            pieces.push(body.start + start..body.start + open);
            start = open + close + 1;
        }
        pieces.push(body.start + start..body.end);
        pieces.retain(|piece| !piece.is_empty());
        pieces
    }

    /// Where the text from `from` ends: where `close` finds its end, or
    /// the text's.
    fn until(&self, from: usize, close: impl Fn(&str) -> Option<usize>) -> usize {
        let rest = self.text.get(from..).unwrap_or_default();
        from + close(rest).unwrap_or(rest.len())
    }

    /// Where the line after the one `at` lies on starts.
    fn next_line(&self, at: usize) -> usize {
        let rest = self.text.get(at..).unwrap_or_default();
        rest.find('\n').map_or(self.text.len(), |end| at + end + 1)
    }

    /// Where the heredoc body starting at `body` ends: at the line that is
    /// `tag` alone, or the text's end.
    fn heredoc_end(&self, body: usize, tag: &str) -> usize {
        let mut line = body;
        while line < self.text.len() {
            let next = self.next_line(line);
            if self
                .text
                .get(line..next)
                .is_some_and(|text| text.trim() == tag)
            {
                return line;
            }
            line = next;
        }
        self.text.len()
    }

    /// The opaque words of the text at `range`, each blank-separated word
    /// without the quotes and the `,` or `;` around it. A text an armor
    /// names as public ([`is_public_armor`]) holds none.
    fn words(&self, range: Range<usize>, emit: &mut impl FnMut(Range<usize>)) {
        let Some(body) = self
            .text
            .get(range.clone())
            .filter(|body| !is_public_armor(body))
        else {
            return;
        };
        let mut at = range.start;
        for word in body.split_inclusive(char::is_whitespace) {
            let trimmed = word.trim_end();
            let inner = trimmed.trim_end_matches([',', ';']).trim_matches(QUOTES);
            let start = at + (trimmed.len() - trimmed.trim_start_matches(QUOTES).len());
            at += word.len();
            if is_run(inner) && is_opaque(inner, OPAQUE_LITERAL_CHARS) && !is_placeholder(inner) {
                emit(start..start + inner.len());
            }
        }
    }

    /// Where the YAML block scalar whose lines start at `from` ends, under
    /// a key indented by `indent`: at the first line with text that is
    /// indented no further than the key.
    fn block_end(&self, from: usize, indent: usize) -> usize {
        let mut line = from;
        while line < self.text.len() {
            let next = self.next_line(line);
            let text = self.text.get(line..next).unwrap_or_default();
            let body = text.trim_start_matches([' ', '\t']);
            if !body.trim().is_empty() && text.len() - body.len() <= indent {
                break;
            }
            line = next;
        }
        line
    }

    /// The items of the YAML block list whose first item's line starts at
    /// `from`, under a key indented by `indent`: each `- ` line indented no
    /// less than the key whose item is one opaque scalar. Returns where the
    /// list ends.
    fn list_items(&self, from: usize, indent: usize, emit: &mut impl FnMut(Range<usize>)) -> usize {
        let mut line = from;
        while line < self.text.len() {
            let next = self.next_line(line);
            let text = self.text.get(line..next).unwrap_or_default();
            let body = text.trim_start_matches([' ', '\t']);
            if body.trim().is_empty() {
                line = next;
                continue;
            }
            let Some(item) = body
                .strip_prefix('-')
                .filter(|item| text.len() - body.len() >= indent && item.starts_with([' ', '\t']))
            else {
                break;
            };
            let scalar = item.trim();
            let inner = scalar.trim_matches(QUOTES);
            let start = line
                + (text.len() - item.trim_start().len())
                + (scalar.len() - scalar.trim_start_matches(QUOTES).len());
            if is_run(inner) && is_opaque(inner, ELEMENT_CHARS) && !is_placeholder(inner) {
                emit(start..start + inner.len());
            }
            line = next;
        }
        line
    }

    /// The values between the bracket at `open` and the one that closes it:
    /// each opaque string that is an item of a list at any depth, a member
    /// whose own name names a credential or is `value`, or, under a name of
    /// `several` credentials, a member right inside the bracket (`apiKeys =
    /// { prod: "…" }`) unless its name says its value is none
    /// ([`names_no_value`]). A member's name is no value, and a member under
    /// any other name is its record's own (`"secret": {"read": {"snapshot":
    /// "…"}}`). A string right inside the bracket as long as a value beside
    /// it is one too: the pieces of one secret, and the secrets of one list,
    /// are of a length. Returns where the collection ends: past its closing
    /// bracket, at a bracket that closes none of its own, or at the text's
    /// end.
    fn bracket(&self, open: usize, several: bool, emit: &mut impl FnMut(Range<usize>)) -> usize {
        let run = |span: &Range<usize>| {
            self.text
                .get(span.clone())
                .filter(|value| is_run(value) && !is_placeholder(value))
        };
        let holds = |span: &Range<usize>, depth: usize| {
            !self.names_a_member(span.end)
                && self.member_name(span.start).is_none_or(|name| {
                    name == VALUE_MEMBER
                        || names_a_credential(name)
                        || (several && depth == 1 && !names_no_value(name))
                })
        };
        let opaque = |span: &Range<usize>, depth: usize| {
            run(span).is_some_and(|value| is_opaque(value, ELEMENT_CHARS)) && holds(span, depth)
        };
        let mut lengths: Vec<usize> = Vec::new();
        let mut any = false;
        // The objects that are JSON Web Keys, by where each opens: the JWK
        // rule hides a key's private members, and its public ones stay.
        let mut web_keys: Vec<usize> = Vec::new();
        let end = self.strings(open, |span, depth, object| {
            if self.text.get(span.clone()) == Some(jwk::KEY_TYPE)
                && web_keys.last() != Some(&object)
            {
                web_keys.push(object);
            }
            if opaque(&span, depth) {
                any = true;
                if depth == 1 && !lengths.contains(&span.len()) {
                    lengths.push(span.len());
                }
            }
        });
        if !any {
            return end;
        }
        web_keys.sort_unstable();
        web_keys.dedup();
        self.strings(open, |span, depth, object| {
            let sibling = depth == 1
                && span.len() >= ELEMENT_CHARS
                && lengths.contains(&span.len())
                && run(&span).is_some()
                && holds(&span, depth);
            if (sibling || opaque(&span, depth)) && web_keys.binary_search(&object).is_err() {
                emit(span);
            }
        });
        end
    }

    /// Whether the string whose body ends at `end` is a member's name: a `:`
    /// follows its closing quote.
    fn names_a_member(&self, end: usize) -> bool {
        self.text
            .get(end + 1..)
            .is_some_and(|after| after.trim_start().starts_with(':'))
    }

    /// The name of the member whose value is the string whose body starts
    /// at `body`: the word before the `:`, `=` or `=>` before its quote.
    fn member_name(&self, body: usize) -> Option<&'t str> {
        let before = self.text.get(..body.checked_sub(1)?)?.trim_end();
        let before = before
            .strip_suffix("=>")
            .or_else(|| before.strip_suffix([':', '=']))?
            .trim_end()
            .trim_end_matches(QUOTES);
        let name = before
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '$')))
            .map_or(before, |at| before.get(at + 1..).unwrap_or_default());
        (!name.is_empty()).then_some(name)
    }

    /// Calls `string` with the body of each string literal between the
    /// bracket at `open` and its close, the depth of brackets it lies at and
    /// where the innermost of them opens, comments skipped; returns where
    /// the walk ends ([`Walk::bracket`]). A `"` or `'` string ends with its
    /// line.
    fn strings(&self, open: usize, mut string: impl FnMut(Range<usize>, usize, usize)) -> usize {
        let bytes = self.text.as_bytes();
        let mut opened: Vec<(usize, u8)> = Vec::new();
        let mut at = open;
        while let Some(&byte) = bytes.get(at) {
            match byte {
                b'[' => opened.push((at, b']')),
                b'(' => opened.push((at, b')')),
                b'{' => opened.push((at, b'}')),
                b']' | b')' | b'}' => {
                    if opened.pop().map(|(_, closer)| closer) != Some(byte) {
                        return at;
                    }
                    if opened.is_empty() {
                        return at + 1;
                    }
                }
                b'"' | b'\'' | b'`' => {
                    let close = string_end(bytes, at);
                    let innermost = opened.last().map_or(open, |(at, _)| *at);
                    string(at + 1..close, opened.len(), innermost);
                    at = close;
                }
                b'/' if bytes.get(at + 1) == Some(&b'/') => at = line_end(bytes, at),
                b'#' if at == 0 || bytes[at - 1].is_ascii_whitespace() => {
                    at = line_end(bytes, at);
                }
                b'/' if bytes.get(at + 1) == Some(&b'*') => {
                    at = self.until(at + 2, |rest| rest.find("*/")) + 1;
                }
                _ => {}
            }
            at += 1;
        }
        bytes.len()
    }
}
