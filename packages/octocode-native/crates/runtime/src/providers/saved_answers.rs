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
use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use crate::security::{ContentSecurity, sanitize_json};
use octocode_github::{ProviderAnswer, SavedAnswer, SavedAnswers};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// An answer unused this long is dropped: a later request reads it again. This
/// bounds how long private bytes sit on disk, on top of the store's byte budget
/// and `cache clear`; it says nothing about freshness.
const UNUSED_RETENTION: CacheClass = CacheClass::Idle {
    ttl: Duration::from_secs(7 * 24 * 60 * 60),
};

const NAMESPACE: &str = "provider-answers";

/// The store behind [`SavedAnswers`].
pub(crate) struct KeptAnswers {
    sealed: Store<SavedAnswer>,
    held: Store<SavedAnswer>,
    security: ContentSecurity,
}

impl KeptAnswers {
    /// `disk: None` keeps everything in this runtime (`storage.mode` memory).
    pub(crate) fn new(disk: Option<PathBuf>) -> Self {
        Self {
            sealed: Store::new(CacheConfig::derived(), disk),
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
}

impl SavedAnswers for KeptAnswers {
    fn load(&self, scope: &str, request: &str) -> Option<SavedAnswer> {
        let key = Self::key(scope, request);
        self.held
            .get(&key)
            .or_else(|| self.sealed.get(&key))
            .map(|kept| (*kept.value).clone())
    }

    fn save(&self, scope: &str, request: &str, saved: SavedAnswer) {
        let key = Self::key(scope, request);
        let bytes = saved.answer.request.len()
            + saved.answer.body.len()
            + saved
                .answer
                .headers
                .iter()
                .map(|(name, value)| name.len() + value.len())
                .sum::<usize>();
        // The other store's older answer must not outlive this one.
        if gate_keeps(&self.security, &saved.answer) {
            self.held.remove(&key);
            self.sealed
                .put_confirmed(key, saved, bytes, UNUSED_RETENTION);
        } else {
            self.sealed.remove(&key);
            self.held.put_confirmed(key, saved, bytes, UNUSED_RETENTION);
        }
    }

    fn forget(&self, scope: &str, request: &str) {
        let key = Self::key(scope, request);
        self.held.remove(&key);
        self.sealed.remove(&key);
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

/// Say on a row that its evidence includes an answer observed at `observed_ms`
/// that its origin was not asked about again: a saved answer is never presented
/// as current. A hint, not a warning: the evidence is complete for that moment.
pub(crate) fn disclose_reuse(data: &mut Value, observed_ms: u64) {
    let seconds = i64::try_from(observed_ms / 1000).unwrap_or(i64::MAX);
    let hint = json!(format!(
        "Answer observed {}: its origin was not asked again, so later changes are not shown.",
        crate::civil_date::iso8601_secs(seconds)
    ));
    let Some(object) = data.as_object_mut() else {
        return;
    };
    match object.entry("hints").or_insert_with(|| json!([])) {
        Value::Array(hints) => hints.push(hint),
        other => *other = json!([hint]),
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
/// A wrong "no" costs a repeat its request, never a credential on disk.
fn gate_verdict(security: &ContentSecurity, answer: &ProviderAnswer) -> bool {
    let mut body = serde_json::from_str::<Value>(&answer.body)
        .unwrap_or_else(|_| Value::String(answer.body.clone()));
    if !decode_contents(&mut body) {
        return false;
    }
    let exposed = json!({
        "path": requested_path(&answer.request),
        "request": answer.request,
        "headers": answer.headers,
        "next": answer.next,
        "body": body,
    });
    let mut judged = exposed.clone();
    sanitize_json(&mut judged, &mut |text, path| {
        Ok::<_, std::convert::Infallible>(security.sanitize_stored_leaf(text, path))
    })
    .unwrap_or_else(|never| match never {});
    judged == exposed
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
