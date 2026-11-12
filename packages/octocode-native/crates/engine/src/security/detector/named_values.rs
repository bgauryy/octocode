//! The values a credential's name holds, wherever source lays them out.
//!
//! The pattern finds a name with a credential's word part and what follows
//! it; this module judges both. A name is a credential's by its word parts
//! ([`names_a_credential`]), never by a substring (`pat`, not `path`). Its
//! value is one only when it is opaque ([`is_opaque`]): the layouts read
//! here are loose (`ENV NAME value`, `--token value`, `the key is value`), so
//! a word, a name, a path or a version after the name stays readable.
//!
//! After an assignment the name may hold several values: the strings of a
//! collection (`tokens = ["…", "…"]`, `auth=("user", "…")`), the lines of a
//! block (`token: |`, `"""`, a heredoc). Each is walked once, and only its
//! opaque strings go: the names, quotes and separators around them stay.

use std::ops::Range;

use regex::Regex;

use super::credential_values::{OPAQUE_LITERAL_CHARS, QUOTES, ValueTest, is_stated_chosen_secret};

mod names;
mod opaque;
mod text;
mod walk;

use names::calls_with_credentials;
use names::follows_a_public_word;
use names::holds_more_than_a_nonce;
use names::names_several;
pub(super) use names::{
    describes_a_credential, is_nonce_shape, is_user_agent_nonce, names_a_credential,
    names_a_published_value, names_a_user_agent_field, spells_a_credential_word,
};
pub(super) use opaque::{is_opaque, is_run};
use text::ends_a_line;
pub(super) use text::{field_end, field_end_after, literal_line_end};
use walk::Walk;

/// The pattern this module judges.
pub(super) const PATTERN: &str = "credentialNamedValue";

/// What ends a sentence or a clause after a value: no part of the value.
const SENTENCE_CLOSERS: [char; 5] = ['.', ',', ';', ')', '?'];

/// Whether `word` is opaque by its shape alone, whatever its length
/// ([`is_opaque`]'s evidence decides): what a generator makes or a person
/// chose to be no word. A config line's rules hide a value under a
/// credential's name at any length (`password: Hunter2Xq9vB7mK`); a value in
/// a sentence or a request is one when it is no word, name or type
/// (`password: str`, `token: Token`, `the password is required`), which no
/// opaque word is. The length a quoted literal or a loose layout asks
/// ([`OPAQUE_LITERAL_CHARS`]) tells a name from a value where a run of
/// token characters is all a rule reads.
pub(super) fn is_opaque_word(word: &str) -> bool {
    is_opaque(word, 0)
}

/// The part of `word`, the first word after a credential's name `name` and
/// its operator or the words that state a value (`is`, `was`), that is a
/// credential, by byte position in `word`: the word without the quotes
/// around it and the marks that close a sentence after it, up to the
/// escape that ends its line ([`literal_line_end`]), when it is opaque ([`is_opaque_word`]) or a value
/// a person chose that the sentence states ([`is_stated_chosen_secret`];
/// `prose`: stated by a word, not an operator), a value the key-name rules
/// keep as a credential ([`ValueTest::KeyName`]), and no nonce a user agent
/// fills ([`is_user_agent_nonce`]). This is [`ValueTest::Named`] without the
/// length a loose layout's run asks.
pub(super) fn named_word(name: &str, word: &str, prose: bool) -> Option<Range<usize>> {
    let opened = word.trim_start_matches(QUOTES);
    // A quote inside the word closes the string the sentence is in
    // (`"the password is <value>".to_owned()`): the code after it is no
    // part of the value.
    let opened_end = opened.find(QUOTES).unwrap_or(opened.len());
    let own = opened
        .get(..opened_end)
        .unwrap_or(opened)
        .trim_end_matches(QUOTES)
        .trim_end_matches(SENTENCE_CLOSERS)
        .trim_end_matches(QUOTES);
    let own = own.get(..literal_line_end(own).unwrap_or(own.len()))?;
    let at = word.len() - opened.len();
    ((is_opaque_word(own) || is_stated_chosen_secret(name, own, prose))
        && !super::is_placeholder(own)
        && ValueTest::KeyName.redacts(name, own)
        && !is_user_agent_nonce(name, own))
    .then_some(at..at + own.len())
}

/// Whether `word`, after a value's word and one blank, is more of that
/// value: a run of an opaque value's characters ([`is_run`]) that mixes
/// letters and digits, as the word before it does (`x-9f8.6d0.818
/// 84c.7d6.59a`). A word of letters alone is the sentence's (`<token> is the
/// key`, `<token> tailJ`), and an option or a setting of its own
/// (`--db-pass=…`, `name=…`) is the line's next field.
fn continues_a_value(word: &str) -> bool {
    is_run(word)
        && !word.starts_with('-')
        && !word.trim_end_matches('=').contains('=')
        && word.bytes().any(|byte| byte.is_ascii_digit())
        && word.bytes().any(|byte| byte.is_ascii_alphabetic())
        && !super::is_placeholder(word)
}

