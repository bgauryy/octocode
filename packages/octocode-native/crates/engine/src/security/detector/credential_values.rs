//! Which values assigned to a credential-named key are credentials: a code
//! name, a file digest or a statement reading another name is kept.

use std::sync::LazyLock;

use super::PATTERNS;
use super::key_facts::{KeyFacts, SpanKeys};
use super::named_values::{
    is_nonce_shape, is_opaque, names_a_credential, spells_a_credential_word,
};
use crate::security::patterns::CREDENTIAL_WORDS;

/// Patterns that take any value assigned to a credential-named key, each
/// with the test that tells which of those values are credentials; every
/// other pattern is [`ValueTest::Every`], or [`ValueTest::Placed`] when it
/// names its value in a `secret` group.
const CREDENTIAL_ASSIGNMENT_PATTERNS: &[(&str, ValueTest)] = &[
    ("envVarSecrets", ValueTest::Quoted),
    ("base64EncodedSecrets", ValueTest::KeyName),
    ("dotenvSecretAssignment", ValueTest::KeyName),
    ("dotenvFileCredential", ValueTest::FileClass),
    ("propertiesFileCredential", ValueTest::FileClass),
    ("yamlSecretValue", ValueTest::Scalar),
    ("dotnetConnectionStrings", ValueTest::Field),
    ("sqlUserPassword", ValueTest::Field),
    ("xmlCredentialElement", ValueTest::Scalar),
    ("xmlCredentialAttribute", ValueTest::Field),
    ("sqlDriverDsnPassword", ValueTest::Field),
    ("passFileLine", ValueTest::Opaque),
    ("discordSocialBotToken", ValueTest::Opaque),
    ("signedUrlSignature", ValueTest::Opaque),
    ("tokenInUrlUserinfo", ValueTest::Opaque),
    ("urlQueryCredential", ValueTest::Written),
    ("commandLinePassword", ValueTest::Written),
    (super::named_values::PATTERN, ValueTest::Named),
];

/// The quotes a literal opens with: the text before a quoted value ends in one.
pub(super) const QUOTES: [char; 3] = ['"', '\'', '`'];

