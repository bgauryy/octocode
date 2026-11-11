//! What a walk reads of the text around its values: where a line and a
//! string end, and an armor that names its body as public.

/// Whether `body` holds an armor that names what it encloses as public: a
/// public key or a certificate under a key's name is published, and a
/// private key beside it is the key rules' own.
pub(super) fn is_public_armor(body: &str) -> bool {
    body.contains("-----BEGIN ")
        && ["PUBLIC KEY", "CERTIFICATE"]
            .iter()
            .any(|kind| body.contains(kind))
}

/// Where the line of `at` ends in `bytes`: its line break, or the end.
pub(super) fn line_end(bytes: &[u8], at: usize) -> usize {
    bytes
        .get(at..)
        .and_then(|rest| rest.iter().position(|byte| matches!(byte, b'\n' | b'\r')))
        .map_or(bytes.len(), |end| at + end)
}

/// Where the body of the string opened by the quote at `open` ends: at its
/// closing quote, an escape stepped over, or for a `"` or `'` string at its
/// line's end.
pub(super) fn string_end(bytes: &[u8], open: usize) -> usize {
    let quote = bytes[open];
    let mut at = open + 1;
    while let Some(&byte) = bytes.get(at) {
        if byte == quote || (quote != b'`' && matches!(byte, b'\n' | b'\r')) {
            return at;
        }
        at += if byte == b'\\' { 2 } else { 1 };
    }
    bytes.len()
}
