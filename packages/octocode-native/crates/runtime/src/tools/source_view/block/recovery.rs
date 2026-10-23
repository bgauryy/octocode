//! Notes for a parse the parser recovered from: which lines it skipped,
//! and why a block or outline may miss declarations there.
use crate::tools::declarations_cache::file_document;

/// The 1-based line spans the parser could not parse and recovered from;
/// empty after a clean parse.
fn unparsed_spans(content: &str, path: &str) -> Vec<[usize; 2]> {
    file_document(content, path)
        .map(|document| {
            document
                .error_lines
                .iter()
                .map(|span| span.map(crate::num::usize_of))
                .collect()
        })
        .unwrap_or_default()
}

/// `spans` as a note names them: `"3, 7-9"`.
fn span_list(spans: &[[usize; 2]]) -> String {
    spans
        .iter()
        .map(|&[start, end]| {
            if start == end {
                start.to_string()
            } else {
                format!("{start}-{end}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Why the parser could not parse lines of `content`, and what `misses`:
/// a span holding a `redacted` line or the line after it (where the parser
/// meets the placeholder's missing syntax) is redacted text the grammar
/// cannot read; the rest are syntax the grammar lacks (Flow types, in
/// JavaScript). `None` after a clean parse.
fn unparsed_note(content: &str, path: &str, redacted: &[usize], misses: &str) -> Option<String> {
    let (by_redaction, by_syntax): (Vec<_>, Vec<_>) = unparsed_spans(content, path)
        .into_iter()
        .partition(|&[start, end]| {
            redacted
                .iter()
                .any(|&line| line <= end && start <= line.saturating_add(1))
        });
    let example = std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| octocode_engine::text::JS_EXTENSIONS.contains(ext))
        .map_or("", |_| ", e.g. Flow types");
    let notes = [
        (!by_syntax.is_empty()).then(|| {
            format!(
                "The parser could not parse lines {} of this file (syntax its grammar lacks{example}) and recovered, so {misses}.",
                span_list(&by_syntax)
            )
        }),
        (!by_redaction.is_empty()).then(|| {
            format!(
                "The parser could not parse lines {} of this file, where secret-shaped text was redacted, so {misses}.",
                span_list(&by_redaction)
            )
        }),
    ];
    let notes = notes.into_iter().flatten().collect::<Vec<_>>();
    (!notes.is_empty()).then(|| notes.join(" "))
}

/// Why a block may miss its enclosing declaration: the parser recovered
/// from syntax errors, named with the skipped line spans. A read states it
/// once, however many of its windows it explains.
pub(super) fn recovered_parse(content: &str, path: &str) -> Option<String> {
    unparsed_note(
        content,
        path,
        &[],
        "the outline may miss the enclosing declaration",
    )
    .map(|note| format!(" {note}"))
}

/// Why a symbols outline may miss declarations: the same recovered parse
/// a block read names, with the `redacted` lines (1-based) of `content`.
pub(in crate::tools::source_view) fn outline_recovery_warning(
    content: &str,
    path: &str,
    redacted: &[usize],
) -> Option<String> {
    unparsed_note(
        content,
        path,
        redacted,
        "this outline may miss declarations there",
    )
}