/// Which values of a pattern's matches are credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ValueTest {
    /// The pattern matches only credentials: every value is one.
    Every,
    /// The pattern knows a credential by where it stands (its `secret`
    /// group: a URL's password, the token after an auth scheme), whatever
    /// its characters: every value is one except a placeholder written in
    /// its place ([`is_written_placeholder`]). A request and a file judge it
    /// alike.
    Placed,
    /// Only the key's name says "credential", so a value that is not one is
    /// kept: a value that spells a code identifier
    /// (`"x-kubernetes-patch-merge-key": "resourceClaimName"`) names a field,
    /// and a hex digest keyed by a file path
    /// (`"bin/with_credentials.mjs": "<sha256>"`) is a checksum manifest entry.
    /// See [`assigns_a_credential`].
    KeyName,
    /// [`ValueTest::KeyName`] for a YAML scalar (or an XML element's text) of
    /// any length, except the keywords YAML reads as a boolean or null
    /// ([`is_non_string_scalar`]), which hold no secret, and a placeholder
    /// ([`is_template`]). A quoted scalar is also a [`ValueTest::Quoted`]
    /// literal: a template or a short value under an opaque-string key
    /// (`token`) stays.
    Scalar,
    /// A quoted literal, judged as [`ValueTest::KeyName`] and by its length:
    /// one of [`OPAQUE_LITERAL_CHARS`] or more under any credential word, a
    /// shorter one only as a chosen secret ([`is_chosen_secret`]).
    Quoted,
    /// The pattern applies only in a file class whose credential names hold
    /// credentials (the dotenv and properties class): a key with a credential segment
    /// ([`file_class_key`]) holds one whatever its value
    /// spells (`DB_PASS=CowSaysMoo`, `DB_PASS=/hunter2`), unless the value
    /// is readable there ([`file_class_value_is_readable`]).
    FileClass,
    /// A field of a value's own grammar (a connection string's `Password=`):
    /// every value is a credential except a placeholder the program fills in
    /// ([`is_template`]: `{value}`, `${PASS}`).
    Field,
    /// The pattern's shape is a token's and a name's alike (dotted
    /// base64url, a URL's userinfo): only an opaque value is a credential
    /// ([`is_opaque`]).
    Opaque,
    /// A password written where a command or a URL takes one (`curl -u
    /// user:…`, `?password=…`): a credential unless it is a reference or a
    /// placeholder, or only a word ([`is_written_secret`]).
    Written,
    /// The pattern finds a name with a credential's word part in a loose
    /// layout: [`super::named_values`] judges the name and walks its values.
    /// Asked for one value, it is a credential when opaque and none the
    /// key-name rules keep.
    Named,
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
            && (!is_sentence(key, secret) || opens_with_opaque_word(key, secret))
            && !secret.contains('`')
    }

    /// Whether [`ValueTest::redacts_in_comment_by`] can accept any value under
    /// `key`: its conjuncts that read no byte of the value. A key that names a
    /// locator holds no credential whatever its value ([`names_a_location`]),
    /// so a reader can skip the value of a key this rejects.
    pub(super) fn admits_key_in_comment(self, key: &impl KeyFacts) -> bool {
        matches!(self, Self::Every | Self::Placed) || !key.names_a_locator()
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
            Self::Placed => !is_written_placeholder(secret),
            // A connection string may quote a field's value (`Password="…"`):
            // the quotes framing it are the string's, not the value's.
            Self::Field => !is_template(secret.trim().trim_matches(QUOTES)),
            Self::KeyName | Self::FileClass => assigns_a_credential(key, secret),
            Self::Opaque => is_opaque(secret, OPAQUE_LITERAL_CHARS),
            Self::Written => is_written_secret(secret),
            Self::Named => {
                is_opaque(secret, OPAQUE_LITERAL_CHARS) && assigns_a_credential(key, secret)
            }
            Self::Scalar => {
                assigns_a_credential(key, secret)
                    && !reads_as_a_sentence(secret)
                    && !is_written_placeholder(secret)
                    && !is_non_string_scalar(secret)
                    && !is_type_annotation(secret)
                    && !is_template(secret.trim())
                    && (!key.ends_with_quote() || is_quoted_credential(key, secret))
            }
            Self::Quoted => {
                let value = joined_pieces(secret);
                if (!key.names_a_credential() && is_label(&value, true))
                    || is_template_of_names(&value)
                {
                    return false;
                }
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

/// Characters of a comment's first word after `name:` that make it a secret
/// whatever words follow it (`// password: Fake0Moo9Zed and more`): the
/// length of a chosen secret's shortest opaque word.
const COMMENT_OPAQUE_WORD_CHARS: usize = 8;

/// Whether the first word of a comment sentence after `name:` is a secret and
/// no word of the sentence: a chosen credential's word ([`is_chosen_secret`])
/// of [`COMMENT_OPAQUE_WORD_CHARS`] or more, or under any credential word one
/// of [`OPAQUE_LITERAL_CHARS`] or more that mixes letters and digits. A
/// sentence of plain words (`must be at least 8 characters`) opens with none.
fn opens_with_opaque_word(key: &impl KeyFacts, secret: &str) -> bool {
    let Some(word) = secret.split_whitespace().next() else {
        return false;
    };
    let word = word.trim_end_matches(['.', ',', ';', ')']);
    let chars = word.chars().count();
    let mixes = word.chars().any(char::is_alphabetic) && word.chars().any(|c| c.is_ascii_digit());
    (chars >= COMMENT_OPAQUE_WORD_CHARS && mixes && is_chosen_secret(key, word))
        || (chars >= OPAQUE_LITERAL_CHARS && mixes && !is_template(word))
}

/// Whether a literal interpolates an expression (`` `pi-${id}` ``,
/// `"${HOME}/x"`, a format argument by name or position, `"{name}\n"` or
/// `"Bearer {0}"`, and in a literal whose prefix interpolates,
/// [`opens_interpolation`], `f'{base}_x'` or `$"{key}"`): the program
/// computes its value, so its text spells no credential.
fn holds_interpolation(key: &impl KeyFacts, value: &str) -> bool {
    let hole = |marker: &str| {
        value
            .split_once(marker)
            .is_some_and(|(_, rest)| rest.contains('}'))
    };
    hole("${")
        || (key.opens_interpolation() && holds_format_hole(value))
        || (0..value.len()).any(|at| format_argument_len(value, at).is_some())
}

/// The length of the format argument that starts at `at` in `value`: a `{`,
/// a name or a position (`{name}`, `{TOKEN}`, `{0}`), then `}`, where no `{`
/// before it escapes the brace (`{{name}}` writes braces). Any format
/// (`format!`, `str.format`, `String.Format`, a logger's) fills it in, so it is
/// no part of the literal's written text.
fn format_argument_len(value: &str, at: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    if bytes.get(at) != Some(&b'{') || (at > 0 && bytes[at - 1] == b'{') {
        return None;
    }
    let name = bytes
        .get(at + 1..)?
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
        .count();
    (name > 0 && bytes.get(at + 1 + name) == Some(&b'}')).then_some(name + 2)
}

/// Whether a literal that interpolates ([`holds_interpolation`]) spells a
/// credential in its text beside the holes: the program computes what the holes
/// hold, but a piece of text between them is written, so each piece is judged as
/// a literal of its own. A credential's piece is one opaque run of
/// [`OPAQUE_LITERAL_CHARS`] or more with no blank in it
/// (`f'Zq7x…{suffix}'`); a hole with no text beside it (`f'{base}'`) and
/// prose or a short word around one (`` `pi-${id}` ``) hold none, and nor does
/// a piece that assigns the hole after it (`//registry.npmjs.org/:_authToken=`
/// before `{TOKEN}`): it is the key of the value the program fills in.
fn holds_opaque_literal(key: &impl KeyFacts, value: &str) -> bool {
    let pieces = outside_holes(key, value);
    let last = pieces.len().saturating_sub(1);
    pieces.into_iter().enumerate().any(|(at, piece)| {
        let piece = piece.trim();
        let assigns_the_hole = at < last && piece.ends_with(['=', ':']);
        !assigns_the_hole
            && literal_chars(piece) >= OPAQUE_LITERAL_CHARS
            && !piece.contains(char::is_whitespace)
            && assigns_a_credential(key, piece)
    })
}

/// The text of `value` outside its holes: `${…}` and a format argument
/// ([`format_argument_len`]) anywhere, and when the literal's prefix
/// interpolates ([`opens_interpolation`]) each `{…}` of
/// [`holds_format_hole`]'s reading, `{{` and `}}` being text.
fn outside_holes<'a>(key: &impl KeyFacts, value: &'a str) -> Vec<&'a str> {
    let format = key.opens_interpolation();
    let bytes = value.as_bytes();
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if !format && let Some(len) = format_argument_len(value, at) {
            pieces.push(value.get(start..at).unwrap_or_default());
            at += len;
            start = at;
            continue;
        }
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

/// The names a language gives its built-in scalar types, any case
/// (`str`, `String`, `number`): after `name:` one is a type annotation.
const SCALAR_TYPE_NAMES: &[&str] = &[
    "str", "string", "int", "integer", "bool", "boolean", "bytes", "number", "float", "double",
    "char", "any", "object", "unknown",
];

/// Whether `secret`, after `name:`, annotates the name's type rather than
/// holding its value (`str`, `Token`, `str = None`, `string | null`,
/// `Optional[str]`): one type or several joined by `|`, each a built-in
/// scalar type's name ([`SCALAR_TYPE_NAMES`]), a capitalized name whose words
/// name a credential (`Token`, `SecretStr`, [`names_a_credential`]), a
/// generic ([`is_type_expression`]) or a name with its parameter in brackets
/// (`Optional[str]`), a union holding the type of no value besides
/// (`string | null`), and after `=` at most a default that holds no value
/// (`None`, `null`, `undefined`, an empty literal, `b""` too).
fn is_type_annotation(secret: &str) -> bool {
    let body = secret.trim().trim_end_matches([',', ';']).trim_end();
    // The default follows a `=` outside every bracket: one inside is a
    // type's argument (`Annotated[str, Field(min_length=8)]`).
    let mut depth = 0_usize;
    let assign = body.char_indices().find_map(|(at, c)| {
        match c {
            '[' | '(' | '<' | '{' => depth += 1,
            ']' | ')' | '>' | '}' => depth = depth.saturating_sub(1),
            '=' if depth == 0 => return Some(at),
            _ => {}
        }
        None
    });
    let (types, default) = match assign {
        Some(at) => (
            body.get(..at).unwrap_or_default().trim(),
            Some(body.get(at + 1..).unwrap_or_default().trim()),
        ),
        None => (body, None),
    };
    // An empty literal, a prefix before its quotes or not (`""`, `b""`).
    let empty_default = |default: &str| {
        let literal = default.trim_start_matches(|c: char| c.is_ascii_alphabetic());
        is_non_string_scalar(default)
            || ["None", "nil", "undefined"].contains(&default)
            || (default.len() - literal.len() <= 2 && ["\"\"", "''"].contains(&literal))
    };
    let types = || types.split('|').map(str::trim);
    default.is_none_or(empty_default)
        && types().any(is_annotated_type)
        && types().all(|one| is_annotated_type(one) || empty_default(one))
}

/// One type of [`is_type_annotation`].
fn is_annotated_type(one: &str) -> bool {
    let capitalized = |name: &str| {
        name.starts_with(|c: char| c.is_ascii_uppercase())
            && name.bytes().all(|byte| byte.is_ascii_alphabetic())
    };
    let one = one.trim_end_matches('?').trim_end_matches("[]");
    // A type parameter (`T`, `K`): one capital letter.
    if one.len() == 1 && one.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return true;
    }
    // A generic: its first argument a type, each other one a type or the
    // metadata a type carries (`Annotated[str, Field(min_length=8)]`), no
    // opaque value among them.
    if let Some((outer, inner)) = one.strip_suffix(']').and_then(|head| head.split_once('[')) {
        let mut parts = inner.split(',').map(str::trim);
        return capitalized(outer)
            && parts.next().is_some_and(is_annotated_type)
            && parts.all(|part| {
                is_annotated_type(part)
                    || part
                        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                        .all(|word| !is_opaque(word, 0))
            });
    }
    let capitalized = capitalized(one);
    SCALAR_TYPE_NAMES
        .iter()
        .any(|name| one.eq_ignore_ascii_case(name))
        || (capitalized && (names_a_credential(one) || is_identifier_name(one)))
        || is_type_expression(one)
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
                .map_or_else(
                    || {
                        if p.regex.contains("(?P<secret") {
                            ValueTest::Placed
                        } else {
                            ValueTest::Every
                        }
                    },
                    |&(_, test)| test,
                )
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
    !key.names_a_published_value()
        && !(key.describes_a_credential() && !is_described_credential(secret))
        && !names_a_location(key, secret)
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
        && !(key.names_a_user_agent_field() && is_nonce_shape(secret.trim_matches(QUOTES)))
        && !is_timestamp(secret)
        && !(is_label(secret, false) && !key.names_a_chosen_credential())
        && !(is_command(secret) && !key.names_a_chosen_credential())
        && !is_version_specifier(secret)
        && !is_mask(secret)
        && !is_api_version(secret)
        && !((is_key_path(secret) || is_namespaced_name(secret) || is_reference(secret))
            && (!key.names_a_chosen_credential() || spells_a_credential_word(secret)))
}

/// Whether `secret`, under a name that describes or identifies a credential
/// ([`KeyFacts::describes_a_credential`]: `secretName`, `tokenHeader`,
/// `kmsKeyId`), is one all the same: a word of it is opaque ([`is_opaque`]),
/// and it is no UUID, the shape of an id. A description (`X-Api-Key`, a
/// resource's path, an algorithm) is words; a credential pasted under such a
/// name (`tokenHeader = "Bearer Zq7x…"`) is not.
fn is_described_credential(secret: &str) -> bool {
    !is_uuid(secret)
        && secret
            .split_whitespace()
            .any(|word| is_opaque(word.trim_matches(QUOTES), OPAQUE_LITERAL_CHARS))
}

/// Whether `secret` is an API's version as a manifest names it (`v1`,
/// `v1alpha1`, `v2beta3`, `v1rc2`): `v`, a number, and at most a pre-release
/// word and its number.
fn is_api_version(secret: &str) -> bool {
    let Some(rest) = secret.strip_prefix('v') else {
        return false;
    };
    let major = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    if major.len() == rest.len() {
        return false;
    }
    let tag = major.trim_end_matches(|c: char| c.is_ascii_digit());
    tag.is_empty() || ["alpha", "beta", "rc"].contains(&tag)
}

/// Whether `secret` is a mask a document writes in a value's place: one
/// character written over and over (`xxxxxxxxxxxxxxxx`, `********`), quotes
/// aside. No generator and no person makes a secret of one character.
fn is_mask(secret: &str) -> bool {
    let value = secret.trim().trim_matches(QUOTES);
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| value.len() > 1 && chars.all(|c| c == first))
}

