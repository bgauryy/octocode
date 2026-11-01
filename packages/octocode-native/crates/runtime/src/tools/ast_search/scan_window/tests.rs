use super::*;

#[test]
fn a_whole_scan_has_no_continuation() {
    assert_eq!(expand_scan(false, Some(5)), None);
    assert_eq!(expand_scan(false, None), None);
}

/// A time-budget cut resumes wherever it stopped, the former 100,000-file
/// walk ceiling and far past it included.
#[test]
fn a_time_budget_cut_resumes_past_any_former_ceiling() {
    for resume in [1, 99_999, 100_000, 100_001, 5_000_000, u32::MAX] {
        assert_eq!(
            expand_scan(true, Some(resume)),
            Some(ScanWindow {
                scan_offset: resume
            }),
            "{resume}"
        );
    }
}
