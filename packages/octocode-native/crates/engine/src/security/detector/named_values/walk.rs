//! The values a name holds: the one after it, and those of the collection
//! or block it opens.

use std::ops::Range;

use super::names::{names_a_credential, names_no_value};
use super::opaque::{is_opaque, is_run};
use super::text::{field_end, hole_end, is_public_armor, line_end, literal_line_end, string_end};
use super::{Named, Opener};
use crate::security::detector::credential_values::{
    OPAQUE_LITERAL_CHARS, QUOTES, ValueTest, expanded_names, is_chosen_literal, is_expanded_name,
};
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
    expanded: std::cell::OnceCell<std::collections::HashSet<Box<str>>>,
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
            expanded: std::cell::OnceCell::new(),
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
    /// sentence closes with. In a quoted value a control-character escape ends
    /// the value's line as a line break does ([`literal_line_end`]: `"X-Api-Key:
    /// <value>\r\nHost: …"`, `"the password is required\n"`), so what precedes
    /// it is judged alone. In a bare one it ends the value's field when the
    /// value is a credential up to there ([`field_end`]); otherwise the
    /// backslash may be its own character, and the value is read whole. A value is one opaque string, no placeholder and no value
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
        let field_end_after =
            |to: usize| field_end(self.text.get(run.start..run.end + to)?)?.checked_sub(run.len());
        let (whole, line) = match quote {
            Some(&quote) => {
                let close = rest
                    .find([char::from(quote), '\n', '\r'])
                    .filter(|close| rest.as_bytes().get(*close) == Some(&quote))?;
                let line = literal_line_end(self.text.get(run.start..run.end + close)?)
                    .and_then(|end| end.checked_sub(run.len()));
                (line.unwrap_or(close), line)
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
            Opener::BlockScalar { header, from } => {
                let end = self.block_end(from, indent);
                if !self.block_value(name.at, header, from..end, &mut emit) {
                    self.words(from..end, &mut emit);
                }
                (from, end)
            }
            Opener::BlockList { from } => (from, self.list_items(from, indent, &mut emit)),
            Opener::Triple { quote, from } => {
                let end = self.until(from, |rest| rest.find(quote));
                self.words(from..end, &mut emit);
                (from, end)
            }
            Opener::Heredoc { tag, from, rest } => {
                let (body, end) = match self.escaped_heredoc(from, tag).filter(|_| rest) {
                    Some(escaped) => escaped,
                    None => {
                        let body = if rest { self.next_line(from) } else { from };
                        (body, self.heredoc_end(body, tag))
                    }
                };
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
    /// every bracket it opens, or a comment), each judged as a value the name
    /// `key` holds. The statement goes on over a line break where its
    /// expression does ([`Walk::continues_below`]). Only a literal is read
    /// ([`Walk::literal_at`]): a quote that opens an operand and closes on
    /// its line, so a quote that closes a string the name stands in, or one
    /// a sentence or a comment writes, opens none, and no value runs on past
    /// its line. A literal is a value when it is opaque
    /// ([`ValueTest::Named`]), or, held whole outside every bracket (a
    /// ternary's arm, a fallback's operand), a secret a person chose
    /// ([`is_chosen_literal`]); a literal that interpolates is judged by its
    /// text outside the holes, and each hole's code as an expression of its
    /// own; a piece joined to others by `+` is part of a value, and only an
    /// opaque one is a value of its own ([`Walk::judge_literal`]). Returns
    /// where the statement ends.
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
                b'\n' | b'\r' if depth == 0 && self.continues_below(from, at, key) => {}
                b';' | b',' | b'\n' | b'\r' if depth == 0 => return at,
                b'/' if bytes.get(at + 1) == Some(&b'/') => return line_end(bytes, at),
                b'#' if at == 0 || bytes[at - 1].is_ascii_whitespace() => {
                    return line_end(bytes, at);
                }
                // An escaped quote is text of a literal the statement is
                // written in (`"let secret = f(\"…\");\n"`): it opens none,
                // and the brackets and `;` around it are the statement's.
                b'"' | b'\'' | b'`' if escaped_at(bytes, at) => {}
                b'"' | b'\'' | b'`' => {
                    // A quote that closes on no later point of its line opens
                    // no literal of this statement (a backtick that a comment
                    // or a sentence writes): the rest of its line is that
                    // string's, and the line's end is read as for any text.
                    let line = line_end(bytes, at);
                    let close = string_end(&bytes[..line], at);
                    if bytes.get(close) != Some(&byte) || close >= line {
                        at = line;
                        continue;
                    }
                    if let Some(prefix) = self.literal_at(from, at, close) {
                        let quoted = format!("{key}{prefix}{}", char::from(byte));
                        self.judge_literal(&quoted, at, close, depth == 0, emit);
                    }
                    at = close;
                }
                _ => {}
            }
            at += 1;
        }
        bytes.len()
    }

    /// The prefix of the literal whose quote at `open` closes at `close`
    /// (`f`, `$`, `r`: letters right before the quote), when it is one: the
    /// quote is no escaped one, it closes on its line, and an operand
    /// starts there: at the expression's start (`from`) or after an
    /// operator, an opening bracket or a `,`, blanks aside. `None` for a
    /// quote that opens no literal.
    fn literal_at(&self, from: usize, open: usize, close: usize) -> Option<&'t str> {
        let bytes = self.text.as_bytes();
        if bytes.get(close) != bytes.get(open) {
            return None;
        }
        let before = self.text.get(from..open)?;
        let head =
            before.trim_end_matches(|c: char| c.is_ascii_alphabetic() || matches!(c, '$' | '@'));
        let prefix = before.get(head.len()..)?;
        if prefix.len() > 3 || head.ends_with('\\') {
            return None;
        }
        // A quote right after a word or a closing mark closes a string; one
        // after a blank, an operator, an opening bracket or a `,` opens one.
        let opens_operand = head.is_empty()
            || head.ends_with(char::is_whitespace)
            || head.ends_with([
                '=', '?', ':', '|', '&', '+', '(', '[', '{', ',', '!', '>', '<',
            ]);
        opens_operand.then_some(prefix)
    }

    /// Calls `emit` for each value the literal whose quote at `open` closes
    /// at `close` holds under `quoted` (the name, its operator, the
    /// literal's prefix and quote: [`Walk::expression`]). `whole`: the
    /// literal is an operand the name may hold whole (a ternary's arm, a
    /// fallback's operand), outside every bracket; one inside a bracket is a
    /// call's argument or a collection's element, a value only when opaque.
    fn judge_literal(
        &self,
        quoted: &str,
        open: usize,
        close: usize,
        whole: bool,
        emit: &mut impl FnMut(Range<usize>),
    ) {
        let bytes = self.text.as_bytes();
        let joined = bytes
            .get(close + 1..)
            .is_some_and(|after| after.trim_ascii_start().first() == Some(&b'+'))
            || bytes
                .get(..open)
                .is_some_and(|before| before.trim_ascii_end().last() == Some(&b'+'));
        let judged = |value: &str| {
            // `=` only as padding: a literal that holds `name=value` is the
            // name's own to judge.
            !value.trim_end_matches('=').contains('=')
                && !is_placeholder(value)
                && !value.contains(crate::security::REDACTION_PREFIX)
                && !is_expanded_name(|| self.expanded(), value)
        };
        let body = open + 1..close;
        if joined {
            for piece in self.literal_text(body) {
                let opaque = self.text.get(piece.clone()).is_some_and(|value| {
                    is_run(value) && judged(value) && ValueTest::Named.redacts(quoted, value)
                });
                if opaque {
                    emit(piece);
                }
            }
            return;
        }
        // A literal is a value when it is opaque, or, held whole, a secret a
        // person chose ([`is_chosen_literal`]): a long readable one (a URL,
        // a message, an identifier) is the code's text.
        let value = |value: &str| {
            judged(value)
                && (ValueTest::Named.redacts(quoted, value)
                    || (whole && is_chosen_literal(quoted, value)))
        };
        let pieces = self.literal_text(body.clone());
        if pieces.len() == 1 && pieces[0] == body {
            if self.text.get(body.clone()).is_some_and(value) {
                emit(body);
            }
            return;
        }
        // A literal that interpolates holds its value in the text beside
        // the holes: each piece that is one alone goes. A hole is code, its
        // own literals judged as the expression's are.
        let mut hole = body.start;
        for piece in pieces
            .into_iter()
            .chain(std::iter::once(body.end..body.end))
        {
            if piece.start > hole {
                self.expression(hole + 2, quoted, emit);
            }
            hole = piece.end;
            let Some(text) = self.text.get(piece.clone()) else {
                continue;
            };
            // The marks that join a piece to a hole (`${u}:…`, `${host}/…`)
            // are the template's, no part of the value.
            let joins = |c: char| {
                c.is_whitespace() || matches!(c, ':' | '@' | '/' | ';' | ',' | '&' | '?' | '#')
            };
            let trimmed = text.trim_matches(joins);
            let start = piece.start + (text.len() - text.trim_start_matches(joins).len());
            if !trimmed.is_empty() && !trimmed.contains(char::is_whitespace) && value(trimmed) {
                emit(start..start + trimmed.len());
            }
        }
    }

    /// The names the text expands as a variable ([`expanded_names`]), read
    /// once when a literal first asks.
    fn expanded(&self) -> &std::collections::HashSet<Box<str>> {
        self.expanded.get_or_init(|| expanded_names(self.text))
    }

    /// Whether the expression assigned from `from` (after the name and
    /// operator `key`) goes on past the line break at `at`: its line so far
    /// holds nothing yet after an `=` (a value on the lines below; after a
    /// bare `:` the lines below are a block's own), or ends with an operator
    /// that takes another operand (`?`, `:`, `||`, `&&`, `??`, `+`, `=`), or
    /// the next line with text opens with one (`? "…"`, `: "…"`, `|| "…"`,
    /// `.call(…)`, `+ "…"`).
    fn continues_below(&self, from: usize, at: usize, key: &str) -> bool {
        let line_start = self
            .text
            .get(..at)
            .and_then(|before| before.rfind(LINE_END_CHARS))
            .map_or(0, |found| found + 1)
            .max(from);
        let so_far = self.text.get(line_start..at).unwrap_or_default().trim();
        let after_colon = {
            let key = key.trim_end();
            key.ends_with(':') && !key.ends_with("::")
        };
        if so_far.is_empty() {
            return line_start == from && !after_colon;
        }
        if so_far.ends_with(['?', ':', '|', '&', '+', '=']) {
            return true;
        }
        let next = self
            .text
            .get(at..)
            .unwrap_or_default()
            .trim_start_matches(|c: char| c.is_whitespace());
        ["?", ":", "||", "&&", "??"]
            .iter()
            .any(|operator| next.starts_with(operator))
            || next
                .strip_prefix('.')
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'))
            || next
                .strip_prefix('+')
                .is_some_and(|rest| rest.starts_with([' ', '\t']))
    }

    /// Whether the statement a collection opened by the bracket at `open`
    /// stands in goes on after the bracket that closes it: the collection
    /// is one operand of an expression (a composite literal's type before
    /// its braces, `map[bool]string{…}`; an index or a call after them),
    /// not the whole value. A line's end, a `;` or `,`, a closing bracket or
    /// a comment after it ends the statement.
    pub(super) fn goes_on_after(&self, open: usize) -> bool {
        let close = self.strings(open, |_, _, _| {});
        let after = self
            .text
            .get(close..)
            .unwrap_or_default()
            .trim_start_matches([' ', '\t']);
        !(after.is_empty()
            || after.starts_with([';', ',', '\n', '\r', ')', ']', '}', '#'])
            || after.starts_with("//")
            || after.starts_with("/*"))
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
            // The hole's own brackets and strings close inside it.
            let close = hole_end(&self.text.as_bytes()[..body.end], body.start + open + 2);
            pieces.push(body.start + start..body.start + open);
            start = close - body.start;
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

    /// The body of a heredoc whose opener ends at `from` and that a string
    /// literal holds (`"token = <<~EOS\n  …\nEOS\n"`): its opener's line goes
    /// on with a line-break escape, so its lines are the literal's escaped
    /// lines, and the body ends at the escaped line that is `tag` alone, or at
    /// the source line's end. `None` when the heredoc's lines are the source's.
    fn escaped_heredoc(&self, from: usize, tag: &str) -> Option<(usize, usize)> {
        const BREAKS: [&str; 2] = ["\\r\\n", "\\n"];
        let line = self.text.get(from..line_end(self.text.as_bytes(), from))?;
        let opened = BREAKS.iter().find_map(|escape| line.strip_prefix(escape))?;
        let body = from + (line.len() - opened.len());
        let mut at = 0;
        for piece in opened.split_inclusive("\\n") {
            let text = piece.trim_end_matches("\\n").trim_end_matches("\\r");
            if text.trim() == tag {
                return Some((body, body + at));
            }
            at += piece.len();
        }
        Some((body, body + opened.len()))
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

    /// Whether the body at `body` of the YAML block scalar whose header
    /// starts at `header`, under the name at `name`, is that name's value as
    /// the one-line `name: value` is ([`ValueTest::Scalar`]): its lines'
    /// text, joined, is judged as the value, and when it is one each line's
    /// text goes (`emit`), its indentation and the header staying. A body
    /// that is no value is read for the opaque words it holds instead.
    fn block_value(
        &self,
        name: usize,
        header: usize,
        body: Range<usize>,
        emit: &mut impl FnMut(Range<usize>),
    ) -> bool {
        let Some(key) = self
            .path_start(name)
            .and_then(|start| self.text.get(start..header))
        else {
            return false;
        };
        let mut lines = Vec::new();
        let mut line = body.start;
        while line < body.end {
            let next = self.next_line(line).min(body.end);
            let text = self.text.get(line..next).unwrap_or_default();
            let start = line + (text.len() - text.trim_start().len());
            let end = start + text.trim().len();
            if end > start {
                lines.push(start..end);
            }
            line = next;
        }
        let value = lines
            .iter()
            .filter_map(|line| self.text.get(line.clone()))
            .collect::<Vec<_>>()
            .join(" ");
        // A public key's armor holds no secret ([`Walk::words`] reads it so).
        let public = self.text.get(body).is_some_and(is_public_armor);
        if lines.is_empty()
            || public
            || is_placeholder(&value)
            || !ValueTest::Scalar.redacts(key, &value)
        {
            return false;
        }
        for line in lines {
            emit(line);
        }
        true
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
                // An escaped quote is text of a literal the collection is
                // written in: it opens no string of the collection's.
                b'"' | b'\'' | b'`' if escaped_at(bytes, at) => {}
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

/// Whether the byte at `at` follows an odd run of backslashes: a backslash
/// escapes it.
fn escaped_at(bytes: &[u8], at: usize) -> bool {
    bytes
        .get(..at)
        .unwrap_or_default()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count()
        % 2
        == 1
}
