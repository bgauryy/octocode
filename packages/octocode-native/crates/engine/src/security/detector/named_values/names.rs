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
/// locator or a descriptor, names none, and so does a header the user agent
/// fills ([`names_a_user_agent_field`]).
pub(in crate::security::detector) fn names_a_credential(name: &str) -> bool {
    if ends_in_locator(name.as_bytes()) || names_a_user_agent_field(name) {
        return false;
    }
    let parts = word_parts(name);
    !describes(&parts) && !publishes(&parts) && parts.iter().any(|part| is_holder_part(part))
}

/// Whether `name` is a header of the HTTP `Sec-` namespace: hyphen-joined
/// words, the first `Sec`, then the feature and the field it names
/// (`Sec-WebSocket-Key`, `sec-websocket-accept`). Fetch reserves these names
/// for the user agent, which fills them per request (a handshake's nonce),
/// so the name alone says no application holds a credential under one; a
/// value of no nonce's shape says otherwise ([`holds_more_than_a_nonce`]).
/// `Sec` before a lone word (`Sec-Key`, `SEC_KEY`) only abbreviates the
/// name's own word.
pub(in crate::security::detector) fn names_a_user_agent_field(name: &str) -> bool {
    let mut words = name.split('-');
    words
        .next()
        .is_some_and(|first| first.eq_ignore_ascii_case("sec"))
        && words.clone().count() >= 2
        && words.all(|word| !word.is_empty() && word.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Whether `value` under `name` is the nonce a user agent fills a field of
/// the `Sec-` namespace with ([`names_a_user_agent_field`]; the name may
/// keep its operator, `Sec-WebSocket-Key: `): base64 of 16 random bytes, 22
/// characters of the base64 alphabet and `==` (RFC 6455's handshake key).
/// Only that shape is the user agent's: any other value under such a name
/// (`Sec-WebSocket-Key: <a token>`) is held as under any credential's name
/// ([`holds_more_than_a_nonce`]).
pub(in crate::security::detector) fn is_user_agent_nonce(name: &str, value: &str) -> bool {
    let name = name.trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ':' | '=' | '"' | '\''));
    names_a_user_agent_field(name) && is_nonce_shape(value)
}

/// Whether `value` has the shape of the nonce a user agent fills a `Sec-`
/// field with: base64 of 16 random bytes, 22 characters of the base64
/// alphabet and `==` ([`is_user_agent_nonce`]).
pub(in crate::security::detector) fn is_nonce_shape(value: &str) -> bool {
    value.strip_suffix("==").is_some_and(|body| {
        body.len() == NONCE_BASE64_CHARS
            && body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/'))
    })
}

/// Base64 characters of a 16-byte nonce before its `==` padding.
const NONCE_BASE64_CHARS: usize = 22;

/// Whether `value` under `name`, a field of the `Sec-` namespace whose own
/// words name a credential (`Sec-WebSocket-Key`), is no nonce the user agent
/// fills ([`is_user_agent_nonce`]): the name holds it as a credential's name
/// does, which [`names_a_credential`] cannot say from the name alone.
pub(in crate::security::detector) fn holds_more_than_a_nonce(name: &str, value: &str) -> bool {
    names_a_user_agent_field(name)
        && name
            .split_once('-')
            .is_some_and(|(_, field)| names_a_credential(field))
        && !is_user_agent_nonce(name, value)
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
