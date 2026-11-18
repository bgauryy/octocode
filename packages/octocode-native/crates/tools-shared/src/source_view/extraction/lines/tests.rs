use super::*;

#[test]
fn offsets_select_exact_source_bytes_for_every_line_shape() {
    for source in [
        "",
        "\n",
        "a",
        "a\n",
        "a\n\n",
        "café\r\n😀\nλ\rfinal",
        "\r\n\r\n",
    ] {
        let offsets = LineOffsets::new(source);
        let expected = source.split_inclusive('\n').collect::<Vec<_>>();
        assert_eq!(offsets.len(), expected.len(), "{source:?}");
        assert!(
            offsets.weight()
                >= std::mem::size_of::<LineOffsets>()
                    + (expected.len() + 1) * std::mem::size_of::<usize>()
        );
        for first in 1..=expected.len() {
            for last in first..=expected.len() {
                assert_eq!(
                    offsets.range(source, first, last),
                    expected[first - 1..last].concat(),
                    "{source:?}, {first}..={last}"
                );
            }
        }
    }
}
