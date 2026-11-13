//! Provider answers kept across requests and processes: what makes a repeat
//! cheap. GitHub and the registries answer through the same store and the same
//! freshness rule ([`octocode_github::SavedAnswer::reuse`]).
//!
//! An answer with nothing any read would redact ([`gate_keeps`]) is kept in the
//! storage mode's store: on disk when persistent, in this runtime when
//! memory-only. Any other answer stays in this process's memory in either mode,
//! so no credential a provider returned reaches a file; every read of it passes
//! the output gate on its way out. A kept answer is keyed by the origin and the
//! exact credential that received it.
//!
//! A stored answer's body lives apart from it, by content: confirming an answer
//! rewrites no body, and a continuation's evidence links the same file
//! ([`crate::cache::Store::link`]). The credential keys the answer that names a
//! body, so a body is reached only through a credential that received it.
//! Answers that share content share its body, listed by each body's
//! references: a body leaves with the last answer that names it.
use crate::cache::{CacheClass, CacheConfig, CacheKey, DiskCodec, Store};
use crate::security::{ContentSecurity, sanitize_json};
use octocode_engine::num::saturating_i64;
use octocode_github::{AnswerBody, ProviderAnswer, SavedAnswer, SavedAnswers};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::Duration;

/// An answer unused this long is dropped: a later request reads it again. This
/// bounds how long private bytes sit on disk, on top of the store's byte budget
/// and `cache clear`; it says nothing about freshness.
const UNUSED_RETENTION: CacheClass = CacheClass::Idle {
    ttl: Duration::from_secs(7 * 24 * 60 * 60),
};

const NAMESPACE: &str = "provider-answers";

/// A stored answer: its body is left out and named by digest.
#[derive(Deserialize, Serialize)]
struct Sealed {
    saved: SavedAnswer,
    body: String,
}

/// A body as its raw text.
pub(crate) struct BodyCodec;

impl DiskCodec<AnswerBody> for BodyCodec {
    fn encode(value: &AnswerBody) -> Option<Vec<u8>> {
        Some(value.as_bytes().to_vec())
    }

    fn decode(bytes: &[u8]) -> Option<AnswerBody> {
        std::str::from_utf8(bytes).ok().map(AnswerBody::from)
    }
}

/// The content address of a body.
pub(crate) fn body_digest(body: &AnswerBody) -> String {
    octocode_engine::digest::sha256(body.as_bytes())
}

/// Where a body with this digest is stored.
pub(crate) fn body_key(digest: &str) -> CacheKey {
    CacheKey {
        namespace: "provider-answer-bodies".into(),
        scope: String::new(),
        resource: digest.into(),
    }
}

/// Where the answers naming the body with this digest are listed.
fn references_key(digest: &str) -> CacheKey {
    CacheKey {
        namespace: "provider-answer-body-references".into(),
        scope: String::new(),
        resource: digest.into(),
    }
}

/// How a body's references name one answer, without its request text.
fn reference(scope: &str, request: &str) -> String {
    octocode_engine::digest::sha256(format!("{}:{scope}{request}", scope.len()).as_bytes())
}

/// The store behind [`SavedAnswers`].
pub(crate) struct KeptAnswers {
    sealed: Store<Sealed>,
    /// Shared by content across requests and credentials.
    bodies: Store<AnswerBody, BodyCodec>,
    /// The answers naming each body, beside it. A body without a list is
    /// named by answers this store cannot tell, so only retention and the
    /// byte budget retire it.
    references: Store<BTreeSet<String>>,
    /// Orders this process's reference updates; the entry lock orders other
    /// processes' ([`Store::with_entry_lock`]).
    listing: Mutex<()>,
    held: Store<SavedAnswer>,
    security: ContentSecurity,
}

impl KeptAnswers {
    /// `disk: None` keeps everything in this runtime (`storage.mode` memory).
    pub(crate) fn new(disk: Option<PathBuf>) -> Self {
        Self {
            sealed: Store::new(CacheConfig::derived(), disk.clone()),
            bodies: Store::new(CacheConfig::derived(), disk.clone()),
            references: Store::new(CacheConfig::derived(), disk),
            listing: Mutex::new(()),
            held: Store::new(CacheConfig::derived(), None),
            security: ContentSecurity::new(),
        }
    }