/// Where a bare value whose first word ends at `end` in `text` ends: past
/// each next word on its line, one blank before it, that continues it
/// ([`continues_a_value`]), so a value of several words goes whole and no
/// word of it stays readable. A closer after a word (`;`, `,`, `.`), two
/// blanks, a comment or the line's end ends the value.
pub(super) fn value_words_end(text: &str, end: usize) -> usize {
    let mut end = end;
    while let Some(after) = text
        .get(end..)
        .and_then(|rest| rest.strip_prefix([' ', '\t']))
    {
        let word = after
            .get(..after.find(char::is_whitespace).unwrap_or(after.len()))
            .unwrap_or_default();
        let own = word.trim_end_matches(SENTENCE_CLOSERS);
        if own.is_empty() || !continues_a_value(own) {
            break;
        }
        end += 1 + own.len();
        if own.len() < word.len() {
            break;
        }
    }
    end
}

/// `span`, a value found in `text`, with the words that continue it when it
/// is bare ([`value_words_end`]): a quoted value ends at its quote.
fn with_value_words(text: &str, span: Range<usize>) -> Range<usize> {
    let quoted = span
        .start
        .checked_sub(1)
        .and_then(|before| text.as_bytes().get(before))
        .is_some_and(|byte| matches!(byte, b'"' | b'\'' | b'`'));
    if quoted {
        return span;
    }
    span.start..value_words_end(text, span.end)
}

/// The operators a shell parameter expansion writes between its name and
/// its word (`${NAME:-default}`, `${NAME:=default}`, `${NAME:?message}`,
/// `${NAME:+alternative}`): syntax, no part of the value.
const SHELL_EXPANSION_OPERATORS: [u8; 4] = *b"-=?+";

/// `run`, the value found after the name at `name` in `text`, without the
/// operator of a shell parameter expansion the name opens (`${API_KEY:-…}`):
/// the default after it is the value, and the expansion keeps its syntax.
fn without_expansion_operator(text: &str, name: Range<usize>, run: Range<usize>) -> Range<usize> {
    let expands = text
        .get(..name.start)
        .is_some_and(|before| before.ends_with("${"))
        && text.get(name.end..run.start) == Some(":")
        && text
            .as_bytes()
            .get(run.start)
            .is_some_and(|byte| SHELL_EXPANSION_OPERATORS.contains(byte));
    if expands && run.start + 1 < run.end {
        run.start + 1..run.end
    } else {
        run
    }
}

/// The capture groups the pattern names.
struct Groups {
    name: usize,
    tag: usize,
    open: usize,
    /// The word a sentence states after the name ([`STATES_A_VALUE`]).
    stated: usize,
    /// The text after an assignment that opens an expression of literals.
    expr: usize,
    /// The text after an assignment whose line holds no literal.
    below: usize,
    /// The run right after an assignment (no gap's): its statement may
    /// hold more literals when the run is none.
    assigned: usize,
    secrets: Vec<usize>,
}

impl Groups {
    fn of(regex: &Regex) -> Option<Self> {
        let index = |wanted: &str| regex.capture_names().position(|name| name == Some(wanted));
        Some(Self {
            name: index("name")?,
            tag: index("tag")?,
            open: index("open")?,
            stated: index("stated")?,
            expr: index("expr")?,
            below: index("below")?,
            assigned: index("secret")?,
            secrets: super::secret_groups(regex),
        })
    }
}

