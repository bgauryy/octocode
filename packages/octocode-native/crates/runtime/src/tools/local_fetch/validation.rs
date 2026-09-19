use super::types::*;
pub fn validate_request(q: &LocalFetchRequest) -> Result<(), String> {
    let full = q.full_content == Some(true);
    let matched = q.match_string.is_some();
    let ranged = q.start_line.is_some() || q.end_line.is_some();
    if q.path.trim().is_empty() {
        return Err("path is required".into());
    }
    if q.minify == Some(MinifyMode::Symbols) && (matched || ranged) {
        return Err("minify:\"symbols\" returns a whole-file signature skeleton and cannot be combined with matchString/startLine/endLine — remove the line/match constraints, or use minify:\"standard\" (or \"none\") to extract a specific range.".into());
    }
    if [full, matched, ranged].iter().filter(|x| **x).count() > 1 {
        return Err("fullContent, matchString, and startLine/endLine are mutually exclusive extraction methods".into());
    }
    match (q.start_line, q.end_line) {
        (Some(start), None) => {
            return Err(format!(
                "startLine={} provided without endLine — both are required for line-range extraction.",
                start
            ));
        }
        (None, Some(end)) => {
            return Err(format!(
                "endLine={} provided without startLine — both are required for line-range extraction.",
                end
            ));
        }
        _ => {}
    }
    if q.context_lines.is_some() && q.context_bytes.is_some() {
        return Err("contextLines and contextBytes are mutually exclusive".into());
    }
    if q.context_bytes.is_some() && !matched {
        return Err("contextBytes requires matchString".into());
    }
    if q.limit == Some(0) {
        return Err("limit must be at least 1".into());
    }
    Ok(())
}
pub fn is_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.contains(&0) {
        return true;
    }
    // A multibyte UTF-8 code point may straddle the 8192-byte sample boundary.
    // Trim to the last complete code point before judging: a truncated *trailing*
    // sequence (Utf8Error::error_len() == None) is inconclusive, not binary,
    // whereas an invalid byte *within* the sample (error_len() == Some(_)) is a
    // genuine non-text signal.
    let sample = match std::str::from_utf8(sample) {
        Ok(text) => text.as_bytes(),
        Err(error) if error.error_len().is_some() => return true,
        Err(error) => &sample[..error.valid_up_to()],
    };
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
        data.extend(std::iter::repeat(b'b').take(256));
        assert_eq!(data.len(), 8190 + 4 + 256);
        assert!(!is_binary(&data));
    }

    #[test]
    fn invalid_byte_within_sample_is_binary() {
        let mut data = vec![b'a'; 100];
        data[50] = 0xff; // genuine invalid UTF-8 byte, not a truncation
        assert!(is_binary(&data));
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
