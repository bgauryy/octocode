//! Classified reads (`classification`): refusal on input, chunk selection,
//! and the routes to what a classified page left out.
use super::super::types::*;
use super::stale_snapshot;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::security::scan::ContentScan;

/// The row for a selection that could not order the source.
fn selection_failure(q: &LocalFetchQuery, failure: crate::selection::Failure) -> LocalFetchResult {
    match failure {
        crate::selection::Failure::Stale => stale_snapshot(q),
        crate::selection::Failure::Cancelled => LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::Cancelled,
            "Selection was cancelled".into(),
        ),
        crate::selection::Failure::Provider(failure) => {
            let crate::selection::ProviderFailure {
                error,
                outcome,
                confirm_ceiling,
                ..
            } = *failure;
            let confirm = match confirm_ceiling.map(|ceiling| confirm_read(q, ceiling)) {
                Some(Ok(read)) => Some(read),
                Some(Err(error)) => return invalid_continuation(q, error),
                None => None,
            };
            let mut result = LocalFetchResult::error(
                local_fetch_query::path(q).to_owned(),
                error.code,
                error.message,
            );
            result.hints = error.hints;
            result.selection = Some(outcome.receipt);
            result.next = Some(NextCalls {
                unfiltered: match unfiltered_read(q) {
                    Ok(read) => Some(read),
                    Err(error) => return invalid_continuation(q, error),
                },
                confirm,
                ..NextCalls::default()
            });
            result
        }
    }
}

/// Source and runtime needed to select evidence before paging.
pub(super) struct SelectionInput<'a> {
    pub(super) q: &'a LocalFetchQuery,
    pub(super) source_path: &'a std::path::Path,
    pub(super) provider_path: &'a str,
    pub(super) sha256: &'a str,
    pub(super) runtime: Option<&'a crate::selection::Runtime>,
    /// The query as the selection keys and prices it, serialized once.
    pub(super) query_json: std::cell::OnceCell<serde_json::Value>,
}

impl SelectionInput<'_> {
    pub(super) fn query_json(&self) -> &serde_json::Value {
        self.query_json
            .get_or_init(|| octocode_runtime_base::response::result::query_value(self.q))
    }

    /// The selection's source key: the path and the digest of its bytes.
    pub(super) fn source_key(&self) -> String {
        format!(
            "fetch-chunks-v2:{}:{}",
            self.source_path.display(),
            self.sha256
        )
    }
}

/// A read that selected nothing carries the selection receipt of its goals; a
/// ranked replay without goals is stale.
pub(super) fn select_no_match(
    input: &SelectionInput,
    mut result: LocalFetchResult,
) -> LocalFetchResult {
    let q = input.q;
    if !local_fetch_query::has_classification(q) {
        return if local_fetch_query::selection(q).is_some() {
            stale_snapshot(q)
        } else {
            result
        };
    }
    match crate::selection::select(
        input.runtime,
        ToolId::LocalFetch,
        input.query_json(),
        &input.source_key(),
        local_fetch_query::selection(q),
        &[],
        None,
        octocode_contract_types::SelectionUnit::Hits,
    ) {
        Ok(outcome) => result.selection = Some(outcome.receipt),
        Err(failure) => return selection_failure(q, failure),
    }
    result
}

/// A classified read its input alone rules out, refused before the source
/// is decoded, redacted or extracted: selection judges raw text in line
/// units, and pages only through its own selection token.
pub(super) fn refused_selection_input(
    q: &LocalFetchQuery,
    digest: &str,
) -> Option<LocalFetchResult> {
    if !local_fetch_query::has_classification(q) {
        return None;
    }
    if local_fetch_query::offset(q).is_some_and(|offset| offset > 0)
        && local_fetch_query::selection(q).is_none()
    {
        return Some(unsupported_selection(
            q,
            "A classified continuation is missing its selection token.",
            Some(digest),
        ));
    }
    None
}

