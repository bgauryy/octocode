//! Order-independent JSON for digests. The workspace enables serde_json's
//! `preserve_order`, so two equal queries can serialize their keys in
//! different orders (a caller's request vs. a runtime-built continuation);
//! a snapshot hashed over raw bytes would then differ between pages.

use serde_json::Value;

/// Keys sorted recursively and null members dropped, so equal values
/// serialize to equal bytes.
pub(crate) fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries = map
                .into_iter()
                .filter(|(_, v)| !v.is_null())
                .collect::<Vec<_>>();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize(value)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonicalize).collect()),
        value => value,
    }
}

#[cfg(test)]
mod tests {
    use super::canonicalize;
    use serde_json::json;

    #[test]
    fn key_order_and_nulls_do_not_change_the_bytes() {
        let a = canonicalize(json!({"b": 1, "a": {"y": [ {"q": 1, "p": null} ], "x": 2}}));
        let b = canonicalize(json!({"a": {"x": 2, "y": [ {"q": 1} ]}, "b": 1}));
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&b).unwrap()
        );
    }
}
