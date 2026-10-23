//! Classified reads (`clasifyContext`): refusal on input, chunk selection,
//! and the routes to what a classified page left out.
use super::super::types::*;
use super::stale_snapshot;
use crate::id::ToolId;
use crate::security::scan::ContentScan;
use octocode_contract_types::ErrorCode;

/// The row for a selection that could not order the source.
fn selection_failure(
    q: &LocalFetchQuery,
    failure: crate::tools::selection::Failure,
) -> LocalFetchResult {
    match failure {
        crate::tools::selection::Failure::Stale => stale_snapshot(q),
        crate::tools::selection::Failure::Cancelled => LocalFetchResult::error(
            q.path.to_string(),
            ErrorCode::Cancelled,
            "Selection was cancelled".into(),
        ),
        crate::tools::selection::Failure::Provider(failure) => {
            let crate::tools::selection::ProviderFailure { error, outcome } = *failure;
            let mut result = LocalFetchResult::error(q.path.to_string(), error.code, error.message);
            result.hints = error.hints;
            result.selection = Some(outcome.receipt);
            result.next = Some(NextCalls {
                unfiltered: unfiltered_read(q),
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
    if q.clasify_context.is_none() {
        return if q.selection.is_some() {
            stale_snapshot(q)
        } else {
            result
        };
    }
    match crate::tools::selection::select(
        input.runtime,
        input.query_json(),
        &input.source_key(),
        q.selection.as_ref().map(|token| token.as_str()),
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
    q.clasify_context.as_ref()?;
    // A first call with an offset is a request, not a stale cursor.
    if local_fetch_query::offset(q).is_some_and(|offset| offset > 0) && q.selection.is_none() {
        return Some(unsupported_selection(
            q,
            "A classified read pages only through its continueWindow selection token; drop offset or clasifyContext.",
            Some(digest),
        ));
    }
    // A match read is verbatim whatever its minify (see `select_view`).
    let transformed = local_fetch_query::minify_mode(q).is_some() && q.match_string.is_none();
    if transformed || (q.unit == Some(WindowUnit::Bytes) && q.selection.is_none()) {
        return Some(unsupported_selection(
            q,
            "clasifyContext requires raw text in line units; omit minify, and omit unit or use unit:lines.",
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
    if q.clasify_context.is_none() {
        if q.selection.is_some() {
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
        q.selection.as_ref().map(|token| token.as_str()),
        unsupported,
    );
    let (outcome, provider) =
        crate::tools::selection::unless_transient(crate::tools::selection::select(
            input.runtime,
            input.query_json(),
            &input.source_key(),
            q.selection.as_ref().map(|s| s.as_str()),
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
) {
    let query = crate::response::result::query_value(q);
    let reads = chosen.unknown_reads(&query);
    calls.unfiltered = unfiltered_read(q);
    for (index, batch) in reads
        .chunks(crate::response::result::rows_per_call(ToolId::LocalFetch))
        .enumerate()
    {
        // A lone read is the typed `unfilteredUnknown`; a batch is the same
        // name as a multi-row call.
        if let ([row], 0) = (batch, index) {
            calls.unfiltered_unknown = serde_json::from_value(row.clone())
                .ok()
                .map(|query| FetchCall { query });
            continue;
        }
        calls.unfiltered_unknown_rest.insert(
            match index {
                0 => "unfilteredUnknown".to_owned(),
                n => format!("unfilteredUnknown{}", n + 1),
            },
            crate::response::result::Continuation::input(
                ToolId::LocalFetch,
                serde_json::json!({ "queries": batch }),
            )
            .build(),
        );
    }
    if let Some(token) = chosen.next_candidates() {
        let mut page = q.clone();
        page.offset = None;
        page.selection = token.parse().ok();
        calls.next_candidate_page = Some(FetchCall { query: page });
    }
}

/// The classified read as an ordinary read ([`crate::tools::selection::unfiltered_row`]).
fn unfiltered_read(q: &LocalFetchQuery) -> Option<FetchCall> {
    let row = crate::tools::selection::unfiltered_row(&serde_json::to_value(q).ok()?, &[]);
    serde_json::from_value(row)
        .ok()
        .map(|query| FetchCall { query })
}

/// A classified read refused before any selection ran: its unfiltered lead
/// is the caller's own read without `clasifyContext`, window and view kept
/// (`unit`, `offset`, `length`, `ranges`, `minify`), so dropping the
/// classifier is the only change. `digest` binds the lead to the source
/// version the refusal saw.
pub(crate) fn unsupported_selection(
    q: &LocalFetchQuery,
    message: &str,
    digest: Option<&str>,
) -> LocalFetchResult {
    let mut result =
        LocalFetchResult::error(q.path.to_string(), ErrorCode::InvalidInput, message.into());
    result.source_sha256 = digest.map(str::to_owned);
    let mut query = q.clone();
    query.clasify_context = None;
    query.selection = None;
    result.next = Some(NextCalls {
        unfiltered: Some(FetchCall { query }),
        ..NextCalls::default()
    });
    result
}
