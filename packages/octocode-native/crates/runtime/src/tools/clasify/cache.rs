//! Process-local cache of provider judgments.
//!
//! A resumed walk (`next.clasify`) or a repeated matrix re-sends pages the
//! provider already judged. The key is a SHA-256 of the exact provider input
//! (endpoint, model, evidence state, and questions), so a hit can only replay the answer
//! to the identical question over identical evidence; any change to the
//! evidence, a brief, or a question misses. Only complete successful answer
//! sets are stored. Entries expire so a provider-side model update is picked
//! up within the TTL. Identical requests in flight at once share one provider
//! call through [`JudgmentCache::flight`].
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 256;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(30 * 60);

struct Entry {
    key: [u8; 32],
    bytes: usize,
    stored: Instant,
    answers: Vec<Value>,
}

pub(crate) struct JudgmentCache {
    entries: Mutex<VecDeque<Entry>>,
    inflight: Mutex<BTreeMap<[u8; 32], Arc<tokio::sync::Mutex<()>>>>,
}

pub(crate) fn key(endpoint: &str, model: &str, state: &Value, questions: &[Value]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for part in [
        endpoint.to_owned(),
        model.to_owned(),
        crate::canonical_json::canonicalize(state.clone()).to_string(),
        crate::canonical_json::canonicalize(Value::Array(questions.to_vec())).to_string(),
    ] {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    digest.finalize().into()
}

impl JudgmentCache {
    pub(crate) const fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            inflight: Mutex::new(BTreeMap::new()),
        }
    }

    /// The lock that serializes requests for `key`: the holder asks the
    /// provider, later holders replay its stored answer. Pair with [`Self::land`].
    pub(crate) fn flight(&self, key: &[u8; 32]) -> Arc<tokio::sync::Mutex<()>> {
        let mut inflight = self.inflight.lock().unwrap_or_else(|p| p.into_inner());
        Arc::clone(inflight.entry(*key).or_default())
    }

    /// Forget `key`'s lock once no request holds or awaits it.
    pub(crate) fn land(&self, key: &[u8; 32]) {
        let mut inflight = self.inflight.lock().unwrap_or_else(|p| p.into_inner());
        if inflight
            .get(key)
            .is_some_and(|lock| Arc::strong_count(lock) == 1)
        {
            inflight.remove(key);
        }
    }

    pub(crate) fn get(&self, key: &[u8; 32]) -> Option<Vec<Value>> {
        self.get_at(key, Instant::now())
    }

    fn get_at(&self, key: &[u8; 32], now: Instant) -> Option<Vec<Value>> {
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        entries.retain(|entry| now.duration_since(entry.stored) < TTL);
        let index = entries.iter().position(|entry| &entry.key == key)?;
        let entry = entries.remove(index)?;
        let answers = entry.answers.clone();
        entries.push_back(entry);
        Some(answers)
    }

    pub(crate) fn put(&self, key: [u8; 32], answers: Vec<Value>) {
        self.put_at(key, answers, Instant::now());
    }

    fn put_at(&self, key: [u8; 32], answers: Vec<Value>, now: Instant) {
        let bytes = answers.iter().map(|answer| answer.to_string().len()).sum();
        if bytes > MAX_BYTES {
            return;
        }
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        entries.retain(|entry| entry.key != key);
        entries.push_back(Entry {
            key,
            bytes,
            stored: now,
            answers,
        });
        let mut total: usize = entries.iter().map(|entry| entry.bytes).sum();
        while entries.len() > MAX_ENTRIES || total > MAX_BYTES {
            let Some(evicted) = entries.pop_front() else {
                break;
            };
            total = total.saturating_sub(evicted.bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn key_changes_with_any_input_and_ignores_object_key_order() {
        let q = vec![json!({"id":"q","question":{"type":"noul","instructions":"x"}})];
        let base = key("https://a/v1", "jev", &json!({"a":1,"b":2}), &q);
        assert_eq!(base, key("https://a/v1", "jev", &json!({"b":2,"a":1}), &q));
        assert_ne!(
            base,
            key("https://b/v1", "jev", &json!({"a":1,"b":2}), &q),
            "another provider judges independently"
        );
        assert_ne!(
            base,
            key("https://a/v1", "jev-2", &json!({"a":1,"b":2}), &q)
        );
        assert_ne!(base, key("https://a/v1", "jev", &json!({"a":1,"b":3}), &q));
        let other = vec![json!({"id":"q","question":{"type":"noul","instructions":"y"}})];
        assert_ne!(
            base,
            key("https://a/v1", "jev", &json!({"a":1,"b":2}), &other)
        );
    }

    #[test]
    fn hits_replay_answers_until_expiry_and_evict_oldest() {
        let cache = JudgmentCache::new();
        let start = Instant::now();
        cache.put_at([1; 32], vec![json!({"noul":0.9})], start);
        assert_eq!(
            cache.get_at(&[1; 32], start),
            Some(vec![json!({"noul":0.9})])
        );
        assert_eq!(cache.get_at(&[2; 32], start), None);
        assert_eq!(cache.get_at(&[1; 32], start + TTL), None, "expired");
        let id = |n: usize| {
            let mut key = [0_u8; 32];
            key[..8].copy_from_slice(&(n as u64).to_le_bytes());
            key
        };
        for n in 0..=MAX_ENTRIES {
            cache.put_at(id(n), vec![json!(n)], start);
        }
        assert_eq!(cache.get_at(&id(0), start), None, "oldest evicted");
        assert!(cache.get_at(&id(MAX_ENTRIES), start).is_some());
    }
}
