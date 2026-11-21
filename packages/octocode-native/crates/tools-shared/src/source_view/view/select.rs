//! The requested view of the extracted text: content, compact, or a symbols
//! outline, with the typed fallback when one stands in for another.
use super::super::outline::render_symbols_view;
use super::super::types::*;
use super::{View, compact};
use octocode_contract_types::{LocalFetchQuery, ReadView};
use octocode_runtime_base::execution::cancel::CancellationCheck;

/// Why no view of the selected text stands in for the one requested.
pub(super) enum Refused {
    /// The selected text is minified ([`minified`]).
    Minified(Minified),
}

/// Selected text this long or longer is judged by its lines ([`minified`]).
const MINIFIED_BYTES: usize = 8 * 1024;

/// An average line this long is minified or generated text: no formatter
/// writes lines this long, so an outline of it lists minifier names and a
/// compact layout has no whitespace left to drop.
const MINIFIED_LINE_BYTES: usize = 1_000;

/// Minified text's lines and the bytes per line they average.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Minified {
    pub(super) lines: usize,
    pub(super) average: usize,
}

/// Source bytes per word at most in minified code: a minifier leaves an
/// identifier, keyword or number every few bytes, while a long comment or
/// string literal is one word, and deep nesting (`((((`) none.
const MINIFIED_BYTES_PER_WORD: usize = 20;

/// Whether `text`, read at `path`, is minified code: a grammar parses its
/// format (only then would an outline or a compact layout parse it), it is
/// at least [`MINIFIED_BYTES`] long with lines averaging
/// [`MINIFIED_LINE_BYTES`] or more, and those lines are dense with words
/// ([`MINIFIED_BYTES_PER_WORD`]). One long line among many short ones is
/// not, nor is a long comment, a string literal or a deeply nested line.
pub(super) fn minified(text: &str, path: &str) -> Option<Minified> {
    if text.len() < MINIFIED_BYTES {
        return None;
    }
    let lines = super::super::extraction::line_count(text).max(1);
    let average = text.len() / lines;
    if average < MINIFIED_LINE_BYTES {
        return None;
    }
    let extension = octocode_engine::text::extension_of(path, true, "");
    octocode_engine::signatures::languages::find_entry(&extension)?;
    // Runs of word bytes, as a lexer cuts identifiers and numbers.
    let (mut words, mut in_word) = (0usize, false);
    for &byte in text.as_bytes() {
        let word = byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80;
        words += usize::from(word && !in_word);
        in_word = word;
    }
    (words.saturating_mul(MINIFIED_BYTES_PER_WORD) >= text.len())
        .then_some(Minified { lines, average })
}

