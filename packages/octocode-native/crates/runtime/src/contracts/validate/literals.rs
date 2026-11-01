use serde_json::{Number, Value};

/// A value outside its allowed literals: the message names the value sent,
/// the literal a near miss means and every allowed value, so a wrong
/// view/role/entryType is self-correcting. The issue's path names the field.
pub(super) fn enum_message(received: Option<&Value>, literals: &[Value]) -> String {
    let allowed = literals
        .iter()
        .map(|literal| {
            literal
                .as_str()
                .map_or_else(|| literal.to_string(), str::to_owned)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let meant = received
        .and_then(Value::as_str)
        .and_then(|sent| super::repair::closest(sent, literals.iter().filter_map(Value::as_str)))
        .map(|meant| format!("use \"{meant}\"; "))
        .unwrap_or_default();
    let sent = received.map_or_else(String::new, |value| format!(" {value}"));
    format!("Value{sent} is outside the allowed enum; {meant}allowed: {allowed}")
}

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
    if !number.is_finite() || number.fract() != 0.0 {
        return None;
    }
    let magnitude = integral_magnitude(number.abs())?;
    if number.is_sign_negative() && magnitude != 0 {
        0_i64.checked_sub_unsigned(magnitude).map(Number::from)
    } else {
        Some(Number::from(magnitude))
    }
}

/// The exact integer an integral, finite, non-negative float holds, read
/// from its bits: `None` past `u64::MAX`, never a rounded or saturated value.
fn integral_magnitude(number: f64) -> Option<u64> {
    const FRACTION_BITS: u32 = 52;
    let bits = number.to_bits();
    let fraction = bits & ((1_u64 << FRACTION_BITS) - 1);
    let biased = i64::try_from(bits >> FRACTION_BITS).ok()?;
    if biased == 0 {
        // Zero or subnormal; an integral subnormal is zero.
        return Some(0);
    }
    let significand = fraction | (1_u64 << FRACTION_BITS);
    // value = significand * 2^(biased - 1023 - 52)
    let shift = biased - 1023 - i64::from(FRACTION_BITS);
    if shift < 0 {
        // Integral, so the dropped bits are zero.
        return Some(significand >> u32::try_from(-shift).ok()?);
    }
    let shift = u32::try_from(shift).ok()?;
    let value = significand.checked_shl(shift)?;
    (value >> shift == significand).then_some(value)
}
