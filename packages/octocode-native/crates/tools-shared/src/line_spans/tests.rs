use super::*;

#[test]
fn spans_merge_when_overlapping_or_adjacent() {
    assert_eq!(
        merge_spans([(10_u32, 12), (1, 3), (4, 5), (11, 20)]),
        vec![(1, 5), (10, 20)]
    );
    assert_eq!(runs([3_usize, 4, 5, 9, 9]), vec![(3, 5), (9, 9), (9, 9)]);
}
