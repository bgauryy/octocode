//! Notes for a parse the parser recovered from: which lines it skipped,
//! and why a block or outline may miss declarations there.
use crate::declarations::file_document;

/// The 1-based line spans the parser could not parse and recovered from;
/// empty after a clean parse.
fn unparsed_spans(
    document: Option<&octocode_engine::graph::GraphFactsDocument>,
) -> Vec<[usize; 2]> {
    document
        .map(|document| {
            document
                .error_lines
                .iter()
                .map(|span| span.map(octocode_engine::num::saturating_usize))
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
            } else if end == u32::MAX as usize {
                // The parser marks a file with more error spans than it
                // lists as one span to its end.
                format!("{start} to the end")
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
/// JavaScript). With `through`, only the spans that start at or before that
/// line: a later span cannot hide what encloses it. `None` after a clean
/// parse, or when no span starts by `through`.
fn unparsed_note(
    document: Option<&octocode_engine::graph::GraphFactsDocument>,
    path: &str,
    redacted: &[usize],
    through: Option<usize>,
    misses: &str,
) -> Option<String> {
    let (by_redaction, by_syntax): (Vec<_>, Vec<_>) = unparsed_spans(document)
        .into_iter()
        .filter(|&[start, _]| through.is_none_or(|line| start <= line))
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
/// from syntax errors, named with the skipped line spans (with `through`,
/// those starting at or before that line). A read states it once, however
/// many of its windows it explains.
pub(super) fn recovered_parse(content: &str, path: &str, through: Option<usize>) -> Option<String> {
    let document = file_document(content, path);
    unparsed_note(
        document.as_deref(),
        path,
        &[],
        through,
        "the outline may miss the enclosing declaration",
    )
    .map(|note| format!(" {note}"))
}

/// Why a symbols outline may miss declarations: the same recovered parse
/// a block read names, with the `redacted` lines (1-based) of `content`.
pub(in crate::source_view) fn outline_recovery_warning(
    document: Option<&octocode_engine::graph::GraphFactsDocument>,
    path: &str,
    redacted: &[usize],
) -> Option<String> {
    use octocode_engine::graph::GraphFactCode;
    // The notes astSearch symbols shows for the same document: which lane
    // read the file (a nesting past oxc's stack) and a parse or walk that
    // stopped before the end.
    let lanes = document
        .into_iter()
        .flat_map(|document| &document.diagnostics)
        .filter(|note| {
            matches!(
                note.code,
                GraphFactCode::ParserFallback
                    | GraphFactCode::ParseDeadline
                    | GraphFactCode::TraversalDeadline
            )
        })
        // A parser's message may quote the source it stopped at.
        .map(|note| octocode_runtime_base::security::quoted(&note.to_string()).into_owned());
    let notes = unparsed_note(
        document,
        path,
        redacted,
        None,
        "this outline may miss declarations there",
    )
    .into_iter()
    .chain(lanes)
    .collect::<Vec<_>>();
    (!notes.is_empty()).then(|| notes.join(" "))
}
