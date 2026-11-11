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

use super::credential_values::{OPAQUE_LITERAL_CHARS, QUOTES};

mod names;
mod opaque;
mod text;
mod walk;

use names::calls_with_credentials;
use names::follows_a_public_word;
use names::names_several;
pub(super) use names::{
    describes_a_credential, names_a_credential, names_a_published_value, spells_a_credential_word,
};
pub(super) use opaque::is_opaque;
use walk::Walk;

/// The pattern this module judges.
pub(super) const PATTERN: &str = "credentialNamedValue";

/// The capture groups the pattern names.
struct Groups {
    name: usize,
    tag: usize,
    open: usize,
    secrets: Vec<usize>,
}

impl Groups {
    fn of(regex: &Regex) -> Option<Self> {
        let index = |wanted: &str| regex.capture_names().position(|name| name == Some(wanted));
        Some(Self {
            name: index("name")?,
            tag: index("tag")?,
            open: index("open")?,
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
        // A token that happens to spell a credential word is no name. A
        // sign-in verb names none either: it takes credentials only as the
        // callee of a call, its arguments right after it.
        let called = text.as_bytes().get(name.end()) == Some(&b'(');
        let named =
            names_a_credential(name.as_str()) || (called && calls_with_credentials(name.as_str()));
        if !named || is_opaque(name.as_str(), OPAQUE_LITERAL_CHARS) {
            continue;
        }
        let line_start = walk.line_start_of(name.start());
        if follows_a_public_word(text.get(line_start..name.start()).unwrap_or_default()) {
            continue;
        }
        let secret = groups.secrets.iter().find_map(|&group| captures.get(group));
        if let Some(secret) = secret {
            if let Some(span) = walk.scalar(name.start(), secret.range()) {
                from = span.end;
                found(name.start()..span.end, span);
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
