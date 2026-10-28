use super::{CancellationToken, DependencyError, check, reserve};
use serde_json::{Map, Value};
use std::io::{self, Write};

pub(super) fn clone_value(
    value: &Value,
    cancel: &CancellationToken,
    depth: usize,
) -> Result<Value, DependencyError> {
    check(cancel)?;
    if depth > 64 {
        return Err(DependencyError::DepthExceeded);
    }
    match value {
        Value::Array(values) => {
            let mut copy = Vec::new();
            reserve(&mut copy, values.len())?;
            for item in values {
                copy.push(clone_value(item, cancel, depth + 1)?);
            }
            Ok(Value::Array(copy))
        }
        Value::Object(values) => {
            let mut copy = Map::new();
            for (key, item) in values {
                check(cancel)?;
                copy.insert(key.clone(), clone_value(item, cancel, depth + 1)?);
            }
            Ok(Value::Object(copy))
        }
        _ => Ok(value.clone()),
    }
}

struct Counter<'a> {
    cancel: &'a CancellationToken,
    chars: usize,
    overflow: bool,
}

impl Write for Counter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::other("cancelled"));
        }
        self.chars = self
            .chars
            .checked_add(octocode_engine::json_size::utf16_units_of_utf8(bytes))
            .ok_or_else(|| {
                self.overflow = true;
                io::Error::other("size overflow")
            })?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn size(value: &Value, cancel: &CancellationToken) -> Result<usize, DependencyError> {
    check(cancel)?;
    let mut writer = Counter {
        cancel,
        chars: 0,
        overflow: false,
    };
    let result = serde_json::to_writer(&mut writer, value);
    check(cancel)?;
    if writer.overflow {
        return Err(DependencyError::SizeOverflow);
    }
    result.map_err(|_| DependencyError::Serialization)?;
    Ok(writer.chars)
}