/// Whether `secret` is a code expression marked as code, no value: code names
/// ([`is_identifier_word`], or a short word of letters) with number and
/// quoted literal operands, joined by member access, indexing and operators,
/// and marked as code by a call (`getToken()`, `hash(pw)`) or an index
/// (`tokens[0]`), a `!` beside a
/// name (`process.env.API_KEY!`, `!opts.token`), `?.`, `??`, `||`, `&&`, or
/// an arithmetic operator between blanks. Only its literals can be values:
/// the rules that read literals judge them. A `,` or `;` that ends a member
/// or a statement is no part of it. A value that holds anything else
/// (`#`, `@`, braces, a backslash), or a name no code word spells
/// (`Zq7Xk2Pm9Vt4(x)`), is no expression.
fn is_marked_code(secret: &str) -> bool {
    let body = secret.trim().trim_end_matches([',', ';']).trim_end();
    let bytes = body.as_bytes();
    let (mut at, mut depth, mut names, mut code) = (0, 0_usize, 0_usize, false);
    let name_byte = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$');
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'_' | b'$' => {
                let end = at + bytes[at..].iter().take_while(|b| name_byte(**b)).count();
                let name = body.get(at..end).unwrap_or_default();
                let word = name.trim_matches('$');
                if !(is_identifier_word(word)
                    || ((1..=4).contains(&word.len())
                        && word.bytes().all(|b| b.is_ascii_alphabetic())))
                {
                    return false;
                }
                names += 1;
                // A postfix `!` asserts a member it ends (`env.API_KEY!`):
                // after a lone word it is text (`Welcome!`).
                let member = at > 0 && bytes[at - 1] == b'.';
                code |= matches!(bytes.get(end), Some(b'(' | b'['))
                    || (member
                        && bytes.get(end) == Some(&b'!')
                        && bytes
                            .get(end + 1)
                            .is_none_or(|next| matches!(next, b'.' | b')' | b' ')));
                at = end;
                continue;
            }
            b'0'..=b'9' => {
                at += bytes[at..]
                    .iter()
                    .take_while(|b| b.is_ascii_alphanumeric() || **b == b'.')
                    .count();
                continue;
            }
            b'"' | b'\'' | b'`' => {
                let Some(close) = bytes[at + 1..].iter().position(|b| *b == byte) else {
                    return false;
                };
                at += close + 2;
                continue;
            }
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                let Some(outer) = depth.checked_sub(1) else {
                    return false;
                };
                depth = outer;
                code |= byte == b')'
                    && bytes.get(at + 1) == Some(&b'!')
                    && bytes
                        .get(at + 2)
                        .is_none_or(|next| matches!(next, b'.' | b')' | b' '));
            }
            // A prefix `!` before a name, or a postfix one after a name or a
            // call that ends the operand (`API_KEY!,`): `pass!word` is text.
            b'!' => {
                code |= (at == 0 || matches!(bytes[at - 1], b' ' | b'(' | b'|' | b'&'))
                    && bytes
                        .get(at + 1)
                        .is_some_and(|next| name_byte(*next) || *next == b'(');
            }
            b'?' | b'|' | b'&' => {
                code |= bytes.get(at + 1) == Some(&byte)
                    || (byte == b'?' && bytes.get(at + 1) == Some(&b'.'));
            }
            b'+' | b'-' | b'*' | b'/' | b'%' => {
                code |= at > 0 && bytes[at - 1] == b' ' && bytes.get(at + 1) == Some(&b' ');
            }
            b'.' | b',' | b':' | b' ' | b'\t' | b'<' | b'>' | b'=' => {}
            _ => return false,
        }
        at += 1;
    }
    depth == 0 && names > 0 && code
}

/// Whether `text` is a UUID: hex digits in groups of 8, 4, 4, 4 and 12.
fn is_uuid(text: &str) -> bool {
    let mut groups = text.split('-');
    [8, 4, 4, 4, 12].into_iter().all(|len| {
        groups
            .next()
            .is_some_and(|group| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
    }) && groups.next().is_none()
}

/// Digits of a number a reference holds as one of its words (an account, a
/// port, a year): a longer run of digits is a code.
const REFERENCE_NUMBER_DIGITS: usize = 12;

/// Whether `secret` says where a thing is kept or how it is addressed, and
/// is none itself: a resource's path (`projects/demo/secrets/db/versions/latest`),
/// a scheme and a path (`vault:secret/data/app#token`,
/// `arn:aws:kms:us-east-1:123456789012:key/<uuid>`, `github:acme/lib#v2`),
/// or a templated key (`user:profile:{id}:settings`). Its segments, between
/// `/` and `:` and after one `#`, are two or more, each words
/// ([`is_reference_label`]) or a placeholder ([`is_template`]) and not all
/// placeholders, and it holds a `/`, a placeholder or three segments: two
/// words about one colon are a pair (`admin:opensesame`), a name and its
/// credential. Base64 holds `/` too, but its pieces mix case and digits into
/// no word. A person may choose a password of words so joined, so a key
/// naming a chosen credential keeps one that names no credential
/// ([`assigns_a_credential`]).
fn is_reference(secret: &str) -> bool {
    let (body, fragment) = match secret.rsplit_once('#') {
        Some((body, fragment)) => (body, Some(fragment)),
        None => (secret, None),
    };
    if fragment.is_some_and(|fragment| !is_reference_label(fragment)) {
        return false;
    }
    let (mut segments, mut holes) = (0_usize, 0_usize);
    let known = body.split(['/', ':']).all(|segment| {
        segments += 1;
        let hole = is_template(segment);
        holes += usize::from(hole);
        hole || is_reference_label(segment)
    });
    known && segments >= 2 && holes < segments && (body.contains('/') || holes > 0 || segments >= 3)
}

/// One segment of [`is_reference`]: a relative step (`.`, `..`), a UUID, or
/// words joined by `-`, `_`, `.` or `@` (`db-password`, `us-east-1`,
/// `@acme`, `token-lib@1.2.3`), each a word written as one is (lower, UPPER
/// or Capitalized: [`is_plain_word`]), a camelCase name, a number of at most
/// [`REFERENCE_NUMBER_DIGITS`] digits, or a version (`v2`).
fn is_reference_label(segment: &str) -> bool {
    if matches!(segment, "." | "..") || is_uuid(segment) {
        return true;
    }
    let is_number = |word: &str| {
        (1..=REFERENCE_NUMBER_DIGITS).contains(&word.len())
            && word.bytes().all(|byte| byte.is_ascii_digit())
    };
    let written_as_a_word = |word: &str| {
        word.bytes().skip(1).all(|byte| byte.is_ascii_lowercase())
            || word.bytes().all(|byte| byte.is_ascii_uppercase())
    };
    let segment = segment.strip_prefix('@').unwrap_or(segment);
    !segment.is_empty()
        && segment.split(['-', '_', '.', '@']).all(|word| {
            is_number(word)
                || word.strip_prefix('v').is_some_and(is_number)
                || (is_plain_word(word) && written_as_a_word(word))
                || is_identifier_name(word)
        })
}

/// Whether `secret` is a version a manifest asks for: a range
/// (`^1.2.3`, `>=1.0.0 <2.0.0`, `~2.1 || ^3.0.0`), or after a protocol
/// (`npm:3.1127.0`, `workspace:^`, `workspace:*`) a range, a bare version or
/// a bare operator. A package named for what it handles
/// (`"token-providers": "npm:3.1127.0"`) holds none of it. Without a
/// protocol or an operator digits and dots are any number (`12.34.56`).
fn is_version_specifier(secret: &str) -> bool {
    let operators: &[char] = &['^', '~', '>', '<', '='];
    let version = |text: &str| {
        let (core, tag) = match text.split_once(['-', '+']) {
            Some((core, tag)) => (core, Some(tag)),
            None => (text, None),
        };
        let core = core.strip_prefix('v').unwrap_or(core);
        let mut parts = 0;
        core.split('.').all(|part| {
            parts += 1;
            matches!(part, "x" | "X" | "*")
                || (!part.is_empty()
                    && part.len() <= REFERENCE_NUMBER_DIGITS
                    && part.bytes().all(|byte| byte.is_ascii_digit()))
        }) && parts <= 3
            && tag.is_none_or(|tag| {
                !tag.is_empty()
                    && tag.len() < OPAQUE_LITERAL_CHARS
                    && tag
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
            })
    };
    let range = |text: &str, bare: bool| {
        let mut comparators = text
            .split(|c: char| c.is_whitespace() || c == '|')
            .filter(|comparator| !comparator.is_empty())
            .peekable();
        comparators.peek().is_some()
            && comparators.all(|comparator| {
                let asked = comparator.trim_start_matches(operators);
                let operated = asked.len() < comparator.len();
                (operated || bare) && ((asked.is_empty() && bare) || version(asked))
            })
    };
    match secret.split_once(':') {
        Some((protocol, asked)) => {
            (2..OPAQUE_LITERAL_CHARS).contains(&protocol.len())
                && protocol
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'+')
                && range(asked, true)
        }
        None => range(secret, false),
    }
}