/// Calls `found(whole, secret)` for each credential the pattern `regex`
/// names in `text`, in order, none overlapping another. A match whose name
/// or value is turned away is read again past its name: the text it took may
/// hold a name of its own.
pub(super) fn secrets(
    regex: &Regex,
    text: &str,
    mut found: impl FnMut(Range<usize>, Range<usize>),
) {
    let Some(groups) = Groups::of(regex) else {
        return;
    };
    let mut walk = Walk::new(text);
    let mut from = 0;
    while let Some(captures) = text.get(from..).and_then(|_| regex.captures_at(text, from)) {
        let Some(name) = captures
            .get(groups.name)
            .or_else(|| captures.get(groups.tag))
        else {
            break;
        };
        from = name.end();
        // A line-ending escape before the name (`…\r\nX-Api-Key`) ends the
        // line before it: its letter is no part of the name.
        let spelled = name
            .as_str()
            .get(1..)
            .filter(|_| ends_a_line(text.as_bytes(), name.start()))
            .unwrap_or(name.as_str());
        // A token that happens to spell a credential word is no name. A
        // sign-in verb names none either: it takes credentials only as the
        // callee of a call, its arguments right after it.
        let called = text.as_bytes().get(name.end()) == Some(&b'(');
        let secret = groups.secrets.iter().find_map(|&group| captures.get(group));
        let named = names_a_credential(spelled)
            || (called && calls_with_credentials(spelled))
            || secret.is_some_and(|secret| holds_more_than_a_nonce(spelled, secret.as_str()));
        if !named || is_opaque(spelled, OPAQUE_LITERAL_CHARS) {
            continue;
        }
        let line_start = walk.line_start_of(name.start());
        if follows_a_public_word(text.get(line_start..name.start()).unwrap_or_default()) {
            continue;
        }
        if let Some(stated) = captures.get(groups.stated) {
            // Stated by a word (`is`, `was`), or by an operator.
            let prose = text.get(name.end()..stated.start()).is_some_and(|between| {
                between.split_whitespace().any(|word| {
                    let word = word.trim_end_matches(':');
                    word.eq_ignore_ascii_case("is") || word.eq_ignore_ascii_case("was")
                })
            });
            if let Some(end) = stated_value(
                text,
                spelled,
                name.start(),
                stated.range(),
                prose,
                &mut found,
            ) {
                from = end;
            }
            continue;
        }
        let secret =
            secret.map(|secret| without_expansion_operator(text, name.range(), secret.range()));
        if let Some(secret) = secret.clone() {
            if let Some(span) = walk.scalar(name.start(), secret.clone()) {
                let span = with_value_words(text, span);
                from = span.end;
                found(name.start()..span.end, span);
                continue;
            }
            // A run after an assignment that is no value may be an operand
            // of an expression whose other literals are (`isProd ? "…"`).
            if captures.get(groups.assigned).is_none() {
                continue;
            }
        }
        // The expression assigned: from a literal on the name's line, from
        // its operator when the line holds none (`below`), or from a run
        // that was no value.
        let expression = captures
            .get(groups.expr)
            .or_else(|| captures.get(groups.below))
            .map(|expr| expr.start())
            .or_else(|| {
                secret
                    .as_ref()
                    .and_then(|_| assignment_end(text, name.end()))
            });
        if let Some(start) = expression {
            if let Some(end) =
                expression_values(&mut walk, text, spelled, name.range(), start, &mut found)
            {
                from = from.max(end);
            }
            continue;
        }
        let opener = match captures.get(groups.open) {
            Some(open) => Opener::of(open.as_str(), open.range()),
            None => Some(Opener::Heredoc {
                tag: name.as_str(),
                from: captures.get(0).map_or(name.end(), |whole| whole.end()),
                rest: false,
            }),
        };
        let Some(opener) = opener else {
            continue;
        };
        let key = text.get(line_start..name.end()).unwrap_or_default();
        let indent = key.len() - key.trim_start_matches([' ', '\t']).len();
        let named = Named {
            at: name.start(),
            indent,
            several: names_several(name.as_str()),
        };
        let mut any = false;
        let walked = walk.values(&opener, &named, &mut |whole, span| {
            any = true;
            found(whole, span);
        });
        if let Some(end) = walked {
            from = from.max(end);
        }
        // A collection that holds no value of its own and is one operand of
        // the expression assigned (a composite literal's type before its
        // braces, `map[bool]string{…}[cond]`) is read as that expression.
        if let (false, Some(open), Opener::Bracket { at }) =
            (any, captures.get(groups.open), &opener)
            && walk.goes_on_after(*at)
            && let Some(end) = expression_values(
                &mut walk,
                text,
                spelled,
                name.range(),
                open.start(),
                &mut found,
            )
        {
            from = from.max(end);
        }
    }
}

/// Calls `found` for each value of the expression assigned from `start` to
/// the name `spelled` at `name` ([`Opener::Expression`]); when no literal
/// of it is one, for the value its first word states, unless that word
/// opens a literal or a collection ([`stated_value`]: `then password =
/// hunter2-…`, `password: Gr8pe-…`). Returns where the text it took ends.
fn expression_values(
    walk: &mut Walk<'_>,
    text: &str,
    spelled: &str,
    name: Range<usize>,
    start: usize,
    found: &mut impl FnMut(Range<usize>, Range<usize>),
) -> Option<usize> {
    let key = walk
        .path_start(name.start)
        .and_then(|path| text.get(path..start))
        .unwrap_or_default();
    let named = Named {
        at: name.start,
        indent: 0,
        several: false,
    };
    let opener = Opener::Expression { from: start, key };
    let mut any = false;
    let walked = walk.values(&opener, &named, &mut |whole, span| {
        any = true;
        found(whole, span);
    });
    if any {
        return walked;
    }
    operator_value(text, start)
        .and_then(|word| stated_value(text, spelled, name.start, word, false, found))
        .or(walked)
}

