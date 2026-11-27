use super::*;

#[test]
fn conversions_saturate() {
    assert_eq!(u32_of_signed(-5), 0);
    assert_eq!(u32_of_signed(i64::MAX), u32::MAX);
    assert_eq!(usize_of_signed(-1), 0);
    assert_eq!(usize_of_signed(9), 9);
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
