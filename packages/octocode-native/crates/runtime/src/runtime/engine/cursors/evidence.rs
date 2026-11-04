//! Provider evidence an issued continuation owns: the GitHub answers the
//! call that issued it observed, so resuming it sends nothing for them. A
//! later page shapes the same bytes the first one did, so its rows continue
//! the first page's exactly, however many pages the walk takes.
//!
//! An answer with nothing any read would redact ([`gate_keeps`]) is retained
//! as text where the continuation is retained. Any other answer (one redacted
//! line of a whole file is enough) is held in this process's memory only, so no
//! credential a provider returned reaches a file: it passes the output gate
//! ([`ContentSecurity::seal_output`]) on every page it answers. Memory
//! pressure, idle expiry or a restart drop it, and its request then reaches
//! GitHub again.
use crate::cache::{CacheKey, Store};
use crate::providers::saved_answers::gate_keeps;
use crate::security::ContentSecurity;
use octocode_github::ProviderEvidence;
use std::sync::Arc;

/// What one call's continuations own, split by [`gate_keeps`].
pub(super) struct Owned {
    /// The digest of everything the call observed: the evidence's key.
    pub(super) digest: String,
    /// The call's facts and the answers [`gate_keeps`] accepts.
    sealed: ProviderEvidence,
    /// The answers it refuses.
    held: ProviderEvidence,
}

impl Owned {
    /// Evidence already retained under `digest`: nothing to store again.
    pub(super) fn retained(digest: &str) -> Self {
        Self {
            digest: digest.to_owned(),
            sealed: ProviderEvidence::default(),
            held: ProviderEvidence::default(),
        }
    }
}

