//! Which names are a credential's, by their word parts.

use crate::security::detector::credential_values::ends_in_locator;
use crate::security::patterns::CREDENTIAL_WORDS;

/// The word parts, beside [`CREDENTIAL_WORDS`], that make a name a
/// credential's: abbreviations (`tkn`, `cred`, `pat`, `sk`), what
/// authenticates a request (`auth`, `bearer`, `cookie`, `session`)
/// and what derives a key (`seed`, `signature`).
const HOLDER_WORDS: &[&str] = &[
    "tkn",
    "auth",
    "authorization",
    "cred",
    "signature",
    "seed",
    "bearer",
    "pat",
    "cookie",
    "session",
    "sk",
];

/// The words a lowercase or uppercase name may end in with no separator
/// (`apikey`, `DBPASSWORD`, `oauth`): the abbreviations of [`HOLDER_WORDS`]
/// end too many ordinary words.
const GLUED_ENDS: &[&str] = &["auth"];

/// A word part that says the name's value is published, not held.
const PUBLIC_WORDS: &[&str] = &["public", "pub"];

/// The word parts of a callee that takes credentials by position
/// (`client.login("svc", "…")`, `getConnection(url, user, "…")`,
/// `signIn(…)`): the call signs in or connects, and what it is given to do so
/// is a credential when opaque. They name no credential: only a call reads
/// them ([`calls_with_credentials`]).
const SIGN_IN_VERBS: &[&str] = &[
    "login",
    "logon",
    "signin",
    "authenticate",
    "connect",
    "connection",
];

/// The last word part of a name that describes its credential instead of
/// holding it (`secretName`, `tokenType`, `keyLength`), or identifies it
/// (`keyFingerprint`, `privateKeyHash`: a stored password's hash has a shape
/// of its own, the `passwordHash` pattern's).
const DESCRIPTOR_ENDS: &[&str] = &[
    "name",
    "names",
    "type",
    "kind",
    "ref",
    "count",
    "length",
    "len",
    "size",
    "ttl",
    "timeout",
    "expiry",
    "expires",
    "prefix",
    "suffix",
    "header",
    "param",
    "field",
    "label",
    "version",
    "format",
    "alg",
    "algorithm",
    "pattern",
    "regex",
    "mode",
    "policy",
    "scope",
    "scopes",
    "provider",
    "issuer",
    "endpoint",
    "hash",
    "digest",
    "checksum",
    "fingerprint",
    "thumbprint",
];

/// Whether `name` names a credential: one of its word parts (split at
/// `_`, `.`, `-`, `$` and at each camelCase word, digits aside) is a
/// credential word, alone, plural, or ending a part written with no
/// separator (`apikey`). A name that says its value is public, or ends in a
/// locator or a descriptor, names none.
pub(in crate::security::detector) fn names_a_credential(name: &str) -> bool {
    if ends_in_locator(name.as_bytes()) {
        return false;
    }
    let parts = word_parts(name);
    !describes(&parts) && !publishes(&parts) && parts.iter().any(|part| is_holder_part(part))
}

/// Whether `name` describes or identifies a credential instead of holding
/// it (`secretName`, `API_KEY_HEADER`, `kmsKeyId`): see [`describes`].
pub(in crate::security::detector) fn describes_a_credential(name: &str) -> bool {
    describes(&word_parts(name))
}

/// Whether `name` says its value is published (`publicKey`, `PUBLIC_KEY_PEM`,
/// `pubkey`): see [`publishes`].
pub(in crate::security::detector) fn names_a_published_value(name: &str) -> bool {
    publishes(&word_parts(name))
}

/// Whether `name`, the callee of a call, takes credentials by position: one
/// of its word parts is a sign-in verb ([`SIGN_IN_VERBS`]), `sign` and `in`
/// written apart included (`signIn`, `sign_in`).
pub(super) fn calls_with_credentials(name: &str) -> bool {
    let parts = word_parts(name);
    parts
        .iter()
        .any(|part| SIGN_IN_VERBS.contains(&part.as_str()))
        || parts
            .windows(2)
            .any(|pair| pair[0] == "sign" && pair[1] == "in")
}

/// Whether a name of these word `parts` says its value is published: a part
/// is a public word ([`PUBLIC_WORDS`]), alone or before the credential word
/// it qualifies with no separator (`pubkey`). A word that only starts like
/// one (`publishToken`) says nothing of the kind.
fn publishes(parts: &[String]) -> bool {
    parts.iter().any(|part| {
        PUBLIC_WORDS.iter().any(|word| {
            part.strip_prefix(word)
                .is_some_and(|rest| rest.is_empty() || is_holder_part(rest))
        })
    })
}

