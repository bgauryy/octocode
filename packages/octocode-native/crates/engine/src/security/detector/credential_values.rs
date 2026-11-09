//! Which values assigned to a credential-named key are credentials: a code
//! name, a file digest or a statement reading another name is kept.

use std::sync::LazyLock;

use super::PATTERNS;
use super::key_facts::{KeyFacts, SpanKeys};
use crate::security::patterns::CREDENTIAL_WORDS;

/// Patterns that take any value assigned to a credential-named key, each
/// with the test that tells which of those values are credentials; every
/// other pattern is [`ValueTest::Every`].
const CREDENTIAL_ASSIGNMENT_PATTERNS: &[(&str, ValueTest)] = &[
    ("envVarSecrets", ValueTest::Quoted),
    ("base64EncodedSecrets", ValueTest::KeyName),
    ("dotenvSecretAssignment", ValueTest::KeyName),
    ("dotenvFileCredential", ValueTest::FileClass),
    ("propertiesFileCredential", ValueTest::FileClass),
    ("yamlSecretValue", ValueTest::Scalar),
    ("dotnetConnectionStrings", ValueTest::Field),
];

/// The quotes a literal opens with: the text before a quoted value ends in one.
pub(super) const QUOTES: [char; 3] = ['"', '\'', '`'];

/// Which values of a pattern's matches are credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ValueTest {
    /// The pattern matches only credentials: every value is one.
    Every,
    /// Only the key's name says "credential", so a value that is not one is
    /// kept: a value that spells a code identifier
    /// (`"x-kubernetes-patch-merge-key": "resourceClaimName"`) names a field,
    /// and a hex digest keyed by a file path
    /// (`"bin/with_credentials.mjs": "<sha256>"`) is a checksum manifest entry.
    /// See [`assigns_a_credential`].
    KeyName,
    /// [`ValueTest::KeyName`] for a YAML scalar of any length, except the
    /// keywords YAML reads as a boolean or null ([`is_non_string_scalar`]),
    /// which hold no secret. A quoted scalar is also a [`ValueTest::Quoted`]
    /// literal: a template or a short value under an opaque-string key
    /// (`token`) stays.
    Scalar,
    /// A quoted literal, judged as [`ValueTest::KeyName`] and by its length:
    /// one of [`OPAQUE_LITERAL_CHARS`] or more under any credential word, a
    /// shorter one only as a chosen secret ([`is_chosen_secret`]).
    Quoted,
    /// The pattern applies only in a file class whose credential names hold
    /// credentials (the dotenv and properties class): a key with a credential segment
    /// ([`file_class_key_names_a_credential`]) holds one whatever its value
    /// spells (`DB_PASS=CowSaysMoo`, `DB_PASS=/hunter2`), unless the value
    /// is readable there ([`file_class_value_is_readable`]).
    FileClass,
    /// A field of a value's own grammar (a connection string's `Password=`):
    /// every value is a credential except a placeholder the program fills in
    /// ([`is_template`]: `{value}`, `${PASS}`).
    Field,
}

impl ValueTest {
    /// Whether a match that starts inside a comment of a source-code file,
    /// after other words of the comment (`x = 1; // the password: required`),
    /// is a credential. The comment is prose before it is a config file: the
    /// file class does not make any value under a credential name one
    /// ([`ValueTest::KeyName`] instead), and a value of letters alone is a
    /// word of the sentence, the deliberate miss of [`is_chosen_secret`] too. After a colon, several words or a backtick span is a sentence, not one chosen
    /// secret.
    pub(super) fn redacts_in_comment(self, key: &str, secret: &str) -> bool {
        self.redacts_in_comment_by(&SpanKeys::new(key).key(0), secret)
    }

    /// [`ValueTest::redacts_in_comment`] for a key known by its facts: the
    /// file class reads as [`ValueTest::KeyName`] there ([`ValueTest::redacts_by_name`]).
    pub(super) fn redacts_in_comment_by(self, key: &impl KeyFacts, secret: &str) -> bool {
        self.redacts_by_name(key, secret)
            && secret.chars().any(|c| !c.is_alphabetic())
            && !is_sentence(key, secret)
            && !secret.contains('`')
    }

    /// Whether [`ValueTest::redacts_in_comment_by`] can accept any value under
    /// `key`: its conjuncts that read no byte of the value. A key that names a
    /// locator holds no credential whatever its value ([`names_a_location`]),
    /// so a reader can skip the value of a key this rejects.
    pub(super) fn admits_key_in_comment(self, key: &impl KeyFacts) -> bool {
        self == Self::Every || !key.names_a_locator()
    }

    /// Whether a match in the text of a source-code file (a string, or a file
    /// that does not parse) is a credential: [`ValueTest::redacts`], except a
    /// value that ends a code member ([`ends_a_code_member`]). The text may
    /// hold a config file, but no config value ends as a member of the
    /// source's own code does: `key: string;` or `token: vi.fn(),` is code.
    pub(super) fn redacts_in_code_text(self, key: &str, secret: &str) -> bool {
        self.redacts(key, secret) && !ends_a_code_member(key, secret)
    }

    /// Whether the match whose value is `secret` is a credential. `key` is
    /// the line's text before the value (ending in the key, its quotes and
    /// its operator).
    pub(super) fn redacts(self, key: &str, secret: &str) -> bool {
        match self {
            Self::FileClass => match file_class_key(key) {
                FileClassKey::Credential => !file_class_value_is_readable(key, secret),
                FileClassKey::Qualified => {
                    !file_class_value_is_readable(key, secret) && !is_setting_value(secret)
                }
                FileClassKey::Other => false,
            },
            other => other.redacts_by_name(&SpanKeys::new(key).key(0), secret),
        }
    }