/// Select complete source windows before response paging.
pub(super) fn select_view_chunks(
    input: &SelectionInput,
    source_text: &str,
    view: &mut super::View,
    ext: &mut super::super::extraction::Extraction,
    security: &impl ContentScan,
) -> Result<Option<super::super::selection::Chosen>, Box<LocalFetchResult>> {
    let q = input.q;
    if !local_fetch_query::has_classification(q) {
        if local_fetch_query::selection(q).is_some() {
            return Err(Box::new(stale_snapshot(q)));
        }
        return Ok(None);
    }
    // A search hit is the unit an agent asks about; a read with no search has
    // only chunks to offer.
    let source_lines = ext.source_lines.clone().unwrap_or_default();
    let by_hit = !ext.matched_lines.is_empty()
        && source_lines.len() == view.text.split_inclusive('\n').count();
    let unit = if by_hit {
        octocode_contract_types::SelectionUnit::Hits
    } else {
        octocode_contract_types::SelectionUnit::Chunks
    };
    let (candidates, chunks) = if by_hit {
        let context = local_fetch_query::context_lines(q);
        let (before, after) = (
            octocode_engine::num::saturating_usize(context.before),
            octocode_engine::num::saturating_usize(context.after),
        );
        let blocks = &ext.blocks;
        super::super::selection::hit_chunks(
            super::super::selection::HitView {
                text: &view.text,
                lines: &source_lines,
                source: source_text,
                path: input.source_path,
                provider_path: input.provider_path,
            },
            &ext.matched_lines,
            |hit| {
                blocks
                    .iter()
                    .find(|block| (block.line..=block.end_line).contains(&hit))
                    .map_or(
                        (hit.saturating_sub(before), hit.saturating_add(after)),
                        |block| (block.line, block.end_line),
                    )
            },
            security,
            input.runtime,
            local_fetch_query::selection(q),
        )
    } else {
        super::super::selection::chunks(
            &view.text,
            ext.source_lines.as_deref(),
            Some(source_text),
            input.source_path,
            input.provider_path,
            security,
            input.runtime,
            local_fetch_query::selection(q),
            None,
        )
    };
    let (outcome, provider) = crate::selection::unless_transient(crate::selection::select(
        input.runtime,
        ToolId::LocalFetch,
        input.query_json(),
        &input.source_key(),
        local_fetch_query::selection(q),
        &candidates,
        None,
        unit,
    ))
    .map_err(|failure| Box::new(selection_failure(q, failure)))?;
    let shown = outcome.shown(chunks.len());
    if by_hit {
        let keep = shown
            .iter()
            .flat_map(|&ordinal| chunks[ordinal].lines())
            .copied()
            .collect();
        let (text, lines) =
            super::super::selection::project_lines(&view.text, &source_lines, &keep);
        view.text = text;
        ext.source_lines = Some(lines);
        ext.matched_lines = shown
            .iter()
            .map(|&ordinal| ext.matched_lines[ordinal])
            .collect();
        ext.count = Some(ext.matched_lines.len());
    } else {
        let (text, lines) = super::super::selection::project(&view.text, &chunks, &shown);
        view.text = text;
        ext.source_lines = Some(lines);
    }
    Ok(Some(super::super::selection::Chosen::new(
        outcome, provider,
    )))
}

/// A classified page's routes to what it left out: `unfiltered` reads the
/// whole unfiltered view and, when some units stayed undecided,
/// `unfilteredUnknown` replays the selection as exactly them (unbilled, the
/// same read, its hits marked in `matchedLines`). `nextCandidatePage` judges
/// what the candidate budget left unjudged.
pub(super) fn attach_unfiltered(
    calls: &mut NextCalls,
    q: &LocalFetchQuery,
    chosen: &super::super::selection::Chosen,
) -> Result<(), String> {
    let replay = |token: &str| -> Result<FetchCall, String> {
        let page = octocode_runtime_base::response::result::with_changes(
            octocode_runtime_base::response::result::query_value(q),
            &octocode_runtime_base::response::paging::candidate_page_changes(
                ToolId::LocalFetch,
                token,
                None,
            ),
        );
        serde_json::from_value(page)
            .map(FetchCall::continued)
            .map_err(|error| error.to_string())
    };
    calls.unfiltered = Some(unfiltered_read(q)?);
    if let Some(token) = chosen.undecided_view() {
        calls.unfiltered_unknown = Some(replay(&token)?);
    }
    if calls.continue_window.is_none()
        && let Some(token) = chosen.next_candidates()
    {
        calls.next_candidate_page = Some(replay(token)?);
    }
    Ok(())
}

/// The classified read as an ordinary read ([`crate::selection::unfiltered_row`]).
fn unfiltered_read(q: &LocalFetchQuery) -> Result<FetchCall, String> {
    let row = crate::selection::unfiltered_row(
        &octocode_runtime_base::response::result::query_value(q),
        &[],
    );
    serde_json::from_value(row)
        .map(FetchCall::continued)
        .map_err(|error| error.to_string())
}

/// The same read with `classification.maxRequests` at `ceiling`
/// ([`crate::selection::confirm_row`]).
fn confirm_read(q: &LocalFetchQuery, ceiling: u64) -> Result<FetchCall, String> {
    let row = crate::selection::confirm_row(
        &octocode_runtime_base::response::result::query_value(q),
        ceiling,
    );
    serde_json::from_value(row)
        .map(FetchCall::continued)
        .map_err(|error| error.to_string())
}

pub(super) fn invalid_continuation(q: &LocalFetchQuery, error: String) -> LocalFetchResult {
    LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::ExecutionFailed,
        error,
    )
}

/// A classified read refused before selection: recover its public view and
/// ranges without classification or private progress. A captured `digest`
/// makes this a source-bound continuation; refusal before capture offers an
/// optional fresh read.
pub fn unsupported_selection(
    q: &LocalFetchQuery,
    message: &str,
    digest: Option<&str>,
) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::InvalidInput,
        message.into(),
    );
    result.source_sha256 = digest.map(str::to_owned);
    result.next = Some(NextCalls {
        unfiltered: match unfiltered_read(q) {
            Ok(read) => Some(if digest.is_some() {
                read
            } else {
                FetchCall::related(read.query)
            }),
            Err(error) => return invalid_continuation(q, error),
        },
        ..NextCalls::default()
    });
    result
}
