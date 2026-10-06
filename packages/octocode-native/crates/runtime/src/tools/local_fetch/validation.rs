/// Source text from file bytes: UTF-8 when valid; otherwise the encoding
/// fallback is flagged with a warning. Bytes holding no valid non-ASCII
/// UTF-8 sequence decode as Latin-1 (ISO-8859-1), one char per byte; text
/// that is mostly UTF-8 keeps it and replaces the stray bytes (U+FFFD).
pub fn decode_text(bytes: &[u8]) -> (String, Option<&'static str>) {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return (text.to_owned(), None);
    }
    let utf8_multibyte = bytes.utf8_chunks().any(|chunk| !chunk.valid().is_ascii());
    if utf8_multibyte {
        return (
            String::from_utf8_lossy(bytes).into_owned(),
            Some("Not valid UTF-8: invalid bytes were replaced with U+FFFD."),
        );
    }
    (
        bytes.iter().map(|&byte| char::from(byte)).collect(),
        Some(
            "Not valid UTF-8: decoded as Latin-1 (ISO-8859-1); characters from other 8-bit encodings may be approximate, and byte offsets count the decoded UTF-8 text.",
        ),
    )
}

pub fn is_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.contains(&0) {
        return true;
    }
    // Binary is NUL bytes or a dense run of C0 controls. UTF-8 validity is
    // an encoding question, not a binary one: Latin-1 and other 8-bit text
    // is decoded (see `decode_text`), never refused.
    let mut stripped = 0usize;
    let mut controls = 0usize;
    let mut i = 0;
    while i < sample.len() {
        if sample[i] == 0x1b && sample.get(i + 1) == Some(&0x5b) {
            i += 2;
            while i < sample.len() && !(0x40..=0x7e).contains(&sample[i]) {
                i += 1
            }
            i += 1;
            continue;
        }
        stripped += 1;
        if sample[i] < 0x20 && !matches!(sample[i], b'\t' | b'\n' | b'\r') {
            controls += 1
        }
        i += 1
    }
    stripped > 0 && (controls as f64 / stripped as f64) > 0.05
}

#[cfg(test)]
mod is_binary_tests {
    use super::is_binary;

    #[test]
    fn multibyte_char_straddling_sample_boundary_is_not_binary() {
        // Place a 4-byte emoji so the 8192-byte sniff boundary splits it: 8190
        // ASCII bytes + "😀" (F0 9F 98 80) means the sample ends after F0 9F,
        // an incomplete trailing sequence. This must be treated as text.
        let mut data = vec![b'a'; 8190];
        data.extend_from_slice("😀".as_bytes());
        data.extend(std::iter::repeat_n(b'b', 256));
        assert_eq!(data.len(), 8190 + 4 + 256);
        assert!(!is_binary(&data));
    }

    /// Encoding is not the binary signal: Latin-1 text is text.
    #[test]
    fn invalid_utf8_text_is_not_binary_but_control_bytes_are() {
        let mut data = vec![b'a'; 100];
        data[50] = 0xff;
        assert!(!is_binary(&data));
        assert!(!is_binary(b"caf\xe9 cr\xe8me br\xfbl\xe9e\n"));
        let mut controls = vec![b'a'; 100];
        controls[..10].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 14, 15]);
        assert!(is_binary(&controls));
    }

    #[test]
    fn null_byte_is_binary() {
        assert!(is_binary(b"abc\0def"));
    }

    #[test]
    fn plain_ascii_is_not_binary() {
        assert!(!is_binary(b"fn main() {}\n"));
    }
}
