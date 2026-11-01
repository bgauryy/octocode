//! Saturating integer conversions for engine positions, counts and sizes.
//!
//! Engine offsets, lines and counts are `u32` (the unit LSP and the public
//! stats use) and file sizes are `i64` on the wire. A value past the target
//! range reads as that type's maximum: it never wraps or truncates, and every
//! in-range value converts exactly.

/// `value` as a `u32`, saturating at `u32::MAX`.
pub(crate) fn saturating_u32(value: impl TryInto<u32>) -> u32 {
    value.try_into().unwrap_or(u32::MAX)
}

/// `value` as an `i64`, saturating at `i64::MAX`.
pub(crate) fn saturating_i64(value: impl TryInto<i64>) -> i64 {
    value.try_into().unwrap_or(i64::MAX)
}

/// `value` as a `usize`, saturating at `usize::MAX`.
pub(crate) fn saturating_usize(value: impl TryInto<usize>) -> usize {
    value.try_into().unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
