//! A structural secret span is read through the window read buffer: its
//! placeholders are built from chunks, a cancel stops it between chunks, and no
//! read holds the whole span. Every value is fake.
use super::super::masks::PlaceholderEdits;
use super::super::stream::stream_pass;
use super::*;
use crate::execution::cancel::NeverCancel;
use octocode_engine::security::{SecretForm, SecretSpan};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The placeholder edits the old whole-span read built: one per line, from its
/// first byte that is not a blank to its end.
fn whole_span_edits(span: &SecretSpan, bytes: &[u8]) -> Vec<Edit> {
    let text: Box<str> =
        octocode_engine::security::redaction_placeholder(span.form.pattern()).into();
    let mut edits = Vec::new();
    let mut at = 0;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        let body = line.strip_suffix(b"\n").unwrap_or(line);
        if let Some(first) = body
            .iter()
            .position(|byte| !matches!(byte, b' ' | b'\t' | b'\r'))
        {
            edits.push(Edit {
                range: span.range.start + at + first..span.range.start + at + body.len(),
                text: text.clone(),
            });
        }
        at += line.len();
    }
    edits
}

/// Fed in chunks of any size, the edits are the ones the whole span gives:
/// indentation, CRLF, blank lines and a last line with no break.
#[test]
fn placeholder_edits_from_chunks_match_the_whole_span() {
    let bytes = b"  key: a\r\n\n\t  b c\n   \n\r\n d\xc3\xa9 e\n  tail";
    let span = SecretSpan {
        range: 100..100 + bytes.len(),
        form: SecretForm::SecretInYaml,
    };
    let whole = whole_span_edits(&span, bytes);
    assert_eq!(whole.len(), 4, "{whole:?}");
    for size in 1..=bytes.len() {
        let (mut lines, mut edits) = (PlaceholderEdits::new(&span), Vec::new());
        for chunk in bytes.chunks(size) {
            lines.feed(chunk, &mut edits);
        }
        lines.finish(&mut edits);
        assert_eq!(edits, whole, "chunks of {size}");
    }
}

/// A JSON source whose one secret value is many read buffers long.
fn long_secret_source() -> String {
    let value = "RmFrZTBWYWx1ZTlOb3Q0UmVhbDhCbG9iN0xvbmc2RW5vdWdo".repeat(READ_BYTES / 8);
    format!(
        "{{\"items\":[\n  {{\"data\":{{\"blob\":\"{value}\"}},\"kind\":\"Secret\"}},\n  {{\"data\":{{\"blob\":\"public\"}},\"kind\":\"ConfigMap\"}}\n]}}\n"
    )
}

/// A source's proof with its structure judged and its edits not yet built, and
/// the bytes of its longest structural span.
fn judged_proof(file: &std::fs::File, path: &std::path::Path) -> (Proven, usize) {
    let pass = stream_pass(
        file,
        path,
        &[Want::lines(2, 2)],
        &crate::security::ContentSecurity::new(),
        &NeverCancel,
    )
    .expect("pass");
    let mut proven = pass.proven;
    let Some(Judged::Exact(spans)) = proven.judged.get() else {
        panic!("the walk places the source's secret");
    };
    let longest = spans.iter().map(|span| span.range.len()).max().unwrap_or(0);
    proven.sealed.take();
    (proven, longest)
}

/// The edits of a span many buffers long come from reads no longer than one
/// buffer, and are the edits of the whole span.
#[test]
fn a_long_structural_span_is_read_one_buffer_at_a_time() {
    let text = long_secret_source();
    let file = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(file.path(), &text).expect("write");
    let handle = std::fs::File::open(file.path()).expect("open");
    let (proven, longest) = judged_proof(&handle, file.path());
    assert!(longest > 4 * READ_BYTES, "the span is {longest} bytes");
    super::super::take_largest_read();
    let edits = proven
        .edits(&handle, &NeverCancel)
        .expect("edits")
        .all(&handle)
        .expect("listed");
    let largest = super::super::take_largest_read();
    assert!(largest <= READ_BYTES, "one read took {largest} bytes");
    let Some(Judged::Exact(spans)) = proven.judged.get() else {
        panic!("judged");
    };
    for span in spans {
        let bytes = text.as_bytes().get(span.range.clone()).expect("span bytes");
        for edit in whole_span_edits(span, bytes) {
            assert!(edits.contains(&edit), "{:?} is shown", edit.range);
        }
    }
}

/// Fails every check after the first `allowed`.
struct CancelAfter {
    allowed: usize,
    checks: AtomicUsize,
}

impl CancellationCheck for CancelAfter {
    fn check(&self) -> Result<(), String> {
        if self.checks.fetch_add(1, Ordering::SeqCst) < self.allowed {
            Ok(())
        } else {
            Err("deadline reached".into())
        }
    }
}

/// A cancel during a long span's read stops it with the cancel's reason, and
/// records no edits.
#[test]
fn a_cancel_stops_a_long_span_between_reads() {
    let text = long_secret_source();
    let file = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(file.path(), &text).expect("write");
    let handle = std::fs::File::open(file.path()).expect("open");
    let (proven, longest) = judged_proof(&handle, file.path());
    assert!(longest > 4 * READ_BYTES, "the span is {longest} bytes");
    let cancel = CancelAfter {
        allowed: 1,
        checks: AtomicUsize::new(0),
    };
    assert_eq!(
        proven
            .edits(&handle, &cancel)
            .err()
            .map(|error| error.to_string())
            .as_deref(),
        Some("deadline reached")
    );
    assert!(proven.sealed.get().is_none());
}
