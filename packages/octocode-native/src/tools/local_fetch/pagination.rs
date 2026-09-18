use super::extraction::line_count;
use super::types::*;
fn utf16(s: &str) -> usize {
    s.encode_utf16().count()
}
fn records(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return vec![];
    }
    let mut o = vec![];
    let mut st = 0;
    for (i, c) in s.char_indices() {
        if c == '\n' {
            o.push(&s[st..=i]);
            st = i + 1
        }
    }
    if st < s.len() {
        o.push(&s[st..])
    }
    o
}
pub struct Page {
    pub text: String,
    pub pagination: Pagination,
    pub view_lines: (usize, usize),
}
pub fn page(content: &str, q: &LocalFetchRequest) -> Result<Page, String> {
    let kind = q.chunk_type.unwrap_or(if q.full_content == Some(true) {
        ChunkType::Bytes
    } else {
        ChunkType::Lines
    });
    let offset = q.offset.unwrap_or(0);
    match kind {
        ChunkType::Bytes => {
            let offset = offset.min(content.len());
            if !content.is_char_boundary(offset) {
                return Err(
                    "byte offset must be a UTF-8 code-point boundary within the selected view"
                        .into(),
                );
            }
            let limit = q.limit.unwrap_or(if q.full_content == Some(true) {
                content.len()
            } else {
                16384
            });
            let mut end = (offset + limit).min(content.len());
            while end < content.len() && !content.is_char_boundary(end) {
                end += 1
            }
            let text = content[offset..end].to_owned();
            let first = content[..offset].bytes().filter(|b| *b == b'\n').count() + 1;
            let last = first + line_count(&text).saturating_sub(1);
            Ok(Page {
                text,
                pagination: Pagination {
                    chunk_type: kind,
                    offset,
                    length: end - offset,
                    limit,
                    total_lines: line_count(content),
                    total_bytes: content.len(),
                    has_more: end < content.len(),
                    next_offset: (end < content.len()).then_some(end),
                },
                view_lines: (first, last),
            })
        }
        ChunkType::Lines => {
            let lines = records(content);
            let offset = offset.min(lines.len());
            let selected =
                q.match_string.is_some() || (q.start_line.is_some() && q.end_line.is_some());
            let requested_limit = q.limit.unwrap_or(if selected {
                lines.len().clamp(1, 50_000)
            } else {
                100
            });
            let mut end = (offset + requested_limit).min(lines.len());
            let mut bytes: usize = lines[offset..end].iter().map(|s| s.len()).sum();
            while bytes > 16384 && end > offset {
                end -= 1;
                bytes -= lines[end].len()
            }
            if end == offset && offset < lines.len() {
                let byte_offset: usize = lines[..offset].iter().map(|line| line.len()).sum();
                let mut byte_query = q.clone();
                byte_query.chunk_type = Some(ChunkType::Bytes);
                byte_query.offset = Some(byte_offset);
                byte_query.limit = Some(16384);
                return page(content, &byte_query);
            }
            let text = lines[offset..end].concat();
            Ok(Page {
                text,
                pagination: Pagination {
                    chunk_type: kind,
                    offset,
                    length: end - offset,
                    limit: q.limit.unwrap_or(end.saturating_sub(offset)),
                    total_lines: lines.len(),
                    total_bytes: content.len(),
                    has_more: end < lines.len(),
                    next_offset: (end < lines.len()).then_some(end),
                },
                view_lines: (offset + 1, end),
            })
        }
    }
}
pub fn continuation(q: &LocalFetchRequest, p: &Pagination) -> Option<NextCalls> {
    p.next_offset.map(|offset| {
        let mut query = q.clone();
        query.offset = Some(offset);
        query.chunk_type = Some(p.chunk_type);
        query.limit = Some(p.limit);
        NextCalls {
            r#continue: Some(Continuation {
                tool: "localFetch".into(),
                query,
                confidence: "exact".into(),
                reason: None,
            }),
            read_bounded_lines: None,
        }
    })
}
pub fn result_counts(s: &str) -> (usize, usize, usize) {
    (utf16(s), s.len(), line_count(s))
}
