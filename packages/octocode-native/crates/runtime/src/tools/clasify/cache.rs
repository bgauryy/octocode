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

/// `F` is the failure a flight hands to the requests queued behind it.
pub(crate) struct JudgmentCache<F> {
    entries: Mutex<VecDeque<Entry>>,
    inflight: Mutex<BTreeMap<[u8; 32], Slot<F>>>,
}

type Slot<F> = Arc<tokio::sync::Mutex<Option<F>>>;

/// One request's place in the single-flight for a key. Dropping it (on
/// completion, cancellation, or panic) forgets the key once no request holds
/// or awaits it, so a failure is shared only within one concurrent wave.
pub(crate) struct Flight<'a, F> {
    cache: &'a JudgmentCache<F>,
    key: [u8; 32],
    slot: Slot<F>,
}

impl<F> Flight<'_, F> {
    /// Waits for this request's turn. The slot holds the failure of an
    /// earlier turn in this wave (replay it), or `None` (check the cache,
    /// then ask the provider and record a failure for the waiters).
    pub(crate) async fn turn(&self) -> tokio::sync::MutexGuard<'_, Option<F>> {
        self.slot.lock().await
    }
}

impl<F> Drop for Flight<'_, F> {
    fn drop(&mut self) {
        let mut inflight = self
            .cache
            .inflight
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        // Only the map and this flight still hold the slot: nobody waits.
        if inflight
            .get(&self.key)
            .is_some_and(|slot| Arc::strong_count(slot) == 2)
        {
            inflight.remove(&self.key);
        }
    }
}

/// The digest of the provider input. The evidence state and the questions
/// are hashed as canonical JSON written straight into the digest: a hit
/// copies nothing. Each JSON value delimits itself, so the two need no
/// length prefix.
pub(crate) fn key(endpoint: &str, model: &str, state: &Value, questions: &[Value]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for part in [endpoint, model] {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    let mut writer = DigestWriter(&mut digest);
    write_canonical(&mut writer, state);
    write_canonical_items(&mut writer, questions);
    digest.finalize().into()
}

struct DigestWriter<'a>(&'a mut Sha256);

impl std::io::Write for DigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The bytes of `canonical_json::canonicalize(value).to_string()`, written
/// from the borrowed value: keys sorted, null members dropped.
fn write_canonical(out: &mut impl std::io::Write, value: &Value) {
    match value {
        Value::Object(map) => {
            let mut members = map
                .iter()
                .filter(|(_, member)| !member.is_null())
                .collect::<Vec<_>>();
            members.sort_unstable_by(|a, b| a.0.cmp(b.0));
            let _ = out.write_all(b"{");
            for (index, (name, member)) in members.into_iter().enumerate() {
                if index > 0 {
                    let _ = out.write_all(b",");
                }
                let _ = serde_json::to_writer(&mut *out, name);
                let _ = out.write_all(b":");
                write_canonical(out, member);
            }
            let _ = out.write_all(b"}");
        }
        Value::Array(items) => write_canonical_items(out, items),
        scalar => {
            let _ = serde_json::to_writer(&mut *out, scalar);
        }
    }
}

fn write_canonical_items(out: &mut impl std::io::Write, items: &[Value]) {
    let _ = out.write_all(b"[");
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            let _ = out.write_all(b",");
        }
        write_canonical(out, item);
    }
    let _ = out.write_all(b"]");
}

impl<F> JudgmentCache<F> {
    pub(crate) const fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            inflight: Mutex::new(BTreeMap::new()),
        }
    }

    /// Joins the flight that serializes requests for `key`: the first turn
    /// asks the provider, later turns replay its stored answer or its failure.
    pub(crate) fn flight(&self, key: &[u8; 32]) -> Flight<'_, F> {
        let mut inflight = self.inflight.lock().unwrap_or_else(|p| p.into_inner());
        let slot = Arc::clone(inflight.entry(*key).or_default());
        Flight {
            cache: self,
            key: *key,
            slot,
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

    /// The digest reads the same canonical bytes the shared canonicalizer
    /// produces, without cloning the value.
    #[test]
    fn canonical_writer_matches_the_canonicalizer() {
        let value = json!({
            "z": [1, null, {"b": null, "a": "q\"u\u{1F600}\n"}],
            "a": {"y": 1.5, "x": -0, "w": [], "v": {}},
            "m": null,
            "n": [true, false, 12345678901234567890u64, "\u{7}"]
        });
        let mut written = Vec::new();
        write_canonical(&mut written, &value);
        assert_eq!(
            String::from_utf8(written).unwrap(),
            crate::canonical_json::canonicalize(value).to_string()
        );
    }

    #[test]
    fn hits_replay_answers_until_expiry_and_evict_oldest() {
        let cache = JudgmentCache::<()>::new();
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

    fn inflight_len<F>(cache: &JudgmentCache<F>) -> usize {
        cache.inflight.lock().unwrap().len()
    }

    /// L6: a flight whose request never finishes (dropped future, panic)
    /// still releases its slot: the map cannot grow without bound.
    #[test]
    fn a_dropped_flight_releases_its_slot() {
        let cache = JudgmentCache::<u8>::new();
        let first = cache.flight(&[7; 32]);
        let second = cache.flight(&[7; 32]);
        drop(first);
        assert_eq!(inflight_len(&cache), 1, "a waiter still holds the slot");
        drop(second);
        assert_eq!(inflight_len(&cache), 0);
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _flight = cache.flight(&[8; 32]);
            panic!("provider page panicked");
        }));
        assert!(panicked.is_err());
        assert_eq!(inflight_len(&cache), 0, "unwinding lands the flight");
    }

    /// L6: waiters queued behind a failed request share its failure instead
    /// of each asking the provider again; once the wave lands, a later
    /// request starts fresh.
    #[tokio::test]
    async fn waiters_share_the_failure_of_their_flight() {
        let cache = JudgmentCache::<&'static str>::new();
        let first = cache.flight(&[9; 32]);
        let waiter = cache.flight(&[9; 32]);
        {
            let mut turn = first.turn().await;
            assert!(turn.is_none(), "the first holder asks the provider");
            *turn = Some("rate limited");
        }
        drop(first);
        assert_eq!(*waiter.turn().await, Some("rate limited"));
        drop(waiter);
        let later = cache.flight(&[9; 32]);
        assert!(later.turn().await.is_none(), "a new wave retries");
    }
}