    /// Whether `secret` is a credential when only the key's name says so, the
    /// key given by its facts. The file class, which reads more of its key
    /// ([`ValueTest::redacts`]), is judged as [`ValueTest::KeyName`].
    fn redacts_by_name(self, key: &impl KeyFacts, secret: &str) -> bool {
        match self {
            Self::Every => true,
            // A connection string may quote a field's value (`Password="…"`):
            // the quotes framing it are the string's, not the value's.
            Self::Field => !is_template(secret.trim().trim_matches(QUOTES)),
            Self::KeyName | Self::FileClass => assigns_a_credential(key, secret),
            Self::Scalar => {
                assigns_a_credential(key, secret)
                    && !is_non_string_scalar(secret)
                    && (!key.ends_with_quote() || is_quoted_credential(key, secret))
            }
            Self::Quoted => {
                let value = joined_pieces(secret);
                if holds_interpolation(key, &value) {
                    return holds_opaque_literal(key, &value);
                }
                assigns_a_credential(key, &value)
                    && (literal_chars(&value) >= OPAQUE_LITERAL_CHARS
                        || is_chosen_secret(key, &value))
            }
        }
    }
}

/// Whether `secret` after `key` is a sentence: after `name:` several words
/// are prose (`password: must be at least 8 characters`); after `NAME=` they
/// are a value.
pub(super) fn is_sentence(key: &impl KeyFacts, secret: &str) -> bool {
    key.ends_with_colon() && secret.contains(char::is_whitespace)
}

/// Whether a literal interpolates an expression (`` `pi-${id}` ``,
/// `"${HOME}/x"`, and in a literal whose prefix interpolates,
/// [`opens_interpolation`], `f'{base}_x'` or `$"{key}"`): the program
/// computes its value, so its text spells no credential.
fn holds_interpolation(key: &impl KeyFacts, value: &str) -> bool {
    let hole = |marker: &str| {
        value
            .split_once(marker)
            .is_some_and(|(_, rest)| rest.contains('}'))
    };
    hole("${") || (key.opens_interpolation() && holds_format_hole(value))
}

/// Whether a literal that interpolates ([`holds_interpolation`]) spells a
/// credential in its text beside the holes: the program computes what the holes
/// hold, but a piece of text between them is written, so each piece is judged as
/// a literal of its own. A credential's piece is one opaque run of
/// [`OPAQUE_LITERAL_CHARS`] or more with no blank in it
/// (`f'Zq7x…{suffix}'`); a hole with no text beside it (`f'{base}'`) and
/// prose or a short word around one (`` `pi-${id}` ``) hold none.
fn holds_opaque_literal(key: &impl KeyFacts, value: &str) -> bool {
    outside_holes(key, value).into_iter().any(|piece| {
        let piece = piece.trim();
        literal_chars(piece) >= OPAQUE_LITERAL_CHARS
            && !piece.contains(char::is_whitespace)
            && assigns_a_credential(key, piece)
    })
}

/// The text of `value` outside its holes: `${…}` anywhere, and when the literal's
/// prefix interpolates ([`opens_interpolation`]) each `{…}` of
/// [`holds_format_hole`]'s reading, `{{` and `}}` being text.
fn outside_holes<'a>(key: &impl KeyFacts, value: &'a str) -> Vec<&'a str> {
    let format = key.opens_interpolation();
    let bytes = value.as_bytes();
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let inner = match (byte, bytes.get(at + 1)) {
            (b'{' | b'}', Some(&next)) if format && next == byte => {
                at += 2;
                continue;
            }
            (b'$', Some(b'{')) => at + 2,
            (b'{', Some(&next)) if format && next != b'}' => at + 1,
            _ => {
                at += 1;
                continue;
            }
        };
        match bytes
            .get(inner..)
            .and_then(|rest| rest.iter().position(|b| *b == b'}'))
        {
            Some(close) => {
                pieces.push(value.get(start..at).unwrap_or_default());
                at = inner + close + 1;
                start = at;
            }
            None => at += 1,
        }
    }
    pieces.push(value.get(start..).unwrap_or_default());
    pieces
}

/// Whether the literal `key` opens has a prefix that interpolates its `{…}`
/// holes: Python's `f` (`f'`, `rf"`, `Fr'`) or C#'s `$` (`$"`, `@$"`), the
/// prefix written right after an operator, a blank or an opening bracket.
/// `key` is the text before the value, ending in the opening quote.
pub(super) fn opens_interpolation(key: &str) -> bool {
    let Some(opened) = key.strip_suffix(QUOTES) else {
        return false;
    };
    let before =
        opened.trim_end_matches(|c: char| c.is_ascii_alphabetic() || matches!(c, '$' | '@'));
    let prefix = opened.get(before.len()..).unwrap_or_default();
    (1..=3).contains(&prefix.len())
        && prefix.contains(['f', 'F', '$'])
        && before.ends_with(|c: char| {
            c.is_whitespace() || matches!(c, '=' | ':' | '>' | '(' | ',' | '|' | '?' | '[' | '{')
        })
}

/// Whether `value` holds a format hole: a lone `{`, something, then a `}`.
/// `{{` and `}}` write a brace, so each pair is stepped over as text
/// (`{{Zq7x…}}` is no hole, `{{{base}}}` holds one).
fn holds_format_hole(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        match (byte, bytes.get(at + 1)) {
            (b'{', Some(b'{')) | (b'}', Some(b'}')) => at += 2,
            (b'{', Some(&next)) if next != b'}' => {
                return bytes.get(at + 2..).is_some_and(|rest| rest.contains(&b'}'));
            }
            _ => at += 1,
        }
    }
    false
}

/// Whether a file-class `secret` ends one member of code rather than a config
/// line: one `;` or `,` closes a type, a name or a call of no arguments
/// (`string;`, `Vault;`, `vi.fn(),`), each dotted part a name
/// ([`is_identifier_word`]) or a short word of letters, or a quoted literal
/// that is no credential as a literal ([`ValueTest::Quoted`]: `'token',`). A
/// value with a digit or a symbol inside (`Fake0Moo9Zed;`) is none of these.
fn ends_a_code_member(key: &str, secret: &str) -> bool {
    let Some(body) = secret.trim_end().strip_suffix([';', ',']) else {
        return false;
    };
    let body = body.trim_end();
    if let Some(literal) = QUOTES
        .into_iter()
        .find_map(|quote| body.strip_prefix(quote)?.strip_suffix(quote))
    {
        return !ValueTest::Quoted.redacts_by_name(&SpanKeys::new(key).key(0), literal);
    }
    let name = body.strip_suffix("()").unwrap_or(body);
    !name.is_empty()
        && name.split('.').all(|part| {
            is_identifier_word(part)
                || ((1..=4).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_alphabetic()))
        })
}

