//! Search response decoding, preserving partial-organization evidence, and
//! a caller's page of a search read through GitHub's largest pages.
use super::decode;
use crate::{GitHubTransport, ProviderError, ProviderErrorKind, RequestContext, RequestSpec};
use serde::Deserialize;
use serde_json::Value;

/// Results GitHub search serves for one query, however many match.
pub const SEARCH_RESULT_CAP: usize = 1000;

/// The largest `per_page` every REST search endpoint (repositories, code,
/// issues, commits) serves.
const SEARCH_PAGE: usize = crate::MAX_PER_PAGE;

/// The provider pages (first, last) that caller page `page` of `per_page`
/// results reads, or `None` when it starts past the result cap (that page is
/// GitHub's own answer, read as asked).
fn provider_pages(page: usize, per_page: usize) -> Option<(usize, usize)> {
    let start = (page - 1).saturating_mul(per_page);
    let end = start.saturating_add(per_page);
    let first = start / SEARCH_PAGE + 1;
    let last = ((end - 1) / SEARCH_PAGE + 1).min(SEARCH_RESULT_CAP.div_ceil(SEARCH_PAGE));
    (first <= last).then_some((first, last))
}

/// The requests a read of caller page `page` of `per_page` results can send,
/// in order ([`GitHubTransport::read_search_page`] stops early at a short
/// provider page).
pub(crate) fn search_requests(
    spec: impl Fn(usize, usize) -> Result<RequestSpec, ProviderError>,
    page: usize,
    per_page: usize,
) -> Result<Vec<RequestSpec>, ProviderError> {
    let (page, per_page) = (page.max(1), per_page.max(1));
    match provider_pages(page, per_page) {
        Some((first, last)) => (first..=last)
            .map(|provider_page| spec(provider_page, SEARCH_PAGE))
            .collect(),
        None => Ok(vec![spec(page, per_page)?]),
    }
}

impl GitHubTransport {
    /// Page `page` of `per_page` results of one search, read through
    /// GitHub's largest search pages and sliced: the results GitHub's own
    /// `page`/`per_page` would answer, in the same order, with the first
    /// read page's totals. `spec` builds the request for one provider page
    /// (`page`, `per_page`). Every caller page inside one provider page asks
    /// for it in the same words, so a continuation whose cursor owns that
    /// answer (the request journal) reads nothing, and a walk at any page
    /// size costs one search request per 100 results.
    pub(crate) async fn read_search_page<T: for<'de> Deserialize<'de>>(
        &self,
        spec: impl Fn(usize, usize) -> Result<RequestSpec, ProviderError>,
        page: usize,
        per_page: usize,
        context: &RequestContext,
        invalid: &'static str,
    ) -> Result<(T, Vec<u64>), ProviderError> {
        let (page, per_page) = (page.max(1), per_page.max(1));
        let start = (page - 1).saturating_mul(per_page);
        let end = start.saturating_add(per_page);
        let Some((first, last)) = provider_pages(page, per_page) else {
            return self
                .read_search(spec(page, per_page)?, context, invalid)
                .await;
        };
        let mut merged: Option<Value> = None;
        let mut items = Vec::new();
        let mut withheld = Vec::<u64>::new();
        for provider_page in first..=last {
            let (mut answer, orgs): (Value, Vec<u64>) = self
                .read_search(spec(provider_page, SEARCH_PAGE)?, context, invalid)
                .await?;
            for org in orgs {
                if !withheld.contains(&org) {
                    withheld.push(org);
                }
            }
            let offset = (provider_page - 1) * SEARCH_PAGE;
            let answered = match answer.get_mut("items").map(Value::take) {
                Some(Value::Array(answered)) => answered,
                _ => Vec::new(),
            };
            let full = answered.len() >= SEARCH_PAGE;
            items.extend(
                answered
                    .into_iter()
                    .enumerate()
                    .filter(|(index, _)| (start..end).contains(&(offset + index)))
                    .map(|(_, item)| item),
            );
            match &mut merged {
                None => merged = Some(answer),
                Some(merged) => {
                    if answer.get("incomplete_results") == Some(&Value::Bool(true)) {
                        merged["incomplete_results"] = Value::Bool(true);
                    }
                }
            }
            // A short provider page is the end of the results.
            if !full {
                break;
            }
        }
        let mut merged = merged.unwrap_or_else(|| Value::Object(Default::default()));
        merged["items"] = Value::Array(items);
        let page = serde_json::from_value(merged)
            .map_err(|_| ProviderError::new(ProviderErrorKind::Decode, invalid))?;
        Ok((page, withheld))
    }

    pub(crate) async fn read_search<T: for<'de> Deserialize<'de>>(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        invalid: &'static str,
    ) -> Result<(T, Vec<u64>), ProviderError> {
        let response = self
            .execute(spec, context)
            .await
            .map_err(unsearchable_repository)?;
        let page = decode(response.body.as_ref(), invalid)?;
        Ok((page, response.sso_partial_organizations()))
    }
}

/// A 422 "cannot be searched" names what GitHub could not search: "users and
/// repositories" is the `repo:`/`user:` scope (the repository was not found,
/// not a bad query); "users" alone is a person qualifier (`author:`,
/// `assignee:`, …) whose login does not exist or is not visible.
fn unsearchable_repository(mut error: ProviderError) -> ProviderError {
    if error.status == Some(422) && error.message.contains("cannot be searched") {
        error.kind = ProviderErrorKind::NotFound;
        let reason = if error.message.contains("repositories") {
            crate::ProviderErrorReason::RepositoryNotFound
        } else {
            crate::ProviderErrorReason::UserNotFound
        };
        error = error.with_reason(reason);
    }
    error
}
