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

use super::credential_values::{OPAQUE_LITERAL_CHARS, QUOTES, ValueTest};

mod names;
mod opaque;
mod text;
mod walk;

use names::calls_with_credentials;
use names::follows_a_public_word;
use names::names_several;
use text::ends_a_line;
use names::holds_more_than_a_nonce;
pub(super) use names::{
    describes_a_credential, is_nonce_shape, is_user_agent_nonce, names_a_credential,
    names_a_published_value, names_a_user_agent_field, spells_a_credential_word,
};
pub(super) use opaque::is_opaque;
pub(super) use text::{field_end, field_end_after};
use walk::Walk;

/// The pattern this module judges.
pub(super) const PATTERN: &str = "credentialNamedValue";

/// The words that state a name's value in a sentence, in place of an
/// operator (`the password is …`, `my password was …`), a colon after them
/// or not (`the password is: …`).
pub(super) const STATES_A_VALUE: [&str; 2] = ["is", "was"];

/// Whether `word`, the word after a credential's name in a sentence, states
/// the name's value ([`STATES_A_VALUE`], in any case, a colon after it or not).
pub(super) fn states_a_value(word: &str) -> bool {
    let word = word.strip_suffix(':').unwrap_or(word);
    STATES_A_VALUE
        .iter()
        .any(|states| word.eq_ignore_ascii_case(states))
}

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
/// its operator or [`STATES_A_VALUE`], that is a credential, by byte position
/// in `word`: the word without the quotes around it and the marks that close
/// a sentence after it, up to a field's escape ([`field_end`]), when it is
/// opaque ([`is_opaque_word`]), a value the key-name rules keep as a
/// credential ([`ValueTest::KeyName`]), and no nonce a user agent fills
/// ([`is_user_agent_nonce`]). This is [`ValueTest::Named`] without the length
/// a loose layout's run asks: a request and a sentence of a file judge a
/// named value by it alike.
pub(super) fn named_word(name: &str, word: &str) -> Option<Range<usize>> {
    let opened = word.trim_start_matches(QUOTES);
    let own = opened
        .trim_end_matches(QUOTES)
        .trim_end_matches(SENTENCE_CLOSERS)
        .trim_end_matches(QUOTES);
    let own = own.get(..field_end(own).unwrap_or(own.len()))?;
    let at = word.len() - opened.len();
    (is_opaque_word(own)
        && !super::is_placeholder(own)
        && ValueTest::KeyName.redacts(name, own)
        && !is_user_agent_nonce(name, own))
    .then_some(at..at + own.len())
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
            if let Some(own) = named_word(spelled, stated.as_str()) {
                let span = stated.start() + own.start..stated.start() + own.end;
                from = span.end;
                found(name.start()..span.end, span);
            }
            continue;
        }
        if let Some(secret) = secret {
            if let Some(span) = walk.scalar(name.start(), secret.range()) {
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
        let expression = captures
            .get(groups.expr)
            .map(|expr| expr.start())
            .or_else(|| secret.and_then(|_| assignment_end(text, name.end())));
        if let Some(start) = expression {
            let key = walk
                .path_start(name.start())
                .and_then(|path| text.get(path..start))
                .unwrap_or_default();
            let named = Named {
                at: name.start(),
                indent: 0,
                several: false,
            };
            let opener = Opener::Expression { from: start, key };
            if let Some(end) = walk.values(&opener, &named, &mut found) {
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
        if let Some(end) = walk.values(&opener, &named, &mut found) {
            from = from.max(end);
        }
    }
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
    /// A YAML block scalar whose lines start at `from`.
    BlockScalar { from: usize },
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
            return Some(Self::BlockScalar { from: range.end });
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