/// `evidence` split by [`gate_keeps`], and its content digest.
pub(super) fn owned(evidence: ProviderEvidence) -> Option<Owned> {
    if evidence.is_empty() {
        return None;
    }
    let digest = crate::digest::json_sha256(&evidence);
    let security = ContentSecurity::new();
    let (kept, held) = evidence
        .answers
        .into_iter()
        .partition(|answer| gate_keeps(&security, answer));
    Some(Owned {
        digest,
        sealed: ProviderEvidence {
            answers: kept,
            notes: evidence.notes,
        },
        held: ProviderEvidence {
            answers: held,
            notes: Default::default(),
        },
    })
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

/// Retain `owned`. Evidence a store refuses costs its continuation a fresh
/// read only.
pub(super) fn retain(
    sealed: &Store<ProviderEvidence>,
    held: &Store<ProviderEvidence>,
    key: CacheKey,
    owned: Owned,
) {
    for (store, evidence) in [(held, owned.held), (sealed, owned.sealed)] {
        if !evidence.is_empty() {
            let bytes = evidence.bytes();
            store.put_confirmed(
                key.clone(),
                evidence,
                bytes,
                crate::cache::CONTINUATION_RETENTION,
            );
        }
    }
}

/// The evidence retained under `key`, sealed and held together.
pub(super) fn open(
    sealed: &Store<ProviderEvidence>,
    held: &Store<ProviderEvidence>,
    key: &CacheKey,
) -> Option<Arc<ProviderEvidence>> {
    let saved = sealed.get(key).map(|cached| cached.value);
    let Some(held) = held.get(key).map(|cached| cached.value) else {
        return saved;
    };
    let Some(saved) = saved else {
        return Some(held);
    };
    let mut whole = (*saved).clone();
    whole.answers.extend(held.answers.iter().cloned());
    Some(Arc::new(whole))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use octocode_github::ProviderAnswer;

    /// A last-commit answer as live GitHub GraphQL sends it (headers and body
    /// shape captured 2026-11-02, identifiers replaced by same-shape values)
    /// is retained verbatim.
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
        let owned = owned(evidence).expect("the dates answer is kept");
        assert_eq!(owned.sealed.answers, vec![live]);
        assert!(owned.held.is_empty());
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
    /// that answer out of what is stored and in what the process holds; a
    /// clean answer is stored verbatim. The digest names all of it, so a
    /// continuation of a call that observed only held answers still has a key.
    #[test]
    fn only_answers_the_gate_leaves_unchanged_are_stored() {
        let secret = format!("token = \"ghp_{}\"\n", "a1B2c3D4e5F6".repeat(3));
        let clean = answer("GET /clean", "fn main() {}\n".into());
        let raw = answer("GET /raw", secret.clone());
        let blob = answer(
            "GET /blob",
            serde_json::json!({
                "encoding": "base64",
                "content": base64::engine::general_purpose::STANDARD.encode(&secret),
            })
            .to_string(),
        );
        let observed = |answers: Vec<ProviderAnswer>| ProviderEvidence {
            answers,
            notes: Default::default(),
        };
        let owned_by = owned(observed(vec![clean.clone(), raw.clone(), blob.clone()]))
            .expect("the answers are owned");
        assert_eq!(owned_by.sealed.answers, vec![clean]);
        assert_eq!(owned_by.held.answers, vec![raw.clone(), blob]);
        assert!(!owned_by.digest.is_empty());
        let only_held = owned(observed(vec![raw])).expect("a held answer is owned");
        assert!(only_held.sealed.is_empty());
        assert_ne!(only_held.digest, owned_by.digest);
        assert!(owned(ProviderEvidence::default()).is_none());
    }

    /// The digest is the evidence's content address: the bytes of its JSON
    /// value, a persisted continuation's key. The split is the same however
    /// often one process judges the same answers.
    #[test]
    fn the_address_and_the_split_stay_on_a_repeat() {
        let secret = format!("token = \"ghp_{}\"\n", "z9X8c7V6b5N4".repeat(3));
        let mut paged = answer("GET /address-paged", "[1]\n".into());
        paged.next = Some("https://api.github.com/next".into());
        let mut missing = answer("GET /address-missing", "gone é\n".into());
        missing.status = 404;
        missing.not_found = Some(octocode_github::ProviderError::new(
            octocode_github::ProviderErrorKind::NotFound,
            "missing",
        ));
        let held = answer("GET /address-held", secret);
        let evidence = ProviderEvidence {
            answers: vec![paged.clone(), missing.clone(), held.clone()],
            notes: [
                ("b".to_owned(), "2".to_owned()),
                ("a".to_owned(), "1".to_owned()),
            ]
            .into_iter()
            .collect(),
        };
        let address = crate::digest::json_sha256(&serde_json::to_value(&evidence).unwrap());
        for _ in 0..2 {
            let split = owned(evidence.clone()).expect("owned");
            assert_eq!(split.digest, address);
            assert_eq!(split.sealed.answers, vec![paged.clone(), missing.clone()]);
            assert_eq!(split.held.answers, vec![held.clone()]);
            assert_eq!(split.sealed.notes, evidence.notes);
            assert!(split.held.notes.is_empty());
        }
    }

    /// A store of `evidence` splits by where each part may live: only the
    /// sealed part reaches the persistent store, and a process without the
    /// held part still opens what was sealed.
    #[test]
    fn held_answers_never_reach_the_persistent_store() {
        let dir = tempfile::tempdir().expect("store");
        let config = crate::cache::CacheConfig::derived();
        let sealed_store: Store<ProviderEvidence> =
            Store::new(config.clone(), Some(dir.path().to_owned()));
        let held_store: Store<ProviderEvidence> = Store::new(config.clone(), None);
        let secret = format!("token = \"ghp_{}\"\n", "a1B2c3D4e5F6".repeat(3));
        let clean = answer("GET /clean", "fn main() {}\n".into());
        let raw = answer("GET /raw", secret.clone());
        let owned = owned(ProviderEvidence {
            answers: vec![clean.clone(), raw.clone()],
            notes: Default::default(),
        })
        .expect("owned");
        let key = key("policy", Some("identity"), &owned.digest);
        retain(&sealed_store, &held_store, key.clone(), owned);
        let whole = open(&sealed_store, &held_store, &key).expect("whole");
        assert_eq!(whole.answers, vec![clean.clone(), raw]);
        let restarted: Store<ProviderEvidence> = Store::new(config, None);
        let saved = open(&sealed_store, &restarted, &key).expect("sealed part");
        assert_eq!(saved.answers, vec![clean]);
        for entry in std::fs::read_dir(dir.path()).expect("entries").flatten() {
            let bytes = std::fs::read(entry.path()).expect("entry");
            let text = String::from_utf8_lossy(&bytes);
            assert!(!text.contains("ghp_"), "{}", entry.path().display());
        }
    }
}
