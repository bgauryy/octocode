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
    if sample.contains(&0) || std::str::from_utf8(sample).is_err() {
        return true;
    }
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