/// Whether `secret` is a keyword YAML reads as a boolean or null, not a
/// string: `true`, `false`, `null` or `~`, in any of YAML's spellings.
fn is_non_string_scalar(secret: &str) -> bool {
    secret == "~"
        || ["true", "false", "null"]
            .iter()
            .any(|keyword| secret.eq_ignore_ascii_case(keyword))
}

/// Index-aligned with PATTERNS: the value test of each pattern's matches.
pub(super) static VALUE_TESTS: LazyLock<Vec<ValueTest>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .map(|p| {
            CREDENTIAL_ASSIGNMENT_PATTERNS
                .iter()
                .find(|(name, _)| *name == p.name)
                .map_or(ValueTest::Every, |&(_, test)| test)
        })
        .collect()
});

/// Whether a credential-assignment match assigns a credential when only its
/// key's name says so: its value says where something lives
/// ([`names_a_location`]) or spells no credential: an identifier name
/// ([`is_identifier_name`]), a hex digest keyed by a file path
/// ([`is_file_digest`]), a statement reading another name
/// ([`reads_a_name`]), a placeholder to fill in ([`is_fill_in`]), an
/// expression of code ([`is_code_expression`]), or under an opaque-string
/// key a phrase of plain words ([`is_plain_phrase`]), a path of names
/// ([`is_key_path`]) or a versioned name ([`is_versioned_name`]).
fn assigns_a_credential(key: &impl KeyFacts, secret: &str) -> bool {
    let secret = without_control_frame(secret);
    !names_a_location(key, secret)
        && !is_identifier_name(secret)
        && !is_fill_in(key, secret)
        && !is_code_expression(key, secret)
        && !(is_versioned_name(secret) && !key.names_a_chosen_credential())
        && !is_file_digest(key, secret)
        && !reads_a_name(key, secret)
        && !is_type_expression(secret)
        && !is_plain_phrase(key, secret)
        && !(is_constant_name(secret) && !key.names_a_chosen_credential())
        && !is_armor_line(secret)
        && !is_timestamp(secret)
        && !(is_word_label(secret) && !key.names_a_chosen_credential())
        && !((is_key_path(secret) || is_namespaced_name(secret))
            && !key.names_a_chosen_credential())
}

/// Whether `secret` is a placeholder a person fills in: plain words
/// ([`is_plain_word`]) joined by `-`, `_` or blanks that say so, `your` and
/// `here` among them (`tvly-YOUR-KEY-HERE`, `your_api_key_here`), or under an
/// opaque-string key plain words in angle brackets (`<your-token-here>`,
/// `<API_KEY>`), unless they are one long word naming no credential
/// ([`is_lone_opaque_word`]). A placeholder's word is written as a word is: lower, UPPER
/// or Capitalized. A piece with a digit, an unpronounceable run or mixed case
/// is a real secret's, pasted into the template (`your-Hunter2-here`,
/// `your-Zq7x…-key-here`, `your-HuNtEr…-here`), so the value stays a
/// credential. Under a key naming a chosen credential every piece names what
/// to fill in, a short word or one naming a credential
/// ([`names_a_credential_word`]: `your-password-here`): other words there are
/// the password a person chose (`your-correct-horse-battery-staple-here`). A
/// person may choose bracketed words (`<open sesame door>`), so a key naming
/// a chosen credential keeps them.
fn is_fill_in(key: &impl KeyFacts, secret: &str) -> bool {
    let chosen = key.names_a_chosen_credential();
    let written_as_a_word = |word: &str| {
        word.bytes().skip(1).all(|byte| byte.is_ascii_lowercase())
            || word.bytes().all(|byte| byte.is_ascii_uppercase())
    };
    let names_what_to_fill =
        |word: &str| word.len() <= 4 || names_a_credential_word(&word.to_ascii_lowercase());
    let plain_words = |text: &str| {
        let mut words = text
            .split(['-', '_', ' '])
            .filter(|word| !word.is_empty())
            .peekable();
        words.peek().is_some()
            && words.all(|word| {
                is_plain_word(word)
                    && written_as_a_word(word)
                    && (!chosen || names_what_to_fill(word))
            })
    };
    let says = |word: &str| {
        secret
            .split(['-', '_', ' '])
            .any(|piece| piece.eq_ignore_ascii_case(word))
    };
    let bracketed =
        angle_inner(secret).is_some_and(|inner| plain_words(inner) && !is_lone_opaque_word(inner));
    (bracketed && !chosen) || (plain_words(secret) && says("your") && says("here"))
}

/// The text between `<` and `>` when `secret` is all of it.
fn angle_inner(secret: &str) -> Option<&str> {
    secret.strip_prefix('<')?.strip_suffix('>')
}

/// Whether the text inside angle brackets is one word of letters that names no
/// credential and, brackets with it, is a literal of [`OPAQUE_LITERAL_CHARS`]
/// or more (`<Kavotemixalupo>`): a placeholder says what to fill in, by several
/// words (`<your-token-here>`) or by a credential's name (`<password>`), and
/// one long word says nothing, so it is a secret a person wrapped in brackets.
fn is_lone_opaque_word(inner: &str) -> bool {
    inner.bytes().all(|byte| byte.is_ascii_alphabetic())
        && inner.len() + 2 >= OPAQUE_LITERAL_CHARS
        && !names_a_credential_word(&inner.to_ascii_lowercase())
}

/// Whether `secret` is a type or member name with a version after it
/// (`GithubTokenProviderV2`): a camelCase name ([`is_identifier_name`]) of
/// [`OPAQUE_LITERAL_CHARS`] or more, then `V` and up to three digits. A
/// version adds no evidence that a value is a name, so the name must be one
/// at a credential literal's length on its own, as a bare name is judged; a
/// shorter one lifted past it by its version (`GyyiXidMolckV598`) is a
/// random token's shape. A name and a bare digit (`resourceClaimName7`) is a
/// token's shape too, and a person may choose words and a version, so a key
/// naming a chosen credential keeps it ([`assigns_a_credential`]).
fn is_versioned_name(secret: &str) -> bool {
    let stem = secret.trim_end_matches(|c: char| c.is_ascii_digit());
    let digits = secret.len() - stem.len();
    (1..=3).contains(&digits)
        && stem
            .strip_suffix('V')
            .is_some_and(|name| name.len() >= OPAQUE_LITERAL_CHARS && is_identifier_name(name))
}