/// The value an operator states at `start` in `text`, blanks before it on
/// its line aside: the word up to a blank or the first character that ends
/// a value in code (a quote, a `,` or `;`, a closing bracket, a pipe, an
/// escape's backslash). `None` when the word opens a literal or a
/// collection, or goes on as code where it ends, a call, an index or a block
/// opening on it (`getToken()`, `env[…]`, `Moo{n}`): an expression is no
/// value, and only its literals are ([`Walk::values`]).
fn operator_value(text: &str, start: usize) -> Option<Range<usize>> {
    let rest = text.get(start..)?;
    let blanks = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let body = rest.get(blanks..)?;
    let end = body
        .find(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '"' | '\'' | '`' | ',' | ';' | '|' | ')' | ']' | '}' | '\\' | '(' | '[' | '{'
                )
        })
        .unwrap_or(body.len());
    let code = body
        .get(end..)
        .is_some_and(|after| after.starts_with(['(', '[', '{']));
    (end > 0 && !code).then_some(start + blanks..start + blanks + end)
}

/// Calls `found` for the value the word at `word` in `text` states for the
/// name `name` that starts at `name_start` ([`named_word`]; `prose`: stated
/// by a word, not an operator), with the words that continue it
/// ([`with_value_words`]); returns where it ends, `None` when it is none.
fn stated_value(
    text: &str,
    name: &str,
    name_start: usize,
    word: Range<usize>,
    prose: bool,
    found: &mut impl FnMut(Range<usize>, Range<usize>),
) -> Option<usize> {
    let own = named_word(name, text.get(word.clone())?, prose)?;
    let span = with_value_words(text, word.start + own.start..word.start + own.end);
    let end = span.end;
    found(name_start..end, span);
    Some(end)
}

/// Where the assignment operator after the name ending at `name_end` ends:
/// past its first `=` or `:` and the `=`, `>` or `!` it is spelled with.
fn assignment_end(text: &str, name_end: usize) -> Option<usize> {
    let rest = text.get(name_end..)?;
    let at = rest.find(['=', ':'])?;
    let after = rest.get(at..)?;
    let operator = after.len() - after.trim_start_matches(['=', ':', '>', '!']).len();
    Some(name_end + at + operator)
}

/// The name whose values a walk reads.
struct Named {
    /// Where it starts.
    at: usize,
    /// How far its line is indented.
    indent: usize,
    /// It names several credentials ([`names_several`]).
    several: bool,
}

/// What opens the values a name holds, and where their text starts.
enum Opener<'t> {
    /// A bracket at `at`.
    Bracket { at: usize },
    /// A YAML block scalar whose header (`|`, `>-`) starts at `header` and
    /// whose lines start at `from`.
    BlockScalar { header: usize, from: usize },
    /// A YAML block list whose first item's line starts at `from`.
    BlockList { from: usize },
    /// A triple-quoted string whose body starts at `from`.
    Triple { quote: &'t str, from: usize },
    /// A heredoc closed by `tag`: its body starts at `from`, or when the
    /// opener's line goes on past it (`rest`), on the line after.
    Heredoc {
        tag: &'t str,
        from: usize,
        rest: bool,
    },
    /// A string continued over the lines from `from` to its closing quote.
    Continued { from: usize },
    /// The expression assigned to the name, from `from` to the end of its
    /// statement; `key` is the name and its operator.
    Expression { from: usize, key: &'t str },
}

impl<'t> Opener<'t> {
    /// The opener the pattern's `open` group spells at `range`.
    fn of(open: &'t str, range: Range<usize>) -> Option<Self> {
        if open.ends_with(['[', '(', '{']) {
            return Some(Self::Bracket { at: range.end - 1 });
        }
        if open.starts_with(['|', '>']) {
            return Some(Self::BlockScalar {
                header: range.start,
                from: range.end,
            });
        }
        if let Some(tag) = open.strip_prefix("<<") {
            let tag = tag.trim_start_matches(['-', '~']).trim_matches(QUOTES);
            return Some(Self::Heredoc {
                tag,
                from: range.end,
                rest: true,
            });
        }
        if open.starts_with("\"\\") {
            return Some(Self::Continued { from: range.end });
        }
        if let Some(quote) = ["\"\"\"", "'''"].into_iter().find(|q| open.ends_with(q)) {
            return Some(Self::Triple {
                quote,
                from: range.end,
            });
        }
        let line = open.find('\n')? + 1;
        Some(Self::BlockList {
            from: range.start + line,
        })
    }
}
