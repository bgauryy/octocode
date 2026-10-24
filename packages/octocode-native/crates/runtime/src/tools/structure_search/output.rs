use crate::response::output::ToolOutput;
use serde_json::Value;

pub(crate) struct Output;

impl ToolOutput for Output {}

pub(super) fn finish(mut data: Value) -> Value {
    if let Some(calls) = data.as_object_mut().and_then(|row| row.remove("calls")) {
        crate::response::result::attach_calls(&mut data, calls);
    }
    data
}
