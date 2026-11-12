//! JSON walker that redacts every domain string leaf — including `location`
//! values and the `query`/`why` leaves of executable `next` continuations —
//! while preserving object structure and the executable `tool` identifier.
//!
//! Every leaf is handed to the sanitizer with the `path` of the nearest
//! enclosing object that names one (a returned file's content, a search
//! match's line): the path says what language the text is in, and nothing
//! else about the net is relaxed.
//!
//! Redaction is a no-op unless a real secret matches, so scanning continuation
//! and location leaves is safe: it fires only on an actual secret (better to
//! break a continuation than to leak a credential), and the `tool` field that
//! selects the next call is always left verbatim. What a continuation carries
//! of the runtime's own state (an issued handle, a query row's `x-cursor`
//! fields) is never judged: no file, provider or request wrote it.
//!
//! A secret is often recognisable only by its context (`awsSecret = "…"`),
//! while another leaf about the same source repeats the bare value (an
//! astSearch capture, a hover fragment). Every value the sanitizer redacted
//! is therefore redacted again where a leaf about that source repeats it;
//! the leaf it was judged in keeps what its own rules decided, and a leaf of
//! another source is judged by its own rules alone.
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Shortest redacted value propagated to other leaves; shorter values are too
/// generic to identify as the same secret.
const MIN_PROPAGATED_SECRET_CHARS: usize = 8;

/// What a sanitizer returns for one string: its sanitized text (`String`),
/// or that text only when it differs (`Option`, `None`: unchanged).
pub(crate) trait Rewrite {
    /// The new text; `None` when it equals `original`.
    fn change(self, original: &str) -> Option<String>;
}

impl Rewrite for String {
    fn change(self, original: &str) -> Option<String> {
        (self != original).then_some(self)
    }
}

impl Rewrite for Option<String> {
    fn change(self, original: &str) -> Option<String> {
        self.filter(|text| text != original)
    }
}

/// Each object key's change within one walk, by key text. A sanitizer is a
/// pure function of its text and path, and a key is scanned without a path,
/// so a key every row repeats (`line`, `value`, `path`) is scanned once.
type KeyChanges = HashMap<String, Option<String>>;

/// `sanitize` gets each string leaf and the file path of its enclosing result,
/// if any, and must be a pure function of the two: an object key repeated in
/// the value is handed to it once. Returns whether any data changed.
pub(crate) fn sanitize_json<E, R: Rewrite>(
    value: &mut Value,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<R, E>,
) -> Result<bool, E> {
    sanitize_judged_json(
        value,
        &super::JudgedLeaves::default(),
        &super::Admitted::default(),
        Messages::Judged,
        &mut |text, path, _| sanitize(text, path),
    )
}

/// How a walk treats the runtime's own messages of a response envelope
/// ([`crate::contracts::envelope::message_paths`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Messages {
    /// Judged like every other leaf.
    Judged,
    /// Never judged as text of the row's file: out of the structured pass,
    /// judged by the pathless net alone, and a value the walk hid in another
    /// leaf is hidden where a message repeats it. A message builds the
    /// outside text it quotes (a provider's error, a server's diagnostic, a
    /// file's rule or path) through [`super::quoted`]; until every site
    /// does, the pathless net stays the backstop for the rest.
    Authored,
}

/// Leaves taken out of a value while it is judged, each with its JSON
/// pointer; `null` stands in their place until they are put back.
struct Held(Vec<(String, Value)>);

impl Held {
    fn take(value: &mut Value, paths: Vec<Vec<String>>) -> Self {
        Self(
            paths
                .iter()
                .filter_map(|path| {
                    let pointer = crate::json_pointer::of(path);
                    let leaf = std::mem::take(value.pointer_mut(&pointer)?);
                    Some((pointer, leaf))
                })
                .collect(),
        )
    }

    /// Each leaf back at its pointer. Judging replaces strings and renames
    /// only keys that hold a secret, never the runtime's own keys a pointer
    /// names; a value the gate replaced whole (a scan failure) takes none.
    fn restore(self, value: &mut Value) {
        for (pointer, leaf) in self.0 {
            if let Some(slot) = value.pointer_mut(&pointer).filter(|slot| slot.is_null()) {
                *slot = leaf;
            }
        }
    }
}