    fn key(scope: &str, request: &str) -> CacheKey {
        CacheKey {
            namespace: NAMESPACE.into(),
            scope: scope.into(),
            resource: request.into(),
        }
    }

    /// The body named `digest`, exactly: a file that no longer holds it is gone.
    fn body(&self, digest: &str) -> Option<AnswerBody> {
        let key = body_key(digest);
        let body = self.bodies.get(&key)?.value;
        if body_digest(&body) != digest {
            self.bodies.remove(&key);
            return None;
        }
        Some((*body).clone())
    }

    /// Run `update` while no other writer changes `digest`'s references.
    fn exclusive<T>(&self, digest: &str, update: impl FnOnce(CacheKey) -> T) -> Option<T> {
        let _turn = self.listing.lock().unwrap_or_else(PoisonError::into_inner);
        let key = references_key(digest);
        self.references
            .with_entry_lock(&key, || update(key.clone()))
    }

    /// Keep `body` under `digest` for the answer `reference`. False leaves the
    /// answer without it: the body or its reference could not be kept.
    fn retain(&self, digest: &str, reference: &str, body: AnswerBody) -> bool {
        self.exclusive(digest, |key| {
            let stored = body_key(digest);
            let existed = self.bodies.refresh(&stored);
            let length = body.len();
            if !existed
                && !self
                    .bodies
                    .put_confirmed(stored.clone(), body, length, UNUSED_RETENTION)
                    .1
            {
                return false;
            }
            let listed = self.references.get(&key);
            // A kept body nobody lists is named by answers this store cannot
            // tell: listing only this one would retire theirs with it.
            if existed && listed.is_none() {
                return true;
            }
            let mut names = listed.map_or_else(BTreeSet::new, |listed| (*listed.value).clone());
            if !names.insert(reference.to_owned()) {
                // Already listed: a confirmation only keeps the list alive.
                return self.references.refresh(&key);
            }
            let bytes = names.iter().map(String::len).sum();
            let listed = self
                .references
                .put_confirmed(key, names, bytes, UNUSED_RETENTION)
                .1;
            if !listed && !existed {
                self.bodies.remove(&stored);
            }
            listed
        })
        .unwrap_or(false)
    }

    /// The answer `reference` no longer names `digest`'s body, which leaves
    /// with the last answer listed for it.
    fn release(&self, digest: &str, reference: &str) {
        self.exclusive(digest, |key| {
            let Some(listed) = self.references.get(&key) else {
                return;
            };
            let mut names = (*listed.value).clone();
            if !names.remove(reference) {
                return;
            }
            if names.is_empty() {
                self.references.remove(&key);
                self.bodies.remove(&body_key(digest));
                return;
            }
            let bytes = names.iter().map(String::len).sum();
            self.references
                .put_confirmed(key, names, bytes, UNUSED_RETENTION);
        });
    }
}

impl SavedAnswers for KeptAnswers {
    fn load(&self, scope: &str, request: &str) -> Option<SavedAnswer> {
        let key = Self::key(scope, request);
        if let Some(kept) = self.held.get(&key) {
            return Some((*kept.value).clone());
        }
        let kept = self.sealed.get(&key)?.value;
        let Some(body) = self.body(&kept.body) else {
            self.sealed.remove(&key);
            self.release(&kept.body, &reference(scope, request));
            return None;
        };
        let mut saved = kept.saved.clone();
        saved.answer.body = body;
        Some(saved)
    }

