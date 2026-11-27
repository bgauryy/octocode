use super::*;

#[test]
fn a_capped_sink_keeps_its_max_and_stops_the_scan_there() {
    let mut sink = CappedSpans::new(2);
    assert!(sink.accept((0, 1)));
    assert!(!sink.accept((2, 3)), "the last kept match stops the scan");
    assert!(!sink.accept((4, 5)), "nothing is kept past the max");
    assert_eq!(sink.into_spans(), vec![(0, 1), (2, 3)]);
    assert!(!CappedSpans::new(0).accept((0, 1)));
}
