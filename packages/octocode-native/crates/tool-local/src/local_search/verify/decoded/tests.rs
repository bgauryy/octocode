use super::*;

fn decoded(bytes: &[u8], order: ByteOrder) -> String {
    let mut text = String::new();
    Utf16Text::new(bytes, order)
        .read_to_string(&mut text)
        .expect("decodes");
    text
}

#[test]
fn it_reads_what_the_engine_decodes_in_either_order() {
    let text = "line é 😀\r\nnext\n";
    let little = text
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let big = text
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect::<Vec<_>>();
    assert_eq!(decoded(&little, ByteOrder::Little), text);
    assert_eq!(decoded(&big, ByteOrder::Big), text);
}

/// One byte at a time, as a short read may hand them, changes nothing.
#[test]
fn a_unit_split_across_reads_still_decodes() {
    let text = "😀a";
    let bytes = text
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    struct OneByOne<'a>(&'a [u8]);
    impl Read for OneByOne<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let one = out.len().min(1);
            self.0.read(&mut out[..one])
        }
    }
    let reader = std::io::BufReader::with_capacity(1, OneByOne(&bytes));
    let mut out = String::new();
    Utf16Text::new(reader, ByteOrder::Little)
        .read_to_string(&mut out)
        .expect("decodes");
    assert_eq!(out, text);
}

#[test]
fn a_malformed_unit_reads_as_a_replacement() {
    let lone_high = [0x3d, 0xd8, b'a', 0];
    assert_eq!(decoded(&lone_high, ByteOrder::Little), "\u{fffd}a");
    let cut_off = [b'a', 0, b'b'];
    assert_eq!(decoded(&cut_off, ByteOrder::Little), "a\u{fffd}");
}
