//! Classified reads (`classification`): refusal on input, chunk selection,
//! and the routes to what a classified page left out.
use super::super::types::*;
use super::stale_snapshot;
use crate::id::ToolId;
use crate::security::scan::ContentScan;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};

/// The row for a selection that could not order the source.
fn selection_failure(
    q: &LocalFetchQuery,
    failure: crate::tools::selection::Failure,
) -> LocalFetchResult {
    match failure {
        crate::tools::selection::Failure::Stale => stale_snapshot(q),
        crate::tools::selection::Failure::Cancelled => LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::Cancelled,
            "Selection was cancelled".into(),
        ),
        crate::tools::selection::Failure::Provider(failure) => {
            let crate::tools::selection::ProviderFailure { error, outcome } = *failure;
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
    pub(super) runtime: Option<&'a crate::tools::selection::Runtime>,
    /// The query as the selection keys and prices it, serialized once.
    pub(super) query_json: std::cell::OnceCell<serde_json::Value>,
}

impl SelectionInput<'_> {
    pub(super) fn query_json(&self) -> &serde_json::Value {
        self.query_json
            .get_or_init(|| crate::response::result::query_value(self.q))
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
    match crate::tools::selection::select(
        input.runtime,
        input.query_json(),
        &input.source_key(),
        local_fetch_query::selection(q),
        &[],
        None,
        octocode_contract_types::SelectionUnit::Chunks,
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
    let unsupported = None;
    let (candidates, chunks) = super::super::selection::chunks(
        &view.text,
        ext.source_lines.as_deref(),
        Some(source_text),
        input.source_path,
        input.provider_path,
        security,
        input.runtime,
        local_fetch_query::selection(q),
        unsupported,
    );
    let (outcome, provider) =
        crate::tools::selection::unless_transient(crate::tools::selection::select(
            input.runtime,
            input.query_json(),
            &input.source_key(),
            local_fetch_query::selection(q),
            &candidates,
            unsupported,
            octocode_contract_types::SelectionUnit::Chunks,
        ))
        .map_err(|failure| Box::new(selection_failure(q, failure)))?;
    let shown = outcome.shown(chunks.len());
    let (text, source_lines) = super::super::selection::project(&view.text, &chunks, &shown);
    view.text = text;
    ext.source_lines = Some(source_lines);
    Ok(Some(super::super::selection::Chosen::new(
        outcome, provider, &chunks,
    )))
}

/// A classified page's routes to what it left out: `unfiltered` reads the
/// whole unfiltered view and, when the unknown chunks are narrower,
/// `unfilteredUnknown` reads exactly them (`unfilteredUnknown2`, … the rest
/// of their ranges; every call but the last holds as many rows as a call
/// takes). `nextCandidatePage` judges what the
/// candidate budget left unjudged.
pub(super) fn attach_unfiltered(
    calls: &mut NextCalls,
    q: &LocalFetchQuery,
    chosen: &super::super::selection::Chosen,
) -> Result<(), String> {
    let query = crate::response::result::query_value(q);
    let reads = chosen.unknown_reads(&query);
    calls.unfiltered = Some(unfiltered_read(q)?);
    for (index, batch) in reads
        .chunks(crate::response::result::rows_per_call(ToolId::LocalFetch))
        .enumerate()
    {
        // A lone read is the typed `unfilteredUnknown`; a batch is the same
        // name as a multi-row call.
        if let ([row], 0) = (batch, index) {
            calls.unfiltered_unknown = Some(FetchCall::continued(
                serde_json::from_value(row.clone()).map_err(|error| error.to_string())?,
            ));
            continue;
        }
        calls.unfiltered_unknown_rest.insert(
            match index {
                0 => "unfilteredUnknown".to_owned(),
                n => format!("unfilteredUnknown{}", n + 1),
            },
            crate::response::result::Continuation::continued_input(
                ToolId::LocalFetch,
                serde_json::json!({ "queries": batch }),
            )
            .build(),
        );
    }
    if calls.continue_window.is_none()
        && let Some(token) = chosen.next_candidates()
    {
        let mut page = q.clone();
        local_fetch_query::set_progress(&mut page, None);
        local_fetch_query::set_selection(&mut page, Some(token))
            .map_err(|error| error.to_string())?;
        calls.next_candidate_page = Some(FetchCall::continued(page));
    }
    Ok(())
}

/// The classified read as an ordinary read ([`crate::tools::selection::unfiltered_row`]).
fn unfiltered_read(q: &LocalFetchQuery) -> Result<FetchCall, String> {
    let row =
        crate::tools::selection::unfiltered_row(&crate::response::result::query_value(q), &[]);
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
pub(crate) fn unsupported_selection(
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