/// Whether `secret` is an expression of code (`resultCount===0&&Result.isErr`,
/// `ctx.accessToken/other.accessToken-1`): pieces joined by operators, each
/// a number or a path of names ([`is_name_segment`] parts), two of them a
/// member path (`a.b`) or a camelCase name. A credential's pieces hold
/// digits inside or unpronounceable runs (`ctx.Zq7x…`). Under a key naming
/// a chosen credential the pieces are joined by a comparison or a logical
/// operator (`==`, `!=`, `&&`, `||`), as a password of words and dots
/// (`hunter.two-fish.cake`) is not, and never when the value is quoted: a
/// literal under a chosen key is the value a person chose
/// (`password = "adminUser==rootPassword"`).
fn is_code_expression(key: &impl KeyFacts, secret: &str) -> bool {
    let body = secret.trim().trim_end_matches([';', ',']);
    let operator = |c: char| {
        matches!(
            c,
            '=' | '!' | '&' | '|' | '<' | '>' | '/' | '+' | '-' | '*' | '%' | '(' | ')' | '?' | ':'
        )
    };
    let mut pieces = 0;
    let mut names = 0;
    let all = body
        .split(operator)
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .all(|piece| {
            pieces += 1;
            if piece.bytes().all(|byte| byte.is_ascii_digit()) {
                return true;
            }
            let named = piece.split('.').all(is_name_segment);
            if named && (piece.contains('.') || is_identifier_name(piece)) {
                names += 1;
            }
            named
        });
    let joined = if key.names_a_chosen_credential() {
        // A quoted literal assigned to a chosen credential is a passphrase the
        // program stores, not code it runs.
        !key.ends_with_quote() && ["==", "!=", "&&", "||"].iter().any(|op| body.contains(op))
    } else {
        body.contains(operator)
    };
    all && joined && pieces >= 2 && names >= 2
}

/// `secret` without the control characters framing it, raw or written as a
/// literal's escape (`\0`, `\n`, `\x1f`, `\u001b`): they delimit a value
/// (`"\0octocode/serverStatus"`, a line's `\n`) and are no credential's
/// bytes, so the value between them is what is judged. An escaped backslash
/// (`\\0`) is text, not a frame.
fn without_control_frame(secret: &str) -> &str {
    let mut value = secret;
    loop {
        let trimmed = value.trim_matches(|c: char| c.is_ascii_control());
        let trimmed = control_escape_len(trimmed)
            .and_then(|len| trimmed.get(len..))
            .unwrap_or(trimmed);
        let trimmed = before_ending_control_escape(trimmed).unwrap_or(trimmed);
        if trimmed.len() == value.len() {
            return value;
        }
        value = trimmed;
    }
}

/// `text` before the control-character escape ending it; `None` when none
/// does, or when its backslash is the second of an escaped one (`\\n`).
fn before_ending_control_escape(text: &str) -> Option<&str> {
    [2, 4, 6].into_iter().find_map(|len| {
        let at = text.len().checked_sub(len)?;
        let (head, tail) = (text.get(..at)?, text.get(at..)?);
        let escaped = head.bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1;
        (control_escape_len(tail) == Some(len) && !escaped).then_some(head)
    })
}

/// The length of the control-character escape `text` starts with: `\0`,
/// `\n`, `\r`, `\t`, `\xHH` or `\u00HH` below 0x20.
fn control_escape_len(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('\\')?;
    let below_space = |hex: &str| u8::from_str_radix(hex, 16).is_ok_and(|code| code < 0x20);
    match rest.as_bytes().first()? {
        b'0' | b'n' | b'r' | b't' => Some(2),
        b'x' if rest.get(1..3).is_some_and(below_space) => Some(4),
        b'u' if rest.get(1..3) == Some("00") && rest.get(3..5).is_some_and(below_space) => Some(6),
        _ => None,
    }
}

/// Whether `secret` is a constant's or an environment variable's name
/// (`OCTOCODE_ASKPASS_TOKEN`, `GITHUB_CLIENT_ID`): two or more uppercase
/// words joined by `_`, each a name word ([`is_name_segment`]). A name that
/// holds a credential is spelled so, as a camelCase name is
/// ([`is_identifier_name`]); a token of capitals holds digits or
/// unpronounceable runs. A chosen password may be capital words so joined
/// (`CORRECT_HORSE_BATTERY_STAPLE`), so a key naming one keeps it.
fn is_constant_name(secret: &str) -> bool {
    secret.split('_').filter(|word| !word.is_empty()).count() >= 2
        && secret
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte == b'_')
        && is_name_segment(secret)
}

/// Whether `value` is a constant's name ([`is_constant_name`], framed or
/// not: [`without_control_frame`]) that its text expands as a variable:
/// `expanded` gives the names the text expands ([`expanded_names`]), read
/// only when `value` is such a name. The name is then the variable's: a
/// program that expands a variable names it, and a chosen password is never
/// also expanded as a variable in its own source. Without an expansion such a
/// name under a key naming a chosen credential stays one
/// ([`assigns_a_credential`]).
pub(super) fn is_expanded_name<'t>(
    expanded: impl FnOnce() -> &'t std::collections::HashSet<Box<str>>,
    value: &str,
) -> bool {
    let name = without_control_frame(value);
    is_constant_name(name) && expanded().contains(name)
}

/// The names `text` expands as a variable, each read whole in one pass:
/// `$NAME` (to the first byte that is no name byte: `$NAME_2` expands
/// `NAME_2`), `${NAME}` and `%NAME%`.
pub(super) fn expanded_names(text: &str) -> std::collections::HashSet<Box<str>> {
    let bytes = text.as_bytes();
    let name_end = |from: usize| {
        from + bytes
            .get(from..)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
            .count()
    };
    let mut names = std::collections::HashSet::new();
    for (at, mark) in text.match_indices(['$', '%']) {
        let (start, closer) = match mark {
            "$" if bytes.get(at + 1) == Some(&b'{') => (at + 2, Some(b'}')),
            "$" => (at + 1, None),
            _ => (at + 1, Some(b'%')),
        };
        let end = name_end(start);
        if end > start
            && closer.is_none_or(|closer| bytes.get(end) == Some(&closer))
            && let Some(name) = text.get(start..end)
            && !names.contains(name)
        {
            names.insert(name.into());
        }
    }
    names
}

