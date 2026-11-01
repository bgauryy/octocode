use serde_json::Value;

pub(super) fn finish(mut data: Value) -> Value {
    crate::response::result::finish_calls(&mut data);
    data
}