    fn save(&self, scope: &str, request: &str, mut saved: SavedAnswer) {
        let key = Self::key(scope, request);
        let reference = reference(scope, request);
        let previous = self.sealed.get(&key).map(|kept| kept.value.body.clone());
        let bytes = saved.answer.request.len()
            + saved
                .answer
                .headers
                .iter()
                .map(|(name, value)| name.len() + value.len())
                .sum::<usize>();
        // The other store's older answer must not outlive this one.
        if !gate_keeps(&self.security, &saved.answer) {
            self.sealed.remove(&key);
            if let Some(previous) = previous {
                self.release(&previous, &reference);
            }
            let bytes = bytes + saved.answer.body.len();
            self.held.put_confirmed(key, saved, bytes, UNUSED_RETENTION);
            return;
        }
        self.held.remove(&key);
        let digest = body_digest(&saved.answer.body);
        let body = std::mem::take(&mut saved.answer.body);
        let kept = self.retain(&digest, &reference, body)
            && self
                .sealed
                .put_confirmed(
                    key.clone(),
                    Sealed {
                        saved,
                        body: digest.clone(),
                    },
                    bytes,
                    UNUSED_RETENTION,
                )
                .1;
        if !kept {
            // The newer origin answer superseded the older one even when it
            // cannot be kept itself.
            self.sealed.remove(&key);
            self.release(&digest, &reference);
        }
        if let Some(previous) = previous.filter(|previous| *previous != digest) {
            self.release(&previous, &reference);
        }
    }

    fn forget(&self, scope: &str, request: &str) {
        let key = Self::key(scope, request);
        self.held.remove(&key);
        let previous = self.sealed.get(&key).map(|kept| kept.value.body.clone());
        self.sealed.remove(&key);
        if let Some(previous) = previous {
            self.release(&previous, &reference(scope, request));
        }
    }
}

/// When the oldest answer of one registry call was reused unasked.
#[derive(Default)]
pub(crate) struct ReusedSince(AtomicU64);

impl ReusedSince {
    #[cfg(test)]
    pub(crate) const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub(crate) fn record(&self, observed_ms: u64) {
        let _ = self
            .0
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |known| {
                Some(if known == 0 {
                    observed_ms
                } else {
                    known.min(observed_ms)
                })
            });
    }

    pub(crate) fn get(&self) -> Option<u64> {
        Some(self.0.load(Ordering::Relaxed)).filter(|observed| *observed > 0)
    }
}

/// State on a row, as `observedAt` (ISO time), that its evidence includes an
/// answer observed at `observed_ms` that its origin was not asked about again:
/// a saved answer is never presented as current. Per-call metadata beside
/// `rateLimit` and `clientWaits`: whether a run reused, and when, differs
/// between executions of one request, so the pager keeps it out of every
/// snapshot and page measure by its name.
pub(crate) fn disclose_reuse(data: &mut Value, observed_ms: u64) {
    let seconds = saturating_i64(observed_ms / 1000);
    if let Some(object) = data.as_object_mut() {
        object.insert(
            "observedAt".into(),
            json!(crate::civil_date::iso8601_secs(seconds)),
        );
    }
}

/// The gate's verdicts in this process, by the digest of everything it judges.
/// [`gate_verdict`] reads nothing else, so a verdict holds for as long as this
/// binary's detector runs. It never outlives the process: another binary's
/// detector judges an old answer again.
static VERDICTS: LazyLock<Store<bool>> = LazyLock::new(|| Store::new(CacheConfig::derived(), None));

fn verdict_key(answer: &ProviderAnswer) -> Option<CacheKey> {
    let digest = octocode_engine::digest::json_sha256(&(
        &answer.request,
        &answer.headers,
        &answer.next,
        &answer.body,
    ))?;
    Some(CacheKey {
        namespace: "gate-verdicts".into(),
        scope: String::new(),
        resource: hex::encode(digest),
    })
}

/// Whether `answer` holds nothing a read of it would redact, so it may be
/// stored: judged once per answer in this process ([`VERDICTS`]).
pub(crate) fn gate_keeps(security: &ContentSecurity, answer: &ProviderAnswer) -> bool {
    let Some(key) = verdict_key(answer) else {
        return gate_verdict(security, answer);
    };
    if let Some(known) = VERDICTS.get(&key) {
        return *known.value;
    }
    let verdict = gate_verdict(security, answer);
    let bytes = key.resource.len() + size_of::<bool>();
    VERDICTS.put_confirmed(key, verdict, bytes, UNUSED_RETENTION);
    verdict
}

