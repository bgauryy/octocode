//! The requested view of the extracted text: content, compact, or a symbols
//! outline, with the typed fallback when one stands in for another.
use super::super::outline::render_symbols_view;
use super::super::types::*;
use super::{View, compact};
use octocode_contract_types::{LocalFetchQuery, ReadView};
use octocode_runtime_base::execution::cancel::CancellationCheck;

/// Apply the requested `view` to the extracted text. A match read
/// stays verbatim evidence; an outline the file cannot give falls back to
/// the compact view when compaction reads the file's grammar, else is the
/// reason (`Err`) no view stands in for it: a whitespace-only compact is
/// near-raw text, not a smaller view of the code, and a source too large to
/// parse is neither outlined nor compacted. A compact view of such a source
/// is its content view, with the reason.
pub(super) fn select_view(
    q: &LocalFetchQuery,
    ext: &mut super::super::extraction::Extraction,
    full_source: &str,
    cancel: &impl CancellationCheck,
) -> Result<View, String> {
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
    // The compact view, with a typed notice when it could only drop whitespace
    // or stands in for an outline. A compact view no smaller than the content
    // view of the same lines is not one: the content view is served, and the
    // notice says why.
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
        // included. The first page says both, so the agent can choose
        // either: no threshold decides for it.
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
        // A format that stays as it is serves its content view: the typed
        // fallback says so, and a size comparison of identical text is noise.
        let identical = !smaller && unreduced == Some(compact::Unreduced::AsIs);
        View {
            fallback: reason.map(|reason| ViewFallback {
                requested,
                applied,
                reason,
            }),
            note: (!identical).then_some(kept),
            ..view(text, mode)
        }
    };
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
                    // it would lex it: no view stands in for it.
                    if compact::too_large(&selected) {
                        return Err(compact::TOO_LARGE.to_owned());
                    }
                    let compact = compact_view(selected, ReadView::Symbols, Some(reason.as_str()));
                    match compact.fallback {
                        Some(fallback) if fallback.reason != reason.as_str() => {
                            return Err(fallback.reason);
                        }
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
