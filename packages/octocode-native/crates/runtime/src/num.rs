//! Saturating conversions from the wire contract's integer types into the
//! engine's units. Validation enforces the schema bounds, so these only guard
//! the type conversion.

/// `value` as a `usize`, saturating at `usize::MAX`.
pub(crate) fn usize_of(value: impl Into<u64>) -> usize {
    usize::try_from(value.into()).unwrap_or(usize::MAX)
}

/// A signed `value` as a `usize`; a value that does not fit reads as 0.
pub(crate) fn usize_of_signed(value: i64) -> usize {
    usize::try_from(value).unwrap_or(0)
}

/// `value` as a `u32`, saturating at `u32::MAX`.
pub(crate) fn u32_of(value: impl Into<u64>) -> u32 {
    u32::try_from(value.into()).unwrap_or(u32::MAX)
}

/// A count or index `value` as a `u32`, saturating at `u32::MAX`.
pub(crate) fn u32_of_usize(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// A signed `value` as a `u32`: negatives read as 0, saturating at `u32::MAX`.
pub(crate) fn u32_of_signed(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

/// A count or size `value` as an `i64`, saturating at `i64::MAX`.
pub(crate) fn i64_of_usize(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// A float measure the caller already rounded or floored, as a `usize`:
/// NaN and negatives read as 0 and values past `usize::MAX` saturate.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a float-to-int `as` saturates and maps NaN to 0: exactly this clamp, which std offers no `try_from` for"
)]
pub(crate) fn usize_of_f64(value: f64) -> usize {
    value as usize
}

/// A float measure the caller already rounded, as an `i64`: NaN reads as 0
/// and values past the `i64` range saturate.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a float-to-int `as` saturates and maps NaN to 0: exactly this clamp, which std offers no `try_from` for"
)]
pub(crate) fn i64_of_f64(value: f64) -> i64 {
    value as i64
}

/// Whole milliseconds of `elapsed`, saturating at `u64::MAX` (~584 million
/// years, so only a corrupt clock reaches it).
pub(crate) fn millis_of(elapsed: std::time::Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

/// The public column of a 0-based engine or LSP column: every column a
/// tool emits or takes is 1-based, in the source's UTF-16 units.
pub(crate) fn one_based_column(zero_based: u32) -> u32 {
    zero_based.saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;

    #[test]
    fn conversions_saturate() {
        assert_eq!(usize_of(NonZeroU64::new(7).unwrap()), 7);
        assert_eq!(u32_of(u64::MAX), u32::MAX);
        assert_eq!(u32_of_usize(usize::MAX), u32::MAX);
        assert_eq!(u32_of_usize(4), 4);
        assert_eq!(u32_of(NonZeroU64::new(3).unwrap()), 3);
        assert_eq!(u32_of_signed(-5), 0);
        assert_eq!(u32_of_signed(i64::MAX), u32::MAX);
        assert_eq!(usize_of_signed(-1), 0);
        assert_eq!(usize_of_signed(9), 9);
        assert_eq!(i64_of_usize(12), 12);
        assert_eq!(i64_of_usize(usize::MAX), i64::MAX);
        assert_eq!(millis_of(std::time::Duration::from_micros(2_500)), 2);
        assert_eq!(millis_of(std::time::Duration::MAX), u64::MAX);
        assert_eq!(usize_of_f64(3.0), 3);
        assert_eq!(usize_of_f64(-2.0), 0);
        assert_eq!(usize_of_f64(f64::NAN), 0);
        assert_eq!(usize_of_f64(f64::INFINITY), usize::MAX);
        assert_eq!(i64_of_f64(-7.0), -7);
        assert_eq!(i64_of_f64(f64::NAN), 0);
        assert_eq!(i64_of_f64(f64::MAX), i64::MAX);
    }
}
