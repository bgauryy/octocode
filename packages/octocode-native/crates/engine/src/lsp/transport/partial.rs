//! Bounded collection of `partialResultToken` `$/progress` chunks, merged into
//! the final response in protocol order.

use crate::error::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex as StdMutex;

/// Cap on the retained partial-result bytes for one token.
const MAX_PARTIAL_RESULT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
struct PartialResultBuffer {
    values: Vec<Value>,
    bytes: usize,
    overflowed: bool,
}

#[derive(Default)]
pub(super) struct PartialResultStore {
    buffers: StdMutex<HashMap<String, PartialResultBuffer>>,
}

impl PartialResultStore {
    /// Starts collecting for `token`. The returned guard discards the buffer on
    /// drop, so a cancelled request never leaks up to 16 MiB of chunks.
    pub(super) fn begin(&self, token: String) -> PartialTokenGuard<'_> {
        if let Ok(mut buffers) = self.buffers.lock() {
            buffers.insert(token.clone(), PartialResultBuffer::default());
        }
        PartialTokenGuard { store: self, token }
    }

    /// Records one chunk. Chunks for unknown (never begun or already finished)
    /// tokens are ignored; this is also how progress tokens are told apart.
    pub(super) fn record(&self, token: &str, value: Value) {
        let Ok(mut buffers) = self.buffers.lock() else {
            return;
        };
        let Some(buffer) = buffers.get_mut(token) else {
            return;
        };
        let bytes = serde_json::to_vec(&value).map_or(MAX_PARTIAL_RESULT_BYTES + 1, |v| v.len());
        if buffer.bytes.saturating_add(bytes) > MAX_PARTIAL_RESULT_BYTES {
            buffer.overflowed = true;
            return;
        }
        buffer.bytes += bytes;
        buffer.values.push(value);
    }

    /// `true` while `token` is an open partial-result token of ours.
    pub(super) fn is_tracking(&self, token: &str) -> bool {
        self.buffers
            .lock()
            .is_ok_and(|buffers| buffers.contains_key(token))
    }

    fn take(&self, token: &str) -> Option<PartialResultBuffer> {
        self.buffers
            .lock()
            .ok()
            .and_then(|mut buffers| buffers.remove(token))
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.buffers.lock().map_or(0, |buffers| buffers.len())
    }
}

/// Owns one partial-result token for the lifetime of a request.
pub(super) struct PartialTokenGuard<'a> {
    store: &'a PartialResultStore,
    token: String,
}

impl PartialTokenGuard<'_> {
    #[cfg(test)]
    pub(super) fn token(&self) -> &str {
        &self.token
    }

    /// Merges the collected chunks (in arrival order) ahead of `final_result`.
    pub(super) fn finish(self, final_result: Value) -> Result<Value> {
        let buffer = self.store.take(&self.token).unwrap_or_default();
        if buffer.overflowed {
            return Err(Error::new(
                "LSP partial results exceeded the bounded collection limit",
            ));
        }
        Ok(buffer
            .values
            .into_iter()
            .rev()
            .fold(final_result, |accumulated, partial| {
                merge_partial_result(partial, accumulated)
            }))
    }
}

impl Drop for PartialTokenGuard<'_> {
    fn drop(&mut self) {
        let _ = self.store.take(&self.token);
    }
}

fn merge_partial_result(partial: Value, final_result: Value) -> Value {
    match (partial, final_result) {
        (Value::Array(mut partial), Value::Array(final_values)) => {
            partial.extend(final_values);
            Value::Array(partial)
        }
        (Value::Object(partial), Value::Object(mut final_values)) => {
            for (key, partial_value) in partial {
                let merged = final_values
                    .remove(&key)
                    .map(|final_value| merge_partial_result(partial_value.clone(), final_value))
                    .unwrap_or(partial_value);
                final_values.insert(key, merged);
            }
            Value::Object(final_values)
        }
        (partial, Value::Null) => partial,
        (_, final_result) => final_result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn partial_result_store_merges_array_and_object_chunks_in_protocol_order() {
        let store = PartialResultStore::default();
        let locations = store.begin("locations".to_owned());
        store.record("locations", json!([{"uri":"a"}]));
        store.record("locations", json!([{"uri":"b"}]));
        assert_eq!(
            locations.finish(json!([{"uri":"c"}])).expect("merge"),
            json!([{"uri":"a"},{"uri":"b"},{"uri":"c"}])
        );

        let diagnostics = store.begin("diagnostics".to_owned());
        store.record("diagnostics", json!({"items":[{"message":"first"}]}));
        assert_eq!(
            diagnostics
                .finish(json!({"kind":"full","items":[{"message":"last"}]}))
                .expect("merge"),
            json!({
                "kind":"full",
                "items":[{"message":"first"},{"message":"last"}]
            })
        );
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn dropped_token_guard_releases_the_buffer() {
        let store = PartialResultStore::default();
        let guard = store.begin("t".to_owned());
        store.record(guard.token(), json!(["chunk"]));
        assert_eq!(store.len(), 1);
        drop(guard);
        assert_eq!(store.len(), 0, "a dropped request must not leak its buffer");
        // Late chunks for the released token are ignored.
        store.record("t", json!(["late"]));
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn overflow_fails_the_merge() {
        let store = PartialResultStore::default();
        let guard = store.begin("big".to_owned());
        store.record("big", json!("x".repeat(MAX_PARTIAL_RESULT_BYTES)));
        assert!(guard.finish(Value::Null).is_err());
    }
}