/// Whether `secret` is an ISO 8601 date or date-time (`2000-01-01`,
/// `2000-01-01T00:00:00Z`): a moment, no credential.
fn is_timestamp(secret: &str) -> bool {
    let bytes = secret.as_bytes();
    bytes.len() >= 10
        && bytes[..10].iter().enumerate().all(|(at, byte)| match at {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
        && bytes[10..].iter().all(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'T' | b'Z' | b':' | b'.' | b'+' | b'-' | b' ')
        })
}

/// Whether `secret` is a label of two or more lowercase words joined by `-`
/// or `_` (`new-login-refresh`), each a pronounceable word and none a
/// credential word. Under a key naming an opaque string (`token`) such a
/// value names something; a chosen password may be words so joined, so a key
/// naming one keeps it. A label that names what it is (`unrelated-secret`,
/// `deploy-bot-token`) is how a person spells a static credential.
fn is_word_label(secret: &str) -> bool {
    secret.split(['-', '_']).count() >= 2
        && secret.split(['-', '_']).all(|word| {
            word.bytes().all(|b| b.is_ascii_lowercase())
                && pronounceable_word(word.as_bytes())
                && !CREDENTIAL_WORDS
                    .split('|')
                    .any(|credential| credential == word)
        })
}

/// Whether `secret` is one PEM armor line (`-----BEGIN RSA PRIVATE KEY-----`):
/// it names the block that follows and holds none of it. A block's body, or
/// an armor line with its body, is the key rules' own.
fn is_armor_line(secret: &str) -> bool {
    secret
        .strip_prefix("-----")
        .and_then(|rest| rest.strip_suffix("-----"))
        .and_then(|label| {
            label
                .strip_prefix("BEGIN ")
                .or_else(|| label.strip_prefix("END "))
        })
        .is_some_and(|label| {
            !label.is_empty() && label.bytes().all(|b| b.is_ascii_uppercase() || b == b' ')
        })
}

/// Whether `secret` is a name under a namespace (`rustAnalyzer/cachePriming`,
/// `app/db.host`): two or more `/`-separated parts, each a name or a key path
/// ([`is_key_path`]). Base64 holds `/` too, but its pieces mix case and
/// digits into no name.
fn is_namespaced_name(secret: &str) -> bool {
    secret.split('/').count() >= 2
        && secret
            .split('/')
            .all(|part| is_name_segment(part) || is_key_path(part) || is_domain_name(part))
}

/// Whether `part` is a domain written in lowercase letters (`io.modelcontextprotocol`,
/// `com.example`): two or more dotted labels of two or more letters each.
fn is_domain_name(part: &str) -> bool {
    part.split('.').count() >= 2
        && part
            .split('.')
            .all(|label| label.len() >= 2 && label.bytes().all(|b| b.is_ascii_lowercase()))
}

/// Whether `name` is a type's name: it starts with a letter or `_` and digits
/// only end it (`u8`, `Vec2`), so a word with a digit inside (`Moo9Zed`) is not.
fn is_type_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && !name
            .as_bytes()
            .windows(2)
            .any(|pair| pair[0].is_ascii_digit() && !pair[1].is_ascii_digit())
}

/// Whether `secret` is a generic type (`Arc<Mutex<Tokens>>,`,
/// `HashMap<String, Value>`): a name opening angle brackets that close again,
/// each part a name that starts with a letter or `_`, and at most a closing
/// `,` or `;`. A struct field `tokens: Arc<Mutex<Tokens>>,` in a fragment of
/// source the syntax cannot place has that shape and no credential's.
fn is_type_expression(secret: &str) -> bool {
    let body = secret.trim_end().trim_end_matches([',', ';']);
    let mut depth = 0usize;
    let mut name = String::new();
    let mut saw_brackets = false;
    let mut closed = false;
    for c in body.chars() {
        match c {
            '<' | '>' | ',' | ' ' => {
                if !name.is_empty() && !is_type_name(&name) {
                    return false;
                }
                if c == '<' {
                    if name.is_empty() {
                        return false;
                    }
                    depth += 1;
                    saw_brackets = true;
                    closed = false;
                } else if c == '>' {
                    let Some(rest) = depth.checked_sub(1) else {
                        return false;
                    };
                    depth = rest;
                    closed = true;
                } else if name.is_empty() && !closed && c == ',' {
                    return false;
                }
                name.clear();
            }
            c if c.is_ascii_alphanumeric() || c == '_' => {
                if closed {
                    return false;
                }
                name.push(c);
            }
            _ => return false,
        }
    }
    saw_brackets && depth == 0 && closed && name.is_empty()
}

/// Whether `secret` is a dotted path of two or more names, each with
/// optional `[n]` indices (`environment.GH_TOKEN.description`,
/// `servers[0].auth.refreshToken`): a key path, a member or a message key
/// (`matchKeys` rows, `secretKeyRef.key`, i18n). A name is a camelCase name
/// ([`is_identifier_name`]) or `_`-joined words of letters, each a
/// pronounceable word ([`pronounceable_word`]) or an abbreviation of at most
/// four letters (`GH`, `cfg`). API keys and tokens hold digits or
/// unpronounceable runs; a chosen password may be words and dots, so a key
/// naming one ([`names_a_chosen_credential`]) keeps it redacted.
fn is_key_path(secret: &str) -> bool {
    let name = |mut segment: &str| {
        while let Some(head) = segment.strip_suffix(']').and_then(|rest| {
            let (head, index) = rest.rsplit_once('[')?;
            (!index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())).then_some(head)
        }) {
            segment = head;
        }
        is_name_segment(segment)
    };
    secret.split('.').count() >= 2 && secret.split('.').all(name)
}