/// Whether `secret` is a command line (`node scripts/mint-token.js --dry`,
/// a manifest's script named for what it makes): a program's name, then
/// arguments, each an option (`--dry`, `-o`, `--out=dist`), a file's path of
/// words ([`is_path_locator`], or segments of [`is_reference_label`] ending
/// in a file name's extension) or a word, an option or a path among them. A
/// phrase a person chose may read so, so a key naming a chosen credential
/// keeps it ([`assigns_a_credential`]).
fn is_command(secret: &str) -> bool {
    let path = |piece: &str| {
        is_path_locator(piece)
            || (piece.contains('/')
                && has_file_extension(piece)
                && piece.split('/').all(is_reference_label))
    };
    let flag = |piece: &str| {
        let Some(name) = piece.strip_prefix('-') else {
            return false;
        };
        let name = name.strip_prefix('-').unwrap_or(name);
        let (name, value) = match name.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (name, None),
        };
        is_label(name, true)
            || (is_plain_word(name)
                && value.is_none_or(|value| is_reference_label(value) || path(value)))
    };
    let mut pieces = secret.split_whitespace();
    let (mut marked, mut count) = (false, 0_usize);
    pieces.next().is_some_and(is_reference_label)
        && pieces.all(|piece| {
            count += 1;
            let mark = flag(piece) || path(piece);
            marked |= mark;
            mark || is_reference_label(piece)
        })
        && marked
        && count >= 1
}

/// Whether `secret` is a placeholder a person fills in: plain words
/// ([`is_plain_word`]) joined by `-`, `_` or blanks that say so, `your`
/// among them (`tvly-YOUR-KEY-HERE`, `your_api_key_here`, `YOUR_TOKEN`), a
/// provider's prefix of a few letters before them (`ghp_your_ghe_token`), or under an
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
            .enumerate()
            .peekable();
        words.peek().is_some()
            && words.all(|(at, word)| {
                let prefix = at == 0
                    && (1..=4).contains(&word.len())
                    && word.bytes().all(|byte| byte.is_ascii_alphabetic());
                (prefix || is_plain_word(word))
                    && written_as_a_word(word)
                    && (!chosen || prefix || names_what_to_fill(word))
            })
    };
    let says = |word: &str| {
        secret
            .split(['-', '_', ' '])
            .any(|piece| piece.eq_ignore_ascii_case(word))
    };
    let bracketed =
        angle_inner(secret).is_some_and(|inner| plain_words(inner) && !is_lone_opaque_word(inner));
    (bracketed && !chosen) || (plain_words(secret) && says("your"))
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
/// (`password = "adminUser==rootPassword"`). Unquoted, a value marked as
/// code by a call, a `!`, `??` or `||` ([`is_marked_code`]) is one too.
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
    (all && joined && pieces >= 2 && names >= 2) || (!key.ends_with_quote() && is_marked_code(body))
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

/// Where the first control-character escape in `text` starts
/// ([`control_escape_len`]: `\n`, `\r`, `\t`, `\u000a`, …): in a string
/// literal the program holds a line break or a field's separator there, which
/// no credential's bytes hold, so a value ends at it when what precedes it is
/// still a credential. A backslash another escapes (`\\n`) starts none.
pub(super) fn escaped_control(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut at = 0;
    while let Some(found) = bytes.get(at..).and_then(|rest| memchr::memchr(b'\\', rest)) {
        let slash = at + found;
        if text.get(slash..).and_then(control_escape_len).is_some() {
            return Some(slash);
        }
        at = slash + 2;
    }
    None
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
/// or `_` (`new-login-refresh`, `__vite_ssr_import__`), each a pronounceable
/// word (or, in a dunder name, an abbreviation, and a camelCase word of such
/// parts: `__vite_ssr_exportAll__`) and none a
/// credential word. Under a key naming an opaque string (`token`) such a
/// value names something; a chosen password may be words so joined, so a key
/// naming one keeps it. A label that names what it is (`unrelated-secret`,
/// `deploy-bot-token`) is how a person spells a static credential.
///
/// With `any_word`, a credential word counts as any other: where the key
/// itself names no credential ([`KeyFacts::names_a_credential`]: an
/// attribute beside one, `type="password" name="current-password"`), such
/// a label is a keyword of the format, not a value.
fn is_label(secret: &str, any_word: bool) -> bool {
    // A name a language marks as its own (`__vite_ssr_import__`) wraps the
    // label in underscores; a short word of it is an abbreviation (`ssr`).
    let dunder = secret.starts_with("__") && secret.ends_with("__") && secret.len() > 4;
    let secret = if dunder {
        secret.trim_matches('_')
    } else {
        secret
    };
    let is_word = |word: &str| {
        word.bytes().all(|b| b.is_ascii_lowercase())
            && (pronounceable_word(word.as_bytes()) || (dunder && (1..=4).contains(&word.len())))
            && (any_word
                || !CREDENTIAL_WORDS
                    .split('|')
                    .any(|credential| credential == word))
    };
    secret.split(['-', '_']).count() >= 2
        && secret.split(['-', '_']).all(|word| {
            if !dunder {
                return is_word(word);
            }
            // A dunder name is an identifier: its words may be camelCase,
            // each part a pronounceable word of its own (an abbreviation is
            // a whole word: `ssr`, never a run of short parts).
            let camel = camel_parts(word).count() > 1;
            word.starts_with(|c: char| c.is_ascii_lowercase())
                && word.bytes().all(|b| b.is_ascii_alphabetic())
                && camel_parts(word).all(|part| {
                    let part = part.to_ascii_lowercase();
                    is_word(&part) && (!camel || pronounceable_word(part.as_bytes()))
                })
        })
}

/// The parts of a camelCase word of letters, each cut before a capital
/// (`exportAll`: `export`, `All`).
fn camel_parts(word: &str) -> impl Iterator<Item = &str> {
    let mut rest = word;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let end = rest
            .char_indices()
            .skip(1)
            .find(|(_, c)| c.is_ascii_uppercase())
            .map_or(rest.len(), |(at, _)| at);
        let (part, after) = rest.split_at(end);
        rest = after;
        Some(part)
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
/// lone opening bracket, which begins a value over the next lines; a
/// single-quoted value is a literal, and `${NAME:default}` holds a default
/// that may be the secret), the tool's own redaction marker, or
/// a value whose key or option names a location (`TOKEN_FILE=/run/token`,
/// `--token-file=/x`), or a path or reference of names under a key that
/// describes a credential (`SECRET_NAME=prod/app/database`,
/// `KEY_ID=arn:aws:kms:…`: [`is_reference`]); any other value there stays
/// one, as the file class holds (`MONKEY_NAME=CowSaysMoo`). A path is no locator by its shape alone, and a
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
        || matches!(value, "(" | "[" | "{")
        || reference
        || (super::named_values::describes_a_credential(key_name(key))
            && (is_namespaced_name(value) || is_reference(value) || is_key_path(value)))
        || is_redaction_marker(value)
        || is_type_expression(value)
        || names_a_locator(key)
        || option(value).is_some_and(|(name, _)| names_a_locator(name))
}

/// Whether `value` is a redaction marker, bare or inside the quotes a
/// quote-aware rule left around it (`"[REDACTED-SPRINGBOOTSECRETS]";`), the
/// control characters that frame it aside (`"[REDACTED-ENVVARSECRETS]\r\n"`,
/// [`without_control_frame`]) and the next field after it
/// ([`super::named_values::field_end_after`]): text this tool wrote is no
/// credential, whichever rule reads the line next.
pub(super) fn is_redaction_marker(value: &str) -> bool {
    // The wrappers a literal's own syntax puts around it: a call's `(` and
    // `)`, a statement's `;` or an item's `,` after it, and the quotes, whose
    // partner may be on a later line (a literal split over lines).
    let value = value
        .trim_start_matches('(')
        .trim_end()
        .trim_end_matches([';', ',', ')']);
    let inner = QUOTES
        .into_iter()
        .find_map(|quote| value.strip_prefix(quote)?.strip_suffix(quote))
        .or_else(|| value.strip_prefix(QUOTES))
        .unwrap_or(value);
    let whole = |text: &str| {
        crate::security::placeholder_regex()
            .find(text)
            .is_some_and(|marker| marker.range() == (0..text.len()))
    };
    let inner = without_control_frame(inner);
    // A marker the next field follows in its literal (`"[REDACTED-…]\r\nHost:
    // example.com"`) ends that literal's value as the value it replaced did.
    let inner = super::named_values::field_end_after(inner, whole)
        .and_then(|at| inner.get(..at))
        .unwrap_or(inner);
    whole(inner)
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
pub(super) const OPAQUE_LITERAL_CHARS: usize = 16;

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
/// joined, as the program reads them, and each line a backslash continues
/// joined to the next ([`without_continuations`]). Any other text is its own
/// value.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are `find` results of an ASCII quote, one byte wide"
    )
)]
fn joined_pieces(secret: &str) -> String {
    let secret = without_continuations(secret);
    let mut value = String::with_capacity(secret.len());
    let mut rest = secret.as_ref();
    while let Some(at) = rest.find(QUOTES) {
        value.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let seam = after_code_gap(after);
        let next = seam.strip_prefix('+').map_or(seam, after_code_gap);
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

/// `text`, a literal's body, as the program holds it over its line breaks:
/// a backslash before a line break continues the literal on the next line
/// (`"Zq7X…\` then the rest), so the two lines are one value with neither
/// the backslash nor the break. An escaped backslash before a break (`\\`)
/// continues nothing.
pub(super) fn without_continuations(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains("\\\n") && !text.contains("\\\r\n") {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut value = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(char) = chars.next() {
        if char != '\\' {
            value.push(char);
            continue;
        }
        match chars.peek() {
            Some('\n') => {
                chars.next();
            }
            Some('\r') => {
                chars.next();
                if chars.peek() == Some(&'\n') {
                    chars.next();
                } else {
                    value.push_str("\\\r");
                }
            }
            Some(&escaped) => {
                chars.next();
                value.push(char);
                value.push(escaped);
            }
            None => value.push(char),
        }
    }
    std::borrow::Cow::Owned(value)
}

/// The parts of a quoted rule's value `secret` (the text between a literal's
/// first opening quote and its last closing one, `key` the text before it)
/// that the source spells, by byte range in `secret`: each piece of a
/// literal written in pieces (`Fake0" + 'Moo9Zed`, read as
/// [`joined_pieces`] reads them), and in each the text outside the holes an
/// interpolation writes ([`outside_holes`]: `${…}`, a format literal's
/// `{…}`), and of a part that writes config entries under credentials' names
/// (`//registry.npmjs.org/:_authToken=<token>\n`) the entries' values alone
/// ([`entry_values`]): the names and the line ends are the entries' text. A
/// value with no seam, no hole and no such entry is one part, itself.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are `find` results of an ASCII quote, one byte wide"
    )
)]
pub(super) fn literal_parts(key: &str, secret: &str) -> Vec<std::ops::Range<usize>> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut from = 0;
    while let Some(at) = secret[from..].find(QUOTES).map(|at| from + at) {
        let after = &secret[at + 1..];
        let seam = after_code_gap(after);
        let next = seam.strip_prefix('+').map_or(seam, after_code_gap);
        match next.strip_prefix(QUOTES) {
            Some(piece) => {
                pieces.push(start..at);
                start = secret.len() - piece.len();
                from = start;
            }
            None => from = at + 1,
        }
    }
    pieces.push(start..secret.len());
    let facts = SpanKeys::new(key);
    let facts = facts.key(0);
    let mut parts = Vec::new();
    for piece in pieces {
        let text = &secret[piece.clone()];
        if !holds_interpolation(&facts, text) {
            parts.push(piece);
            continue;
        }
        let base = secret.as_ptr() as usize;
        // A part's text is judged without the control escapes framing it
        // (`{prefix}<key>\n`): they are the literal's line end, which stays.
        parts.extend(
            outside_holes(&facts, text)
                .into_iter()
                .map(without_control_frame)
                .filter(|part| !part.is_empty())
                .map(|part| {
                    let at = part.as_ptr() as usize - base;
                    at..at + part.len()
                }),
        );
    }
    parts
        .into_iter()
        .flat_map(|part| continued_lines(&secret[part.clone()], part.start))
        .flat_map(|part| {
            let values = entry_values(&secret[part.clone()]);
            if values.is_empty() {
                return vec![part];
            }
            values
                .into_iter()
                .map(|value| part.start + value.start..part.start + value.end)
                .collect()
        })
        .collect()
}

