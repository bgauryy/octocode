//! Search response decoding, preserving partial-organization evidence, and
//! a caller's page of a search: a first page asks for what it needs, and a walk
//! reads provider pages that double in size up to GitHub's largest.
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

/// The provider page (its number and size) that holds caller page `page` of
/// `per_page` results while a walk is still short: GitHub's second page at
/// `per_page` times the largest power of two no greater than `page - 1`, so
/// caller page 2 asks for exactly itself, pages 3-4 share one answer, 5-8 the
/// next, and each answer is as large as everything the walk read before it.
/// GitHub's time and bytes grow with the page size asked for (100 issues with
/// their bodies run past a megabyte), so a second page costs what a first one
/// does, and a walk of `n` pages still sends about `log2(n)` requests. `None`
/// for a first page and once that size passes GitHub's largest page: from there
/// the walk reads whole provider pages ([`provider_pages`]).
fn grown_page(page: usize, per_page: usize) -> Option<(usize, usize)> {
    let before = page.checked_sub(1).filter(|before| *before > 0)?;
    let size = per_page.checked_mul(1 << before.ilog2())?;
    (size <= SEARCH_PAGE).then_some((2, size))
}

impl GitHubTransport {
    /// Whether caller page `page` of `per_page` results asks GitHub for exactly
    /// that page: a first page does, since GitHub's time and bytes grow with the
    /// page it is asked for and most searches stop there, unless a whole provider
    /// page already held answers it with nothing sent.
    fn asks_exact_first_page(
        &self,
        spec: &impl Fn(usize, usize) -> Result<RequestSpec, ProviderError>,
        page: usize,
        per_page: usize,
        context: &RequestContext,
    ) -> Result<bool, ProviderError> {
        Ok(page == 1
            && per_page < SEARCH_PAGE
            && !self.answers_unasked(&spec(1, SEARCH_PAGE)?, context))
    }

    /// The provider pages (number, size) a read of caller page `page` of
    /// `per_page` results asks for, in order: the first page as asked; whole
    /// provider pages when answers already held cover it with nothing sent;
    /// else the walk's grown page ([`grown_page`]), or whole provider pages
    /// past it. A page that starts past the result cap is GitHub's own answer,
    /// read as asked.
    fn provider_reads(
        &self,
        spec: &impl Fn(usize, usize) -> Result<RequestSpec, ProviderError>,
        page: usize,
        per_page: usize,
        context: &RequestContext,
    ) -> Result<Vec<(usize, usize)>, ProviderError> {
        if self.asks_exact_first_page(spec, page, per_page, context)? {
            return Ok(vec![(1, per_page)]);
        }
        let Some((first, last)) = provider_pages(page, per_page) else {
            return Ok(vec![(page, per_page)]);
        };
        let whole = (first..=last)
            .map(|provider_page| (provider_page, SEARCH_PAGE))
            .collect::<Vec<_>>();
        let Some(grown) = grown_page(page, per_page) else {
            return Ok(whole);
        };
        let mut held = true;
        for (provider_page, size) in &whole {
            held &= self.answers_unasked(&spec(*provider_page, *size)?, context);
        }
        Ok(if held { whole } else { vec![grown] })
    }

    /// The requests a read of caller page `page` of `per_page` results can send,
    /// in order ([`Self::read_search_page`] stops early at a short provider page).
    pub(crate) fn search_requests(
        &self,
        spec: impl Fn(usize, usize) -> Result<RequestSpec, ProviderError>,
        page: usize,
        per_page: usize,
        context: &RequestContext,
    ) -> Result<Vec<RequestSpec>, ProviderError> {
        let (page, per_page) = (page.max(1), per_page.max(1));
        self.provider_reads(&spec, page, per_page, context)?
            .into_iter()
            .map(|(provider_page, size)| spec(provider_page, size))
            .collect()
    }

    /// Page `page` of `per_page` results of one search: the results GitHub's own
    /// `page`/`per_page` would answer, in the same order, with the first read
    /// page's totals. A first page asks for itself ([`Self::asks_exact_first_page`]).
    /// A later one reads the provider page that holds it ([`Self::provider_reads`])
    /// and slices it. `spec` builds the request for one provider page (`page`,
    /// `per_page`). Every caller page inside one provider page asks for it in
    /// the same words, so a continuation whose cursor owns that answer (the
    /// request journal) reads nothing: a walk at any page size sends a request
    /// each time it doubles, then one per 100 results.
    pub(crate) async fn read_search_page<T: for<'de> Deserialize<'de>>(
        &self,
        spec: impl Fn(usize, usize) -> Result<RequestSpec, ProviderError>,
        page: usize,
        per_page: usize,
        context: &RequestContext,
        invalid: &'static str,
    ) -> Result<(T, Vec<u64>), ProviderError> {
        let (page, per_page) = (page.max(1), per_page.max(1));
        let reads = self.provider_reads(&spec, page, per_page, context)?;
        // One read at the caller's own page and size is GitHub's answer whole.
        if reads == [(page, per_page)] {
            return self
                .read_search(spec(page, per_page)?, context, invalid)
                .await;
        }
        let start = (page - 1).saturating_mul(per_page);
        let end = start.saturating_add(per_page);
        let mut merged: Option<Value> = None;
        let mut items = Vec::new();
        let mut withheld = Vec::<u64>::new();
        for (provider_page, size) in reads {
            let (mut answer, orgs): (Value, Vec<u64>) = self
                .read_search(spec(provider_page, size)?, context, invalid)
                .await?;
            for org in orgs {
                if !withheld.contains(&org) {
                    withheld.push(org);
                }
            }
            let offset = (provider_page - 1) * size;
            let answered = match answer.get_mut("items").map(Value::take) {
                Some(Value::Array(answered)) => answered,
                _ => Vec::new(),
            };
            let full = answered.len() >= size;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every caller page of a walk lies whole inside the provider page it
    /// reads, a second page asks for exactly itself, and no page asks for more
    /// than everything the walk read before it.
    #[test]
    fn a_walk_reads_provider_pages_that_double() {
        for per_page in [1, 3, 7, 30, 50, 99] {
            for page in 2..=40 {
                let Some((provider_page, size)) = grown_page(page, per_page) else {
                    assert!(per_page * (1 << (page - 1).ilog2()) > SEARCH_PAGE);
                    continue;
                };
                let held = (provider_page - 1) * size..provider_page * size;
                let asked = (page - 1) * per_page..page * per_page;
                assert!(held.start <= asked.start && asked.end <= held.end);
                assert!(size <= asked.start, "page {page} asks for more than was read");
                assert!(size <= SEARCH_PAGE);
            }
        }
        assert_eq!(grown_page(1, 5), None);
        assert_eq!(grown_page(2, 5), Some((2, 5)));
        assert_eq!(grown_page(3, 5), Some((2, 10)));
        assert_eq!(grown_page(4, 5), Some((2, 10)));
        assert_eq!(grown_page(5, 5), Some((2, 20)));
        assert_eq!(grown_page(2, 100), Some((2, 100)));
        assert_eq!(grown_page(3, 100), None);
    }
}
