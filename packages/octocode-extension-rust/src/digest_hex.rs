pub(crate) fn lower_hex(bytes: impl AsRef<[u8]>) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";

    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::lower_hex;

    #[test]
    fn encodes_lowercase_hex_without_separators() {
        assert_eq!(lower_hex([0x00, 0x09, 0xaf, 0xff]), "0009afff");
    }
}