/// The lines of `text`, a literal's part at `base`, that a backslash before
/// each line break continues (`"\` then `<value>"`), by byte range from
/// `base`: each line's text without the backslash, the break and the blanks
/// that indent the next line, which stay where they are written. `text`
/// alone when no line continues, or when it holds a quote: a backslash
/// between two literals (`"ghp_" \` then `"<rest>"`) continues the
/// statement, not a literal, and the pieces are judged as one value.
fn continued_lines(text: &str, base: usize) -> Vec<std::ops::Range<usize>> {
    let whole = base..base + text.len();
    if text.contains(QUOTES) {
        return vec![whole];
    }
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while let Some(found) = bytes.get(at..).and_then(|rest| memchr::memchr(b'\\', rest)) {
        let slash = at + found;
        let after = slash + 1;
        let next = match bytes.get(after) {
            Some(b'\n') => after + 1,
            Some(b'\r') if bytes.get(after + 1) == Some(&b'\n') => after + 2,
            _ => {
                at = after + 1;
                continue;
            }
        };
        lines.push(base + start..base + slash);
        start = next
            + bytes
                .get(next..)
                .unwrap_or_default()
                .iter()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
        at = start;
    }
    lines.push(base + start..base + text.len());
    lines.retain(|line| !line.is_empty());
    lines
}

/// The values of the config entries `text`, a literal's part, writes under
/// credentials' names (`NAME=value`, [`names_a_credential`]: `_authToken`,
/// `password`), by byte range in `text`: each from past its `=` to the
/// blank, `;`, `&`, quote or control escape that ends it
/// ([`escaped_control`]: the literal's line end). An `=` the value only pads
/// with ends nothing; empty when `text` writes no such entry.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are `find` results of ASCII marks, one byte wide"
    )
)]
fn entry_values(text: &str) -> Vec<std::ops::Range<usize>> {
    let is_name = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.');
    let mut values = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find('=').map(|at| from + at) {
        let name_start = text[..at]
            .rfind(|c: char| !is_name(c))
            .map_or(0, |before| before + 1);
        let after = &text[at + 1..];
        let mut len = after
            .find(|c: char| c.is_whitespace() || matches!(c, ';' | '&' | '"' | '\'' | '`'))
            .unwrap_or(after.len());
        if let Some(escape) = escaped_control(&after[..len]) {
            len = escape;
        }
        let value = at + 1..at + 1 + len;
        from = value.end.max(at + 1);
        if name_start < at
            && names_a_credential(&text[name_start..at])
            && !after[..len].trim_end_matches('=').is_empty()
            && !after.starts_with('=')
        {
            values.push(value);
        }
    }
    values
}

/// Whether `part`, a part of a quoted value ([`literal_parts`]), is a value
/// of its own: a credential under `key` alone ([`ValueTest::Quoted`]) at a
/// literal's full length ([`OPAQUE_LITERAL_CHARS`]). The text joined to it
/// (`"<key>" + "-suffix"`, `` `<key>${id}` ``) is then the program's own,
/// judged apart; pieces no one of which is a value alone are one value.
pub(super) fn is_value_of_its_own(key: &str, part: &str) -> bool {
    literal_chars(part) >= OPAQUE_LITERAL_CHARS && ValueTest::Quoted.redacts(key, part)
}

/// `text` after the blanks, line breaks and comments it starts with (`/* …
/// */`, and `// …` or `# …` to the end of its line): what source code may
/// write between the pieces of one literal.
fn after_code_gap(text: &str) -> &str {
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start();
        let after = if let Some(body) = trimmed.strip_prefix("/*") {
            body.split_once("*/").map(|(_, after)| after)
        } else if trimmed.starts_with("//") || trimmed.starts_with('#') {
            trimmed.split_once(['\n', '\r']).map(|(_, after)| after)
        } else {
            None
        };
        match after {
            Some(after) => rest = after,
            None => return trimmed,
        }
    }
}