/// [`sanitize_json`] where `sanitize` also gets, for a leaf `judged` marks
/// (and whose text still has the marked digest), the path its whole source
/// was judged at. Marks are resolved after the structured pass, which may
/// replace a leaf; each is taken once, as the walk reaches its leaf. A value
/// redacted in one leaf is redacted where a leaf about the same source
/// repeats it ([`Hidden`]), except inside a string of `own`, the request's
/// admitted text, that a leaf repeats ([`super::Admitted::replace_outside`]).
///
/// The runtime state of every continuation call
/// ([`crate::contracts::envelope::state_paths`]) is held out of the
/// whole walk: it is no text of any file, provider or request, and a
/// placeholder there would only break the call. With
/// [`Messages::Authored`], the envelope's own messages are held out of the
/// structured pass and the file-context walk ([`Messages::Authored`]).
pub(crate) fn sanitize_judged_json<E, R: Rewrite>(
    value: &mut Value,
    judged: &super::JudgedLeaves,
    own: &super::Admitted,
    messages: Messages,
    sanitize: &mut impl FnMut(&str, Option<&str>, Option<&str>) -> Result<R, E>,
) -> Result<bool, E> {
    let state = Held::take(value, crate::contracts::envelope::state_paths(value));
    let changed = judge_json(value, judged, own, messages, sanitize);
    state.restore(value);
    changed
}

fn judge_json<E, R: Rewrite>(
    value: &mut Value,
    judged: &super::JudgedLeaves,
    own: &super::Admitted,
    messages: Messages,
    sanitize: &mut impl FnMut(&str, Option<&str>, Option<&str>) -> Result<R, E>,
) -> Result<bool, E> {
    let mut authored = match messages {
        Messages::Authored => Held::take(value, crate::contracts::envelope::message_paths(value)),
        Messages::Judged => Held(Vec::new()),
    };
    let mut redacted: Vec<Hidden> = Vec::new();
    let mut changed;
    match octocode_engine::security::sanitizer::sanitize_structure(value) {
        Ok(removed) => {
            changed = !removed.is_empty();
            let mut values = Vec::new();
            for (secret, placeholder) in removed {
                push_value(&secret, &placeholder, &mut values);
            }
            redacted.extend(values.into_iter().map(|(secret, placeholder)| Hidden {
                secret,
                placeholder,
                source: Reach::Everywhere,
                origin: None,
            }));
        }
        Err(_) => {
            *value = Value::String(octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.into());
            return Ok(true);
        }
    }
    let mut marks = if judged.is_empty() {
        HashMap::new()
    } else {
        judged.resolve(value)
    };
    let judging = walk(value, None, &mut KeyChanges::new(), &mut |text: &str,
                                                                  path: Option<
        &str,
    >| {
        let source = marks.remove(&(text.as_ptr() as usize));
        let clean = sanitize(text, path, source)?.change(text);
        if let Some(clean) = &clean {
            changed = true;
            Hidden::of_leaf(text, clean, path, &mut redacted);
        }
        Ok(clean)
    });
    let judging = judging.and_then(|()| {
        for (_, message) in &mut authored.0 {
            let Value::String(text) = message else {
                continue;
            };
            if let Some(clean) = sanitize(text, None, None)?.change(text) {
                changed = true;
                Hidden::of_leaf(text, &clean, None, &mut redacted);
                *text = clean;
            }
        }
        Ok(())
    });
    authored.restore(value);
    judging?;
    if redacted.is_empty() {
        return Ok(changed);
    }
    // Longest first, so a value never leaves the tail of a longer one behind.
    redacted.sort_by(|a, b| {
        b.secret
            .len()
            .cmp(&a.secret.len())
            .then_with(|| a.secret.cmp(&b.secret))
            .then_with(|| a.source.cmp(&b.source))
    });
    redacted.dedup_by(|a, b| a.secret == b.secret && a.source == b.source && a.origin == b.origin);
    walk(value, None, &mut KeyChanges::new(), &mut |text: &str,
                                                    path: Option<
        &str,
    >| {
        let mut changed: Option<String> = None;
        let digest = std::cell::OnceCell::new();
        for hidden in &redacted {
            if !hidden.reaches(text, path, &digest) {
                continue;
            }
            let current = changed.as_deref().unwrap_or(text);
            if let Some(replaced) =
                own.replace_outside(current, &hidden.secret, &hidden.placeholder)
            {
                changed = Some(replaced);
            }
        }
        Ok(changed)
    })?;
    Ok(changed)
}

