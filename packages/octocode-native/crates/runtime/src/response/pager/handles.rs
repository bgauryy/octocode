//! Issued handles inside an envelope are per-call facts, like provider
//! request ids: two executions of the same work over unchanged sources
//! publish the same evidence under fresh handles. The page snapshot reads
//! each handle as the sizing placeholder (same length, so the page plan is
//! unchanged), so it names the content alone and a re-execution of evicted
//! work can prove its sources unchanged.
use super::super::cursor::issued;
use serde_json::{Map, Value};

/// `f` over `structured` with every executable call's issued `cursor`
/// replaced by the placeholder; the handles are restored afterwards.
pub(super) fn masked<R>(
    structured: &mut Map<String, Value>,
    f: impl FnOnce(&Map<String, Value>) -> R,
) -> R {
    let placeholder = issued::placeholder();
    let mut taken = Vec::new();
    for value in structured.values_mut() {
        swap(value, &mut |handle| {
            taken.push(std::mem::replace(handle, placeholder.clone()));
        });
    }
    let result = f(structured);
    let mut restored = taken.into_iter();
    for value in structured.values_mut() {
        swap(value, &mut |handle| {
            if let Some(original) = restored.next() {
                *handle = original;
            }
        });
    }
    result
}

/// Visit, in document order, each issued handle in `value` that is the whole
/// input of an executable call (`{"kind","tool","input":{"cursor"}}`).
fn swap(value: &mut Value, visit: &mut impl FnMut(&mut String)) {
    match value {
        Value::Object(object) => {
            if object.contains_key("kind")
                && object.contains_key("tool")
                && let Some(Value::Object(input)) = object.get_mut("input")
                && input.len() == 1
                && let Some(Value::String(handle)) = input.get_mut("cursor")
                && issued::parts(handle).is_some()
            {
                visit(handle);
                return;
            }
            for child in object.values_mut() {
                swap(child, visit);
            }
        }
        Value::Array(items) => {
            for item in items {
                swap(item, visit);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn handle(byte: u8) -> String {
        let id = issued::encode(&[byte; issued::ID_BYTES]);
        issued::format(&id, &id)
    }

    #[test]
    fn handles_read_as_the_placeholder_and_are_restored() {
        let call =
            |byte| json!({"kind":"continue","tool":"localSearch","input":{"cursor":handle(byte)}});
        let envelope = |byte| {
            json!({"results":[{"index":0,"data":{
                "next":{"nextPage":call(byte)},
                // Evidence that merely looks like a handle is content.
                "value": handle(byte),
                "related":[{"kind":"related","tool":"localFetch","input":{"queries":[{"path":"a"}]}}]
            }}]})
        };
        let object = |value: Value| value.as_object().cloned().expect("an envelope object");
        let (mut first, mut second) = (object(envelope(1)), object(envelope(2)));
        let original = first.clone();
        let seen = |map: &Map<String, Value>| Value::Object(map.clone());
        let first_seen = masked(&mut first, seen);
        let second_seen = masked(&mut second, seen);
        assert_eq!(first, original, "handles are restored");
        assert_eq!(
            first_seen["results"][0]["data"]["next"], second_seen["results"][0]["data"]["next"],
            "fresh handles of the same call read alike"
        );
        assert_ne!(
            first_seen["results"][0]["data"]["value"],
            second_seen["results"][0]["data"]["value"]
        );
        assert_eq!(
            first_seen["results"][0]["data"]["next"]["nextPage"]["input"]["cursor"]
                .as_str()
                .map(str::len),
            Some(handle(1).len()),
            "the placeholder keeps the handle's length, so the page plan is unchanged"
        );
    }
}