/// Whether a quoted literal shorter than [`OPAQUE_LITERAL_CHARS`] is a
/// secret a person chose (D6 round 4): its key names one
/// ([`CHOSEN_CREDENTIAL_WORDS`]), and the value has no whitespace, holds a
/// digit or a symbol (not `_`, `.` or `-`: a letters-only word is the one
/// deliberate miss) and is no template (`<…>`, `${…}`, `{{…}}`). Marks alone
/// (`~`, `***`) are a null, a mask or an operator: no secret holds no letter
/// or digit.
fn is_chosen_secret(key: &impl KeyFacts, secret: &str) -> bool {
    key.names_a_chosen_credential()
        && !is_template(secret)
        && !secret.contains(char::is_whitespace)
        && secret.chars().any(char::is_alphanumeric)
        && secret
            .chars()
            .any(|c| c.is_ascii_digit() || !(c.is_alphanumeric() || matches!(c, '_' | '.' | '-')))
}

/// Whether `value`, a literal of an expression assigned under `key` (the
/// name, its operator and the literal's quote), is a secret a person chose
/// there: the quoted assignment of it alone holds one ([`ValueTest::Quoted`])
/// for its choice, not its length ([`is_chosen_secret`]). A long readable
/// literal of an expression (a URL, a message, a name) is the code's text.
pub(super) fn is_chosen_literal(key: &str, value: &str) -> bool {
    ValueTest::Quoted.redacts(key, value) && is_chosen_secret(&SpanKeys::new(key).key(0), value)
}

/// Whether `word`, the value a sentence states for the credential `name`
/// names (`my password was Pl4net-Orb1t9`, `then password = …`), is the
/// secret it states: under a name of a chosen credential, a word that mixes
/// letters with a digit or a symbol ([`is_chosen_secret`]), as a person
/// chooses one; a word of letters alone (`changed`, `required`) is prose.
/// Stated by a word (`prose`: `is`, `was`), it is one of
/// [`COMMENT_OPAQUE_WORD_CHARS`] or more, a chosen secret's shortest. After
/// an operator the text may be code or a setting of any file: it is one run
/// of an opaque value's characters, no code name (`password2`,
/// `!options.secret_token`), and of [`OPAQUE_LITERAL_CHARS`] or more, the
/// length floor a setting outside the dotenv class keeps (`DB_PASS=Fake0Moo`).
pub(super) fn is_stated_chosen_secret(name: &str, word: &str, prose: bool) -> bool {
    let named = word.trim_start_matches(['!', '&', '*']);
    let code_name = named.starts_with(|c: char| c.is_ascii_alphabetic() || matches!(c, '_' | '$'))
        && named
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'.'));
    let chars = word.chars().count();
    let shaped = if prose {
        chars >= COMMENT_OPAQUE_WORD_CHARS
    } else {
        chars >= OPAQUE_LITERAL_CHARS && !code_name && super::named_values::is_run(word)
    };
    shaped
        && word.chars().any(char::is_alphabetic)
        && is_chosen_secret(&SpanKeys::new(name).key(0), word)
}

/// Whether `secret`, written where a command or a URL takes a password, is
/// one: no reference to a variable (`$PASS`, `%PASS%`, `:param`), no
/// placeholder ([`is_template`], or one holding a hole), no option, and it
/// holds a digit or a symbol, as a chosen secret does ([`is_chosen_secret`]:
/// a word of letters is the one deliberate miss).
fn is_written_secret(secret: &str) -> bool {
    let value = secret.trim_matches(QUOTES);
    !is_template(value)
        && !value.starts_with(['$', '%', ':', '-'])
        && !["${", "{{", "$("].iter().any(|hole| value.contains(hole))
        && value
            .chars()
            .any(|c| c.is_ascii_digit() || !(c.is_alphanumeric() || matches!(c, '_' | '.' | '-')))
}

/// Whether `secret`, written where a credential stands (a URL's password,
/// the token after an auth scheme), is a placeholder a document writes in
/// its place: a template ([`is_template`]: `<token>`, `${TOKEN}`), a value to
/// fill in ([`is_fill_in`]: `YOUR_ACCESS_TOKEN_HERE`), or the credential's
/// word itself (`password`, `pass`, `token`, any case). A generated or
/// chosen value spells something else.
pub(super) fn is_written_placeholder(secret: &str) -> bool {
    let value = secret
        .trim_matches(QUOTES)
        .trim_end_matches(['.', ',', ';', ')', '?']);
    let lower = value.to_ascii_lowercase();
    let singular = lower.strip_suffix('s').unwrap_or(&lower);
    let names_itself = CREDENTIAL_WORDS
        .split('|')
        .any(|word| word == lower || word == singular);
    is_template(value) || is_fill_in(&SpanKeys::new("").key(0), value) || names_itself
}

/// Whether `secret` is a placeholder to fill in: `<…>`, `${…}`, `{{…}}`, `{%…%}` or a
/// format argument (`{value}`, `{0}`: a name or a position in braces).
fn is_template(secret: &str) -> bool {
    angle_inner(secret).is_some_and(|inner| !is_lone_opaque_word(inner))
        || (secret.starts_with("${") && secret.ends_with('}'))
        || (secret.starts_with("{{") && secret.ends_with("}}"))
        || (secret.starts_with("{%") && secret.ends_with("%}"))
        || secret
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
            .is_some_and(|name| {
                !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
}

/// Whether `secret` is a placeholder a template engine fills in, naming
/// what it reads (`{{ vault_token }}`, `${secrets.DB_PASSWORD}`,
/// `{% include "token" %}`): a template ([`is_template`]) in braces whose
/// text is names, each a word, a camelCase name or a number, with the marks
/// an expression puts between them. A value pasted into the braces
/// (`{{Zq7x…}}`) is no name.
fn is_template_of_names(secret: &str) -> bool {
    let secret = secret.trim();
    secret.contains(['{', '}'])
        && is_template(secret)
        && secret
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|word| !word.is_empty())
            .all(|word| {
                is_plain_word(word)
                    || is_identifier_name(word)
                    || (word.len() <= REFERENCE_NUMBER_DIGITS
                        && word.bytes().all(|byte| byte.is_ascii_digit()))
            })
}

/// Where the value of the literal at `secret` in `text` ends when its last
/// piece is no piece of it: a quoted name on a line of its own with a colon
/// after it is the next entry's key (a lockfile's
/// `"a": "npm:1.0.0"⏎"b": "npm:2.0.0"`), which pieces written side by side
/// (`"Fake0" "Moo9Zed"`) only look like. The value then ends where the piece
/// before that key closes. `None` when the last piece is the value's own: a
/// `+` joins it, it sits on the same line, or no colon follows it.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are `find` results of ASCII quotes inside a regex match"
    )
)]
pub(super) fn before_next_key(text: &str, secret: &std::ops::Range<usize>) -> Option<usize> {
    let after = text.get(secret.end..)?.strip_prefix(QUOTES)?;
    if !after.trim_start_matches([' ', '\t']).starts_with(':') {
        return None;
    }
    let body = text.get(secret.clone())?;
    let opened = body.rfind(QUOTES)?;
    let before = body[..opened].trim_end();
    let seam = &body[before.len()..opened];
    (before.ends_with(QUOTES) && seam.contains(['\n', '\r']))
        .then(|| secret.start + before.len() - 1)
}

/// Whether a quoted YAML scalar of any length is a credential (B157): no
/// template, and either [`OPAQUE_LITERAL_CHARS`] or more under any credential
/// word or, shorter, under a key naming a chosen secret (`password`).
fn is_quoted_credential(key: &impl KeyFacts, secret: &str) -> bool {
    !is_template(secret)
        && (literal_chars(secret) >= OPAQUE_LITERAL_CHARS || key.names_a_chosen_credential())
}

/// Marks a phrase puts around a word or inside a piece of it: punctuation
/// outside every token alphabet (`e.g.`, `line's`, `devDependencies.vite`,
/// `{"line":289}`). Base64's `+/=` and base64url's `_` are not marks, so a
/// split never turns an encoded token's piece into words, and neither are
/// the characters a person puts in a password in a letter's place (`$`,
/// `@`). `-` is base64url's too: it joins words (`re-authenticate`), and
/// what it leaves of a token is judged as any word is.
const WORD_MARKS: &[char] = &[
    '.', ',', ';', ':', '!', '?', '(', ')', '[', ']', '{', '}', '<', '>', '*', '#', '-', '\'', '"',
    '`',
];

