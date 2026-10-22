//! Saved evaluations, keyed by the exact provider input, on the runtime's
//! shared [`Store`] (memory always; disk under persistent storage), with one
//! [`KeyedFlight`] so identical concurrent evaluations make one request.
//!
//! The key covers the account partition, the model asked for, the state and
//! questions, resolved asset bytes and capability options; any change misses.
//! Only the digest is stored as the key. Entries expire after
//! [`TTL`] so an aliased model's update is picked up.
use super::{EvaluationInput, Judged, QuestionOutcomes};
use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use octocode_github::Partition;
use octocode_io::flight::KeyedFlight;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

const NAMESPACE: &str = "judgment/v4";
const MAX_ENTRIES: usize = 256;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const DISK_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const TTL: Duration = Duration::from_secs(30 * 60);

/// The exact provider input a saved judgment answers, borrowed: hashing it
/// never copies the state, questions or image bytes.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity<'a> {
    model: &'a str,
    state: &'a Value,
    questions: &'a [Value],
    assets: Option<&'a Value>,
    images: bool,
    long_context: bool,
    transport: &'static str,
}

/// Hex SHA-256 of `identity`'s compact JSON, hashed while it serializes.
fn digest(identity: &Identity<'_>) -> String {
    struct Hasher(sha2::Sha256);
    impl std::io::Write for Hasher {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            sha2::Digest::update(&mut self.0, bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut hasher = Hasher(<sha2::Sha256 as sha2::Digest>::new());
    // Borrowed JSON values and plain fields always serialize.
    let _ = serde_json::to_writer(&mut hasher, identity);
    hex::encode(sha2::Digest::finalize(hasher.0))
}

pub(super) struct Judgments {
    store: Store<Judged>,
    /// A turn's slot holds the provider failure of its wave, if any.
    pub(super) flight: KeyedFlight<String, Option<QuestionOutcomes>>,
}

impl Judgments {
    pub(super) fn new(disk: Option<PathBuf>) -> Self {
        Self {
            store: Store::new(
                CacheConfig {
                    max_entries: MAX_ENTRIES,
                    max_bytes: MAX_BYTES,
                    disk_bytes: DISK_BYTES,
                },
                disk,
            ),
            flight: KeyedFlight::transient(),
        }
    }

    pub(super) fn key(partition: &Partition, model: &str, input: EvaluationInput<'_>) -> CacheKey {
        CacheKey {
            namespace: NAMESPACE.to_owned(),
            scope: partition.identity().to_owned(),
            resource: digest(&Identity {
                model,
                state: input.state,
                questions: input.questions,
                assets: input.assets,
                images: input.images(),
                long_context: input.long_context,
                transport: "json",
            }),
        }
    }

    pub(super) fn get(&self, key: &CacheKey) -> Option<Judged> {
        self.store
            .get(key)
            .filter(|hit| hit.fresh)
            .map(|hit| (*hit.value).clone())
    }

    pub(super) fn put(&self, key: CacheKey, judged: &Judged) {
        let bytes = octocode_engine::json_size::bytes(judged).unwrap_or(usize::MAX);
        self.store.put(
            key,
            judged.clone(),
            bytes,
            CacheClass::Volatile { ttl: TTL },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;
    use serde_json::json;

    fn key(partition: &Partition, model: &str, state: &Value, questions: &[Value]) -> CacheKey {
        Judgments::key(
            partition,
            model,
            EvaluationInput {
                state,
                questions,
                assets: None,
                long_context: false,
            },
        )
    }

    fn judged() -> Judged {
        Judged {
            answers: vec![json!({"type":"noul","noul":0.9})],
            model: "jev-1.13.0".into(),
            input_tokens: 10,
            output_tokens: 2,
            provider_calls: 1,
            known_usage_calls: 1,
            replayed: false,
        }
    }

    fn partition(key: &str) -> Partition {
        Partition::new(
            "https://api.example.test/v1/systemone",
            Some(&SecretString::from(key.to_owned())),
        )
    }

    #[test]
    fn a_saved_evaluation_replays_only_for_the_identical_input() {
        let cache = Judgments::new(None);
        let question = [json!({"type":"noul","instructions":"q"})];
        let key = key(&partition("a"), "m", &json!("s"), &question);
        cache.put(key.clone(), &judged());
        let hit = cache.get(&key).expect("saved");
        assert_eq!(hit.answers, judged().answers);
        for other in [
            self::key(&partition("b"), "m", &json!("s"), &question),
            self::key(&partition("a"), "n", &json!("s"), &question),
            self::key(&partition("a"), "m", &json!("t"), &question),
            self::key(
                &partition("a"),
                "m",
                &json!("s"),
                &[json!({"type":"noul","instructions":"r"})],
            ),
        ] {
            assert!(cache.get(&other).is_none(), "{other:?}");
        }
    }

    #[test]
    fn the_streamed_digest_is_the_digest_of_the_serialized_identity() {
        let state = json!({"text":"é and \"quotes\""});
        let questions = [json!({"type":"noul","instructions":"q"})];
        let identity = Identity {
            model: "m",
            state: &state,
            questions: &questions,
            assets: None,
            images: false,
            long_context: true,
            transport: "json",
        };
        assert_eq!(digest(&identity), crate::digest::json_sha256(&identity));
    }

    #[test]
    fn a_persistent_store_serves_a_fresh_process() {
        let dir = tempfile::tempdir().expect("dir");
        let question = [json!({"type":"noul","instructions":"q"})];
        let key = key(&partition("a"), "m", &json!("s"), &question);
        Judgments::new(Some(dir.path().to_owned())).put(key.clone(), &judged());
        let fresh = Judgments::new(Some(dir.path().to_owned()));
        let hit = fresh.get(&key).expect("disk tier");
        assert_eq!(hit.model, "jev-1.13.0");
        assert_eq!(hit.input_tokens, 10);
    }

    #[test]
    fn media_bytes_and_capability_options_partition_answers_without_storing_media() {
        let state = json!("opaque-placeholder");
        let questions = [json!({"type":"box"})];
        let assets = json!({"opaque-placeholder":{"type":"image","format":"png","data_base64":"first-secret-bytes"}});
        let other_assets = json!({"opaque-placeholder":{"type":"image","format":"png","data_base64":"second-secret-bytes"}});
        let input = EvaluationInput {
            state: &state,
            questions: &questions,
            assets: Some(&assets),
            long_context: false,
        };
        let key = Judgments::key(&partition("a"), "vision", input);
        assert_ne!(
            key,
            Judgments::key(
                &partition("a"),
                "vision",
                EvaluationInput {
                    assets: Some(&other_assets),
                    ..input
                }
            )
        );
        assert_ne!(
            key,
            Judgments::key(
                &partition("a"),
                "vision",
                EvaluationInput {
                    long_context: true,
                    ..input
                }
            )
        );
        assert_ne!(
            key,
            Judgments::key(
                &partition("a"),
                "vision",
                EvaluationInput {
                    assets: None,
                    ..input
                }
            )
        );
        assert!(!format!("{key:?}").contains("secret-bytes"));
        let cache = Judgments::new(None);
        cache.put(key.clone(), &judged());
        assert_eq!(cache.get(&key).unwrap().answers, judged().answers);
    }
}