/// A value the walk hid, and where else it is hidden: in the leaves about the
/// same source (`source`: the path the leaf was judged with, a fragment of
/// that file such as a capture or a hover), except the leaf it was judged in
/// (`origin`, the digest of its judged text), whose own rules already said
/// which of its occurrences are secret. A value a JSON member's name hid
/// ([`Reach::Everywhere`]) is hidden wherever it recurs.
struct Hidden {
    secret: String,
    placeholder: String,
    source: Reach,
    origin: Option<[u8; 32]>,
}

/// Which leaves a hidden value is hidden in.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Reach {
    /// Every leaf: a JSON member's name hid it.
    Everywhere,
    /// The leaves about this source (`None`: about no known file).
    Source(Option<String>),
}

impl Hidden {
    fn of_leaf(original: &str, clean: &str, path: Option<&str>, out: &mut Vec<Self>) {
        let mut values = Vec::new();
        redacted_values(original, clean, &mut values);
        let origin = octocode_engine::digest::sha256_digest(clean.as_bytes());
        out.extend(values.into_iter().map(|(secret, placeholder)| Self {
            secret,
            placeholder,
            source: Reach::Source(path.map(str::to_owned)),
            origin: Some(origin),
        }));
    }

    /// Whether this value is hidden in the leaf `text`, about `path`.
    fn reaches(
        &self,
        text: &str,
        path: Option<&str>,
        digest: &std::cell::OnceCell<[u8; 32]>,
    ) -> bool {
        if let Reach::Source(source) = &self.source
            && source.as_deref() != path
        {
            return false;
        }
        self.origin.is_none_or(|origin| {
            *digest.get_or_init(|| octocode_engine::digest::sha256_digest(text.as_bytes()))
                != origin
        })
    }
}

/// The original values behind each placeholder of `clean`, paired with that
/// placeholder. Redaction keeps line count, so lines align one to one; a line
/// the redaction emptied (a multi-line secret's body) is wholly secret.
fn redacted_values(original: &str, clean: &str, out: &mut Vec<(String, String)>) {
    let original_lines: Vec<&str> = original.split('\n').collect();
    let clean_lines: Vec<&str> = clean.split('\n').collect();
    if original_lines.len() != clean_lines.len() {
        return;
    }
    let mut last_placeholder: Option<&str> = None;
    for (before, after) in original_lines.into_iter().zip(clean_lines) {
        if before == after {
            continue;
        }
        if after.trim().is_empty() {
            if let Some(placeholder) = last_placeholder {
                push_value(before.trim(), placeholder, out);
            }
            continue;
        }
        let placeholders: Vec<_> = octocode_engine::security::placeholder_regex()
            .find_iter(after)
            .collect();
        let Some(last) = placeholders.last() else {
            continue;
        };
        last_placeholder = Some(last.as_str());
        let Some(rest) = before.strip_prefix(&after[..placeholders[0].start()]) else {
            continue;
        };
        let tail = &after[last.end()..];
        let Some(mut rest) = rest.strip_suffix(tail) else {
            continue;
        };
        // Between two placeholders the kept text separates their values; an
        // empty separator leaves the two values joined under the first.
        for (index, placeholder) in placeholders.iter().enumerate() {
            let separator = placeholders
                .get(index + 1)
                .map(|next| &after[placeholder.end()..next.start()])
                .filter(|separator| !separator.is_empty());
            match separator.and_then(|separator| rest.find(separator).map(|at| (at, separator))) {
                Some((at, separator)) => {
                    push_value(&rest[..at], placeholder.as_str(), out);
                    rest = &rest[at + separator.len()..];
                }
                None => {
                    push_value(rest, placeholder.as_str(), out);
                    break;
                }
            }
        }
    }
}

