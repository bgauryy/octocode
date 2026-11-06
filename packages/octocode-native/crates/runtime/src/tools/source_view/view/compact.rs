use std::sync::Arc;

use crate::execution::cancel::CancellationCheck;
use crate::response::numbered::{GAP, push_numbered};
use crate::tools::page_memo::PageMemo;
use crate::tools::source_view::extraction::OMISSION_LINE;
use octocode_engine::minify::apply::{Lexing, WhitespaceOnly, compact_view, lex};

/// Where each source's comments and literals are, by its digest and path, so
/// later pages lay out lines without parsing again. Only byte offsets are
/// kept, never source text.
static LEXINGS: PageMemo<Arc<Lexing>> = PageMemo::new();

/// `viewFallback.reason` of a compact read served as content: compact kept
/// no fewer bytes than the content view of the same lines.
pub(super) const NOT_SMALLER: &str = "not-smaller";

/// The bytes the content view serves for `selected`: each line under its
/// source number's gutter (`N\t`), an omission between windows as the gap
/// line, as the response numbers them.
pub(super) fn content_bytes(selected: &str, source_lines: Option<&[usize]>) -> usize {
    let gutter = |number: usize| {
        number
            .checked_ilog10()
            .map_or(1, |digits| digits as usize + 1)
            + 1
    };
    selected
        .split_inclusive('\n')
        .enumerate()
        .map(
            |(index, line)| match source_lines.map(|lines| lines.get(index)) {
                None => gutter(index + 1) + line.len(),
                Some(Some(&OMISSION_LINE)) => GAP.len(),
                Some(Some(&number)) => gutter(number) + line.len(),
                Some(None) => line.len(),
            },
        )
        .sum()
}

/// Compact the whole source, for its full lexical context (a selected range
/// may begin inside a literal whose delimiter is outside it), then project the
/// requested source lines. Every kept line carries the source line it is from.
/// Also returns why the view holds only whitespace changes, if it does.
pub(super) fn render(
    full_source: &str,
    selected: &str,
    path: &str,
    source_lines: Option<&[usize]>,
    cancel: &impl CancellationCheck,
) -> (String, Option<WhitespaceOnly>) {
    let source = if source_lines.is_some() {
        full_source
    } else {
        selected
    };
    let view = compact_view(source, &lexing(source, path, cancel));
    let mut output = String::with_capacity(source.len());
    let Some(wanted) = source_lines else {
        for row in &view.lines {
            push_numbered(&mut output, row.line, &row.text);
        }
        return (output, view.whitespace_only);
    };
    let mut rows = view.lines.iter().peekable();
    for &number in wanted {
        if number == 0 {
            output.push_str(GAP);
            continue;
        }
        while rows.peek().is_some_and(|row| row.line < number) {
            rows.next();
        }
        if let Some(row) = rows.next_if(|row| row.line == number) {
            push_numbered(&mut output, row.line, &row.text);
        }
    }
    (output, view.whitespace_only)
}

/// `source`'s lexing, from the store when an earlier page made it. A parse
/// ends when the request is cancelled or its deadline passes.
fn lexing(source: &str, path: &str, cancel: &impl CancellationCheck) -> Arc<Lexing> {
    let digest = octocode_engine::digest::sha256(source.as_bytes());
    if let Some(kept) = LEXINGS.get(&digest, path, Arc::clone) {
        return kept;
    }
    let stop = || {
        cancel.check().is_err()
            || cancel
                .deadline()
                .is_some_and(|deadline| std::time::Instant::now() >= deadline)
    };
    let lexing = Arc::new(lex(source, path, &stop));
    if lexing.reusable() {
        LEXINGS.put(
            digest,
            path.to_owned(),
            lexing.heap_bytes(),
            Arc::clone(&lexing),
        );
    }
    lexing
}