/// Whether the line's text `before` a name ends in a word that says the
/// name's value is published (`public key: …`).
pub(super) fn follows_a_public_word(before: &str) -> bool {
    let before = before.trim_end();
    let word = before
        .rfind(|c: char| !c.is_ascii_alphabetic())
        .map_or(before, |at| before.get(at + 1..).unwrap_or_default());
    PUBLIC_WORDS
        .iter()
        .any(|public| word.eq_ignore_ascii_case(public))
}

/// Whether a name of these word `parts` describes a thing instead of
/// holding it: its last part is a descriptor ([`DESCRIPTOR_ENDS`]), or an
/// id, which names its thing. A session's id is the session.
fn describes(parts: &[String]) -> bool {
    let Some((last, before)) = parts.split_last() else {
        return false;
    };
    // One descriptor or several (`tokenProviders`): the plural describes too.
    let singular = last.strip_suffix('s').unwrap_or(last);
    DESCRIPTOR_ENDS.contains(&last.as_str())
        || DESCRIPTOR_ENDS.contains(&singular)
        || (singular == "id" && before.last().is_none_or(|part| part != "session"))
}

/// Whether a word part of `text` (split as a name's are: [`word_parts`]) is
/// one of [`CREDENTIAL_WORDS`], alone or plural: the text names a
/// credential (`errors.secret_missing`, `db-password`), as `monkey` or
/// `bypass` does not.
pub(in crate::security::detector) fn spells_a_credential_word(text: &str) -> bool {
    word_parts(text).iter().any(|part| {
        let singular = part.strip_suffix('s').unwrap_or(part);
        CREDENTIAL_WORDS
            .split('|')
            .any(|word| word == part || word == singular)
    })
}

/// Whether `name` names several credentials: its last word part is a
/// plural (`apiKeys`, `my_secrets`, `TOKENS`). Such a collection maps names
/// to credentials; one under a singular name (`auth`, `secret`) is one
/// credential's record.
pub(super) fn names_several(name: &str) -> bool {
    word_parts(name)
        .last()
        .is_some_and(|part| part.ends_with('s') && !part.ends_with("ss"))
}

/// Whether `name`, a member's inside a credential's collection, says its
/// value is none: a locator, a descriptor or an id.
pub(super) fn names_no_value(name: &str) -> bool {
    ends_in_locator(name.as_bytes()) || describes(&word_parts(name))
}

/// Whether lowercase `part` is a credential word, its plural, or a word
/// ending in one of [`CREDENTIAL_WORDS`] or [`GLUED_ENDS`].
fn is_holder_part(part: &str) -> bool {
    let singular = if part.ends_with("ss") {
        part
    } else {
        part.strip_suffix('s').unwrap_or(part)
    };
    let words = CREDENTIAL_WORDS.split('|');
    words
        .clone()
        .chain(HOLDER_WORDS.iter().copied())
        .any(|word| word == part || word == singular)
        || words
            .chain(GLUED_ENDS.iter().copied())
            .any(|word| singular.len() > word.len() && singular.ends_with(word))
}

/// The word parts of `name`, lowercase, without the digits that number them.
fn word_parts(name: &str) -> Vec<String> {
    let mut parts = Vec::new();
    for chunk in name.split(|c: char| !c.is_ascii_alphanumeric()) {
        for piece in letter_pieces(chunk.as_bytes()) {
            parts.push(String::from_utf8_lossy(piece).to_ascii_lowercase());
        }
    }
    parts
}

/// The runs of letters in `bytes`, each cut where a camelCase word starts:
/// before a capital after a lowercase letter, and before the last capital of
/// a run of capitals that a lowercase letter follows (`APIKey`: `API`,
/// `Key`).
pub(super) fn letter_pieces(bytes: &[u8]) -> Vec<&[u8]> {
    let mut pieces = Vec::new();
    let mut start = None;
    for at in 0..=bytes.len() {
        let letter = bytes.get(at).is_some_and(u8::is_ascii_alphabetic);
        let Some(from) = start else {
            if letter {
                start = Some(at);
            }
            continue;
        };
        let cut = !letter
            || (at > from
                && bytes[at].is_ascii_uppercase()
                && (bytes[at - 1].is_ascii_lowercase()
                    || bytes.get(at + 1).is_some_and(u8::is_ascii_lowercase)));
        if cut {
            pieces.push(&bytes[from..at]);
            start = letter.then_some(at);
        }
    }
    pieces
}
