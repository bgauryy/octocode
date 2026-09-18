use std::collections::BTreeSet;

use regex::RegexBuilder;

use crate::types::{
    ByteRange, ExtractMatchingLinesOptions, ExtractMatchingLinesResult, MatchRange,
};

pub(crate) fn extract_matching_bytes(
    content: &str,
    pattern: &str,
    options: ExtractMatchingLinesOptions,
) -> ExtractMatchingLinesResult {
    let sensitive = options.case_sensitive.unwrap_or(false);
    let is_regex = options.is_regex.unwrap_or(false);
    let expression = if is_regex {
        pattern.to_owned()
    } else {
        regex::escape(pattern)
    };
    let regex = RegexBuilder::new(&expression)
        .case_insensitive(!sensitive)
        .build();
    let mut hits = Vec::new();
    let mut starts = Vec::new();
    let mut offset = 0;
    for (i, record) in content.split_inclusive('\n').enumerate() {
        starts.push(offset);
        let line = record.trim_end_matches('\n').trim_end_matches('\r');
        if !pattern.is_empty() {
            if let Ok(re) = &regex {
                hits.extend(
                    re.find_iter(line)
                        .map(|m| (offset + m.start(), offset + m.end(), i + 1)),
                );
            }
        }
        offset += record.len();
    }
    // Match the line extractor's whitespace-insensitive literal fallback while
    // retaining offsets into the original UTF-8 source.
    if hits.is_empty() && !is_regex && !pattern.is_empty() {
        let (needle, _) = compact(pattern, sensitive);
        if !needle.is_empty() {
            for (i, record) in content.split_inclusive('\n').enumerate() {
                let (line, map) = compact(
                    record.trim_end_matches('\n').trim_end_matches('\r'),
                    sensitive,
                );
                for (at, _) in line.match_indices(&needle) {
                    hits.push((
                        starts[i] + map[at].0,
                        starts[i] + map[at + needle.len() - 1].1,
                        i + 1,
                    ));
                }
            }
        }
    }
    let all_lines: BTreeSet<usize> = hits.iter().map(|hit| hit.2).collect();
    let matched: BTreeSet<usize> = all_lines
        .iter()
        .copied()
        .take(options.max_matches.map_or(usize::MAX, |n| n as usize))
        .collect();
    let context = options.context_bytes.unwrap_or(0) as usize;
    let mut windows: Vec<(usize, usize)> = Vec::new();
    for (start, end, line) in hits {
        if !matched.contains(&line) {
            continue;
        }
        let mut start = start.saturating_sub(context);
        let mut end = end.saturating_add(context).min(content.len());
        while !content.is_char_boundary(start) {
            start -= 1;
        }
        while !content.is_char_boundary(end) {
            end += 1;
        }
        if let Some(last) = windows.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        windows.push((start, end));
    }
    ExtractMatchingLinesResult {
        lines: vec![],
        matching_lines: matched.into_iter().map(|n| n as u32).collect(),
        match_count: all_lines.len() as u32,
        match_ranges: windows
            .iter()
            .map(|&(start, end)| MatchRange {
                start: starts.partition_point(|&n| n <= start) as u32,
                end: starts.partition_point(|&n| n <= end.saturating_sub(1).max(start)) as u32,
            })
            .collect(),
        byte_ranges: Some(
            windows
                .into_iter()
                .map(|(start, end)| ByteRange {
                    start: start as u32,
                    end: end as u32,
                })
                .collect(),
        ),
    }
}

fn compact(text: &str, sensitive: bool) -> (String, Vec<(usize, usize)>) {
    let mut output = String::new();
    let mut map = Vec::new();
    for (start, ch) in text.char_indices().filter(|(_, ch)| !ch.is_whitespace()) {
        let value = if sensitive {
            ch.to_string()
        } else {
            ch.to_lowercase().collect()
        };
        map.extend(std::iter::repeat_n(
            (start, start + ch.len_utf8()),
            value.len(),
        ));
        output.push_str(&value);
    }
    (output, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(source: &str, needle: &str, context: u32, regex: bool) -> String {
        let result = extract_matching_bytes(
            source,
            needle,
            ExtractMatchingLinesOptions {
                context_bytes: Some(context),
                is_regex: Some(regex),
                ..Default::default()
            },
        );
        result
            .byte_ranges
            .unwrap()
            .iter()
            .map(|r| &source[r.start as usize..r.end as usize])
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn preserves_unicode_boundaries_and_exact_match() {
        assert_eq!(
            read("prefix🌍needleéafter", "needle", 1, false),
            "🌍needleé"
        );
        assert_eq!(read("prefixNEEDLEsuffix", "needle", 0, false), "NEEDLE");
    }

    #[test]
    fn merges_overlap_and_keeps_separate_same_line_occurrences() {
        assert_eq!(read("a hit b hit c", "hit", 3, false), "a hit b hit c");
        assert_eq!(read("hit------hit", "hit", 0, false), "hit\nhit");
    }

    #[test]
    fn supports_regex_anchors_crlf_and_literal_fallback() {
        assert_eq!(
            read("skip\r\nneedle🌍\r\ntail", "^needle", 0, true),
            "needle"
        );
        assert_eq!(read("x café =  2; y", "CAFÉ=2", 0, false), "café =  2");
        assert_eq!(read("hello", "absent", 3, false), "");
    }
}