/// The marks that join the parts of one word (`re-authenticate`, `line's`,
/// `block(s)`): a piece with any other mark inside is a name or an example
/// ([`Piece::Name`]), not a word.
const WORD_JOINTS: &[char] = &['-', '\'', '(', ')'];

/// Digits of a number a sentence holds beside its words (a count, a port, a
/// year). Two groups of this many in a row are a code read off in groups
/// ([`GROUP_CHARS`]).
const NUMBER_DIGITS: usize = 4;

/// Characters of one group of a grouped code (`4821-9930`, an app password's
/// `abcd efgh ijkl mnop`): the shortest group a person reads off.
const GROUP_CHARS: usize = 4;

/// Whether `secret` is prose (`meta form value 1`, `Enter your API key
/// here`, `!!ALREADY FORMATTED!!`, `each line's key path, e.g.
/// devDependencies.vite`) assigned to a key whose credential word names an
/// opaque machine string: `key`, `token` or `credential`. API keys, tokens
/// and credential strings never put spaces between dictionary words, so
/// such a phrase is a label, a placeholder, a description or test data. A
/// `password`, `passphrase` or `secret` may be a phrase a person chose
/// (`correct horse battery staple`), so a key naming one keeps its phrase
/// redacted.
///
/// The value is read as the literal it may be written in
/// ([`without_literal_escapes`]), and each of its lines is a sentence of its
/// own ([`is_prose_line`]): a token on a line of its own
/// (`live\ntofabimereduloka`) is a token.
fn is_plain_phrase(key: &impl KeyFacts, secret: &str) -> bool {
    if key.names_a_chosen_credential() {
        return false;
    }
    let read = without_literal_escapes(secret);
    let mut lines = read
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .peekable();
    lines.peek().is_some() && lines.all(is_prose_line)
}

/// Whether one line of a value is a sentence: natural-language text, not
/// data. Its blank-separated pieces are words ([`Piece::Word`]), two or more,
/// with numbers beside them ([`Piece::Number`], [`Piece::Group`]), and no
/// piece is opaque (`Bearer <token>` keeps its token piece, so it stays a
/// credential). A marked name or example ([`Piece::Name`]) needs a sentence
/// around it, of three or more words that outnumber such pieces: an auth
/// scheme and its credential (`Bearer tofa.bime.redu.loka`) or pieces of
/// marks alone (`a.1.b.2 c.3.d.4`) are none. Data is no sentence whatever
/// words it holds: a serialized structure ([`is_serialized_structure`]), a
/// list of pairs ([`is_pair_list`]) or a code read off in groups, two numbers
/// of [`NUMBER_DIGITS`] digits in a row (`1234 5678`) or letters
/// ([`is_grouped_letters`]).
fn is_prose_line(line: &str) -> bool {
    if is_serialized_structure(line) || is_pair_list(line) || is_grouped_letters(line) {
        return false;
    }
    let (mut words, mut names) = (0_usize, 0_usize);
    let mut after_group = false;
    let plain = line.split_whitespace().all(|piece| {
        let kind = Piece::of(piece);
        let grouped = kind == Piece::Group && after_group;
        after_group = kind == Piece::Group;
        match kind {
            Piece::Word => words += 1,
            Piece::Number | Piece::Group => {}
            Piece::Name => names += 1,
            Piece::Opaque => return false,
        }
        !grouped
    });
    plain && words >= 2 && (names == 0 || (words >= 3 && words > names))
}

/// The words that make a phrase a sentence: English's closed classes, the
/// articles, pronouns, prepositions, conjunctions, auxiliaries and modals a
/// statement is built with. A phrase a person chose as a password strings
/// open words together (`correct horse battery staple`); a sentence about a
/// value (`must be at least 8 characters`) needs these to say anything.
const FUNCTION_WORDS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "it", "its", "you", "your", "we", "our",
    "they", "their", "he", "she", "his", "her", "is", "are", "was", "were", "be", "been", "being",
    "am", "has", "have", "had", "do", "does", "did", "must", "should", "can", "could", "will",
    "would", "may", "might", "shall", "not", "no", "of", "to", "in", "on", "at", "for", "with",
    "from", "by", "as", "than", "and", "or", "but", "if", "when", "which", "who",
];

/// Whether a scalar `secret` under a credential's name reads as a sentence
/// about the value (`password: must be at least 8 characters`, a block
/// scalar's lines saying the same): three or more blank-separated pieces, each
/// a plain word or a number with only sentence punctuation around it (no digit
/// or mark mixed into a word: `Hunter2`, `p@ss`), one of them a function word
/// ([`FUNCTION_WORDS`]). It is prose, not a secret. A phrase of open words
/// alone (`correct horse battery staple`, `correct horse 9`) may be a chosen
/// password and stays one.
fn reads_as_a_sentence(secret: &str) -> bool {
    let mut pieces = 0_usize;
    let mut function_word = false;
    let plain = secret.split_whitespace().all(|piece| {
        pieces += 1;
        let word = piece
            .trim_start_matches(['(', '"', '\''])
            .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '"', '\'']);
        function_word |= FUNCTION_WORDS
            .iter()
            .any(|function| word.eq_ignore_ascii_case(function));
        !word.is_empty()
            && (word.bytes().all(|byte| byte.is_ascii_digit())
                || (word.bytes().all(|byte| byte.is_ascii_alphabetic()) && is_word_part(word)))
    });
    plain && pieces >= 3 && function_word
}

/// What one blank-separated piece of a line is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Piece {
    /// A dictionary-shaped word, sentence punctuation around it and only
    /// [`WORD_JOINTS`] inside (`path,`, `line's`, `re-authenticate`, `[i]`).
    Word,
    /// A number shorter than [`NUMBER_DIGITS`] digits.
    Number,
    /// A number of [`NUMBER_DIGITS`] digits: a port or a year alone, one
    /// group of a code beside another.
    Group,
    /// Words and numbers with marks inside: an abbreviation, a dotted name,
    /// an address, a quoted or bracketed example (`e.g.`,
    /// `devDependencies.vite`, `127.0.0.1:8080`, `{"line":289}`).
    Name,
    /// Anything else: no part of a sentence.
    Opaque,
}

impl Piece {
    /// The kind of `piece`. Each part [`WORD_MARKS`] split it into is a word
    /// ([`is_word_part`]) or a number of at most [`NUMBER_DIGITS`] digits; a
    /// part of anything else makes the piece opaque, and so does the shape of
    /// data: a bare `name:value` pair ([`holds_bare_pair`]), two numbers of
    /// [`NUMBER_DIGITS`] digits (`4821-9930`), or three or more parts of one
    /// length ([`is_grouped`]: `KaToFiMe-ReDuLoKa-PiZoWaNe`).
    fn of(piece: &str) -> Self {
        let parts = || piece.split(WORD_MARKS).filter(|part| !part.is_empty());
        let is_number = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        let (mut letters, mut numbers, mut groups) = (0_usize, 0_usize, 0_usize);
        let known = parts().all(|part| {
            if is_number(part) {
                numbers += 1;
                groups += usize::from(part.len() == NUMBER_DIGITS);
                return part.len() <= NUMBER_DIGITS;
            }
            letters += 1;
            is_word_part(part)
        });
        if !known || letters + numbers == 0 || groups >= 2 {
            return Self::Opaque;
        }
        if holds_bare_pair(piece) || is_grouped(parts()) {
            return Self::Opaque;
        }
        let inner = piece.trim_matches(WORD_MARKS);
        let joined = inner
            .split(WORD_JOINTS)
            .all(|part| !part.contains(WORD_MARKS));
        match (letters, numbers) {
            (0, 1) if groups == 1 => Self::Group,
            (0, 1) => Self::Number,
            (_, 0) if joined => Self::Word,
            _ => Self::Name,
        }
    }
}

/// Whether `part`, letters between a piece's marks, is dictionary-shaped: a
/// camelCase name ([`is_identifier_name`]: `devDependencies`), one letter, or
/// a pronounceable word ([`pronounceable_word`], any case) shorter than an
/// opaque literal ([`OPAQUE_LITERAL_CHARS`]): no dictionary word a
/// description uses is that long, and a run of letters that long is a token
/// that happens to read aloud.
fn is_word_part(part: &str) -> bool {
    let bytes = part.as_bytes();
    if !bytes.iter().all(u8::is_ascii_alphabetic) {
        return false;
    }
    is_identifier_name(part)
        || bytes.len() == 1
        || (bytes.len() < OPAQUE_LITERAL_CHARS
            && pronounceable_word(part.to_ascii_lowercase().as_bytes()))
}

