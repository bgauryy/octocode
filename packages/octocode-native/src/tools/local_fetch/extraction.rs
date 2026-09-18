use super::types::{LineRange, LocalFetchRequest, RegexMatch};

pub struct Extraction {
    pub text: String,
    pub source_lines: Option<Vec<usize>>,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub match_ranges: Vec<LineRange>,
    pub matched_lines: Vec<usize>,
    pub count: Option<usize>,
    pub warnings: Vec<String>,
}
fn records(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c == '\n' {
            out.push(&s[start..=i]);
            start = i + 1
        }
    }
    if start < s.len() {
        out.push(&s[start..])
    }
    out
}
pub fn line_count(s: &str) -> usize {
    records(s).len()
}
pub fn extract(
    q: &LocalFetchRequest,
    content: &str,
    regex: &impl RegexMatch,
) -> Result<Extraction, String> {
    let lines = records(content);
    let total = lines.len();
    if let Some(pattern) = q.match_string.as_ref() {
        return match_extract(q, content, &lines, pattern, regex);
    }
    if let (Some(start), Some(end)) = (q.start_line, q.end_line) {
        if end < start {
            return Err(format!(
                "startLine {start} is greater than endLine {end} — startLine must be ≤ endLine"
            ));
        }
        if start > total {
            return Err(format!(
                "Requested startLine {start} exceeds file length ({total} lines)"
            ));
        }
        let end2 = end.min(total);
        let selected = &lines[start.saturating_sub(1)..end2];
        return Ok(Extraction {
            text: selected.concat(),
            source_lines: Some((start..=end2).collect()),
            start: Some(start.max(1)),
            end: Some(end2),
            match_ranges: vec![],
            matched_lines: vec![],
            count: None,
            warnings: if end > total {
                vec![format!(
                    "Requested endLine {end} adjusted to {total} (file end)"
                )]
            } else {
                vec![]
            },
        });
    }
    Ok(Extraction {
        text: content.into(),
        source_lines: None,
        start: None,
        end: None,
        match_ranges: vec![],
        matched_lines: vec![],
        count: None,
        warnings: vec![],
    })
}
fn match_extract(
    q: &LocalFetchRequest,
    content: &str,
    lines: &[&str],
    pattern: &str,
    regex: &impl RegexMatch,
) -> Result<Extraction, String> {
    let sensitive = q.match_string_case_sensitive.unwrap_or(false);
    let regex_ranges = if q.match_string_is_regex.unwrap_or(false) {
        Some(regex.matching_ranges(pattern, sensitive, content).map_err(|error| {
            if pattern == "[" { "Invalid regex pattern: Invalid regular expression: /[/: Unterminated character class".to_owned() } else { error }
        })?)
    } else {
        None
    };
    let needle = if sensitive {
        pattern.into()
    } else {
        pattern.to_lowercase()
    };
    let mut hits = vec![];
    let mut line_start = 0;
    for (i, line) in lines.iter().enumerate() {
        let line_end = line_start + line.len();
        let found = regex_ranges
            .as_ref()
            .map(|ranges| {
                ranges
                    .iter()
                    .any(|(start, _)| *start >= line_start && *start < line_end)
            })
            .unwrap_or_else(|| {
                if sensitive {
                    line.contains(pattern)
                } else {
                    line.to_lowercase().contains(&needle)
                }
            });
        if found {
            hits.push(i + 1)
        }
        line_start = line_end;
    }
    if hits.is_empty() {
        return Ok(Extraction {
            text: String::new(),
            source_lines: Some(vec![]),
            start: None,
            end: None,
            match_ranges: vec![],
            matched_lines: vec![],
            count: Some(0),
            warnings: vec![],
        });
    }
    let context = q
        .context_lines
        .unwrap_or(if q.context_bytes.is_none() { 5 } else { 0 });
    let mut ranges: Vec<LineRange> = vec![];
    for &line in &hits {
        let r = LineRange {
            start: line.saturating_sub(context).max(1),
            end: (line + context).min(lines.len()),
        };
        if let Some(last) = ranges.last_mut().filter(|last| r.start <= last.end + 1) {
            last.end = last.end.max(r.end)
        } else {
            ranges.push(r)
        }
    }
    let mut selected = vec![];
    let mut text = String::new();
    for r in &ranges {
        for line in r.start..=r.end {
            text.push_str(lines[line - 1]);
            selected.push(line)
        }
    }
    if let Some(bytes) = q.context_bytes {
        let mut spans = vec![];
        if let Some(found) = regex_ranges.as_ref() {
            spans.extend(found.iter().copied());
        } else {
            let hay = if sensitive {
                content.into()
            } else {
                content.to_lowercase()
            };
            let mut from = 0;
            while let Some(pos) = hay[from..].find(&needle) {
                let start = from + pos;
                spans.push((start, start + needle.len()));
                from = start + needle.len().max(1)
            }
        }
        text = String::new();
        let mut last_end = 0;
        selected.clear();
        for (start, end) in spans {
            let mut a = start.saturating_sub(bytes);
            while !content.is_char_boundary(a) {
                a += 1
            }
            let mut b = (end + bytes).min(content.len());
            while b < content.len() && !content.is_char_boundary(b) {
                b += 1
            }
            if !text.is_empty() && a > last_end {
                text.push('\n')
            }
            text.push_str(&content[a..b]);
            let first_line = content[..a].bytes().filter(|byte| *byte == b'\n').count() + 1;
            let last_byte = b.saturating_sub(1);
            let last_line = content.as_bytes()[..last_byte.min(content.len())]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
                + 1;
            selected.extend(first_line..=last_line.max(first_line));
            last_end = b
        }
        selected.sort_unstable();
        selected.dedup();
        ranges = super::executor::compress_ranges(&selected);
    }
    Ok(Extraction {
        text,
        source_lines: Some(selected),
        start: ranges.first().map(|r| r.start),
        end: ranges.last().map(|r| r.end),
        match_ranges: ranges,
        matched_lines: hits.clone(),
        count: Some(hits.len()),
        warnings: vec![],
    })
}
