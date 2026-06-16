// ── JSON ─────────────────────────────────────────────────────────────────────

pub fn minify_json_core_inner(content: &str) -> (String, bool) {
    // Try direct parse first
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        return (
            serde_json::to_string(&v).unwrap_or_else(|_| content.trim().to_owned()),
            false,
        );
    }
    // JSONC / JSON5: strip comments + trailing commas then parse
    let cleaned = strip_json_noise(content);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&cleaned) {
        (
            serde_json::to_string(&v).unwrap_or_else(|_| content.trim().to_owned()),
            false,
        )
    } else {
        (content.trim().to_owned(), true)
    }
}

pub fn minify_json_readable_inner(content: &str) -> (String, bool) {
    if serde_json::from_str::<serde_json::Value>(content).is_ok() {
        return (content.to_owned(), false); // already clean JSON — return as-is
    }
    let cleaned = strip_json_noise(content);
    if serde_json::from_str::<serde_json::Value>(&cleaned).is_err() {
        return (content.trim().to_owned(), true);
    }
    let cleaned = cleaned
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    // collapse ≥3 blank lines
    let mut result = String::with_capacity(cleaned.len());
    let mut blanks = 0u32;
    for line in cleaned.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks <= 2 {
                result.push('\n');
            }
        } else {
            blanks = 0;
            result.push_str(line);
            result.push('\n');
        }
    }
    (result.trim().to_owned(), false)
}

fn strip_json_noise(s: &str) -> String {
    let after_comments = strip_json_comments(s);
    strip_trailing_commas(&after_comments)
}

fn strip_json_comments(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut result = String::with_capacity(content.len());
    let mut i = 0;
    let mut in_str = false;
    let mut escaped = false;
    while i < bytes.len() {
        let ch = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if ch == b'\\' {
                escaped = true;
            } else if ch == b'"' {
                in_str = false;
            }
            i = super::copy_seq(content, i, &mut result);
            continue;
        }
        if ch == b'"' {
            in_str = true;
            result.push('"');
            i += 1;
            continue;
        }
        if ch == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if ch == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 < bytes.len() {
                i += 2;
            }
            continue;
        }
        i = super::copy_seq(content, i, &mut result);
    }
    result
}

fn strip_trailing_commas(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut result = String::with_capacity(content.len());
    let mut i = 0;
    let mut in_str = false;
    let mut escaped = false;
    while i < bytes.len() {
        let ch = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if ch == b'\\' {
                escaped = true;
            } else if ch == b'"' || ch == b'\'' {
                in_str = false;
            }
            i = super::copy_seq(content, i, &mut result);
            continue;
        }
        if ch == b'"' || ch == b'\'' {
            in_str = true;
            result.push(ch as char);
            i += 1;
            continue;
        }
        if ch == b',' {
            let mut look = i + 1;
            while look < bytes.len() && matches!(bytes[look], b' ' | b'\t' | b'\n' | b'\r') {
                look += 1;
            }
            if look < bytes.len() && matches!(bytes[look], b'}' | b']') {
                i += 1;
                continue;
            }
        }
        i = super::copy_seq(content, i, &mut result);
    }
    result
}
