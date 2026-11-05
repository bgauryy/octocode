//! Every value of a cut of a document, for when no walk could keep the document's
//! structure: names and structure stay, and no value can be told from another.
use super::source_names::{Named, SecretForm, SecretSpan};
use std::ops::Range;

/// The spans of every value `bytes` shows of what `named` says it holds. A text
/// `cut` inside a line starts in an unknown state, so all of that line goes.
#[must_use]
pub fn every_value_spans(bytes: &[u8], cut: bool, named: Named) -> Vec<SecretSpan> {
    let first_line = if cut {
        bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .unwrap_or(bytes.len())
    } else {
        0
    };
    let mut spans = Vec::new();
    let form = if named.objects {
        SecretForm::SecretInJson
    } else {
        SecretForm::SecretInYaml
    };
    if first_line > 0 {
        spans.push(SecretSpan {
            range: 0..first_line,
            form,
        });
    }
    if named.objects {
        strings(bytes, first_line, &mut spans);
    }
    if named.yaml {
        yaml_values(bytes, first_line, &mut spans);
    }
    spans
}

fn push(spans: &mut Vec<SecretSpan>, range: Range<usize>, form: SecretForm) {
    if !range.is_empty() {
        spans.push(SecretSpan { range, form });
    }
}

/// Every JSON string that is not a member's name, from `from` on. No string spans
/// a raw line, so one left open ends at the line's end.
fn strings(bytes: &[u8], from: usize, spans: &mut Vec<SecretSpan>) {
    let mut at = from;
    while let Some(open) = bytes[at..].iter().position(|byte| *byte == b'"') {
        let start = at + open + 1;
        let mut end = start;
        while let Some(byte) = bytes.get(end) {
            match byte {
                b'"' | b'\n' => break,
                b'\\' => end += 2,
                _ => end += 1,
            }
        }
        let end = end.min(bytes.len());
        let closed = bytes.get(end) == Some(&b'"');
        let after = end
            + 1
            + bytes[(end + 1).min(bytes.len())..]
                .iter()
                .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
                .count();
        let is_name =
            closed && bytes.get(after) == Some(&b':') && bytes.get(after + 1) != Some(&b':');
        if !is_name {
            push(spans, start..end, SecretForm::SecretInJson);
        }
        at = (end + usize::from(closed)).min(bytes.len());
    }
}

/// What each YAML line from `from` on holds after its name's colon, or all of it
/// when it has no name: a block's lines hold plain text.
fn yaml_values(bytes: &[u8], from: usize, spans: &mut Vec<SecretSpan>) {
    let mut start = from;
    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |at| start + at);
        let mut body = start;
        while matches!(bytes.get(body), Some(b' ' | b'\t'))
            || (matches!(bytes.get(body), Some(b'-'))
                && matches!(bytes.get(body + 1), Some(b' ' | b'\t')))
        {
            body += 1;
        }
        let line = &bytes[body..end];
        let separator = matches!(line, b"---" | b"..." | b"---\r" | b"...\r");
        let value = line
            .iter()
            .enumerate()
            .position(|(at, byte)| {
                *byte == b':' && matches!(line.get(at + 1), None | Some(b' ' | b'\t' | b'\r'))
            })
            .map_or(body, |colon| body + colon + 1);
        if !separator {
            push(spans, value..end, SecretForm::SecretInYaml);
        }
        start = end + 1;
    }
}
