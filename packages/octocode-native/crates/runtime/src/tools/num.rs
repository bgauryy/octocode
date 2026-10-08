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

/// A signed `value` as a `u32`: negatives read as 0, saturating at `u32::MAX`.
pub(crate) fn u32_of_signed(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

/// The public column of a 0-based engine or LSP column: every column a
/// tool emits or takes is 1-based (D2), in the source's UTF-16 units.
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
        assert_eq!(u32_of(NonZeroU64::new(3).unwrap()), 3);
        assert_eq!(u32_of_signed(-5), 0);
        assert_eq!(u32_of_signed(i64::MAX), u32::MAX);
        assert_eq!(usize_of_signed(-1), 0);
        assert_eq!(usize_of_signed(9), 9);
    }
}