/// Whether `piece` holds a bare `name:value` pair (`admin:opensesame`): a
/// letter right before a colon and a letter or digit right after it. A
/// sentence puts a blank after its colon (`file: each line`), an address a
/// digit before it (`127.0.0.1:8080`) and a serialized example a quote
/// (`"line":289`).
fn holds_bare_pair(piece: &str) -> bool {
    piece
        .as_bytes()
        .windows(3)
        .any(|at| at[0].is_ascii_alphabetic() && at[1] == b':' && at[2].is_ascii_alphanumeric())
}

/// Whether `parts` are a code written in groups: three or more, all of one
/// length above [`GROUP_CHARS`] (`KaToFiMe-ReDuLoKa-PiZoWaNe`). The parts of
/// a name or a compound word vary in length.
fn is_grouped<'a>(mut parts: impl Iterator<Item = &'a str>) -> bool {
    let Some(len) = parts.next().map(str::len) else {
        return false;
    };
    let mut count = 1;
    len > GROUP_CHARS
        && parts.all(|part| {
            count += 1;
            part.len() == len
        })
        && count >= 3
}

/// Whether `line` is only groups of letters, four or more and all of one
/// length of [`GROUP_CHARS`] or more (an app password's `abcd efgh ijkl
/// mnop`, `qhxv ktzm wjrp ysbn`): a sentence's words vary in length. Words
/// do come out of one length (`date time size name`), and those are told
/// from a generated code by what a generator cannot give its letters
/// ([`reads_as_words`]).
fn is_grouped_letters(line: &str) -> bool {
    let mut groups = line.split_whitespace();
    let Some(len) = groups.next().map(str::len) else {
        return false;
    };
    let mut count = 0;
    len >= GROUP_CHARS
        && line.split_whitespace().all(|group| {
            count += 1;
            group.len() == len && group.bytes().all(|byte| byte.is_ascii_alphabetic())
        })
        && count >= 4
        && !reads_as_words(line)
}

/// The letters a language's words seldom use and a generator draws as often
/// as any other.
const RARE_LETTERS: &[u8] = b"jkqvwxz";

/// Whether the groups of letters of `line` read as words, not as letters
/// drawn at random: every group is pronounceable ([`pronounceable_word`]),
/// they do not count through the alphabet (`abcd efgh`), and fewer than one
/// letter in six is a rare one ([`RARE_LETTERS`]: a generator draws more
/// than one in four). No dictionary tells a pronounceable code from words,
/// so a code that passes all three (`tofa bime redu loka`) reads as words.
fn reads_as_words(line: &str) -> bool {
    let letters: Vec<u8> = line
        .bytes()
        .filter(u8::is_ascii_alphabetic)
        .map(|byte| byte.to_ascii_lowercase())
        .collect();
    let counted = letters
        .windows(2)
        .filter(|pair| pair[1] == pair[0] + 1)
        .count();
    let rare = letters
        .iter()
        .filter(|letter| RARE_LETTERS.contains(letter))
        .count();
    counted * 2 <= letters.len()
        && rare * 6 < letters.len()
        && line
            .split_whitespace()
            .all(|group| pronounceable_word(group.to_ascii_lowercase().as_bytes()))
}

/// Whether `line` is one serialized structure: all of it inside one pair of
/// braces or brackets, with a member inside (a `:` or `=` pair, or a quoted
/// item). A credential blob stored as a string
/// (`{"username": "admin", "password": "…"}`) is data whatever words it
/// holds; bracketed words alone (`[your key here]`) are a placeholder.
fn is_serialized_structure(line: &str) -> bool {
    [('{', '}'), ('[', ']')].into_iter().any(|(open, close)| {
        line.strip_prefix(open)
            .and_then(|rest| rest.strip_suffix(close))
            .is_some_and(|inner| inner.contains([':', '=', '"', '\'']))
    })
}

/// Whether `line` is a list of pairs (`user: admin, password: letmein`): two
/// or more clauses between `,` or `;`, each one name, a `:` or `=`, and one
/// value with no blank inside. A sentence's clause holds several words on a
/// side (`file: each hit line's key path`).
fn is_pair_list(line: &str) -> bool {
    let single = |side: &str| {
        let side = side.trim();
        !side.is_empty() && !side.contains(char::is_whitespace)
    };
    let mut clauses = 0;
    line.split([',', ';'])
        .filter(|clause| !clause.trim().is_empty())
        .all(|clause| {
            clauses += 1;
            clause
                .split_once([':', '='])
                .is_some_and(|(name, value)| single(name) && single(value))
        })
        && clauses >= 2
}

/// `secret` as the string literal it may be written in reads: an escaped
/// line break or tab (`\n`, `\r`, `\t`) is the line break it stands for, so
/// what it separates is judged apart, and an escaped mark (`\"`) the mark.
/// Any other escape stays as written, so it is no part of a word.
fn without_literal_escapes(secret: &str) -> std::borrow::Cow<'_, str> {
    if !secret.contains('\\') {
        return secret.into();
    }
    let mut read = String::with_capacity(secret.len());
    let mut chars = secret.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            read.push(ch);
            continue;
        }
        match chars.next() {
            Some('n' | 'r' | 't') => read.push('\n'),
            Some(mark) if WORD_MARKS.contains(&mark) => read.push(mark),
            Some(other) => read.extend(['\\', other]),
            None => read.push('\\'),
        }
    }
    read.into()
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
        c.is_whitespace()
            || QUOTES.contains(&c)
            || matches!(c, ':' | '=' | '>' | '|' | '?' | '<' | '-')
    };
    let head = without_literal_prefix(key).trim_end_matches(operators);
    // A constructor call that wraps the literal (`token: Some(`,
    // `String::from(`) is the value's, not the name's.
    let head = head.strip_suffix('(').map_or(head, |call| {
        // A macro's `!` is the call's too (`format!(`, `String::from(`).
        let call = call.strip_suffix('!').unwrap_or(call);
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
                && !holds_quote_or_brace(&head[at..])
        })
        .map_or_else(|| go_declared_name(head), |colon| &head[..colon]);
    name.rsplit(|c: char| c.is_whitespace() || QUOTES.contains(&c) || matches!(c, '{' | ',' | '('))
        .next()
        .unwrap_or_default()
}

/// Whether `text` holds a quote that opens a literal or a brace. A lifetime's
/// quote (`&'static str`, `Foo<'a>`) opens no literal.
fn holds_quote_or_brace(text: &str) -> bool {
    let mut before = None;
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        let lifetime = c == '\''
            && matches!(before, Some('&' | '<'))
            && chars
                .peek()
                .is_some_and(|&(_, next)| next.is_ascii_alphabetic() || next == '_');
        if !lifetime && (QUOTES.contains(&c) || matches!(c, '{' | '}')) {
            return true;
        }
        if !c.is_whitespace() {
            before = Some(c);
        }
    }
    false
}

/// The name of a declaration that puts the type after it (`var password
/// string`, Go): the word before a type word, when it names a credential and
/// the key holds no operator or bracket before them. Any other text is its
/// own name.
fn go_declared_name(head: &str) -> &str {
    let is_word = |word: &str, extra: &[u8]| {
        word.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || extra.contains(&b))
    };
    let mut words = head.rsplit(char::is_whitespace);
    match (words.next(), words.next()) {
        (Some(declared), Some(name))
            if names_a_credential_word(&name.to_ascii_lowercase())
                && is_word(declared, b".*[]")
                && is_word(name, b".-") =>
        {
            name
        }
        _ => head,
    }
}

/// Whether an unquoted `secret` opens with a code statement that assigns
/// another name: an optional `!`, `&` or `*`, a dotted path of identifier
/// words ([`is_identifier_word`], or one letter, as minified code names
/// one) and the `;` terminator, or the `,` that ends a member of an object
/// or an argument (`token: this.token,`, `password: undefined,`), at the
/// end of the value (`frame.expect_key = frame.object;`) or before the next
/// statement of minified code (`x.password=t.password;username=u`). Dotenv, INI,
/// properties and shell values carry no statement terminator, and a quoted
/// value (`key` ends in its opening quote) is a literal whatever it spells.
fn reads_a_name(key: &impl KeyFacts, secret: &str) -> bool {
    !key.ends_with_quote()
        && secret.split_once([';', ',']).is_some_and(|(name, rest)| {
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
pub(super) fn pronounceable_word(word: &[u8]) -> bool {
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