/// One name of [`is_key_path`]: a camelCase name ([`is_identifier_name`]) or
/// `_`-joined plain words ([`is_plain_word`]).
fn is_name_segment(segment: &str) -> bool {
    is_identifier_name(segment)
        || (segment.bytes().any(|byte| byte != b'_')
            && segment
                .split('_')
                .filter(|word| !word.is_empty())
                .all(is_plain_word))
}

/// Whether `word` is a plain word: letters only, and a pronounceable word
/// ([`pronounceable_word`], any case) or an abbreviation of at most four
/// letters (`GH`, `cfg`, `tvly`).
fn is_plain_word(word: &str) -> bool {
    word.bytes().all(|byte| byte.is_ascii_alphabetic())
        && (word.len() <= 4 || pronounceable_word(word.to_ascii_lowercase().as_bytes()))
}

/// What a file-class key says about its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FileClassKey {
    /// A credential word sits in the key's last dotted segment
    /// (`spring.datasource.password`, `app.keys[0]`): the value is a credential.
    Credential,
    /// A credential word sits only in an earlier segment
    /// (`db.password.value`, `PASSWORD.0`, `password.min.length`): a
    /// qualifier, a position or a setting of the credential, so the value is
    /// one unless it has a setting's shape ([`is_setting_value`]).
    Qualified,
    /// No segment names a credential.
    Other,
}

/// Whether lowercase `text` contains a credential word ([`CREDENTIAL_WORDS`])
/// anywhere, with no separator needed (`dbpassword`, `apikey`).
fn names_a_credential_word(text: &str) -> bool {
    CREDENTIAL_WORDS.split('|').any(|word| text.contains(word))
}

/// Which [`FileClassKey`] a key (the line's text before the value) is. A
/// segment names a credential when it contains a credential word anywhere, in
/// any case and with no separator (`DB_PASS_PROD`, `dbPassword`, `APIKEY`,
/// `KEYSTOREPASS`). No exclusion list: in a dotenv file `MONKEY` or
/// `BYPASS_CACHE` fails closed rather than let a compound credential name
/// through.
fn file_class_key(key: &str) -> FileClassKey {
    let name = key_name(key).to_ascii_lowercase();
    let mut segments = name.rsplit('.');
    if segments.next().is_some_and(names_a_credential_word) {
        FileClassKey::Credential
    } else if segments.any(names_a_credential_word) {
        FileClassKey::Qualified
    } else {
        FileClassKey::Other
    }
}

/// Whether `secret` has the shape of a setting rather than a chosen
/// credential: a boolean or a number (`true`, `8`, `0.5`, `-1`).
fn is_setting_value(secret: &str) -> bool {
    let value = secret.trim();
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("false")
        || (!digits.is_empty()
            && digits
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
            && digits.bytes().filter(|byte| *byte == b'.').count() <= 1
            && digits.bytes().any(|byte| byte.is_ascii_digit()))
}

/// Whether a file-class value under a credential name is no credential:
/// empty, a reference to another variable or key without a default
/// (`${NAME}`, `${app.db.password}`, or `$NAME` in env-var shape; a
/// single-quoted value is a literal, and `${NAME:default}` holds a default
/// that may be the secret), the tool's own redaction marker, or
/// a value whose key or option names a location (`TOKEN_FILE=/run/token`,
/// `--token-file=/x`). A path is no locator by its shape alone, and a
/// bracketed value is no placeholder: either may be a password.
fn file_class_value_is_readable(key: &str, secret: &str) -> bool {
    let value = secret.trim();
    let reference = !key.ends_with('\'')
        && (value
            .strip_prefix("${")
            .and_then(|rest| rest.strip_suffix('}'))
            .is_some_and(|name| is_reference_name(name, true))
            || value
                .strip_prefix('$')
                .is_some_and(|name| is_reference_name(name, false)));
    value.is_empty()
        || reference
        || is_redaction_marker(value)
        || is_type_expression(value)
        || names_a_locator(key)
        || option(value).is_some_and(|(name, _)| names_a_locator(name))
}

/// Whether `value` is a redaction marker, bare or inside the quotes a
/// quote-aware rule left around it (`"[REDACTED-SPRINGBOOTSECRETS]"`): text
/// this tool wrote is no credential, whichever rule reads the line next.
fn is_redaction_marker(value: &str) -> bool {
    let inner = QUOTES
        .into_iter()
        .find_map(|quote| value.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(value);
    crate::security::placeholder_regex()
        .find(inner)
        .is_some_and(|marker| marker.range() == (0..inner.len()))
}

/// Whether `name` is what a reference to another variable or key holds, and
/// nothing an expansion could read as an operator (`-`, `:`, `+`, `=`, `?`
/// open a default, which may be the secret): a letter or `_`, then letters,
/// digits or `_`. A braced reference (`${app.db.password}`) may also dot its
/// key; a bare `$NAME` stops at the dot and, to be no password, is
/// uppercase only.
fn is_reference_name(name: &str, braced: bool) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.bytes().all(|byte| {
            byte == b'_'
                || byte.is_ascii_digit()
                || (byte == b'.' && braced)
                || (byte.is_ascii_alphabetic() && (braced || byte.is_ascii_uppercase()))
        })
}

/// Whether an assignment says where something lives rather than holding it:
/// its key names a locator ([`names_a_locator`]) or its value is a path
/// ([`is_path_locator`]).
fn names_a_location(key: &impl KeyFacts, secret: &str) -> bool {
    key.names_a_locator() || is_path_locator(secret)
}

/// Whether `secret` says where something lives rather than holding it: an
/// absolute (`/run/secrets/db`), home (`~/.ssh/id_ed25519`) or explicitly
/// relative (`./certs/key.pem`) path, or a command-line option assigning one
/// (`--token-file=/var/run/token`). A locator's value is no credential, as a
/// locator key's is not ([`names_a_locator`]). Every segment reads as a path
/// segment: lowercase letters, digits, `.`, `_` and `-`, or one capitalized
/// word (`/Users/Shared`). Base64 can start with `/`, but its mixed-case,
/// `+`-laden pieces read as neither, so an encoded secret stays redacted.
fn is_path_locator(secret: &str) -> bool {
    let path = option_value(secret).unwrap_or(secret);
    let Some(rest) = ["/", "~/", "./", "../"]
        .iter()
        .find_map(|root| path.strip_prefix(root))
    else {
        return false;
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    !rest.is_empty()
        && rest.split('/').all(|segment| {
            (!segment.is_empty()
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-')
                }))
                || pronounceable_word(segment.as_bytes())
        })
}