/// Apply the requested `view` to the extracted text. A match read
/// stays verbatim evidence; an outline the file cannot give falls back to
/// the compact view when compaction reads the file's grammar, else to the
/// content view: a whitespace-only compact is near-raw text, not a smaller
/// view of the code, and a source too large to parse is neither outlined
/// nor compacted. A compact view of such a source is its content view too.
/// Every fallback names the view served and the reason (`viewFallback`).
/// Minified text gets neither view and is not parsed for one (`Err`):
/// matchString selects from it.
pub(super) fn select_view(
    q: &LocalFetchQuery,
    ext: &mut super::super::extraction::Extraction,
    full_source: &str,
    cancel: &impl CancellationCheck,
) -> Result<View, Refused> {
    let requested = local_fetch_query::view(q);
    let selected = std::mem::take(&mut ext.text);
    // matchString reads are verbatim source; the tool description says so,
    // so the row carries no fallback notice. Byte windows address the
    // selected view (a numbered walk pages an oversized line by view bytes),
    // so they take no view of their own here.
    // Their preparation scanned the whole source (see `prepare_source`).
    if local_fetch_query::has_matches(q) {
        return Ok(View {
            text: selected,
            mode: None,
            fallback: None,
            note: None,
        });
    }
    // Every transformed view keeps the citation gutter (`N\t`): the standard
    // view numbers each kept line with its source line, the outline uses the
    // same separator in every language.
    let view = |text, mode| View {
        text,
        mode,
        fallback: None,
        note: None,
    };
    // The compact view, with a typed notice when it stands in for an outline
    // or the content view stands in for it. A compact view no smaller than
    // the content view of the same lines is not one: the content view is
    // served, and the notice says why.
    let compact_view = |selected: String, requested: ReadView, outline_reason: Option<&str>| {
        let Some((text, unreduced)) = compact::render(
            full_source,
            &selected,
            local_fetch_query::path(q),
            ext.source_lines.as_deref(),
            cancel,
        ) else {
            // Too large to lex: the content view is served, and says why.
            let reason = outline_reason
                .into_iter()
                .chain([compact::TOO_LARGE])
                .collect::<Vec<_>>()
                .join("; ");
            return View {
                fallback: Some(ViewFallback {
                    requested,
                    applied: ReadView::Content,
                    reason,
                }),
                ..view(selected, None)
            };
        };
        // Like with like: what the content view serves for the same
        // selection (every page of it, not the page that says it), gutters
        // included. The first page of a smaller compact view says both, so
        // the agent can choose either: no threshold decides for it.
        let content = compact::content_bytes(&selected, ext.source_lines.as_deref());
        let smaller = text.len() < content;
        let hundredths = text.len().saturating_mul(10_000) / content.max(1);
        let kept = format!(
            "compact view: {} of {} bytes the content view serves for the selected lines, all pages ({}.{:02}%).",
            text.len(),
            content,
            hundredths / 100,
            hundredths % 100
        );
        let reasons: Vec<&str> = outline_reason
            .into_iter()
            .chain(unreduced.map(compact::Unreduced::reason))
            // A format that stays as it is has said why its content is
            // served: it is no smaller by design.
            .chain(
                (!smaller && unreduced != Some(compact::Unreduced::AsIs))
                    .then_some(compact::NOT_SMALLER),
            )
            .collect();
        let reason = (!reasons.is_empty()).then(|| reasons.join("; "));
        let (text, mode, applied) = if smaller {
            (text, Some(ReadView::Compact), ReadView::Compact)
        } else {
            (selected, None, ReadView::Content)
        };
        View {
            // A fallback names a view served in place of the one requested:
            // the requested view, served, is what the row's `view` says.
            fallback: reason
                .filter(|_| applied != requested)
                .map(|reason| ViewFallback {
                    requested,
                    applied,
                    reason,
                }),
            // The content view served in place of a compact one no smaller
            // has said why (`viewFallback`): a size comparison that removed
            // nothing is noise.
            note: smaller.then_some(kept),
            ..view(text, mode)
        }
    };
    if requested != ReadView::Content
        && let Some(minified) = minified(&selected, local_fetch_query::path(q))
    {
        return Err(Refused::Minified(minified));
    }
    Ok(match requested {
        ReadView::Content => view(selected, None),
        ReadView::Compact => compact_view(selected, ReadView::Compact, None),
        ReadView::Symbols => {
            match render_symbols_view(&selected, local_fetch_query::path(q)) {
                super::super::outline::SymbolsView {
                    outline: Ok(outline),
                    warning,
                } => {
                    View {
                        // An outline from a recovered parse says where it is
                        // blind, once: on the outline's first page.
                        note: warning,
                        ..view(outline, Some(ReadView::Symbols))
                    }
                }
                // `viewFallback` is the typed notice of the applied view.
                super::super::outline::SymbolsView {
                    outline: Err(reason),
                    ..
                } => {
                    // A source whose parse is estimated past the memory
                    // budget was not parsed for its outline, and compacting
                    // it would lex it: its content view is served.
                    if compact::too_large(&selected) {
                        return Ok(View {
                            fallback: Some(ViewFallback {
                                requested: ReadView::Symbols,
                                applied: ReadView::Content,
                                reason: compact::TOO_LARGE.to_owned(),
                            }),
                            ..view(selected, None)
                        });
                    }
                    let content = selected.clone();
                    let compact = compact_view(selected, ReadView::Symbols, Some(reason.as_str()));
                    match compact.fallback {
                        // Compaction that reads no grammar, or removes
                        // nothing, is near-raw text and no smaller view of
                        // the code: the content view is served as what it
                        // is, and the fallback says why.
                        Some(fallback) if fallback.reason != reason.as_str() => View {
                            fallback: Some(ViewFallback {
                                applied: ReadView::Content,
                                ..fallback
                            }),
                            ..view(content, None)
                        },
                        fallback => View {
                            fallback,
                            ..compact
                        },
                    }
                }
            }
        }
    })
}
