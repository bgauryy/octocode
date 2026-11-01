use super::*;

#[test]
fn in_range_values_convert_exactly() {
    assert_eq!(saturating_u32(7_usize), 7);
    assert_eq!(saturating_u32(u64::from(u32::MAX)), u32::MAX);
    assert_eq!(saturating_i64(42_u64), 42);
    assert_eq!(saturating_usize(9_u64), 9);
}

#[test]
fn out_of_range_values_saturate() {
    assert_eq!(saturating_u32(u64::from(u32::MAX) * 2), u32::MAX);
    assert_eq!(saturating_u32(usize::MAX), u32::MAX);
    assert_eq!(saturating_i64(u64::MAX), i64::MAX);
    assert_eq!(saturating_usize(u128::MAX), usize::MAX);
}
