//! Provider evidence an issued continuation owns: the GitHub answers the
//! call that issued it observed, so resuming it sends nothing for them.
//!
//! Only an answer the output gate ([`ContentSecurity::seal_output`]) leaves
//! unchanged is retained, as text: a later page shapes the same bytes the
//! first one did, so its rows continue the first page's exactly, and no
//! credential a provider returned reaches the store. An answer the gate
//! would rewrite is dropped; its request reaches GitHub again when resumed.
use crate::cache::{CacheKey, Store};
use crate::security::ContentSecurity;
use octocode_github::{ProviderAnswer, ProviderEvidence};
use serde_json::Value;
use std::sync::Arc;

/// Evidence as the output gate leaves it, and its content digest.
pub(super) fn sealed(evidence: ProviderEvidence) -> Option<(String, ProviderEvidence)> {
    let security = ContentSecurity::new();
    let sealed = ProviderEvidence {
        answers: evidence
            .answers
            .into_iter()
            .filter(|answer| gate_keeps(&security, answer))
            .collect(),
        notes: evidence.notes,
    };
    if sealed.is_empty() {
        return None;
    }
    let digest = crate::digest::json_sha256(&serde_json::to_value(&sealed).ok()?);
    Some((digest, sealed))
}

/// Whether the gate leaves everything `answer` exposes unchanged: its
/// headers and its body, a JSON body as its values with any base64 file
/// content decoded (GitHub's contents and blob payloads).
fn gate_keeps(security: &ContentSecurity, answer: &ProviderAnswer) -> bool {
    let mut body = serde_json::from_str::<Value>(&answer.body)
        .unwrap_or_else(|_| Value::String(answer.body.clone()));
    if !decode_contents(&mut body) {
        return false;
    }
    let exposed = serde_json::json!({
        "request": answer.request,
        "headers": answer.headers,
        "next": answer.next,
        "body": body,
    });
    security
        .seal_output(exposed.clone(), false, || {
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap_or_else(|never| match never {})
        .into_value()
        == exposed
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

/// Where evidence is stored: under the replay policy and the GitHub
/// identity of the call that observed it, by its content digest.
pub(super) fn key(policy: &str, identity: Option<&str>, digest: &str) -> CacheKey {
    CacheKey {
        namespace: "continuation-provider-evidence-v1".into(),
        scope: format!("{policy}\n{}", identity.unwrap_or_default()),
        resource: digest.into(),
    }
}

/// Retain `evidence`; false when the store refused it (its continuation
/// then reads GitHub again).
pub(super) fn retain(
    store: &Store<ProviderEvidence>,
    key: CacheKey,
    evidence: ProviderEvidence,
) -> bool {
    let bytes = evidence.bytes();
    store
        .put_confirmed(key, evidence, bytes, crate::cache::CONTINUATION_RETENTION)
        .1
}

pub(super) fn open(
    store: &Store<ProviderEvidence>,
    key: &CacheKey,
) -> Option<Arc<ProviderEvidence>> {
    store.get(key).map(|cached| cached.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    /// A last-commit-dates answer as live GitHub GraphQL sends it (headers
    /// and body shape captured 2026-11-02, identifiers replaced by
    /// same-shape values) is retained verbatim: a later tree page replays it.
    #[test]
    fn a_live_shaped_graphql_dates_answer_is_retained() {
        let headers = [
            ("access-control-allow-origin", "*"),
            (
                "access-control-expose-headers",
                "ETag, Link, Location, Retry-After, X-GitHub-OTP, X-RateLimit-Limit, X-RateLimit-Remaining, X-RateLimit-Used, X-RateLimit-Resource, X-RateLimit-Reset, X-OAuth-Scopes, X-Accepted-OAuth-Scopes, X-Poll-Interval, X-GitHub-Media-Type, X-GitHub-SSO, X-GitHub-Request-Id, Deprecation, Sunset, Warning",
            ),
            ("content-security-policy", "default-src 'none'"),
            ("content-type", "application/json; charset=utf-8"),
            ("date", "Mon, 02 Nov 2026 22:09:43 GMT"),
            (
                "referrer-policy",
                "origin-when-cross-origin, strict-origin-when-cross-origin",
            ),
            ("server", "github.com"),
            (
                "strict-transport-security",
                "max-age=31536000; includeSubdomains; preload",
            ),
            ("vary", "Accept-Encoding, Accept, X-Requested-With"),
            ("x-accepted-oauth-scopes", "repo"),
            ("x-content-type-options", "nosniff"),
            ("x-frame-options", "deny"),
            ("x-github-edge-region", "israelcentral"),
            ("x-github-media-type", "github.v4; format=json"),
            ("x-github-request-id", "C0A4:3B2F1:1A2B3C4:1D2E3F5:67890ABC"),
            ("x-oauth-client-id", "178c6fc778ccc68e1d6a"),
            ("x-oauth-scopes", "gist, read:org, repo, workflow"),
            ("x-ratelimit-limit", "5000"),
            ("x-ratelimit-remaining", "4997"),
            ("x-ratelimit-reset", "1793660543"),
            ("x-ratelimit-resource", "graphql"),
            ("x-ratelimit-used", "3"),
            ("x-xss-protection", "0"),
        ];
        let request = format!(
            "POST https://api.github.com/graphql\n\n{}",
            serde_json::json!({
                "query": "query($owner:String!,$name:String!,$oid:GitObjectID!,$p0:String!,$p1:String!){repository(owner:$owner,name:$name){object(oid:$oid){...on Commit{committedDate p0:history(first:1,path:$p0){nodes{committedDate author{name user{login}}}} p1:history(first:1,path:$p1){nodes{committedDate author{name user{login}}}}}}}}",
                "variables": {"name": "tokio", "oid": "9b5e0d8c2f7a41e6b3c58d9f0a1e2b7c4d6f8a03",
                    "owner": "tokio-rs", "p0": "tokio/src/lib.rs", "p1": "tokio/src/runtime"}
            })
        );
        let live = ProviderAnswer {
            request,
            status: 200,
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            body: r#"{"data":{"repository":{"object":{"committedDate":"2026-10-31T16:16:33Z","p0":{"nodes":[{"committedDate":"2026-10-13T20:54:34Z","author":{"name":"Alice Ryhl","user":{"login":"Darksonn"}}}]},"p1":{"nodes":[{"committedDate":"2026-10-29T04:49:08Z","author":{"name":"Daksh","user":{"login":"Daksh14"}}}]}}}}}"#.into(),
            next: None,
            not_found: None,
        };
        let evidence = ProviderEvidence {
            answers: vec![live.clone()],
            notes: Default::default(),
        };
        let (_, sealed) = sealed(evidence).expect("the dates answer is kept");
        assert_eq!(sealed.answers, vec![live]);
    }

    fn answer(request: &str, body: String) -> ProviderAnswer {
        ProviderAnswer {
            request: request.into(),
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body,
            next: None,
            not_found: None,
        }
    }

    /// A credential in an answer, as text or as base64 file content, keeps
    /// that answer out of the store; a clean answer is retained verbatim.
    #[test]
    fn only_answers_the_gate_leaves_unchanged_are_retained() {
        let secret = format!("token = \"ghp_{}\"\n", "a1B2c3D4e5F6".repeat(3));
        let clean = answer("GET /clean", "fn main() {}\n".into());
        let evidence = ProviderEvidence {
            answers: vec![
                clean.clone(),
                answer("GET /raw", secret.clone()),
                answer(
                    "GET /blob",
                    serde_json::json!({
                        "encoding": "base64",
                        "content": base64::engine::general_purpose::STANDARD.encode(&secret),
                    })
                    .to_string(),
                ),
            ],
            notes: Default::default(),
        };
        let (digest, sealed) = sealed(evidence).expect("the clean answer is kept");
        assert_eq!(sealed.answers, vec![clean]);
        assert!(!digest.is_empty());
    }
}
