use serde_json::{Number, Value};

/// JSON Schema compares numeric values, independently of JSON integer/float spelling.
pub(super) fn equivalent(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            let left = integral_number(left).unwrap_or_else(|| left.clone());
            let right = integral_number(right).unwrap_or_else(|| right.clone());
            left == right
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| equivalent(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, left)| right.get(key).is_some_and(|right| equivalent(left, right)))
        }
        _ => left == right,
    }
}

/// Convert only exactly representable integral floats; never round an integer via f64.
pub(super) fn integral_number(number: &Number) -> Option<Number> {
    if !number.is_f64() {
        return None;
    }
    let number = number.as_f64()?;
    if number.fract() != 0.0 {
        return None;
    }
    // u64::MAX rounds to 2^64; the exclusive bound prevents a saturating cast.
    if number >= 0.0 && number < u64::MAX as f64 {
        Some(Number::from(number as u64))
    } else if number >= i64::MIN as f64 && number < 0.0 {
        Some(Number::from(number as i64))
    } else {
        None
    }
}