/// Whether nothing in `answer`'s request, headers, next link or body would be
/// redacted, a JSON body as its values with any base64 file content decoded
/// (GitHub's contents and blob payloads). Reads apply patterns by the file they
/// name, which an answer need not say (a raw file is only its bytes), so the
/// judgment applies them whatever the path ([`ContentSecurity::sanitize_stored_leaf`]).
/// A wrong "no" costs a repeat its request, never a credential on disk. A file's
/// raw bytes may parse as JSON, but a read shows them as one text, whose values
/// are judged beside their keys.
fn gate_verdict(security: &ContentSecurity, answer: &ProviderAnswer) -> bool {
    let text = answer.body.as_str();
    let mut body =
        serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_owned()));
    let file_text = (requested_path(&answer.request).contains("/contents/") && !body.is_string())
        .then_some(text);
    if holds_credential_file(&answer.request, &body) || !decode_contents(&mut body) {
        return false;
    }
    let mut exposed = json!({
        "path": requested_path(&answer.request),
        "request": answer.request,
        "headers": answer.headers,
        "next": answer.next,
        "body": body,
        "fileText": file_text,
    });
    // The walk reports whether it changed anything, so the value is judged in
    // place: no second copy of the answer to build and compare.
    let changed = sanitize_json(&mut exposed, &mut |text, path| {
        Ok::<_, std::convert::Infallible>(security.sanitize_stored_leaf(text, path))
    })
    .unwrap_or_else(|never| match never {});
    !changed
}

/// Whether `answer` holds a credential-named file's text, which may hold a
/// secret in no format the gate recognizes: a contents read of that file, a blob
/// read for it, a code-search page, whose index fragments are text of every file
/// it lists, or a changed-file list (a commit, a comparison, a pull request's
/// files) with a patch of one, by its name or a rename's old name. It stays in
/// this process, as an answer holding a recognized secret does.
fn holds_credential_file(request: &str, body: &Value) -> bool {
    let is_withheld = crate::policy::discovery::is_sensitive_repository_file;
    if octocode_github::requested_file(request).is_some_and(is_withheld) {
        return true;
    }
    let path = requested_path(request);
    if let Some((_, file)) = path.split_once("/contents/") {
        return is_withheld(file);
    }
    if path.ends_with("/search/code") {
        return body["items"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["path"].as_str().is_some_and(is_withheld))
        });
    }
    body["files"]
        .as_array()
        .or_else(|| body.as_array())
        .is_some_and(|files| {
            files.iter().any(|file| {
                file["patch"].is_string()
                    && ["filename", "previous_filename"]
                        .iter()
                        .any(|name| file[*name].as_str().is_some_and(is_withheld))
            })
        })
}

/// The location `METHOD url[?query]…` asks for, with its path's separators as a
/// file's: what a body with no path of its own is about.
fn requested_path(request: &str) -> String {
    let url = request.split_whitespace().nth(1).unwrap_or(request);
    let path = url.split_once('?').map_or(url, |(path, _)| path);
    path.replace("%2F", "/").replace("%2f", "/")
}

/// Replace every base64 `content` with its text; false when one is not
/// UTF-8 text the gate can judge.
fn decode_contents(value: &mut Value) -> bool {
    match value {
        Value::Object(object) => {
            if object.get("encoding").and_then(Value::as_str) == Some("base64")
                && let Some(Value::String(content)) = object.get("content")
            {
                let Some(text) =
                    octocode_github::decode_content(Some("base64"), Some(content.clone()))
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                else {
                    return false;
                };
                object.insert("content".into(), Value::String(text));
            }
            object.values_mut().all(decode_contents)
        }
        Value::Array(items) => items.iter_mut().all(decode_contents),
        _ => true,
    }
}

#[cfg(test)]
mod tests;