/// The value of a command-line option that assigns one (`--name=value`,
/// `-name=value`): `None` for any other text.
fn option_value(text: &str) -> Option<&str> {
    option(text).map(|(_, value)| value)
}

/// The name and value of a command-line option that assigns one.
fn option(text: &str) -> Option<(&str, &str)> {
    let option = text.strip_prefix("--").or_else(|| text.strip_prefix('-'))?;
    let (name, value) = option.split_once('=')?;
    (name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
    .then_some((name, value))
}

/// Locator fields describe where data lives, not the data itself. Recognizable
/// credentials inside a locator still match the independent token/URL rules.
///
/// A locator name ends in one of [`LOCATOR_SUFFIXES`], a whole word of it: the
/// name is only that suffix, or a separator or a capital begins it. Only the
/// end of `name` is read, so a name cut at its start (the later keys of one
/// path, [`SpanKeys`]) is judged without reading what it lost.
pub(super) fn ends_in_locator(name: &[u8]) -> bool {
    LOCATOR_SUFFIXES.iter().any(|suffix| {
        let Some(prefix_len) = name.len().checked_sub(suffix.len()) else {
            return false;
        };
        name[prefix_len..].eq_ignore_ascii_case(suffix.as_bytes())
            && (prefix_len == 0
                || matches!(name[prefix_len - 1], b'_' | b'-' | b'.')
                || name[prefix_len].is_ascii_uppercase())
    })
}

/// The names a locator field ends in.
const LOCATOR_SUFFIXES: [&str; 15] = [
    "url",
    "urls",
    "uri",
    "uris",
    "path",
    "paths",
    "dir",
    "dirs",
    "directory",
    "directories",
    "folder",
    "folders",
    "file",
    "files",
    "home",
];

/// Whether the key's name is a locator ([`ends_in_locator`]).
fn names_a_locator(key: &str) -> bool {
    ends_in_locator(key_name(key).as_bytes())
}

/// Credential words that name a value a person chooses: a phrase, or a short
/// password. `pass` alone is not one (`bypass`, `passport`), nor `pwd` (a
/// working directory).
pub(super) const CHOSEN_CREDENTIAL_WORDS: &[&str] = &["secret", "password", "passwd", "passphrase"];

/// Characters of a quoted literal that is a credential under any credential
/// word (`key`, `token`): shorter ones there are names and test strings.
const OPAQUE_LITERAL_CHARS: usize = 16;

/// Characters of a quoted literal's source text, an escape (`\\` and the
/// character after it) counting as one.
fn literal_chars(secret: &str) -> usize {
    let mut chars = secret.chars();
    let mut count = 0;
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            chars.next();
        }
        count += 1;
    }
    count
}

/// Whether the literal after `key` backs up a read (`process.env.DB_PASS || "…"`,
/// `os.getenv("DB_PASS", "…")`): it is the read's default, a chosen secret
/// when any name on the line says so, the declared one (`password`) or the
/// variable's, where a `pass` segment (`DB_PASS`) counts too
/// ([`SpanKeys`]). Only the end of the key says it reads.
pub(super) fn is_fallback_read(key: &str) -> bool {
    let head = key.trim_end_matches(|c: char| c.is_whitespace() || QUOTES.contains(&c));
    let head = head
        .trim_end_matches('#')
        .trim_end_matches(|c: char| c.is_ascii_alphabetic());
    ["||", "??", ","]
        .iter()
        .any(|operator| head.trim_end().ends_with(operator))
        || key
            .trim_end_matches(|c: char| c.is_whitespace() || QUOTES.contains(&c))
            .ends_with(" or")
}

/// `key` less the prefix of the literal it ends in (`r#"`, `b"`): the letters
/// and `#` after an operator are the literal's, not the name's.
fn without_literal_prefix(key: &str) -> &str {
    let Some(opened) = key.strip_suffix(QUOTES) else {
        return key;
    };
    let hashes = opened.trim_end_matches('#');
    let before = hashes.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let letters = hashes.len() - before.len();
    if (1..=2).contains(&letters)
        && before.ends_with(|c: char| {
            c.is_whitespace() || matches!(c, '=' | ':' | '>' | '(' | ',' | '|' | '?')
        })
    {
        before
    } else {
        key
    }
}

/// The value of a literal written in pieces (`Fake0" + 'Moo9Zed`, `Fake0" "Moo9Zed`, the text
/// between the first opening quote and the last closing one): the pieces
/// joined, as the program reads them. Any other text is its own value.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are `find` results of an ASCII quote, one byte wide"
    )
)]
fn joined_pieces(secret: &str) -> String {
    let mut value = String::with_capacity(secret.len());
    let mut rest = secret;
    while let Some(at) = rest.find(QUOTES) {
        value.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let seam = after.trim_start();
        let next = seam.strip_prefix('+').map_or(seam, str::trim_start);
        match next.strip_prefix(QUOTES) {
            Some(piece) => rest = piece,
            None => {
                value.push_str(&rest[at..=at]);
                rest = after;
            }
        }
    }
    value.push_str(rest);
    value
}

/// Whether a quoted literal shorter than [`OPAQUE_LITERAL_CHARS`] is a
/// secret a person chose (D6 round 4): its key names one
/// ([`CHOSEN_CREDENTIAL_WORDS`]), and the value has no whitespace, holds a
/// digit or a symbol (not `_`, `.` or `-`: a letters-only word is the one
/// deliberate miss) and is no template (`<…>`, `${…}`, `{{…}}`).
fn is_chosen_secret(key: &impl KeyFacts, secret: &str) -> bool {
    key.names_a_chosen_credential()
        && !is_template(secret)
        && !secret.contains(char::is_whitespace)
        && secret
            .chars()
            .any(|c| c.is_ascii_digit() || !(c.is_alphanumeric() || matches!(c, '_' | '.' | '-')))
}

