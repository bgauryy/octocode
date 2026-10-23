//! Which values assigned to a credential-named key are credentials: a code
//! name, a file digest or a statement reading another name is kept.

use std::sync::LazyLock;

use super::PATTERNS;
use super::is_source_code_path;

/// Patterns that take any value assigned to a credential-named key. Only the
/// key's name says "credential", so a value that is not one is kept: a value
/// that spells a code identifier
/// (`"x-kubernetes-patch-merge-key": "resourceClaimName"`) names a field, and
/// a hex digest keyed by a file path (`"bin/with_credentials.mjs": "<sha256>"`)
/// is a checksum manifest entry. See [`assigns_a_credential`].
const CREDENTIAL_ASSIGNMENT_PATTERNS: &[&str] =
    &["envVarSecrets", "dotenvSecretAssignment", "yamlSecretValue"];

/// Index-aligned with PATTERNS: the pattern keeps values that are not credentials.
pub(super) static KEEPS_NON_CREDENTIAL_VALUES: LazyLock<Vec<bool>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .map(|p| CREDENTIAL_ASSIGNMENT_PATTERNS.contains(&p.name))
        .collect()
});

/// Whether a credential-assignment match assigns a credential: its `secret`
/// value is not an identifier name ([`is_identifier_name`]), not a hex
/// digest keyed by a file path ([`is_file_digest`]), not a statement reading
/// another name ([`reads_a_name`]) and not a phrase of plain words under an
/// opaque-string key ([`is_plain_phrase`]). `key` is the line's text before
/// the value (ending in the key, its quotes and its operator).
pub(super) fn assigns_a_credential(key: &str, secret: &str) -> bool {
    !is_identifier_name(secret)
        && !is_file_digest(key, secret)
        && !reads_a_name(key, secret)
        && !is_plain_phrase(key, secret)
}

/// Credential words that name a value a person may choose as a phrase.
const PHRASE_CREDENTIAL_WORDS: &[&str] = &["secret", "password", "passwd", "passphrase"];

/// Whether `secret` is a phrase of plain words (`meta form value 1`,
/// `Enter your API key here`) assigned to a key whose credential word names
/// an opaque machine string: `key`, `token` or `credential`. API keys,
/// tokens and credential strings never put spaces between dictionary
/// words, so such a phrase is a label, a placeholder or test data. A
/// `password`, `passphrase` or `secret` may be a phrase a person chose
/// (`correct horse battery staple`), so a key naming one keeps its phrase
/// redacted. A plain word is a pronounceable word ([`pronounceable_word`],
/// any case, one trailing punctuation mark) or a number of at most four
/// digits; at least two words and no other piece (`Bearer <token>` keeps
/// its token piece, so it stays a credential).
fn is_plain_phrase(key: &str, secret: &str) -> bool {
    let name = key_name(key).to_ascii_lowercase();
    if PHRASE_CREDENTIAL_WORDS
        .iter()
        .any(|word| name.contains(word))
    {
        return false;
    }
    let mut words = 0;
    let mut pieces = 0;
    let plain = secret.split_whitespace().all(|piece| {
        pieces += 1;
        let piece = piece.strip_suffix(['.', ',', ';', ':', '!', '?']).unwrap_or(piece);
        if !piece.is_empty() && piece.len() <= 4 && piece.bytes().all(|byte| byte.is_ascii_digit()) {
            return true;
        }
        words += 1;
        pronounceable_word(piece.to_ascii_lowercase().as_bytes())
    });
    plain && pieces >= 2 && words >= 2
}

/// The key a credential assignment names: the last word of the line's text
/// before the value, without its quotes and operator (`"db.password": ` →
/// `db.password`).
fn key_name(key: &str) -> &str {
    key.trim_end_matches(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ':' | '=' | '>'))
        .rsplit(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '{' | ',' | '('))
        .next()
        .unwrap_or_default()
}

/// Whether an unquoted `secret` ends a code statement that assigns another
/// name: an optional `!`, `&` or `*`, a dotted path of identifier words
/// ([`is_identifier_word`]) and the `;` terminator
/// (`frame.expect_key = frame.object;`). Dotenv, INI, properties and shell
/// values carry no statement terminator, and a quoted value (`key` ends in
/// its opening quote) is a literal whatever it spells.
fn reads_a_name(key: &str, secret: &str) -> bool {
    !key.ends_with(['"', '\''])
        && secret.strip_suffix(';').is_some_and(|name| {
            name.trim_start_matches(['!', '&', '*'])
                .split('.')
                .all(is_identifier_word)
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
fn is_file_digest(key: &str, secret: &str) -> bool {
    let digest = matches!(secret.len(), 32 | 40 | 64 | 128)
        && secret.bytes().all(|byte| byte.is_ascii_hexdigit());
    let name = key_name(key);
    digest
        && ((name.contains(['/', '\\']) && has_file_extension(name)) || is_source_code_path(name))
}

/// Whether the last segment of `path` is a file name with an extension: a
/// stem, then a dot and up to 10 ASCII letters or digits, a letter among them.
fn has_file_extension(path: &str) -> bool {
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
