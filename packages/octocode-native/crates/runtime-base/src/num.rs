//! Runtime-only numeric rules: signed wire values (a negative reads as 0),
//! float measures, durations and public columns. Unsigned saturating
//! conversions have one owner, [`octocode_engine::num`].

/// A signed `value` as a `usize`; a value that does not fit reads as 0.
pub fn usize_of_signed(value: i64) -> usize {
    usize::try_from(value).unwrap_or(0)
}

/// A signed `value` as a `u32`: negatives read as 0, saturating at `u32::MAX`.
pub fn u32_of_signed(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

/// A float measure the caller already rounded or floored, as a `usize`:
/// NaN and negatives read as 0 and values past `usize::MAX` saturate.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a float-to-int `as` saturates and maps NaN to 0: exactly this clamp, which std offers no `try_from` for"
)]
pub fn usize_of_f64(value: f64) -> usize {
    value as usize
}

/// A float measure the caller already rounded, as an `i64`: NaN reads as 0
/// and values past the `i64` range saturate.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a float-to-int `as` saturates and maps NaN to 0: exactly this clamp, which std offers no `try_from` for"
)]
pub fn i64_of_f64(value: f64) -> i64 {
    value as i64
}

/// Whole milliseconds of `elapsed`, saturating at `u64::MAX` (~584 million
/// years, so only a corrupt clock reaches it).
pub fn millis_of(elapsed: std::time::Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

/// The public column of a 0-based engine or LSP column: every column a
/// tool emits or takes is 1-based, in the source's UTF-16 units.
pub fn one_based_column(zero_based: u32) -> u32 {
    zero_based.saturating_add(1)
}

#[cfg(test)]
mod tests;
