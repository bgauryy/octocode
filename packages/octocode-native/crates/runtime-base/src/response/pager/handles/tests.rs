use super::*;
use serde_json::json;

fn handle(byte: u8) -> String {
    let id = issued::encode(&[byte; issued::ID_BYTES]);
    issued::format(&id, &id)
}

#[test]
fn handles_read_as_the_placeholder_and_are_restored() {
    let call = |byte| crate::response::result::Continuation::cursor("localSearch", handle(byte));
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