fn push_value(value: &str, placeholder: &str, out: &mut Vec<(String, String)>) {
    if value.chars().count() >= MIN_PROPAGATED_SECRET_CHARS
        && value.chars().any(char::is_alphanumeric)
        && !octocode_engine::security::placeholder_regex().is_match(value)
    {
        out.push((value.to_owned(), placeholder.to_owned()));
    }
}

/// `sanitize` returns a string's new text, `None` when it is unchanged.
fn walk<E>(
    value: &mut Value,
    path: Option<&str>,
    keys: &mut KeyChanges,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<Option<String>, E>,
) -> Result<(), E> {
    match value {
        Value::String(text) => {
            if let Some(clean) = sanitize(text, path)? {
                *text = clean;
            }
            Ok(())
        }
        Value::Array(values) => {
            for child in values {
                walk(child, path, keys, sanitize)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            // Read before any redaction rewrites the entry: only its extension
            // is consulted.
            let own = map.get("path").and_then(Value::as_str).map(str::to_owned);
            let path = own.as_deref().or(path);
            sanitize_keys(map, keys, sanitize)?;
            for (key, child) in map.iter_mut() {
                match key.as_str() {
                    // `location` is scanned like any other subtree — its string
                    // leaves (e.g. a cloned location.path) can carry a secret.
                    "next" => sanitize_next_map(child, path, keys, sanitize)?,
                    _ => walk(child, path, keys, sanitize)?,
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Redact secrets in object keys (content-derived keys such as paths or ids
/// can carry one). The map is rebuilt in order only when a key changes; a
/// redacted key that collides with another gets a ` #n` suffix so no entry
/// is dropped.
fn sanitize_keys<E>(
    map: &mut Map<String, Value>,
    keys: &mut KeyChanges,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<Option<String>, E>,
) -> Result<(), E> {
    let mut renamed = Vec::new();
    for (index, key) in map.keys().enumerate() {
        let change = match keys.get(key) {
            Some(change) => change.clone(),
            None => {
                // A key is a name, never a file's text: no path context.
                let change = sanitize(key, None)?;
                keys.insert(key.clone(), change.clone());
                change
            }
        };
        if let Some(clean) = change {
            renamed.push((index, clean));
        }
    }
    if renamed.is_empty() {
        return Ok(());
    }
    let mut renamed = renamed.into_iter().peekable();
    let entries = std::mem::take(map);
    for (index, (key, value)) in entries.into_iter().enumerate() {
        let key = match renamed.next_if(|(at, _)| *at == index) {
            Some((_, clean)) => clean,
            None => key,
        };
        let mut unique = key.clone();
        let mut n = 2;
        while map.contains_key(&unique) {
            unique = format!("{key} #{n}");
            n += 1;
        }
        map.insert(unique, value);
    }
    Ok(())
}

fn sanitize_next_map<E>(
    next: &mut Value,
    path: Option<&str>,
    keys: &mut KeyChanges,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<Option<String>, E>,
) -> Result<(), E> {
    let Some(map) = next.as_object_mut() else {
        return walk(next, path, keys, sanitize);
    };
    // Flat form: `next` is itself a single executable call (`{tool, query, …}`).
    if map.get("tool").is_some_and(Value::is_string) {
        return sanitize_call_fields(map, path, keys, sanitize);
    }
    // Named-call form: each value under `next` is a call object.
    for call in map.values_mut() {
        match call.as_object_mut() {
            Some(fields) => sanitize_call_fields(fields, path, keys, sanitize)?,
            None => walk(call, path, keys, sanitize)?,
        }
    }
    Ok(())
}

/// Sanitize every leaf of a continuation call (`query`, `why`, …) except the
/// `tool` identifier, which must survive verbatim so the call stays executable.
fn sanitize_call_fields<E>(
    fields: &mut Map<String, Value>,
    path: Option<&str>,
    keys: &mut KeyChanges,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<Option<String>, E>,
) -> Result<(), E> {
    for (key, value) in fields.iter_mut() {
        if key == "tool" {
            continue;
        }
        walk(value, path, keys, sanitize)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod placeholder_coverage_tests;
