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
//!
//! A retained body the saved answers also hold is one file with two hard links
//! ([`Store::link`]): evidence keeps its snapshot through a refresh, the idle
//! drop or the eviction of that saved answer, and the other way around.
use octocode_github::{AnswerBody, ProviderEvidence};
use octocode_runtime_base::cache::{CacheKey, Disk, Store};
use octocode_runtime_base::providers::saved_answers::{body_digest, body_key, gate_keeps};
use octocode_runtime_base::security::ContentSecurity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Retained evidence: the bodies at `linked`'s answer indexes are left out and
/// named by digest. Whole or absent, as one stored entry always was.
#[derive(Deserialize, Serialize)]
pub(super) struct Sealed {
    evidence: ProviderEvidence,
    linked: BTreeMap<usize, String>,
}

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
    let digest = octocode_runtime_base::digest::json_sha256(&evidence);
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
        namespace: "continuation-provider-evidence-v2".into(),
        scope: format!("{policy}\n{}", identity.unwrap_or_default()),
        resource: digest.into(),
    }
}

/// Retain `owned`. Evidence a store refuses costs its continuation a fresh
/// read only. A body a saved answer holds in `answers` is linked, not copied.
pub(super) fn retain(
    sealed: &Store<Sealed>,
    held: &Store<ProviderEvidence>,
    answers: Option<&Disk>,
    key: CacheKey,
    owned: Owned,
) {
    if !owned.held.is_empty() {
        let bytes = owned.held.bytes();
        held.put_confirmed(
            key.clone(),
            owned.held,
            bytes,
            octocode_runtime_base::cache::CONTINUATION_RETENTION,
        );
    }
    let mut evidence = owned.sealed;
    if evidence.is_empty() {
        return;
    }
    let linked = answers.map_or_else(BTreeMap::new, |answers| {
        link_bodies(sealed, answers, &mut evidence)
    });
    let bytes = evidence.bytes();
    sealed.put_confirmed(
        key,
        Sealed { evidence, linked },
        bytes,
        octocode_runtime_base::cache::CONTINUATION_RETENTION,
    );
}

fn link_bodies(
    sealed: &Store<Sealed>,
    answers: &Disk,
    evidence: &mut ProviderEvidence,
) -> BTreeMap<usize, String> {
    let mut linked = BTreeMap::new();
    for (index, answer) in evidence.answers.iter_mut().enumerate() {
        if answer.body.is_empty() {
            continue;
        }
        let digest = body_digest(&answer.body);
        if sealed.link(answers, &body_key(&digest)) {
            answer.body = AnswerBody::default();
            linked.insert(index, digest);
        }
    }
    linked
}

/// The evidence `kept` stored, with every linked body exactly as it was.
fn whole(store: &Store<Sealed>, kept: &Sealed) -> Option<ProviderEvidence> {
    let mut evidence = kept.evidence.clone();
    for (index, digest) in &kept.linked {
        let bytes = store.read_link(&body_key(digest))?;
        let body = AnswerBody::from(String::from_utf8(bytes).ok()?);
        if body_digest(&body) != *digest {
            return None;
        }
        evidence.answers.get_mut(*index)?.body = body;
    }
    Some(evidence)
}

/// The evidence retained under `key`, sealed and held together.
pub(super) fn open(
    sealed: &Store<Sealed>,
    held: &Store<ProviderEvidence>,
    key: &CacheKey,
) -> Option<Arc<ProviderEvidence>> {
    let saved = sealed
        .get(key)
        .and_then(|cached| whole(sealed, &cached.value));
    let Some(held) = held.get(key).map(|cached| cached.value) else {
        return saved.map(Arc::new);
    };
    let Some(mut saved) = saved else {
        return Some(held);
    };
    saved.answers.extend(held.answers.iter().cloned());
    Some(Arc::new(saved))
}

#[cfg(test)]
mod tests;