/// Whether `secret` is a placeholder to fill in: `<…>`, `${…}`, `{{…}}` or a
/// format argument (`{value}`, `{0}`: a name or a position in braces).
fn is_template(secret: &str) -> bool {
    angle_inner(secret).is_some_and(|inner| !is_lone_opaque_word(inner))
        || (secret.starts_with("${") && secret.ends_with('}'))
        || (secret.starts_with("{{") && secret.ends_with("}}"))
        || secret
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
            .is_some_and(|name| {
                !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
}

/// Whether a quoted YAML scalar of any length is a credential (B157): no
/// template, and either [`OPAQUE_LITERAL_CHARS`] or more under any credential
/// word or, shorter, under a key naming a chosen secret (`password`).
fn is_quoted_credential(key: &impl KeyFacts, secret: &str) -> bool {
    !is_template(secret)
        && (literal_chars(secret) >= OPAQUE_LITERAL_CHARS || key.names_a_chosen_credential())
}

/// Marks a phrase may put around a word: punctuation outside every token
/// alphabet. Base64's `+/=` and base64url's `-_` are not marks, so trimming
/// never turns an encoded token's piece into a word.
const WORD_MARKS: &[char] = &[
    '.', ',', ';', ':', '!', '?', '(', ')', '[', ']', '{', '}', '<', '>', '*', '#',
];

/// Whether `secret` is a phrase of plain words (`meta form value 1`,
/// `Enter your API key here`, `!!ALREADY FORMATTED!!`) assigned to a key whose credential word names
/// an opaque machine string: `key`, `token` or `credential`. API keys,
/// tokens and credential strings never put spaces between dictionary
/// words, so such a phrase is a label, a placeholder or test data. A
/// `password`, `passphrase` or `secret` may be a phrase a person chose
/// (`correct horse battery staple`), so a key naming one keeps its phrase
/// redacted. A plain word is a pronounceable word ([`pronounceable_word`],
/// any case) or a number of at most four digits, either within
/// [`WORD_MARKS`]; at least two words and no other piece (`Bearer <token>` keeps
/// its token piece, so it stays a credential).
fn is_plain_phrase(key: &impl KeyFacts, secret: &str) -> bool {
    if key.names_a_chosen_credential() {
        return false;
    }
    let mut words = 0;
    let mut pieces = 0;
    let plain = secret.split_whitespace().all(|piece| {
        pieces += 1;
        let piece = piece.trim_matches(WORD_MARKS);
        if !piece.is_empty() && piece.len() <= 4 && piece.bytes().all(|byte| byte.is_ascii_digit())
        {
            return true;
        }
        words += 1;
        // A word may join words (`re-authenticate`, `private-key`) or mark
        // a plural (`block(s)`): each part is a word or one letter.
        let piece = piece.to_ascii_lowercase();
        piece
            .split(['-', '(', ')', '\''])
            .filter(|part| !part.is_empty())
            .all(|part| {
                pronounceable_word(part.as_bytes())
                    || (part.len() == 1 && part.bytes().all(|b| b.is_ascii_alphabetic()))
            })
            && piece.bytes().any(|b| b.is_ascii_alphabetic())
    });
    plain && pieces >= 2 && words >= 2
}

/// The key a credential assignment names: the last word of the line's text
/// before the value, without its quotes and operator (`"db.password": ` →
/// `db.password`). In a typed declaration (`const PASSWORD: &str = `) the
/// key is the name before the type's colon, not the type: text after a colon
/// that holds a quote or brace is another entry's, not a type.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "`at` is the index of an ASCII colon and `colon` the same"
    )
)]
pub(super) fn key_name(key: &str) -> &str {
    let operators = |c: char| {
        c.is_whitespace() || QUOTES.contains(&c) || matches!(c, ':' | '=' | '>' | '|' | '?')
    };
    let head = without_literal_prefix(key).trim_end_matches(operators);
    // A constructor call that wraps the literal (`token: Some(`,
    // `String::from(`) is the value's, not the name's.
    let head = head.strip_suffix('(').map_or(head, |call| {
        call.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == ':')
            .trim_end_matches(operators)
    });
    let bytes = head.as_bytes();
    let name = (0..bytes.len())
        .rev()
        .find(|&at| {
            bytes[at] == b':'
                && at.checked_sub(1).is_none_or(|before| bytes[before] != b':')
                && bytes.get(at + 1) != Some(&b':')
                && !head[at..].contains(|c: char| QUOTES.contains(&c) || matches!(c, '{' | '}'))
        })
        .map_or(head, |colon| &head[..colon]);
    name.rsplit(|c: char| c.is_whitespace() || QUOTES.contains(&c) || matches!(c, '{' | ',' | '('))
        .next()
        .unwrap_or_default()
}

/// Whether an unquoted `secret` opens with a code statement that assigns
/// another name: an optional `!`, `&` or `*`, a dotted path of identifier
/// words ([`is_identifier_word`], or one letter, as minified code names
/// one) and the `;` terminator, at the end of the
/// value (`frame.expect_key = frame.object;`) or before the next statement
/// of minified code (`x.password=t.password;username=u`). Dotenv, INI,
/// properties and shell values carry no statement terminator, and a quoted
/// value (`key` ends in its opening quote) is a literal whatever it spells.
fn reads_a_name(key: &impl KeyFacts, secret: &str) -> bool {
    !key.ends_with_quote()
        && secret.split_once(';').is_some_and(|(name, rest)| {
            (rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'))
                && name
                    .trim_start_matches(['!', '&', '*'])
                    .split('.')
                    .all(|segment| {
                        is_identifier_word(segment)
                            || (segment.len() == 1
                                && segment.bytes().all(|byte| byte.is_ascii_alphabetic()))
                    })
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
fn is_file_digest(key: &impl KeyFacts, secret: &str) -> bool {
    matches!(secret.len(), 32 | 40 | 64 | 128)
        && secret.bytes().all(|byte| byte.is_ascii_hexdigit())
        && key.names_a_file()
}

/// Whether the last segment of `path` is a file name with an extension: a
/// stem, then a dot and up to 10 ASCII letters or digits, a letter among them.
pub(super) fn has_file_extension(path: &str) -> bool {
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
pub(super) fn is_identifier_name(value: &str) -> bool {
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
